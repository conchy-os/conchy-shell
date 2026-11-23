//! System-wide equalizer: ten bands and a preamp on everything the machine
//! plays.
//!
//! The service owns the settings (`~/.config/conchy/equalizer.kdl`) and the
//! audio path. See [`engine`] for how sound reaches the filters and
//! [`dsp`] for the maths behind the curve the UI draws.

pub mod dsp;
mod engine;
pub mod model;

use std::sync::{Arc, Mutex, PoisonError};

use futures_signals::signal::{Mutable, MutableSignalCloned};

pub use self::engine::SINK_NAME;
pub use self::model::{Curve, Preset, Settings, builtin_presets};
use crate::jobs::JobQueue;
use crate::lifecycle::{Lifecycle, ManagedService};

/// A real output the equalized sound can be played to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDevice {
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
    /// On, waiting for the filters (or for an output) to appear.
    Starting,
    /// Everything plays through the equalizer.
    Running,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EqualizerData {
    pub settings: Settings,
    pub engine: EngineState,
    /// Every real output, by name.
    pub outputs: Vec<OutputDevice>,
    /// The output the sound goes to (or would, with the equalizer on).
    pub output: Option<String>,
}

impl EqualizerData {
    /// What the output is called on screen.
    pub fn output_description(&self) -> Option<&str> {
        let name = self.output.as_deref()?;
        self.outputs
            .iter()
            .find(|device| device.name == name)
            .map(|device| device.description.as_str())
    }
}

#[derive(Debug, Clone)]
pub enum EqualizerCommand {
    SetEnabled(bool),
    /// Gain of one band, in dB.
    SetBand { band: usize, gain: f32 },
    /// Preamp, in dB.
    SetPreamp(f32),
    SetAutoPreamp(bool),
    ApplyPreset(String),
    /// Every band and the preamp back to 0 dB.
    Reset,
    /// Keep the current curve as a preset of this name.
    SavePreset(String),
    DeletePreset(String),
    /// Play to another sink (by node name): makes it the system default,
    /// which the equalizer follows.
    SetOutput(String),
}

/// Reactive access to the equalizer and its commands.
#[derive(Debug, Clone)]
pub struct EqualizerSubscriber {
    data: Mutable<EqualizerData>,
    lifecycle: Lifecycle,
    jobs: JobQueue<EqualizerCommand>,
}

impl Default for EqualizerSubscriber {
    fn default() -> Self {
        use EqualizerCommand::{SetBand, SetPreamp};
        Self {
            data: Mutable::default(),
            lifecycle: Lifecycle::default(),
            // A slider drag only needs its last value per band.
            jobs: JobQueue::new("Equalizer").coalesce(|older, newer| match (older, newer) {
                (SetBand { band: a, .. }, SetBand { band: b, .. }) => a == b,
                (SetPreamp(_), SetPreamp(_)) => true,
                _ => false,
            }),
        }
    }
}

impl EqualizerSubscriber {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self) -> MutableSignalCloned<EqualizerData> {
        self.data.signal_cloned()
    }

    pub fn get(&self) -> EqualizerData {
        self.data.get_cloned()
    }

    /// Queue a command. It runs on the PipeWire thread; failures are logged.
    pub fn dispatch(&self, command: EqualizerCommand) {
        self.jobs.send(command);
    }
}

impl ManagedService for EqualizerSubscriber {
    fn name(&self) -> &'static str {
        "Equalizer"
    }

    fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    fn memory_bytes(&self) -> usize {
        size_of::<EqualizerData>()
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
        token.supervise_blocking("Equalizer", move |token| {
            engine::run(&data, token, &jobs, &slot)
        });
    }
}
