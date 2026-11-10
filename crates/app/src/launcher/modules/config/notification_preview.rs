//! A thumbnail screen showing the popup stack in its corner, sized and
//! inset from the live config, with the notification center faint on the
//! right. Clicking a corner moves the popups there.

use gpui::{AnyElement, App, Div, div, prelude::*, px};
use ui::{ActiveTheme, Fade, Opacity, Radius};

use super::preview::{Zone, frame, hit_zones, scale, screen, set_field};
use super::schema::ConfigPath;
use crate::notification::{NotificationConfig, NotificationPopupPosition};

/// Space between stacked cards.
const CARD_GAP: f32 = 2.;
/// More cards than this read as noise at thumbnail size.
const MAX_CARDS: usize = 3;

pub fn notification_preview(path: &ConfigPath<NotificationConfig>, cx: &App) -> AnyElement {
    let config = path.read(cx).clone();
    let colors = cx.theme().colors();
    let position = config.popup_position;

    let mut screen = screen(cx).child(center_panel(&config, cx));

    let card_width = scale(config.popup_width, 24., 100.);
    let card_height = scale(config.popup_card_collapsed_height, 6., 20.);
    let stack_height = scale(config.popup_height, 10., 110.);
    // As many cards as the stack height holds, within the stack limit.
    let fits = ((f32::from(stack_height) + CARD_GAP) / (f32::from(card_height) + CARD_GAP)).floor()
        as usize;
    let cards = fits
        .clamp(1, MAX_CARDS)
        .min(config.popup_stack_limit.max(1));
    let card = || {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .px(px(3.))
            .w(card_width)
            .h(card_height)
            .rounded(Radius::Small.pixels())
            .border_1()
            .border_color(colors.border)
            .bg(colors.element_background)
            .child(
                div()
                    .flex_none()
                    .size(px(3.))
                    .rounded(Radius::Full.pixels())
                    .bg(colors.accent),
            )
            .child(
                div()
                    .h(px(2.))
                    .w(px(f32::from(card_width) * 0.5))
                    .rounded(Radius::Full.pixels())
                    .bg(colors.border),
            )
    };
    // Only the margins on the anchored sides apply, as in the real window.
    let margin = |value: f32| scale(value, 0., 40.);
    let top = margin(config.popup_margin_top);
    let right = margin(config.popup_margin_right);
    let bottom = margin(config.popup_margin_bottom);
    let left = margin(config.popup_margin_left);
    let stack = div()
        .absolute()
        .flex()
        .flex_col()
        .gap(px(CARD_GAP))
        .map(|stack| match position {
            NotificationPopupPosition::TopLeft => stack.top(top).left(left),
            NotificationPopupPosition::TopRight => stack.top(top).right(right),
            NotificationPopupPosition::BottomLeft => stack.bottom(bottom).left(left),
            NotificationPopupPosition::BottomRight => stack.bottom(bottom).right(right),
        })
        .children((0..cards).map(|_| card()));
    screen = screen.child(stack);

    let setter = path.clone();
    screen = hit_zones(
        screen,
        "notification-preview",
        &NotificationPopupPosition::ALL,
        position,
        corner_zone,
        move |position, cx| {
            set_field(
                &setter,
                "popup_position",
                position,
                |config, p| config.popup_position = p,
                cx,
            );
        },
        cx,
    );

    frame(
        screen,
        format!(
            "{} · {:.0} × {:.0} px",
            position.label(),
            config.popup_width,
            config.popup_height
        ),
    )
}

fn corner_zone(position: NotificationPopupPosition) -> Zone {
    match position {
        NotificationPopupPosition::TopLeft => Zone::TopLeft,
        NotificationPopupPosition::TopRight => Zone::TopRight,
        NotificationPopupPosition::BottomLeft => Zone::BottomLeft,
        NotificationPopupPosition::BottomRight => Zone::BottomRight,
    }
}

/// The notification center, faint so the popups stay the subject.
fn center_panel(config: &NotificationConfig, cx: &App) -> Div {
    let colors = cx.theme().colors();
    div()
        .absolute()
        .top(px(2.))
        .right(px(2.))
        .w(scale(config.center_width, 30., 110.))
        .h(scale(config.center_height, 40., 116.))
        .rounded(Radius::Small.pixels())
        .border_1()
        .border_color(colors.border.fade(Opacity::Soft))
        .bg(colors.element_background.fade(Opacity::Subtle))
}
