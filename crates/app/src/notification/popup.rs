//! Notification popups: one window per output. At rest a pile (newest card,
//! up to two peeking edges); hovered, it fans out, shows full cards and holds
//! expiry. The window fits its content and only the cards take input.

use std::sync::Arc;

use chrono::Utc;
use futures_signals::signal::SignalExt;
use futures_util::StreamExt;
use gpui::{
    AnyElement, AnyWindowHandle, App, Bounds, Context, DisplayId, Entity, MouseButton, Point,
    Render, Size, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div,
    layer_shell::*, point, prelude::*, px, relative, size,
};
use services::{Notification, NotificationCommand, NotificationSubscriber};
use ui::patterns::{PanelSurface, surface_radius};
use ui::{ActiveTheme, Spacing};

use crate::config::ActiveConfig;
use crate::state::AppState;
use crate::windows::{ShellWindows, WindowFit};

use super::card::{activate_notification, is_critical, notification_card_body};
use super::config::{NotificationConfig, NotificationPopupPosition};

const PEEK_COUNT: usize = 2;
const PEEK_HEIGHT: f32 = 6.0;
/// Per side, per level of depth.
const PEEK_INSET: f32 = 10.0;
const EXPIRY_LINE_HEIGHT: f32 = 2.0;

pub(crate) struct PopupWindowState {
    handle: AnyWindowHandle,
    view: Entity<NotificationPopupStack>,
}

struct NotificationPopupStack {
    subscriber: NotificationSubscriber,
    notifications: Vec<Arc<Notification>>,
    hovered: bool,
    /// Where the countdowns stand still while the popups are held.
    held_since_ms: Option<i64>,
    fit: WindowFit,
    /// Window size the input region was last set for.
    input_region_for: Option<Size<gpui::Pixels>>,
}

impl NotificationPopupStack {
    fn new(subscriber: NotificationSubscriber, notifications: Vec<Arc<Notification>>) -> Self {
        Self {
            subscriber,
            notifications,
            hovered: false,
            held_since_ms: None,
            fit: WindowFit::default(),
            input_region_for: None,
        }
    }

    fn set_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.hovered == hovered {
            return;
        }
        self.hovered = hovered;
        self.subscriber
            .dispatch(NotificationCommand::HoldPopups(hovered));
        cx.notify();
    }

    /// Compact at rest, full when fanned out.
    fn render_card(
        &self,
        notification: &Notification,
        compact: bool,
        config: &NotificationConfig,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let clicked = notification.clone();

        div()
            .id(("notification-card", notification.id))
            .w_full()
            .panel_surface_with(config.opacity, cx)
            .p(Spacing::Medium.pixels())
            .flex()
            .flex_col()
            .gap(Spacing::Small.pixels())
            .when(is_critical(notification), |el| {
                el.border_color(theme.colors.status.error)
            })
            .when(notification.has_default_action(), |el| {
                el.cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        activate_notification(&clicked, cx);
                    })
            })
            .child(notification_card_body(
                notification,
                window,
                cx,
                compact,
                &self.subscriber,
            ))
            .when_some(
                expiry_fraction(notification, self.held_since_ms),
                |el, fraction| el.child(self.render_expiry(fraction, cx)),
            )
            .into_any_element()
    }

    /// The time left before the card goes, as a line that shrinks.
    fn render_expiry(&self, fraction: f32, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        div()
            .w_full()
            .h(px(EXPIRY_LINE_HEIGHT))
            .rounded_full()
            .bg(theme.colors.border_variant)
            .child(
                div()
                    .h_full()
                    .w(relative(fraction))
                    .rounded_full()
                    .bg(theme.colors.accent),
            )
            .into_any_element()
    }

    /// A peeking edge of a notification under the top card.
    fn render_peek(
        &self,
        depth: usize,
        config: &NotificationConfig,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let inset = PEEK_INSET * (depth + 1) as f32;
        div()
            .h(px(PEEK_HEIGHT))
            .mx(px(inset))
            .bg(theme.surface(config.opacity))
            .border_1()
            .border_t_0()
            .border_color(theme.colors.border)
            .rounded_b(surface_radius())
            .into_any_element()
    }

    fn build_stack(
        &self,
        config: &NotificationConfig,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fanned = self.hovered;
        let shown = if fanned {
            self.notifications.len()
        } else {
            self.notifications.len().min(1)
        };
        let peeks = if fanned {
            0
        } else {
            self.notifications.len().saturating_sub(1).min(PEEK_COUNT)
        };
        let cards: Vec<AnyElement> = self.notifications[..shown]
            .iter()
            .map(|notification| self.render_card(notification, !fanned, config, window, cx))
            .collect();

        div()
            .id("notification-popup-stack")
            .w_full()
            .max_h_full()
            .p(Spacing::Medium.pixels())
            .flex()
            .flex_col()
            // gpui reports a hover at the window origin on first paint, before any
            // pointer entered; only a real pointer counts, and a move fills in the enter.
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                this.set_hovered(*hovered && window.is_window_hovered(), cx);
            }))
            .on_mouse_move(cx.listener(|this, _event, _window, cx| {
                this.set_hovered(true, cx);
            }))
            .child(
                // Taller than the screen: scroll rather than cut the last card.
                div()
                    .id("notification-popup-cards")
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap(Spacing::XSmall.pixels())
                    .children(cards),
            )
            .children((0..peeks).map(|depth| self.render_peek(depth, config, cx)))
            .into_any_element()
    }
}

