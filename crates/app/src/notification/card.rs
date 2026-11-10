//! The body of a notification card, shared by the popups and the center:
//! a header line (app icon, name, time, dismiss), the summary and body with
//! the image beside them, and the actions when there is room.

use gpui::prelude::*;
use std::path::Path;

use gpui::{
    AnyElement, App, Context, EntityId, FontWeight, MouseButton, Size, Window, div, img, px,
};
use services::{CompositorCommand, Notification, NotificationCommand, NotificationSubscriber};
use ui::{
    ActiveTheme, Button, ButtonCommon, ButtonSize, ButtonStyle, Clickable, Color, Icon, IconSize,
    Radius, Spacing, TextSize, thumbnail,
};

use crate::config::ActiveConfig;
use crate::launcher::row::thumbnail_image;
use crate::state::AppState;

/// Side of the image slot.
const IMAGE_SIZE: f32 = 56.0;
/// Side of the app icon in the header.
const APP_ICON_SIZE: f32 = 18.0;
/// Body lines a full card shows before it is cut.
const BODY_LINES: usize = 8;

/// A click on a card with a default action. Wayland apps can't raise their
/// own window without an activation token, so the compositor focuses it.
pub(super) fn activate_notification(notification: &Notification, cx: &mut App) {
    let compositor = AppState::compositor(cx);
    if let Some(window) = compositor
        .get()
        .app_window(notification.sender_pid, &notification.app_ids())
    {
        compositor.dispatch(CompositorCommand::FocusWindow(window.id.clone()));
    }
    AppState::notification(cx).dispatch(NotificationCommand::Activate(notification.id));
}

pub(super) fn is_critical(notification: &Notification) -> bool {
    notification.urgency == 2
}

/// `compact`: one line each of summary and body, no actions (a popup at rest).
pub(super) fn notification_card_body<V: 'static>(
    notification: &Notification,
    window: &Window,
    cx: &Context<V>,
    compact: bool,
    subscriber: &NotificationSubscriber,
) -> AnyElement {
    let theme = cx.theme();
    let owner = cx.entity_id();
    let id = notification.id;
    let summary = notification.summary.clone();
    // One line at rest: the sender's line breaks would each take a row.
    let body = if compact {
        notification
            .body
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        notification.body.clone()
    };
    let timestamp = format_notification_time(notification.timestamp_ms);
    let close = cx.config().notification.icons.close();
    let dismiss = subscriber.clone();
    let dismiss_hover = theme.colors.ghost_element_hover;
    let app_name_color = if is_critical(notification) {
        theme.colors.status.error
    } else {
        theme.colors.text_muted
    };

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(Spacing::Small.pixels())
        .child(
            div()
                .flex()
                .items_center()
                .gap(Spacing::Small.pixels())
                .child(app_icon_slot(notification, cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(TextSize::XSmall.rems())
                        .text_color(app_name_color)
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(notification.app_name.clone()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(TextSize::XSmall.rems())
                        .text_color(theme.colors.text_muted)
                        .child(timestamp),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .p(px(2.0))
                        .rounded(Radius::Small.pixels())
                        .cursor_pointer()
                        .hover(move |el| el.bg(dismiss_hover))
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            // Not a click on the card itself.
                            cx.stop_propagation();
                            dismiss.dispatch(NotificationCommand::Dismiss(id));
                        })
                        .child(Icon::new(close).size(IconSize::XSmall).color(Color::Muted)),
                ),
        )
        .child(
            div()
                .flex()
                .items_start()
                .gap(Spacing::Medium.pixels())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(TextSize::Small.rems())
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.colors.text)
                                .line_height(px(18.0))
                                .text_ellipsis()
                                .line_clamp(if compact { 1 } else { 2 })
                                .child(summary),
                        )
                        .when(!body.is_empty(), |el| {
                            el.child(
                                div()
                                    .text_size(TextSize::Small.rems())
                                    .text_color(theme.colors.text_muted)
                                    .line_height(px(17.0))
                                    .text_ellipsis()
                                    .line_clamp(if compact { 1 } else { BODY_LINES })
                                    .child(body),
                            )
                        }),
                )
                .when_some(notification.image_path.clone(), |el, path| {
                    el.child(image_slot(&path, owner, window, cx))
                }),
        )
        .when(!notification.actions.is_empty() && !compact, |el| {
            el.child(actions_row(notification, subscriber))
        })
        .into_any_element()
}

fn app_icon_slot<V>(notification: &Notification, cx: &Context<V>) -> AnyElement {
    let theme = cx.theme();
    match &notification.app_icon_path {
        Some(path) => img(path.clone())
            .size(px(APP_ICON_SIZE))
            .flex_shrink_0()
            .object_fit(gpui::ObjectFit::Contain)
            .into_any_element(),
        None => div()
            .size(px(APP_ICON_SIZE))
            .flex_shrink_0()
            .rounded(Radius::Small.pixels())
            .bg(theme.colors.element_background)
            .flex()
            .items_center()
            .justify_center()
            .text_size(TextSize::XSmall.rems())
            .text_color(theme.colors.text_muted)
            .child(icon_fallback(
                &notification.app_name,
                &notification.app_icon,
            ))
            .into_any_element(),
    }
}

fn image_slot<V>(path: &Path, owner: EntityId, window: &Window, cx: &Context<V>) -> AnyElement {
    div()
        .size(px(IMAGE_SIZE))
        .flex_shrink_0()
        .rounded(Radius::Medium.pixels())
        .overflow_hidden()
        .bg(cx.theme().colors.surface_background)
        .children(notification_image(path, owner, window, cx))
        .into_any_element()
}

fn actions_row(notification: &Notification, subscriber: &NotificationSubscriber) -> AnyElement {
    let id = notification.id;
    div()
        .flex()
        .flex_wrap()
        .gap(Spacing::XSmall.pixels())
        // A button press is not a click on the card.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(
            notification
                .actions
                .iter()
                .enumerate()
                .map(|(index, (key, label))| {
                    let sub = subscriber.clone();
                    let key = key.clone();
                    Button::new(("notification-action", index), label.clone())
                        .style(ButtonStyle::Outlined)
                        .size(ButtonSize::Compact)
                        .on_click(move |_, _, _| {
                            sub.dispatch(NotificationCommand::InvokeAction(id, key.clone()));
                        })
                }),
        )
        .into_any_element()
}

fn icon_fallback(app_name: &str, app_icon_name: &str) -> String {
    if !app_icon_name.is_empty() {
        return app_icon_name.chars().take(2).collect();
    }
    app_name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn format_notification_time(timestamp_ms: i64) -> String {
    use chrono::{Local, TimeZone};

    Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_default()
}

/// The notification's image at slot size. Raster files go through the
/// thumbnail cache so a full-size screenshot is never decoded whole; while
/// it loads the slot stays empty and the owner is redrawn when it is ready.
fn notification_image(
    path: &Path,
    owner: EntityId,
    window: &Window,
    cx: &App,
) -> Option<AnyElement> {
    let fit = |source: gpui::ImageSource| {
        img(source)
            .size_full()
            .object_fit(gpui::ObjectFit::Cover)
            .into_any_element()
    };
    if !thumbnail::is_thumbnailable(path) {
        return Some(fit(path.to_path_buf().into()));
    }
    let slot = Size {
        width: px(IMAGE_SIZE),
        height: px(IMAGE_SIZE),
    };
    thumbnail_image(path, None, slot, owner, window, cx).map(|image| fit(image.into()))
}
