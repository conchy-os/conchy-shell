//! The audio path of the microphone, on PipeWire.
//!
//! Turning the chain on does three things:
//!
//! 1. starts a PipeWire *filter-chain* (a child `pipewire -c <file>`) that
//!    records the real microphone, runs it through the stages of
//!    [`super::chain`] and publishes the result as a virtual microphone,
//!    `conchy_mic_source`;
//! 2. makes that source the default, so every app that records follows it;
//! 3. keeps the settings live: a slider move is one `Props` update on the
//!    source's node. Only adding or removing a stage (or switching the noise
//!    remover) rebuilds the chain, which costs a blip of silence.
//!
//! The "real microphone" is the one picked in the settings, or else whatever
//! the system's default source was; when that changes the chain is rebuilt
//! for the new one. Turning the chain off stops it and puts the default
//! back. The child dies with the shell (`PDEATHSIG`), and WirePlumber falls
//! back to a real source when the virtual one vanishes.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::process::Child;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context as _, anyhow};
use futures_signals::signal::Mutable;
use pipewire::{
    context::ContextRc,
    main_loop::MainLoopRc,
    metadata::Metadata,
    node::Node,
    proxy::Listener,
    spa::{param::ParamType, pod::Pod},
    types::ObjectType,
};
use tracing::{debug, info, warn};

use super::chain::{self, Plugins, SOURCE_NAME, Stage};
use super::model::{self, Settings, Voice};
use super::{EngineState, InputDevice, MicCommand, MicData};
use crate::jobs::JobReceiver;
use crate::lifecycle::RunToken;
use crate::pwutil::{self, default_name, params_pod};

const KEY_CONFIGURED: &str = "default.configured.audio.source";
const KEY_ACTUAL: &str = "default.audio.source";

/// How long a chain gets to show its source before the attempt counts as
/// failed. DeepFilterNet loads a model, so this is generous.
const START_TIMEOUT: Duration = Duration::from_secs(20);
/// A slider drag is one write to disk, this long after its last move.
const SAVE_DELAY: Duration = Duration::from_millis(400);
const POLL: Duration = Duration::from_millis(500);

/// What the loop thread is told.
pub enum Wake {
    /// Commands are waiting in the job queue.
    Jobs,
    /// Time to look at the chain and the unsaved settings.
    Tick,
    /// The service stopped.
    Quit,
}

struct Source {
    name: String,
    description: String,
}

struct Chain {
    child: Child,
    input: String,
    /// What the graph was built of; a different list means a rebuild.
    stages: Vec<Stage>,
    started: Instant,
}

struct State {
    settings: Settings,
    engine: EngineState,
    plugins: Plugins,
    /// Every real source, by registry id.
    sources: BTreeMap<u32, Source>,
    mic_id: Option<u32>,
    mic_node: Option<Node>,
    /// The chain's controls were sent since the node appeared.
    params_sent: bool,
    metadata: Option<Metadata>,
    configured: Option<String>,
    actual: Option<String>,
    /// What the user last set as the default source, the chain aside; what
    /// turning it off puts back (`None`: the key was never set).
    user_configured: Option<String>,
    /// The source the system records from.
    system_input: Option<String>,
    chain: Option<Chain>,
    dirty_since: Option<Instant>,
    /// When the default was last pointed at the chain, so a slow metadata
    /// round trip is not answered with a second write.
    default_set_at: Option<Instant>,
    failure: Option<String>,
}

pub struct Engine {
    data: Mutable<MicData>,
    token: RunToken,
    wake: pipewire::channel::Sender<Wake>,
    state: RefCell<State>,
    tick_pending: Cell<bool>,
    /// Proxies and their listeners, alive as long as the engine.
    keep: RefCell<Vec<Box<dyn Listener>>>,
}

impl Engine {
    pub fn new(
        data: Mutable<MicData>,
        token: RunToken,
        wake: pipewire::channel::Sender<Wake>,
        settings: Settings,
    ) -> Rc<Self> {
        Rc::new(Self {
            data,
            token,
            wake,
            state: RefCell::new(State {
                settings,
                engine: EngineState::Off,
                plugins: Plugins::detect(),
                sources: BTreeMap::new(),
                mic_id: None,
                mic_node: None,
                params_sent: false,
                metadata: None,
                configured: None,
                actual: None,
                user_configured: None,
                system_input: None,
                chain: None,
                dirty_since: None,
                default_set_at: None,
                failure: None,
            }),
            tick_pending: Cell::new(false),
            keep: RefCell::new(Vec::new()),
        })
    }