impl Render for NotificationPopupStack {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        window.set_rem_size(cx.theme().font_size);
        let config = cx.config().notification.clone();
        let margin = popup_margin(&config);
        self.held_since_ms = self.subscriber.popups_held_since_ms();

        // The expiry lines move every frame while something counts down.
        let counting = self.held_since_ms.is_none()
            && self
                .notifications
                .iter()
                .any(|notification| notification.expires_at_ms.is_some());
        if counting {
            window.request_animation_frame();
        }

        let measure = self.build_stack(&config, window, cx);
        // Fanned out, every card shows in full, bounded only by the screen.
        let max_height = if self.hovered {
            px(f32::INFINITY)
        } else {
            px(config.popup_height)
        };
        let paint = self.fit.fit_height(
            window,
            cx,
            measure,
            px(config.popup_width),
            max_height,
            px(margin.0 + margin.2),
        );

        // Clicks outside the cards pass through to the desktop.
        if let Some(window_size) = self.fit.requested()
            && self.input_region_for != Some(window_size)
        {
            let pad = Spacing::Medium.pixels();
            let region = Bounds {
                origin: point(pad, pad),
                size: size(
                    (window_size.width - pad * 2.).max(px(0.)),
                    (window_size.height - pad * 2.).max(px(0.)),
                ),
            };
            window.set_input_region(Some(&[region]));
            self.input_region_for = Some(window_size);
        }

        div()
            .id("notification-popup-window")
            .size_full()
            .when(paint, |el| el.child(self.build_stack(&config, window, cx)))
    }
}

/// How much of the popup's time is left, frozen while the popups are held.
fn expiry_fraction(notification: &Notification, held_since_ms: Option<i64>) -> Option<f32> {
    let expires_at = notification.expires_at_ms?;
    if notification.timeout_ms <= 0 {
        return None;
    }
    let now = held_since_ms.unwrap_or_else(|| Utc::now().timestamp_millis());
    let remaining = (expires_at - now).max(0) as f32;
    Some((remaining / notification.timeout_ms as f32).clamp(0.0, 1.0))
}

fn popup_window_options(
    config: &NotificationConfig,
    display_id: Option<DisplayId>,
) -> WindowOptions {
    let margin = popup_margin(config);
    WindowOptions {
        display_id,
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point::new(px(0.), px(0.)),
            size: Size::new(px(config.popup_width), px(config.popup_height)),
        })),
        app_id: Some("conchy-shell-notification-popup".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "notification-popup".to_string(),
            layer: Layer::Overlay,
            anchor: popup_anchor(config.popup_position),
            exclusive_zone: None,
            margin: Some((px(margin.0), px(margin.1), px(margin.2), px(margin.3))),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        focus: false,
        ..Default::default()
    }
}

