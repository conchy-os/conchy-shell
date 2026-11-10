//! A thumbnail screen showing where the bar sits and how its style looks,
//! redrawn from the live config. Clicking an edge moves the bar there.

use gpui::{AnyElement, App, Hsla, Pixels, div, prelude::*, px};
use ui::{ActiveTheme, Radius};

use super::preview::{
    SCREEN_HEIGHT, SCREEN_WIDTH, Zone, frame, hit_zones, scale, screen, set_field,
};
use super::schema::ConfigPath;
use crate::bar::config::{BarConfig, BarPosition, BarStyle};

pub fn bar_preview(path: &ConfigPath<BarConfig>, cx: &App) -> AnyElement {
    let bar = path.read(cx).clone();
    let theme = cx.theme();
    let colors = theme.colors();
    // Below 0.4 the strip would vanish into the screen.
    let bar_color = colors
        .accent
        .opacity(theme.surface_opacity(bar.opacity).max(0.4));
    let thickness = scale(bar.size, 4., 18.);
    let gap = scale(bar.floating.gap, 0., 24.);
    let frame_width = scale(bar.frame.thickness, 2., 10.);

    let mut screen = screen(cx);
    screen = match bar.style {
        BarStyle::Attached => screen.child(bar_strip(&bar, thickness, px(0.), px(0.), bar_color)),
        BarStyle::Floating => {
            let radius = scale(f32::from(bar.floating.radius()), 0., 12.);
            screen.child(bar_strip(&bar, thickness, gap, radius, bar_color))
        }
        BarStyle::Bubbles => screen.child(bubbles(&bar, thickness, bar_color)),
        // The frame is the screen itself; the content shows as a hole.
        BarStyle::Frame => {
            let radius = scale(f32::from(bar.frame.radius()), 0., 12.);
            let f = frame_width;
            let hole = div()
                .absolute()
                .rounded(radius)
                .bg(colors.surface_background)
                .map(|hole| match bar.position {
                    BarPosition::Left => hole.top(f).right(f).bottom(f).left(thickness),
                    BarPosition::Right => hole.top(f).left(f).bottom(f).right(thickness),
                    BarPosition::Top => hole.left(f).right(f).bottom(f).top(thickness),
                    BarPosition::Bottom => hole.left(f).right(f).top(f).bottom(thickness),
                });
            screen.bg(bar_color).child(hole)
        }
    };

    let setter = path.clone();
    screen = hit_zones(
        screen,
        "bar-preview",
        &BarPosition::ALL,
        bar.position,
        bar_zone,
        move |position, cx| {
            set_field(&setter, "position", position, |bar, p| bar.position = p, cx);
        },
        cx,
    );

    frame(
        screen,
        format!("{} · {}", bar.position.label(), bar.style.label()),
    )
}

pub fn bar_zone(position: BarPosition) -> Zone {
    match position {
        BarPosition::Left => Zone::Left,
        BarPosition::Right => Zone::Right,
        BarPosition::Top => Zone::Top,
        BarPosition::Bottom => Zone::Bottom,
    }
}

/// The bar as one strip on its edge, inset by `gap` and rounded by `radius`.
fn bar_strip(
    bar: &BarConfig,
    thickness: Pixels,
    gap: Pixels,
    radius: Pixels,
    color: Hsla,
) -> impl IntoElement {
    div()
        .absolute()
        .rounded(radius)
        .bg(color)
        .map(|strip| match bar.position {
            BarPosition::Left => strip.top(gap).bottom(gap).left(gap).w(thickness),
            BarPosition::Right => strip.top(gap).bottom(gap).right(gap).w(thickness),
            BarPosition::Top => strip.left(gap).right(gap).top(gap).h(thickness),
            BarPosition::Bottom => strip.left(gap).right(gap).bottom(gap).h(thickness),
        })
}

/// Three chips along the edge, the way bubbles group widgets.
fn bubbles(bar: &BarConfig, thickness: Pixels, color: Hsla) -> impl IntoElement {
    let inset = px(4.);
    let vertical = bar.is_vertical();
    let chip = |weight: f32| {
        div().rounded(Radius::Full.pixels()).bg(color).map(|chip| {
            if vertical {
                chip.w(thickness).h(px(SCREEN_HEIGHT * weight))
            } else {
                chip.h(thickness).w(px(SCREEN_WIDTH * weight))
            }
        })
    };
    div()
        .absolute()
        .flex()
        .justify_between()
        .map(|row| if vertical { row.flex_col() } else { row })
        .map(|row| match bar.position {
            BarPosition::Left => row.top(inset).bottom(inset).left(inset).w(thickness),
            BarPosition::Right => row.top(inset).bottom(inset).right(inset).w(thickness),
            BarPosition::Top => row.left(inset).right(inset).top(inset).h(thickness),
            BarPosition::Bottom => row.left(inset).right(inset).bottom(inset).h(thickness),
        })
        .child(chip(0.22))
        .child(chip(0.3))
        .child(chip(0.34))
}