    /// The error that ended the run, if one did.
    pub fn take_failure(&self) -> Option<String> {
        self.state.borrow_mut().failure.take()
    }

    // -- events ---------------------------------------------------------------

    pub fn on_global(
        self: &Rc<Self>,
        registry: &pipewire::registry::RegistryRc,
        global: &pipewire::registry::GlobalObject<&pipewire::spa::utils::dict::DictRef>,
    ) {
        match global.type_ {
            ObjectType::Node => {
                let Some(props) = global.props else { return };
                if props.get("media.class") != Some("Audio/Source") {
                    return;
                }
                let name = props.get("node.name").unwrap_or_default().to_string();
                if name == SOURCE_NAME {
                    match registry.bind::<Node, _>(global) {
                        Ok(node) => {
                            debug!("Microphone source is up (node {})", global.id);
                            let mut state = self.state.borrow_mut();
                            state.mic_id = Some(global.id);
                            state.mic_node = Some(node);
                            state.params_sent = false;
                        }
                        Err(err) => warn!("Binding the microphone source: {err}"),
                    }
                } else {
                    let description = props
                        .get("node.description")
                        .or_else(|| props.get("node.nick"))
                        .unwrap_or(&name)
                        .to_string();
                    self.state
                        .borrow_mut()
                        .sources
                        .insert(global.id, Source { name, description });
                }
            }
            ObjectType::Metadata => {
                let Some(props) = global.props else { return };
                if props.get("metadata.name") != Some("default") {
                    return;
                }
                match registry.bind::<Metadata, _>(global) {
                    Ok(metadata) => {
                        let engine = Rc::downgrade(self);
                        let listener = metadata
                            .add_listener_local()
                            .property(move |subject, key, _type, value| {
                                if subject == 0
                                    && let Some(engine) = engine.upgrade()
                                {
                                    engine.on_metadata(key, value);
                                }
                                0
                            })
                            .register();
                        self.state.borrow_mut().metadata = Some(metadata);
                        self.keep.borrow_mut().push(Box::new(listener));
                    }
                    Err(err) => warn!("Binding the default metadata: {err}"),
                }
            }
            _ => return,
        }
        self.reconcile();
    }

    pub fn on_global_remove(&self, id: u32) {
        {
            let mut state = self.state.borrow_mut();
            state.sources.remove(&id);
            if state.mic_id == Some(id) {
                state.mic_id = None;
                state.mic_node = None;
                state.params_sent = false;
            }
        }
        self.reconcile();
    }

    fn on_metadata(&self, key: Option<&str>, value: Option<&str>) {
        {
            let mut state = self.state.borrow_mut();
            match key {
                Some(KEY_CONFIGURED) => {
                    let name = default_name(value);
                    if name.as_deref() != Some(SOURCE_NAME) {
                        state.user_configured.clone_from(&name);
                    }
                    state.configured = name;
                }
                Some(KEY_ACTUAL) => state.actual = default_name(value),
                _ => return,
            }
        }
        self.reconcile();
    }

    pub fn on_wake(&self, jobs: &Mutex<JobReceiver<MicCommand>>) {
        jobs.lock().unwrap_or_else(PoisonError::into_inner).drain(|command| {
            self.apply(command);
            Ok(())
        });
    }

    pub fn on_tick(&self) {
        self.tick_pending.set(false);

        let flush = {
            let state = self.state.borrow();
            state
                .dirty_since
                .is_some_and(|since| since.elapsed() >= SAVE_DELAY)
        };
        if flush {
            self.save();
        }

        let problem = {
            let mut state = self.state.borrow_mut();
            let starting = state.mic_node.is_none();
            state.chain.as_mut().and_then(|chain| match chain.child.try_wait() {
                Ok(Some(status)) => Some(format!(
                    "the filter-chain exited ({status}; see $XDG_RUNTIME_DIR/conchy/mic-chain.log)"
                )),
                Ok(None) if starting && chain.started.elapsed() > START_TIMEOUT => Some(
                    "the filter-chain did not come up (see $XDG_RUNTIME_DIR/conchy/mic-chain.log)"
                        .to_string(),
                ),
                Ok(None) => None,
                Err(err) => Some(format!("watching the filter-chain: {err}")),
            })
        };
        if let Some(problem) = problem {
            self.fail(problem);
            return;
        }
        self.schedule_tick();
    }

