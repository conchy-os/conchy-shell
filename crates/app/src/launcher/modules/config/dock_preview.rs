//! A thumbnail screen showing the dock on its edge: its icons at the
//! configured size, one under the cursor with the hover effect, and how the
//! visibility mode hides it. Clicking an edge moves the dock there.

use gpui::{AnyElement, App, BoxShadow, Div, Pixels, div, point, prelude::*, px};
use ui::{ActiveTheme, Fade, Opacity, Radius};

use super::bar_preview::bar_zone;
use super::preview::{frame, hit_zones, scale, screen, set_field};
use super::schema::ConfigPath;
use crate::bar::config::BarPosition;
use crate::dock::config::{DockAlign, DockConfig, DockHoverEffect, DockStyle, DockVisibility};

/// How far a floating dock floats off the screen edge.
const EDGE_GAP: f32 = 3.;
/// How far a lifted icon rises; a quarter of the real 8px lift is invisible.
const LIFT: f32 = 3.;
/// A hidden dock shows faintly so the edge it would appear on stays legible.
const HIDDEN_OPACITY: Opacity = Opacity::Muted;
const ICONS: usize = 5;
/// The icon drawn under the cursor.
const HOVERED: usize = 2;

pub fn dock_preview(path: &ConfigPath<DockConfig>, cx: &App) -> AnyElement {
    let dock = path.read(cx).clone();
    let colors = cx.theme().colors();
    let position = dock.position;
    let icon = scale(dock.icon_size, 5., 14.);

    let gap = match dock.style {
        DockStyle::Floating => EDGE_GAP,
        DockStyle::Attached => 0.,
    };
    // Dock thickness in preview pixels: icon, strip padding and gap.
    let thickness = f32::from(icon) + 2. * 2. + gap;

    let mut screen = screen(cx);
    if dock.reserve_space {
        // Windows stop short of a reserved dock.
        screen = screen.child(window_near_edge(position, px(thickness + 3.), cx));
    } else if dock.visibility == DockVisibility::DodgeWindows {
        screen = screen.child(window_near_edge(position, px(2.), cx));
    }

    // The launcher button leads: a ring where the app dots are discs.
    let launcher = dock.launcher.then(|| {
        div()
            .flex_none()
            .rounded(Radius::Full.pixels())
            .size(icon)
            .border_1()
            .border_color(colors.accent)
    });
    let icons = (0..ICONS).map(|ix| {
        let hovered = ix == HOVERED;
        let mut dot = div()
            .flex_none()
            .rounded(Radius::Full.pixels())
            .size(icon)
            .bg(if hovered {
                colors.accent
            } else {
                colors.accent.fade(Opacity::Soft)
            });
        if hovered {
            let effect = dock.hover_effect;
            if matches!(
                effect,
                DockHoverEffect::Magnify | DockHoverEffect::MagnifyLift
            ) {
                dot = dot.size(icon * 1.4);
            }
            if matches!(effect, DockHoverEffect::Lift | DockHoverEffect::MagnifyLift) {
                dot = dot.relative().map(|dot| match position {
                    BarPosition::Left => dot.left(px(LIFT)),
                    BarPosition::Right => dot.right(px(LIFT)),
                    BarPosition::Top => dot.top(px(LIFT)),
                    BarPosition::Bottom => dot.bottom(px(LIFT)),
                });
            }
            if effect == DockHoverEffect::Glow {
                dot = dot.shadow(vec![BoxShadow {
                    color: colors.accent.fade(Opacity::Soft),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(4.),
                    spread_radius: px(1.5),
                    inset: false,
                }]);
            }
        }
        dot
    });
    let radius = Radius::Small.pixels();
    let strip = div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(2.))
        .p(px(2.))
        .border_color(colors.border)
        .bg(colors.element_background)
        .map(|strip| {
            if position.is_vertical() {
                strip.flex_col()
            } else {
                strip
            }
        })
        .map(|strip| match (dock.style, dock.stretch) {
            (DockStyle::Floating, _) => strip.rounded(radius).border_1(),
            // Flush to the edge: round and outline only what faces the screen.
            (DockStyle::Attached, stretched) => match position {
                BarPosition::Top => strip.border_b_1().when(!stretched, |s| {
                    s.rounded_b(radius).border_l_1().border_r_1()
                }),
                BarPosition::Bottom => strip.border_t_1().when(!stretched, |s| {
                    s.rounded_t(radius).border_l_1().border_r_1()
                }),
                BarPosition::Left => strip.border_r_1().when(!stretched, |s| {
                    s.rounded_r(radius).border_t_1().border_b_1()
                }),
                BarPosition::Right => strip.border_l_1().when(!stretched, |s| {
                    s.rounded_l(radius).border_t_1().border_b_1()
                }),
            },
        })
        .when(dock.stretch && position.is_vertical(), |strip| {
            strip.h_full()
        })
        .when(dock.stretch && !position.is_vertical(), |strip| {
            strip.w_full()
        })
        .map(|strip| justify(strip, dock.align))
        .children(launcher)
        .children(icons);
    let dock_layer = along_edge(
        justify(div().absolute().flex(), dock.align),
        position,
        px(gap),
    )
    .when(
        dock.effective_visibility() != DockVisibility::AlwaysVisible,
        |layer| layer.opacity(HIDDEN_OPACITY.value()),
    )
    .child(strip);
    screen = screen.child(dock_layer);

    let setter = path.clone();
    screen = hit_zones(
        screen,
        "dock-preview",
        &BarPosition::ALL,
        position,
        bar_zone,
        move |position, cx| {
            set_field(
                &setter,
                "position",
                position,
                |dock, p| dock.position = p,
                cx,
            );
        },
        cx,
    );

    let shape = match (dock.style, dock.stretch) {
        (style, false) => style.label().to_string(),
        (style, true) => format!("{}, stretched", style.label()),
    };
    let behaviour = if dock.reserve_space {
        "Reserves its space"
    } else {
        dock.visibility.label()
    };
    frame(
        screen,
        format!("{} · {shape} · {behaviour}", position.label()),
    )
}

