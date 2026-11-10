//! Config sections as signals, so a window reacts to its own section only.

use futures_signals::signal::Mutable;
use gpui::{App, Global};

use super::{ActiveConfig, BarConfig, DockConfig, FrameConfig, PanelsConfig, State};

/// Effective config values, republished with `set_neq` after every reload or
/// state write, so an edit to one section wakes only that section's watchers.
pub struct ConfigSignals {
    pub bar: Mutable<BarConfig>,
    pub dock: Mutable<DockConfig>,
    pub frame: Mutable<FrameConfig>,
    pub panels: Mutable<PanelsConfig>,
    /// [`State::pinned`]: the UI's list, else `dock.pinned`.
    pub pinned: Mutable<Vec<String>>,
    /// Bumped on every publish, for pages that show the whole config.
    pub revision: Mutable<u64>,
}

impl Global for ConfigSignals {}

impl ConfigSignals {
    pub(super) fn init(cx: &mut App) {
        let signals = Self {
            bar: Mutable::new(cx.config().bar.clone()),
            dock: Mutable::new(cx.config().dock.clone()),
            frame: Mutable::new(cx.config().frame.clone()),
            panels: Mutable::new(cx.config().panels.clone()),
            pinned: Mutable::new(State::pinned(cx).to_vec()),
            revision: Mutable::new(0),
        };
        cx.set_global(signals);
    }

    #[inline(always)]
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub(super) fn publish(cx: &App) {
        let signals = Self::global(cx);
        signals.bar.set_neq(cx.config().bar.clone());
        signals.dock.set_neq(cx.config().dock.clone());
        signals.frame.set_neq(cx.config().frame.clone());
        signals.panels.set_neq(cx.config().panels.clone());
        signals.pinned.set_neq(State::pinned(cx).to_vec());
        signals.revision.set(signals.revision.get() + 1);
    }
}
