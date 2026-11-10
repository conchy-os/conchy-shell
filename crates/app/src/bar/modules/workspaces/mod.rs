//! Workspaces widget for displaying and switching compositor workspaces.

mod config;
pub use config::WorkspacesConfig;

use gpui::{AnyElement, Context, Pixels, Window, prelude::*, px};
use services::{CompositorCommand, CompositorState};
use ui::{IconName, h_flex};

/// What one scroll-wheel line counts as, and the jitter a trackpad may
/// send without switching workspace. Input thresholds, not layout.
const SCROLL_LINE: Pixels = px(1.0);
const SCROLL_DEADZONE: Pixels = px(0.5);

use super::{BarCtx, BarWidget, BarWidgetDescriptor, BarWidgetMeta, BarWidgetShell, boxed, style};
use crate::config::ActiveConfig;
use crate::state::AppState;
use crate::state::watch;

pub const fn descriptor() -> BarWidgetDescriptor {
    BarWidgetDescriptor::new(
        BarWidgetMeta {
            name: "Workspaces",
            aliases: &[],
            label: "Workspaces",
            description: "Workspace switcher for the compositor.",
            icon: IconName::Layers,
        },
        |cx| boxed(cx, Workspaces::new),
    )
}

/// Workspaces widget that displays workspace indicators and allows switching.
pub struct Workspaces {
    compositor: services::CompositorSubscriber,
    state: CompositorState,
}

impl Workspaces {
    /// Create a new Workspaces widget.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let compositor = AppState::compositor(cx).clone();
        let state = compositor.get();

        watch(cx, compositor.subscribe(), |this, new_state, cx| {
            this.state = new_state;
            cx.notify();
        });

