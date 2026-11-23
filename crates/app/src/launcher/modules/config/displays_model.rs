//! The displays as the compositor reports them in full (with their modes),
//! changes to them, and the profiles saved from them.
//!
//! A change of mode, scale, rotation, position or power can leave a screen
//! that cannot be read, so it comes with a countdown: unless the user keeps
//! the new setup, the old one is put back.

use std::time::Duration;

use gpui::{Context, prelude::*};
use services::display_profiles::{Profile, ProfileStore};
use services::compositor::outputs::query_monitors;
use services::{CompositorBackend, CompositorCommand, CompositorSubscriber, Monitor, OutputConfig};

use crate::state::{AppState, watch};

/// Seconds the user has to keep a new setup.
pub const REVERT_SECONDS: u32 = 15;

pub enum State {
    Loading,
    Ready(Vec<Monitor>),
    Failed(String),
}

pub struct Revert {
    pub previous: Vec<OutputConfig>,
    pub seconds_left: u32,
}

pub struct Notice {
    pub text: String,
    pub error: bool,
}

pub struct DisplayModel {
    backend: CompositorBackend,
    compositor: CompositorSubscriber,
    pub state: State,
    pub profiles: Vec<Profile>,
    store: Option<ProfileStore>,
    pub notice: Option<Notice>,
    pub revert: Option<Revert>,
    /// Bumped whenever a countdown ends, so an old one stops ticking.
    countdown: u64,
    /// Monitor names last seen, to reload only when one comes or goes.
    seen: Vec<String>,
}

fn names(monitors: &[services::Monitor]) -> Vec<String> {
    let mut names: Vec<String> = monitors.iter().map(|m| m.name.clone()).collect();
    names.sort();
    names
}

