//! The microphone: a chain of effects between the real microphone and every
//! app that records, in the manner of EasyEffects.
//!
//! The service owns the settings (`~/.config/conchy/mic.kdl`) and the audio
//! path. See [`chain`] for the graph and [`engine`] for how the sound is
//! routed.

pub mod chain;
mod engine;
pub mod model;

use std::sync::{Arc, Mutex, PoisonError};

use futures_signals::signal::{Mutable, MutableSignalCloned};

pub use self::chain::{Plugins, SOURCE_NAME};
pub use self::model::{Denoiser, Param, Preset, Settings, Voice, builtin_presets};
use crate::jobs::JobQueue;
use crate::lifecycle::{Lifecycle, ManagedService};

/// A real microphone the voice can be taken from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDevice {
    /// PipeWire node name.
    pub name: String,
    /// What the device is called on screen.
    pub description: String,
}

/// Where the audio path is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EngineState {
    /// Not in the audio path.
    #[default]
    Off,
    /// On, waiting for the filters (or for a microphone) to appear.
    Starting,
    /// Every app hears the microphone through the chain.
    Running,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MicData {
    pub settings: Settings,
    pub engine: EngineState,
    /// Every real microphone.
    pub inputs: Vec<InputDevice>,
    /// The microphone the voice is taken from (or would be, with the chain
    /// on).
    pub input: Option<String>,
    /// Which plugins the stages can be made of.
    pub plugins: Plugins,
}

impl MicData {
    /// What the microphone is called on screen.
    pub fn input_description(&self) -> Option<&str> {
        let name = self.input.as_deref()?;
        self.inputs
            .iter()
            .find(|device| device.name == name)
            .map(|device| device.description.as_str())
    }

    /// The packages the voice needs that are not installed.
    pub fn missing_packages(&self) -> Vec<&'static str> {
        self.plugins.missing(&self.settings.voice)
    }
}

#[derive(Debug, Clone)]
pub enum MicCommand {
    SetEnabled(bool),
    /// One setting of the voice.
    Set(Param, f32),
    ApplyPreset(String),
    /// Every setting back to its default.
    Reset,
    /// Keep the current voice as a preset of this name.
    SavePreset(String),
    DeletePreset(String),
    /// Take the voice from another microphone (by node name).
    SetInput(String),
    /// Look for plugins again, after the user installed some.
    Rescan,
}

/// Reactive access to the microphone chain and its commands.
#[derive(Debug, Clone)]
pub struct MicSubscriber {
    data: Mutable<MicData>,
    lifecycle: Lifecycle,
    jobs: JobQueue<MicCommand>,
}

impl Default for MicSubscriber {
    fn default() -> Self {
        Self {
            data: Mutable::default(),
            lifecycle: Lifecycle::default(),
            // A slider drag only needs its last value per setting.
            jobs: JobQueue::new("Microphone").coalesce(|older, newer| match (older, newer) {
                (MicCommand::Set(a, _), MicCommand::Set(b, _)) => a == b,
                _ => false,
            }),
        }
    }
}

impl MicSubscriber {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self) -> MutableSignalCloned<MicData> {
        self.data.signal_cloned()
    }

    pub fn get(&self) -> MicData {
        self.data.get_cloned()
    }

    /// Queue a command. It runs on the PipeWire thread; failures are logged.
    pub fn dispatch(&self, command: MicCommand) {
        self.jobs.send(command);
    }
}

impl ManagedService for MicSubscriber {
    fn name(&self) -> &'static str {
        "Microphone"
    }

    fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    fn memory_bytes(&self) -> usize {
        size_of::<MicData>()
            + self.data.lock_ref().settings.user_presets.len() * size_of::<Preset>()
    }

    fn start(&self) {
        let token = self.lifecycle.begin();

        // The loop thread is asleep in PipeWire's loop; a message on this
        // run's channel wakes it for a queued command. The slot holds the
        // sender of the attempt that is running now.
        let slot: Arc<Mutex<Option<pipewire::channel::Sender<engine::Wake>>>> = Arc::default();
        let jobs = Arc::new(Mutex::new(self.jobs.attach({
            let slot = slot.clone();
            move || {
                if let Some(sender) = slot.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
                    let _ = sender.send(engine::Wake::Jobs);
                }
            }
        })));

        let data = self.data.clone();
        token.supervise_blocking("Microphone", move |token| {
            engine::run(&data, token, &jobs, &slot)
        });
    }
}
