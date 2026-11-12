use gpui::{
    App, Bounds, DisplayId, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    layer_shell::*, point, prelude::*, px,
};

use futures::StreamExt;
use futures_signals::signal::SignalExt;

use crate::config::{ActiveConfig, ConfigSignals, State};
use crate::state::{AppState, record_window_display};

use super::{
    Dock, DockMonitors, dock_extent, dock_item_count, dock_thickness, monitor_name_for_display,
};

fn window_options(display_id: Option<DisplayId>, cx: &App) -> WindowOptions {
    let config = &cx.config().dock;
    let compositor_state = AppState::compositor(cx).get();
    let item_count = dock_item_count(
        &compositor_state.windows,
        AppState::applications(cx).all(),
        State::pinned(cx),
        monitor_name_for_display(display_id, &compositor_state, cx).as_deref(),
    );
    let window_size = dock_extent(
        config,
        item_count,
        crate::state::display_extent(display_id, cx),
    );

    // A stretched dock is anchored at both ends too, like the bar; a fitted
    // one at the end it is aligned to, the margin keeping it off that end.
    let (edge, start, end) = match config.position {
        crate::bar::config::BarPosition::Left => (Anchor::LEFT, Anchor::TOP, Anchor::BOTTOM),
        crate::bar::config::BarPosition::Right => (Anchor::RIGHT, Anchor::TOP, Anchor::BOTTOM),
        crate::bar::config::BarPosition::Top => (Anchor::TOP, Anchor::LEFT, Anchor::RIGHT),
        crate::bar::config::BarPosition::Bottom => (Anchor::BOTTOM, Anchor::LEFT, Anchor::RIGHT),
    };
    let anchor = if config.stretch {
        edge | start | end
    } else {
        match config.align {
            super::config::DockAlign::Start => edge | start,
            super::config::DockAlign::Center => edge,
            super::config::DockAlign::End => edge | end,
        }
    };
    let gap = px(config.edge_gap());

    WindowOptions {
        display_id,
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.), px(0.)),
            size: window_size,
        })),
        app_id: Some("conchy-shell-dock".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "dock".to_string(),
            layer: Layer::Top,
            anchor,
            // Compositors add the margin on the anchored edge to this zone,
            // so it is the thickness alone, as for the bar.
            exclusive_zone: config
                .reserve_space
                .then(|| dock_thickness(config, window_size)),
            margin: (gap > px(0.)).then_some((gap, gap, gap, gap)),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn open_with_config(display_id: Option<DisplayId>, cx: &mut App) -> bool {
    match cx.open_window(window_options(display_id, cx), move |window, cx| {
        record_window_display(window.window_handle(), display_id, cx);
        cx.new(|cx| Dock::new(display_id, cx))
    }) {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!("Failed to open dock window: {err}");
            false
        }
    }
}

fn open_all(cx: &mut App) -> usize {
    if !cx.config().dock.enabled {
        return 0;
    }
    let mut opened = 0;
    let monitors = cx.config().dock.monitors;
    let displays = cx.displays();

    match monitors {
        DockMonitors::PrimaryOnly => {
            let target_display = displays
                .iter()
                .find(|d| d.bounds().size.width > d.bounds().size.height)
                .cloned()
                .or_else(|| cx.primary_display());
            if open_with_config(target_display.map(|display| display.id()), cx) {
                opened += 1;
            }
        }
        DockMonitors::All if displays.is_empty() => {
            if open_with_config(None, cx) {
                opened += 1;
            }
        }
        DockMonitors::All => {
            for display in displays {
                if open_with_config(Some(display.id()), cx) {
                    opened += 1;
                }
            }
        }
    }

    opened
}

pub fn reload(cx: &mut App) {
    let old_windows: Vec<_> = cx
        .windows()
        .into_iter()
        .filter(|handle| handle.downcast::<Dock>().is_some())
        .collect();

    if open_all(cx) == 0 {
        tracing::warn!("Dock reload skipped closing old windows because no new dock window opened");
        return;
    }

    for handle in old_windows {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

pub fn init(cx: &mut App) {
    // Only the layer-shell anchor, margins, exclusive zone and the monitor
    // set need new windows; size, pins and the rest are applied by
    // `Dock::render`. The zone is the dock's thickness, so while it is
    // reserved the keys that thickness derives from reopen it too.
    let layout = ConfigSignals::global(cx)
        .dock
        .signal_ref(|dock| {
            let thickness = dock
                .reserve_space
                .then(|| (dock.icon_size.to_bits(), dock.hover_effect));
            (
                dock.enabled,
                dock.position,
                dock.monitors,
                dock.style,
                dock.stretch,
                dock.align,
                dock.reserve_space,
                thickness,
            )
        })
        .dedupe()
        .to_stream();

    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(std::time::Duration::from_millis(100))
            .await;

        let mut layout = layout.skip(1);
        tracing::info!("Dock opened");
        // The bar may have opened the dock already, to put it after itself.
        cx.update(reload);
        while layout.next().await.is_some() {
            tracing::info!("Dock layout changed; reopening dock windows");
            cx.update(reload);
        }
    })
    .detach();
}