        Self { compositor, state }
    }

    /// Handle clicking on a workspace to focus it.
    fn focus_workspace(&self, workspace_id: i32) {
        self.compositor
            .dispatch(CompositorCommand::FocusWorkspace(workspace_id));
    }

    /// Resolve the compositor monitor name for the display this widget's
    /// window is on, so we only show that monitor's workspaces.
    ///
    /// Neither `Window::display()` nor display bounds/position nor ordinal
    /// list position are reliable here: gpui_linux derives `Window::display()`
    /// from `primary_output_scale()` (whichever output has the highest scale
    /// factor, unrelated to which output this layer-shell window is actually
    /// anchored to); every display's `bounds().origin` comes back as (0, 0)
    /// regardless of its real position; and `cx.displays()` isn't guaranteed
    /// to enumerate outputs in the same order as the compositor's own monitor
    /// list (confirmed to differ on at least one real setup, which silently
    /// swapped which bar controlled which monitor). Instead we match by
    /// identity: gpui_linux's `PlatformDisplay::uuid()` is a deterministic
    /// hash of the Wayland output's name (`Uuid::new_v5(NAMESPACE_DNS,
    /// name)`), so we look up the `DisplayId` this bar window was opened
    /// with (`crate::state::display_id_for_window`), and compare its uuid
    /// against the same hash computed from each compositor monitor's name.
    fn current_monitor_name(&self, window: &Window, cx: &gpui::App) -> Option<String> {
        let display_id = crate::state::display_id_for_window(window, cx)?;
        crate::state::monitor_for_display(Some(display_id), &self.state, cx).map(|m| m.name.clone())
    }

    /// Handle scrolling to switch workspaces.
    fn scroll_workspace(&self, direction: i32) {
        self.compositor
            .dispatch(CompositorCommand::ScrollWorkspace(direction));
    }

    fn workspace_label(ws: &services::Workspace, show_numbers: bool, show_icons: bool) -> String {
        if !show_numbers && !show_icons {
            return String::new();
        }

        let name = ws.name.trim();
        let is_numeric_name = !name.is_empty() && name.chars().all(|ch| ch.is_ascii_digit());

        if show_icons && !show_numbers {
            if name.is_empty() || is_numeric_name {
                return String::new();
            }
            return name.chars().take(3).collect::<String>().to_uppercase();
        }

        if show_numbers && !show_icons {
            return ws.id.to_string();
        }

        if name.is_empty() {
            return ws.id.to_string();
        }

        // The same label at both orientations: a vertical cell is as wide as
        // a horizontal one is tall, so three letters still fit.
        if is_numeric_name {
            name.to_string()
        } else {
            name.chars().take(3).collect::<String>().to_uppercase()
        }
    }

    fn render_cell(
        &self,
        ws: &services::Workspace,
        active_workspace_id: Option<i32>,
        ctx: &BarCtx,
        config: &WorkspacesConfig,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ink = ctx.ink;
        let cell = ctx.metrics.cell;
        let workspace_id = ws.id;
        let is_active = active_workspace_id == Some(ws.id);
        let has_windows = ws.windows > 0;
        let is_urgent = ws.has_urgent && !is_active;
        let label = Self::workspace_label(ws, config.show_numbers, config.show_icons);

        // Not a `ButtonLike`: the cell carries three background states
        // (active / has windows / empty) plus an urgent border, and
        // `ButtonStyle` has no per-state palette to express them.
        h_flex()
            .id(format!("workspace-{}", ws.id))
            .justify_center()
            // A cell is at least its square (a pill when active) and grows
            // with a named workspace, padded so the fill holds the text.
            .min_w(if is_active {
                cell.width_active
            } else {
                cell.width
            })
            .px(ctx.rhythm.unit)
            .h(cell.height)
            .rounded(cell.radius)
            .cursor_pointer()
            .bg(if is_active {
                ink.accent
            } else if has_windows {
                ink.occupied
            } else {
                gpui::transparent_black()
            })
            .when(is_urgent, |s| s.border_1().border_color(ink.status.error))
            .hover(move |s| {
                if is_active {
                    s.bg(ink.accent)
                } else {
                    s.bg(ink.hover)
                }
            })
            .on_click(cx.listener(move |this, _, _, _| {
                this.focus_workspace(workspace_id);
            }))
            .when(!label.is_empty(), |this| {
                let color = if is_active {
                    ink.on_accent
                } else if is_urgent {
                    ink.status.error
                } else if has_windows {
                    ink.text
                } else {
                    ink.muted
                };
                this.child(style::label_in(ctx, label, color))
            })
            .into_any_element()
    }

    fn render(&self, ctx: &BarCtx, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let config = cx.config().bar.modules.workspaces.clone();
        let monitor_name = self.current_monitor_name(window, cx);

        // Prefer this monitor's own active tag over the compositor-wide
        // `active_workspace_id`, which only ever reflects whichever monitor
        // currently has focus - on multi-monitor setups that's a different
        // monitor from this bar's as soon as focus moves elsewhere, which
        // would otherwise make this bar lose (or misreport) its highlight.
        let active_workspace_id = monitor_name
            .as_deref()
            .and_then(|name| self.state.monitors.iter().find(|m| m.name == name))
            .map(|m| m.active_workspace_id)
            .filter(|&id| id >= 0)
            .or(self.state.active_workspace_id);

        style::stack(ctx)
            .id("workspaces")
            .justify_center()
            .gap(ctx.metrics.group_gap)
            .on_scroll(
                cx.listener(|this, event: &gpui::ScrollEvent, _window, _cx| {
                    let delta = event.delta.pixel_delta(SCROLL_LINE);
                    if delta.y.abs() > SCROLL_DEADZONE {
                        let direction = if delta.y > Pixels::ZERO { -1 } else { 1 };
                        this.scroll_workspace(direction);
                    }
                }),
            )
            .children(
                self.state
                    .workspaces
                    .iter()
                    .filter(|ws| !ws.is_special)
                    .filter(|ws| {
                        monitor_name
                            .as_deref()
                            .is_none_or(|name| ws.monitor == name)
                    })
                    .map(|ws| self.render_cell(ws, active_workspace_id, ctx, &config, cx)),
            )
            .into_any_element()
    }
}

impl BarWidget for Workspaces {
    fn shell(&self) -> BarWidgetShell {
        BarWidgetShell::Group
    }

    fn render_vertical(
        &mut self,
        ctx: &BarCtx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, window, cx)
    }

    fn render_horizontal(
        &mut self,
        ctx: &BarCtx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, window, cx)
    }
}
