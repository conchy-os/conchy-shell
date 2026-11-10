//! A thumbnail screen showing the on-screen display pill mid-edge, where a
//! volume or brightness change would flash it up. Clicking an edge moves it.

use gpui::{AnyElement, App, div, prelude::*, px, relative};
use ui::{ActiveTheme, Radius};

use super::preview::{Zone, frame, hit_zones, scale, screen, set_field};
use super::schema::ConfigPath;
use crate::osd::{OsdConfig, OsdPosition};

/// The real pill is 280 × 56 px, 24 px off its edge.
const LONG: f32 = 280.;
const SHORT: f32 = 56.;
const MARGIN: f32 = 24.;
/// A sample level, so the track shows a fill.
const LEVEL: f32 = 0.65;

pub fn osd_preview(path: &ConfigPath<OsdConfig>, cx: &App) -> AnyElement {
    let position = path.read(cx).position;
    let colors = cx.theme().colors();
    let vertical = matches!(position, OsdPosition::Left | OsdPosition::Right);
    let long = scale(LONG, 40., 100.);
    let short = scale(SHORT, 8., 20.);
    let margin = scale(MARGIN, 0., 20.);

    let icon = div()
        .flex_none()
        .size(px(4.))
        .rounded(Radius::Full.pixels())
        .bg(colors.accent);
    let fill = div().rounded(Radius::Full.pixels()).bg(colors.accent);
    let track = div()
        .flex_1()
        .rounded(Radius::Full.pixels())
        .bg(colors.border)
        .overflow_hidden()
        .map(|track| {
            if vertical {
                // Grows upward, so the fill sits at the bottom of the track.
                track
                    .w(px(3.))
                    .flex()
                    .flex_col()
                    .justify_end()
                    .child(fill.w_full().h(relative(LEVEL)))
            } else {
                track.h(px(3.)).child(fill.h_full().w(relative(LEVEL)))
            }
        });
    let pill = div()
        .flex()
        .items_center()
        .gap(px(3.))
        .rounded(Radius::Full.pixels())
        .border_1()
        .border_color(colors.border)
        .bg(colors.element_background)
        .map(|pill| {
            if vertical {
                pill.flex_col()
                    .w(short)
                    .h(long)
                    .py(px(4.))
                    .child(track)
                    .child(icon)
            } else {
                pill.w(long).h(short).px(px(4.)).child(icon).child(track)
            }
        });

    let layer = div()
        .absolute()
        .flex()
        .justify_center()
        .map(|layer| match position {
            OsdPosition::Left => layer.top_0().bottom_0().left(margin).flex_col(),
            OsdPosition::Right => layer.top_0().bottom_0().right(margin).flex_col(),
            OsdPosition::Top => layer.left_0().right_0().top(margin),
            OsdPosition::Bottom => layer.left_0().right_0().bottom(margin),
        })
        .child(pill);

    let setter = path.clone();
    let screen = hit_zones(
        screen(cx).child(layer),
        "osd-preview",
        &OsdPosition::ALL,
        position,
        osd_zone,
        move |position, cx| {
            set_field(&setter, "position", position, |osd, p| osd.position = p, cx);
        },
        cx,
    );

    frame(screen, format!("{} edge", position.label()))
}

fn osd_zone(position: OsdPosition) -> Zone {
    match position {
        OsdPosition::Top => Zone::Top,
        OsdPosition::Bottom => Zone::Bottom,
        OsdPosition::Left => Zone::Left,
        OsdPosition::Right => Zone::Right,
    }
}
