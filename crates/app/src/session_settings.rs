//! Settings the shell puts back by itself: input settings on a compositor that
//! forgets them, and the display profile that fits the monitors connected.

use std::time::{Duration, Instant};

use futures_signals::signal::SignalExt;
use futures_util::StreamExt;
use gpui::{App, AppContext};
use services::compositor::outputs::query_monitors;
use services::display_profiles::{ProfileStore, pending_auto};
use services::input::InputBackend;
use services::{CompositorBackend, CompositorCommand, OutputConfig};

use crate::state::AppState;

/// How long a compositor takes to settle after a monitor comes or goes.
const SETTLE: Duration = Duration::from_millis(900);
/// A profile that was just applied is not applied again right away: a setup
/// the compositor cannot reach exactly (a rounded scale) must not loop.
const COOLDOWN: Duration = Duration::from_secs(15);

pub fn init(cx: &mut App) {
    restore_input(cx);
    apply_profiles(cx);
}

fn restore_input(cx: &mut App) {
    let backend = InputBackend::from_compositor(AppState::compositor(cx).backend());
    cx.background_spawn(async move {
        match backend.restore() {
            Ok(0) => {}
            Ok(count) => tracing::info!("Restored {count} input settings"),
            Err(err) => tracing::warn!("Failed to restore the input settings: {err:#}"),
        }
    })
    .detach();
}

/// The profile that fits the monitors connected, if it is not applied yet.
fn pending(backend: CompositorBackend) -> Option<(String, Vec<OutputConfig>)> {
    let monitors = query_monitors(backend).ok()?;
    let profiles = ProfileStore::standard().ok()?.load().ok()?;
    let profile = pending_auto(&profiles, &monitors)?;
    Some((profile.name.clone(), profile.plan(&monitors)?))
}

fn apply_profiles(cx: &mut App) {
    let compositor = AppState::compositor(cx).clone();
    let backend = compositor.backend();
    if backend == CompositorBackend::Mango {
        return;
    }
    cx.spawn(async move |cx| {
        let mut stream = compositor.subscribe().to_stream();
        let mut seen: Option<Vec<String>> = None;
        let mut last: Option<(String, Instant)> = None;
        while let Some(state) = stream.next().await {
            let mut names: Vec<String> = state.monitors.iter().map(|m| m.name.clone()).collect();
            names.sort();
            // The state changes with every window; only monitors matter here.
            if seen.as_ref() == Some(&names) {
                continue;
            }
            seen = Some(names);

            cx.background_executor().timer(SETTLE).await;
            let found = cx
                .background_executor()
                .spawn(async move { pending(backend) })
                .await;
            let Some((name, plan)) = found else { continue };
            if last
                .as_ref()
                .is_some_and(|(n, at)| *n == name && at.elapsed() < COOLDOWN)
            {
                continue;
            }
            last = Some((name.clone(), Instant::now()));
            tracing::info!("Applying the display profile “{name}”");
            for config in plan {
                if let Err(err) = compositor
                    .request(CompositorCommand::ApplyOutput(config))
                    .await
                {
                    tracing::warn!("Failed to apply the display profile “{name}”: {err:#}");
                    break;
                }
            }
        }
    })
    .detach();
}
