//! A thumbnail screen showing the launcher window: hung from the top edge
//! like the real layer-shell surface, sized and pushed in by the live config.

use gpui::{AnyElement, App, div, prelude::*, px};
use ui::{ActiveTheme, Fade, Opacity, Radius};

use super::preview::{frame, scale_to_display, screen};
use super::schema::ConfigPath;
use crate::launcher::config::LauncherConfig;

pub fn launcher_preview(path: &ConfigPath<LauncherConfig>, cx: &App) -> AnyElement {
    let config = path.read(cx);
    let colors = cx.theme().colors();
    // True to the display, so the window reads at its real share of the screen.
    let width = scale_to_display(config.width, 20., 190., cx);
    let height = scale_to_display(config.height, 12., 110., cx);
    let margin = |value: f32| scale_to_display(value, 0., 100., cx);
    let header = px((f32::from(height) * 0.2).clamp(4., 8.));

    let line = |width: f32, opacity: Option<Opacity>| {
        div()
            .h(px(2.))
            .w(px(width))
            .rounded(Radius::Full.pixels())
            .bg(opacity.map_or(colors.border, |opacity| colors.border.fade(opacity)))
    };
    let window = div()
        .flex_none()
        .flex()
        .flex_col()
        .w(width)
        .h(height)
        .rounded(Radius::Small.pixels())
        .border_1()
        .border_color(colors.border)
        .bg(colors.element_background)
        .overflow_hidden()
        .child(
            div()
                .flex()
                .items_center()
                .px(px(4.))
                .h(header)
                .border_b_1()
                .border_color(colors.border)
                .child(line(f32::from(width) * 0.3, None)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .px(px(4.))
                .py(px(3.))
                .child(line(f32::from(width) * 0.5, Some(Opacity::Soft)))
                .child(line(f32::from(width) * 0.4, Some(Opacity::Soft))),
        );

    // The margins shrink the area the window is centred in, top first.
    let area = div()
        .absolute()
        .top(margin(config.margin_top))
        .right(margin(config.margin_right))
        .bottom(margin(config.margin_bottom))
        .left(margin(config.margin_left))
        .flex()
        .justify_center()
        .items_start()
        .child(window);

    frame(
        screen(cx).child(area),
        format!(
            "{:.0} × {:.0} px · {:.0} px from top",
            config.width, config.height, config.margin_top
        ),
    )
}