fn popup_anchor(position: NotificationPopupPosition) -> Anchor {
    match position {
        NotificationPopupPosition::TopLeft => Anchor::TOP | Anchor::LEFT,
        NotificationPopupPosition::TopRight => Anchor::TOP | Anchor::RIGHT,
        NotificationPopupPosition::BottomLeft => Anchor::BOTTOM | Anchor::LEFT,
        NotificationPopupPosition::BottomRight => Anchor::BOTTOM | Anchor::RIGHT,
    }
}

fn popup_margin(config: &NotificationConfig) -> (f32, f32, f32, f32) {
    match config.popup_position {
        NotificationPopupPosition::TopLeft => {
            (config.popup_margin_top, 0.0, 0.0, config.popup_margin_left)
        }
        NotificationPopupPosition::TopRight => {
            (config.popup_margin_top, config.popup_margin_right, 0.0, 0.0)
        }
        NotificationPopupPosition::BottomLeft => (
            0.0,
            0.0,
            config.popup_margin_bottom,
            config.popup_margin_left,
        ),
        NotificationPopupPosition::BottomRight => (
            0.0,
            config.popup_margin_right,
            config.popup_margin_bottom,
            0.0,
        ),
    }
}

fn popup_targets(cx: &App) -> Vec<Option<DisplayId>> {
    let displays = cx.displays();
    if displays.is_empty() {
        vec![None]
    } else {
        let filter = cx.config().bar.monitors.clone();
        if let Some(target) = filter {
            let target_uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, target.as_bytes());
            if let Some(d) = displays.iter().find(|d| d.uuid().ok() == Some(target_uuid)) {
                return vec![Some(d.id())];
            }
        }
        let target = displays
            .iter()
            .find(|d| d.bounds().size.width > d.bounds().size.height)
            .cloned()
            .or_else(|| cx.primary_display());
        vec![target.map(|d| d.id())]
    }
}

fn close_popups(subscriber: &NotificationSubscriber, cx: &mut App) {
    for (_, state) in ShellWindows::take_popups(cx) {
        let _ = cx.update_window(state.handle, |_, window, _cx| {
            window.remove_window();
        });
    }
    // A window closed under the pointer never saw it leave.
    subscriber.dispatch(NotificationCommand::HoldPopups(false));
}

fn sync_popup(subscriber: &NotificationSubscriber, cx: &mut App) {
    let config = cx.config().notification.clone();
    let notifications = subscriber.popup_notifications(config.popup_stack_limit);
    if notifications.is_empty() {
        close_popups(subscriber, cx);
        return;
    }

    let targets = popup_targets(cx);
    let mut popups = ShellWindows::take_popups(cx);

    popups.retain(|display_id, state| {
        if targets.contains(display_id) {
            true
        } else {
            let _ = cx.update_window(state.handle, |_, window, _cx| {
                window.remove_window();
            });
            false
        }
    });

    for display_id in targets {
        if let Some(existing) = popups.get(&display_id) {
            let view = existing.view.clone();
            let handle = existing.handle;
            let updated = cx
                .update_window(handle, |_, _window, cx| {
                    view.update(cx, |popup, cx| {
                        popup.notifications = notifications.clone();
                        cx.notify();
                    });
                })
                .is_ok();
            if updated {
                continue;
            }
        }

        popups.remove(&display_id);
        let sub = subscriber.clone();
        let notifications = notifications.clone();
        if let Ok(handle) = cx
            .open_window(popup_window_options(&config, display_id), move |_, cx| {
                cx.new(|_| NotificationPopupStack::new(sub, notifications))
            })
        {
            let view = handle.update(cx, |_, _, cx| cx.entity().clone()).unwrap();
            popups.insert(
                display_id,
                PopupWindowState {
                    handle: handle.into(),
                    view,
                },
            );
        }
    }
    ShellWindows::set_popups(cx, popups);
}

pub fn init(cx: &mut App) {
    let subscriber = AppState::notification(cx).clone();
    cx.spawn({
        let mut signal = subscriber.subscribe().to_stream();
        let service = subscriber.clone();
        async move |cx| {
            while signal.next().await.is_some() {
                cx.update(|cx| sync_popup(&service, cx));
            }
        }
    })
    .detach();
}
