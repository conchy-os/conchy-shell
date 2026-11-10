use gpui::{App, Bounds, DisplayId, Size, point, px};

use super::DOCK_HIDDEN_STRIP_SIZE;
use super::config::DockGeometry;

pub(super) fn monitor_name_for_display(
    display_id: Option<DisplayId>,
    state: &services::CompositorState,
    cx: &App,
) -> Option<String> {
    crate::state::monitor_for_display(display_id, state, cx).map(|monitor| monitor.name.clone())
}

pub(super) fn focused_window_is_on_other_monitor(
    focused_window: Option<&services::Window>,
    monitor_name: Option<&str>,
) -> bool {
    focused_window.is_some_and(|window| Some(window.monitor.as_str()) != monitor_name)
}

pub(super) fn geometry_overlaps_dock(
    geometry: services::WindowGeometry,
    dock_bounds: Bounds<gpui::Pixels>,
) -> bool {
    let dock_left: f32 = dock_bounds.origin.x.into();
    let dock_top: f32 = dock_bounds.origin.y.into();
    let dock_right = dock_left + f32::from(dock_bounds.size.width);
    let dock_bottom = dock_top + f32::from(dock_bounds.size.height);

    let window_left = geometry.x as f32;
    let window_top = geometry.y as f32;
    let window_right = window_left + geometry.width as f32;
    let window_bottom = window_top + geometry.height as f32;

    window_left < dock_right
        && window_right > dock_left
        && window_top < dock_bottom
        && window_bottom > dock_top
}

/// Where the dock sits, in the logical layout space window geometry uses.
/// Monitors report physical pixels, so their size is scaled down.
pub(super) fn dock_bounds_in_compositor_space(
    geometry: DockGeometry,
    dock_size: Size<gpui::Pixels>,
    monitor: &services::Monitor,
) -> Bounds<gpui::Pixels> {
    let scale = if monitor.scale > 0.0 {
        monitor.scale
    } else {
        1.0
    };
    let monitor_width = monitor.width as f32 / scale;
    let monitor_height = monitor.height as f32 / scale;
    let dock_width: f32 = dock_size.width.into();
    let dock_height: f32 = dock_size.height.into();
    let gap = geometry.gap;
    let (x, y) = match geometry.position {
        crate::bar::config::BarPosition::Top => (
            monitor.x as f32 + geometry.along(monitor_width, dock_width),
            monitor.y as f32 + gap,
        ),
        crate::bar::config::BarPosition::Bottom => (
            monitor.x as f32 + geometry.along(monitor_width, dock_width),
            monitor.y as f32 + monitor_height - dock_height - gap,
        ),
        crate::bar::config::BarPosition::Left => (
            monitor.x as f32 + gap,
            monitor.y as f32 + geometry.along(monitor_height, dock_height),
        ),
        crate::bar::config::BarPosition::Right => (
            monitor.x as f32 + monitor_width - dock_width - gap,
            monitor.y as f32 + geometry.along(monitor_height, dock_height),
        ),
    };

    Bounds::new(point(px(x), px(y)), dock_size)
}

pub(super) fn hidden_strip_size(
    position: crate::bar::config::BarPosition,
    dock_size: Size<gpui::Pixels>,
) -> Size<gpui::Pixels> {
    if position.is_vertical() {
        Size::new(px(DOCK_HIDDEN_STRIP_SIZE), dock_size.height)
    } else {
        Size::new(dock_size.width, px(DOCK_HIDDEN_STRIP_SIZE))
    }
}

pub(super) fn revealed_after_visibility_update(
    was_hidden_by_rule: bool,
    was_revealed: bool,
    is_hidden_by_rule: bool,
) -> bool {
    if !is_hidden_by_rule {
        true
    } else if !was_hidden_by_rule {
        false
    } else {
        was_revealed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DockHideBehavior {
    Visible,
    Delayed,
    Immediate,
}

impl DockHideBehavior {
    pub(super) fn hides(self) -> bool {
        !matches!(self, Self::Visible)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InitialHideAction {
    Reveal,
    Keep,
    HideNow,
    ScheduleDelayedHide,
}

pub(super) fn initial_hide_action(
    was_hidden_by_rule: bool,
    hide_behavior: DockHideBehavior,
) -> InitialHideAction {
    match (was_hidden_by_rule, hide_behavior) {
        (_, DockHideBehavior::Visible) => InitialHideAction::Reveal,
        (false, DockHideBehavior::Delayed) => InitialHideAction::ScheduleDelayedHide,
        (false, DockHideBehavior::Immediate) => InitialHideAction::HideNow,
        (true, _) => InitialHideAction::Keep,
    }
}

/// Hide while any window on `monitor`'s current workspace covers the dock.
/// A window the compositor cannot place (Niri) counts as covering it: its
/// column spans the full height, so it reaches the dock's edge.
pub(super) fn dodge_windows_hide_behavior(
    windows: &[services::Window],
    monitor: Option<&services::Monitor>,
    geometry: DockGeometry,
    dock_size: Size<gpui::Pixels>,
) -> DockHideBehavior {
    let Some(monitor) = monitor else {
        return DockHideBehavior::Visible;
    };
    let dock_bounds = dock_bounds_in_compositor_space(geometry, dock_size, monitor);
    let covered = windows
        .iter()
        .filter(|window| {
            window.monitor == monitor.name
                && window.workspace_id == monitor.active_workspace_id
                && !window.is_minimized
        })
        .any(|window| {
            window
                .geometry
                .is_none_or(|geometry| geometry_overlaps_dock(geometry, dock_bounds))
        });

    if covered {
        DockHideBehavior::Immediate
    } else {
        DockHideBehavior::Visible
    }
}