/// Pack a row's children where `align` puts them along the edge.
fn justify(row: Div, align: DockAlign) -> Div {
    match align {
        DockAlign::Start => row.justify_start(),
        DockAlign::Center => row.justify_center(),
        DockAlign::End => row.justify_end(),
    }
}

/// Stretch `layer` along `position`'s edge, `offset` in from it, so its
/// children sit along the edge as justified.
fn along_edge(layer: Div, position: BarPosition, offset: Pixels) -> Div {
    match position {
        BarPosition::Left => layer.top_0().bottom_0().left(offset).flex_col(),
        BarPosition::Right => layer.top_0().bottom_0().right(offset).flex_col(),
        BarPosition::Top => layer.left_0().right_0().top(offset),
        BarPosition::Bottom => layer.left_0().right_0().bottom(offset),
    }
}

/// A window whose side towards the dock's edge stops `near` from it: over
/// the dock, it is the reason a dodging dock hides; short of it, what a
/// reserved dock keeps windows to.
fn window_near_edge(position: BarPosition, near: Pixels, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let far = px(22.);
    div()
        .absolute()
        .rounded(Radius::Small.pixels())
        .border_1()
        .border_color(colors.border)
        .bg(colors.surface_background)
        .map(|window| match position {
            BarPosition::Left => window.top(far).bottom(far).right(far).left(near),
            BarPosition::Right => window.top(far).bottom(far).left(far).right(near),
            BarPosition::Top => window.left(far).right(far).bottom(far).top(near),
            BarPosition::Bottom => window.left(far).right(far).top(far).bottom(near),
        })
        .child(
            div()
                .w_full()
                .h(px(5.))
                .border_b_1()
                .border_color(colors.border)
                .bg(colors.element_background),
        )
}