    // -- commands -------------------------------------------------------------

    fn apply(&self, command: MicCommand) {
        use MicCommand as C;
        let mut changed_voice = false;
        {
            let mut guard = self.state.borrow_mut();
            let state = &mut *guard;
            let settings = &mut state.settings;
            match command {
                C::SetEnabled(on) => settings.enabled = on,
                C::Set(param, value) => {
                    settings.voice.set(param, value);
                    changed_voice = true;
                }
                C::ApplyPreset(name) => {
                    if settings.apply_preset(&name) {
                        changed_voice = true;
                    } else {
                        warn!("No microphone preset called {name:?}");
                    }
                }
                C::Reset => {
                    settings.voice = Voice::default();
                    changed_voice = true;
                }
                C::SavePreset(name) => {
                    if let Err(err) = settings.save_preset(&name) {
                        warn!("Saving the preset: {err:#}");
                    }
                }
                C::DeletePreset(name) => {
                    settings.delete_preset(&name);
                }
                C::SetInput(name) => {
                    settings.input = Some(name.clone());
                    // With the chain off, picking a microphone is picking
                    // the system's own.
                    if !settings.enabled
                        && let Some(metadata) = &state.metadata
                    {
                        pwutil::set_default(metadata, KEY_CONFIGURED, &name);
                    }
                }
                C::Rescan => state.plugins = Plugins::detect(),
            }
            state.dirty_since = Some(Instant::now());
        }
        if changed_voice {
            self.push_params();
        }
        self.reconcile();
        self.schedule_tick();
    }

    // -- the chain ------------------------------------------------------------

    /// Bring the chain, the default source and the published state in line
    /// with the settings.
    fn reconcile(&self) {
        self.refresh_system_input();

        let (enabled, input, wanted_stages, running) = {
            let state = self.state.borrow();
            let known = |name: &String| state.sources.values().any(|source| &source.name == name);
            let input = state
                .settings
                .input
                .clone()
                .filter(known)
                .or_else(|| state.system_input.clone().filter(known));
            (
                state.settings.enabled,
                input,
                chain::stages(&state.settings.voice, &state.plugins),
                state
                    .chain
                    .as_ref()
                    .map(|chain| (chain.input.clone(), chain.stages.clone())),
            )
        };

        if !enabled {
            if running.is_some() {
                self.stop_chain();
            }
            self.set_engine(EngineState::Off);
            return;
        }

        let Some(input) = input else {
            if running.is_some() {
                self.stop_chain();
            }
            self.set_engine(EngineState::Starting);
            return;
        };

        if let Some((current, stages)) = &running
            && (*current != input || *stages != wanted_stages)
        {
            info!("Microphone chain changed ({input}), rebuilding");
            self.stop_chain();
        }
        if self.state.borrow().chain.is_none()
            && let Err(err) = self.start_chain(&input, wanted_stages)
        {
            self.fail(format!("{err:#}"));
            return;
        }

        let ready = self.state.borrow().mic_node.is_some();
        if ready {
            if !self.state.borrow().params_sent {
                self.push_params();
            }
            self.point_default_at_the_chain();
            self.set_engine(EngineState::Running);
        } else {
            self.set_engine(EngineState::Starting);
        }
    }

    /// The real source the voice should come from: what the system names as
    /// its default, unless that is the chain itself. Once known it stays
    /// until the user picks another: when the chain is rebuilt the daemon's
    /// own fallback (`default.audio.source`) can name a different device
    /// for a moment, and that is not a pick.
    fn refresh_system_input(&self) {
        let mut state = self.state.borrow_mut();
        let configured = state
            .configured
            .clone()
            .filter(|name| name != SOURCE_NAME);
        let pick = configured.or_else(|| {
            let kept = state.system_input.clone();
            kept.or_else(|| state.actual.clone().filter(|name| name != SOURCE_NAME))
                .filter(|name| name != SOURCE_NAME)
        });
        if let Some(pick) = pick {
            state.system_input = Some(pick);
        }
    }

