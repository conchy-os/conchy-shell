//! The bell in the bar: unread count, do-not-disturb state, and the
//! notification center behind a click.

use gpui::{AnyElement, App, ClickEvent, Context, Pixels, Point, Window, prelude::*};
use services::{NotificationCommand, NotificationData, NotificationSubscriber};
use ui::{ButtonCommon, ButtonLike, ButtonStyle, Clickable, IconName, LabelCommon, TextSize};

use crate::bar::modules::{BarCtx, BarWidget, BarWidgetDescriptor, BarWidgetMeta, boxed, style};
use crate::config::{ActiveConfig, Config};
use crate::state::{AppState, watch};
use crate::windows::{PanelConfig, PanelSize, ShellWindows, panel_placement_from_event};

use super::panel::NotificationCenter;

pub const fn descriptor() -> BarWidgetDescriptor {
    BarWidgetDescriptor::new(
        BarWidgetMeta {
            name: "Notifications",
            aliases: &["Notification"],
            label: "Notifications",
            description: "Unread count and the notification center.",
            icon: IconName::Bell,
        },
        |cx| boxed(cx, NotificationWidget::new),
    )
}

/// Notification widget for the bar.
pub struct NotificationWidget {
    subscriber: NotificationSubscriber,
    data: NotificationData,
}

impl NotificationWidget {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscriber = AppState::notification(cx).clone();
        let data = subscriber.get();

        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });

        Self { subscriber, data }
    }

    fn toggle_center(&self, at: Point<Pixels>, window: &Window, cx: &mut App) {
        let config = Config::global(cx);
        let notification_config = &config.notification;
        let size = PanelSize::FitHeight {
            width: notification_config.center_width,
            max_height: notification_config.center_height,
        };
        let (anchor, margin) =
            panel_placement_from_event(config.bar.position, at, window, cx, size.initial());
        let subscriber = self.subscriber.clone();
        subscriber.dispatch(NotificationCommand::MarkAllRead);

        let panel_config = PanelConfig {
            size,
            anchor,
            margin,
            namespace: "notification-center".to_string(),
        };

        ShellWindows::toggle_panel(cx, window, "notification-center", panel_config, move |cx| {
            NotificationCenter::new(subscriber, cx)
        });
    }

    fn render(&self, ctx: &BarCtx, cx: &mut Context<Self>) -> AnyElement {
        let config = &cx.config().notification;
        let icon = if self.data.dnd {
            config.icons.bell_off()
        } else {
            config.icons.bell()
        };
        let unread = self.data.unread_count;

        ButtonLike::new("notification-widget")
            .style(ButtonStyle::Transparent)
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                this.toggle_center(event.position(), window, cx)
            }))
            .child(
                style::stack(ctx)
                    .justify_center()
                    .gap(ctx.metrics.gap)
                    .child(style::icon_in(
                        ctx,
                        icon,
                        if self.data.dnd {
                            ctx.ink.muted
                        } else {
                            ctx.ink.text
                        },
                    ))
                    .when(unread > 0, |el| {
                        el.child(
                            style::label_in(ctx, unread.to_string(), ctx.ink.accent)
                                .size(TextSize::XSmall),
                        )
                    }),
            )
            .into_any_element()
    }
}

impl BarWidget for NotificationWidget {
    fn is_interactive(&self) -> bool {
        true
    }

    fn render_vertical(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, cx)
    }

    fn render_horizontal(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, cx)
    }
}