impl DisplayModel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let compositor = AppState::compositor(cx).clone();
        let backend = compositor.backend();
        let store = ProfileStore::standard().ok();

        // A monitor plugged in or out changes the list of names.
        watch(cx, compositor.subscribe(), |this, state, cx| {
            let now = names(&state.monitors);
            if now != this.seen {
                this.seen = now;
                this.reload(cx);
            }
        });

        let mut model = Self {
            backend,
            compositor,
            state: State::Loading,
            profiles: Vec::new(),
            store,
            notice: None,
            revert: None,
            countdown: 0,
            seen: Vec::new(),
        };
        model.reload_profiles();
        model.reload(cx);
        model
    }

    pub fn monitors(&self) -> &[Monitor] {
        match &self.state {
            State::Ready(monitors) => monitors,
            _ => &[],
        }
    }

    pub fn monitor(&self, name: &str) -> Option<&Monitor> {
        self.monitors().iter().find(|m| m.name == name)
    }

    pub fn supported(&self) -> bool {
        self.backend != CompositorBackend::Mango
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend;
        let task = cx.background_spawn(async move {
            query_monitors(backend).map_err(|err| format!("{err:#}"))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.state = match result {
                    Ok(monitors) => State::Ready(monitors),
                    Err(err) => State::Failed(err),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// After a change: compositors need a moment before they report it.
    fn reload_soon(&mut self, cx: &mut Context<Self>) {
        for delay in [600u64, 1800] {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(delay))
                    .await;
                this.update(cx, |this, cx| this.reload(cx)).ok();
            })
            .detach();
        }
    }

    pub fn reload_profiles(&mut self) {
        if let Some(store) = &self.store {
            match store.load() {
                Ok(profiles) => self.profiles = profiles,
                Err(err) => self.say(format!("Could not read the profiles: {err:#}"), true),
            }
        }
    }

    fn say(&mut self, text: impl Into<String>, error: bool) {
        self.notice = Some(Notice {
            text: text.into(),
            error,
        });
    }

    // -- applying --------------------------------------------------------------

    /// Apply `configs`. With `confirm` the old setup comes back unless the user
    /// keeps the new one in time.
    pub fn apply(&mut self, configs: Vec<OutputConfig>, confirm: bool, cx: &mut Context<Self>) {
        let monitors = self.monitors().to_vec();
        if configs.is_empty() {
            return;
        }
        // A setup with every screen off cannot be undone from the screen.
        let on_after = monitors
            .iter()
            .filter(|m| match configs.iter().find(|c| c.output == m.name) {
                Some(c) => c.enabled,
                None => !m.is_disabled,
            })
            .count();
        if on_after == 0 {
            self.say("At least one display has to stay on.", true);
            cx.notify();
            return;
        }

        let previous: Vec<OutputConfig> = configs
            .iter()
            .filter_map(|c| monitors.iter().find(|m| m.name == c.output))
            .map(OutputConfig::from_monitor)
            .collect();
        self.notice = None;

        // One at a time, in order: the compositor queues them the same way.
        let requests: Vec<_> = configs
            .into_iter()
            .map(|c| self.compositor.request(CompositorCommand::ApplyOutput(c)))
            .collect();
        cx.spawn(async move |this, cx| {
            let mut failure = None;
            for request in requests {
                if let Err(err) = request.await {
                    failure = Some(format!("{err:#}"));
                    break;
                }
            }
            this.update(cx, |this, cx| {
                if let Some(err) = failure {
                    tracing::warn!("Failed to apply the display setup: {err}");
                    this.say(format!("Could not apply: {err}"), true);
                    this.revert = None;
                }
                this.reload_soon(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();

        if confirm && !previous.is_empty() {
            self.start_countdown(previous, cx);
        }
        cx.notify();
    }

    /// Change one field of one display.
    pub fn change_monitor(
        &mut self,
        name: &str,
        confirm: bool,
        edit: impl FnOnce(&mut OutputConfig, &Monitor),
        cx: &mut Context<Self>,
    ) {
        let Some(monitor) = self.monitor(name).cloned() else {
            return;
        };
        let mut config = OutputConfig::from_monitor(&monitor);
        edit(&mut config, &monitor);
        if config == OutputConfig::from_monitor(&monitor) {
            return;
        }
        self.apply(vec![config], confirm, cx);
    }

    fn start_countdown(&mut self, previous: Vec<OutputConfig>, cx: &mut Context<Self>) {
        // Keep the oldest setup if a change comes while one is waiting.
        let previous = match self.revert.take() {
            Some(waiting) => waiting.previous,
            None => previous,
        };
        self.countdown += 1;
        let id = self.countdown;
        self.revert = Some(Revert {
            previous,
            seconds_left: REVERT_SECONDS,
        });
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let keep_going = this
                    .update(cx, |this, cx| {
                        if this.countdown != id {
                            return false;
                        }
                        let Some(revert) = &mut this.revert else {
                            return false;
                        };
                        revert.seconds_left = revert.seconds_left.saturating_sub(1);
                        if revert.seconds_left == 0 {
                            this.revert_now(cx);
                            return false;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        })
        .detach();
    }

    /// Keep what is on the screen.
    pub fn keep(&mut self, cx: &mut Context<Self>) {
        self.countdown += 1;
        self.revert = None;
        cx.notify();
    }

    /// Put the old setup back.
    pub fn revert_now(&mut self, cx: &mut Context<Self>) {
        self.countdown += 1;
        if let Some(revert) = self.revert.take() {
            self.say("Went back to the previous display setup.", false);
            self.apply(revert.previous, false, cx);
        }
        cx.notify();
    }

    // -- profiles --------------------------------------------------------------

    pub fn save_profile(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(store) = &self.store else { return };
        if self.monitors().is_empty() {
            return;
        }
        let profile = Profile::snapshot(name, true, self.monitors());
        match store.put(profile) {
            Ok(()) => self.say(format!("Saved the profile “{name}”."), false),
            Err(err) => self.say(format!("Could not save the profile: {err:#}"), true),
        }
        self.reload_profiles();
        cx.notify();
    }

    pub fn apply_profile(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(profile) = self.profiles.iter().find(|p| p.name == name).cloned() else {
            return;
        };
        match profile.plan(self.monitors()) {
            Some(plan) => self.apply(plan, true, cx),
            None => {
                self.say(
                    format!("“{name}” was saved with other displays than the ones connected."),
                    true,
                );
                cx.notify();
            }
        }
    }

    pub fn delete_profile(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(store) = &self.store
            && let Err(err) = store.delete(name)
        {
            self.say(format!("Could not delete the profile: {err:#}"), true);
        }
        self.reload_profiles();
        cx.notify();
    }

    pub fn set_profile_auto(&mut self, name: &str, auto: bool, cx: &mut Context<Self>) {
        if let Some(store) = &self.store
            && let Err(err) = store.set_auto(name, auto)
        {
            self.say(format!("Could not change the profile: {err:#}"), true);
        }
        self.reload_profiles();
        cx.notify();
    }
}