    fn start_chain(&self, input: &str, stages: Vec<Stage>) -> anyhow::Result<()> {
        let conf = chain::conf_path();
        let log = chain::log_path();
        if let Some(dir) = conf.parent() {
            std::fs::create_dir_all(dir).context("creating the runtime directory")?;
        }
        {
            let state = self.state.borrow();
            std::fs::write(&conf, chain::chain_config(input, &state.settings.voice, &state.plugins))
                .context("writing the filter-chain config")?;
        }
        let child = pwutil::spawn_chain(&conf, &log)?;
        info!("Microphone chain started for {input} (pid {})", child.id());

        let mut state = self.state.borrow_mut();
        state.chain = Some(Chain {
            child,
            input: input.to_string(),
            stages,
            started: Instant::now(),
        });
        state.mic_id = None;
        state.mic_node = None;
        state.params_sent = false;
        drop(state);
        self.schedule_tick();
        Ok(())
    }

    /// Stop the chain. When the default still points at it, give it back to
    /// the real microphone first.
    fn stop_chain(&self) {
        let mut state = self.state.borrow_mut();
        if state.configured.as_deref() == Some(SOURCE_NAME)
            && let Some(metadata) = &state.metadata
        {
            match &state.user_configured {
                Some(name) => pwutil::set_default(metadata, KEY_CONFIGURED, name),
                None => metadata.set_property(0, KEY_CONFIGURED, None, None),
            }
        }
        if let Some(mut chain) = state.chain.take() {
            let _ = chain.child.kill();
            let _ = chain.child.wait();
            info!("Microphone chain stopped");
        }
        state.mic_node = None;
        state.mic_id = None;
        state.params_sent = false;
        state.default_set_at = None;
    }

    /// Whatever records follows the default source; make that the chain.
    fn point_default_at_the_chain(&self) {
        let mut state = self.state.borrow_mut();
        if state.configured.as_deref() == Some(SOURCE_NAME) {
            return;
        }
        if state
            .default_set_at
            .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        if let Some(metadata) = &state.metadata {
            pwutil::set_default(metadata, KEY_CONFIGURED, SOURCE_NAME);
            state.default_set_at = Some(Instant::now());
        }
    }

    fn push_params(&self) {
        let mut state = self.state.borrow_mut();
        let Some(node) = &state.mic_node else { return };
        let Some(bytes) = params_pod(&chain::controls(&state.settings.voice, &state.plugins)) else {
            warn!("Could not build the microphone controls");
            return;
        };
        let Some(pod) = Pod::from_bytes(&bytes) else {
            return;
        };
        node.set_param(ParamType::Props, 0, pod);
        state.params_sent = true;
        drop(state);
        self.publish();
    }

    // -- bookkeeping ----------------------------------------------------------

    fn set_engine(&self, engine: EngineState) {
        self.state.borrow_mut().engine = engine;
        self.publish();
    }

    fn fail(&self, message: String) {
        warn!("Microphone: {message}");
        {
            let mut state = self.state.borrow_mut();
            state.engine = EngineState::Failed(message.clone());
            state.failure = Some(message);
        }
        self.publish();
        let _ = self.wake.send(Wake::Quit);
    }

    fn publish(&self) {
        let snapshot = {
            let state = self.state.borrow();
            let mut inputs: Vec<InputDevice> = state
                .sources
                .values()
                .map(|source| InputDevice {
                    name: source.name.clone(),
                    description: source.description.clone(),
                })
                .collect();
            inputs.sort_by(|a, b| a.description.cmp(&b.description));
            let known = |name: &String| inputs.iter().any(|input| &input.name == name);
            let input = state
                .settings
                .input
                .clone()
                .filter(known)
                .or_else(|| state.system_input.clone());
            MicData {
                settings: state.settings.clone(),
                engine: state.engine.clone(),
                inputs,
                input,
                plugins: state.plugins,
            }
        };
        self.token.write(&self.data, |data| {
            if *data != snapshot {
                *data = snapshot;
            }
        });
    }

    fn save(&self) {
        let settings = {
            let mut state = self.state.borrow_mut();
            state.dirty_since = None;
            state.settings.clone()
        };
        if let Err(err) = model::save(&settings) {
            warn!("Saving the microphone settings: {err:#}");
        }
    }

    fn schedule_tick(&self) {
        let needed = {
            let state = self.state.borrow();
            state.chain.is_some() || state.dirty_since.is_some()
        };
        if !needed || self.tick_pending.replace(true) {
            return;
        }
        let wake = self.wake.clone();
        tokio::spawn(async move {
            tokio::time::sleep(POLL).await;
            let _ = wake.send(Wake::Tick);
        });
    }

    /// The run is over: write what is unsaved and stop the chain. Nothing is
    /// sent to the daemon any more, so the default is left to WirePlumber's
    /// fallback.
    pub fn shutdown(&self) {
        if self.state.borrow().dirty_since.is_some() {
            self.save();
        }
        let mut state = self.state.borrow_mut();
        if let Some(mut chain) = state.chain.take() {
            let _ = chain.child.kill();
            let _ = chain.child.wait();
        }
    }
}

/// One PipeWire session: connect, follow the graph, serve commands until
/// the token is superseded or the chain fails.
pub fn run(
    data: &Mutable<MicData>,
    token: &RunToken,
    jobs: &Arc<Mutex<JobReceiver<MicCommand>>>,
    sender_slot: &std::sync::Mutex<Option<pipewire::channel::Sender<Wake>>>,
) -> anyhow::Result<()> {
    let settings = match model::load() {
        Ok(settings) => settings,
        Err(err) => {
            warn!("Microphone settings are unreadable, starting from the defaults: {err:#}");
            if let Ok(path) = model::path() {
                let _ = std::fs::copy(&path, path.with_extension("kdl.bak"));
            }
            Settings::default()
        }
    };

    let mainloop = MainLoopRc::new(None)?;
    let (wake_tx, wake_rx) = pipewire::channel::channel::<Wake>();
    *sender_slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(wake_tx.clone());

    let engine = Engine::new(data.clone(), token.clone(), wake_tx.clone(), settings);
    engine.publish();

    // Dropped when this attempt returns, which retires the canceller.
    let (_attempt, attempt_done) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn({
        let token = token.clone();
        let wake = wake_tx.clone();
        async move {
            tokio::select! {
                _ = token.cancelled() => { let _ = wake.send(Wake::Quit); }
                _ = attempt_done => {}
            }
        }
    });

    let jobs = jobs.clone();
    let _wake = wake_rx.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        let engine = engine.clone();
        let jobs = jobs.clone();
        move |wake| match wake {
            Wake::Jobs => engine.on_wake(&jobs),
            Wake::Tick => engine.on_tick(),
            Wake::Quit => mainloop.quit(),
        }
    });

    let context = ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let core_failure = Rc::new(RefCell::new(None::<String>));
    let _core_listener = core
        .add_listener_local()
        .error({
            let failure = core_failure.clone();
            let mainloop = mainloop.clone();
            move |id, _seq, res, message| {
                if id == pipewire::core::PW_ID_CORE {
                    *failure.borrow_mut() = Some(format!("{message} ({res})"));
                    mainloop.quit();
                }
            }
        })
        .register();

    let registry = core.get_registry_rc()?;
    let registry_weak = registry.downgrade();
    let _registry_listener = registry
        .add_listener_local()
        .global({
            let engine = engine.clone();
            move |global| {
                if let Some(registry) = registry_weak.upgrade() {
                    engine.on_global(&registry, global);
                }
            }
        })
        .global_remove({
            let engine = engine.clone();
            move |id| engine.on_global_remove(id)
        })
        .register();

    // Commands queued before this attempt had a channel to wake.
    let _ = wake_tx.send(Wake::Jobs);
    token.active();
    mainloop.run();

    engine.shutdown();
    *sender_slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;

    if let Some(failure) = engine.take_failure() {
        return Err(anyhow!(failure));
    }
    if let Some(failure) = core_failure.borrow_mut().take() {
        return Err(anyhow!("PipeWire connection lost: {failure}"));
    }
    Ok(())
}
