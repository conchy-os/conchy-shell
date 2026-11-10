//! What the section previews share: a thumbnail screen, its caption and the
//! clickable zones along its edges or in its corners that move a thing there.

use std::rc::Rc;

use gpui::{AnyElement, App, Div, Pixels, SharedString, div, prelude::*, px};
use serde::Serialize;
use ui::{ActiveTheme, Color, Fade, Label, LabelCommon, Opacity, Radius, Spacing, TextSize};

use super::schema::{ConfigPath, to_value};

pub const SCREEN_WIDTH: f32 = 200.;
pub const SCREEN_HEIGHT: f32 = 120.;
/// Config pixels to preview pixels: the thumbnail stands for an 800px screen.
pub const SCALE: f32 = 0.25;
/// How far in from an edge a click still means "that edge".
const EDGE_HIT: f32 = 22.;
/// Corner zones are squarer and a little bigger, so a corner is easy to hit.
const CORNER_HIT: f32 = 36.;

/// `value` config px as preview px, held within `lo..=hi` so an extreme
/// setting still draws something legible.
pub fn scale(value: f32, lo: f32, hi: f32) -> Pixels {
    px((value * SCALE).clamp(lo, hi))
}

/// `value` config px as preview px against the real primary display, for a
/// preview where true proportions matter more than legibility.
pub fn scale_to_display(value: f32, lo: f32, hi: f32, cx: &App) -> Pixels {
    let display_width = cx
        .primary_display()
        .map(|display| f32::from(display.bounds().size.width))
        .filter(|width| *width > 0.)
        .unwrap_or(1920.);
    px((value * SCREEN_WIDTH / display_width).clamp(lo, hi))
}

/// The empty screen every preview draws on.
pub fn screen(cx: &App) -> Div {
    let colors = cx.theme().colors();
    div()
        .relative()
        .w(px(SCREEN_WIDTH))
        .h(px(SCREEN_HEIGHT))
        .rounded(Radius::Medium.pixels())
        .border_1()
        .border_color(colors.border)
        .bg(colors.background)
        .overflow_hidden()
}

pub fn caption(text: impl Into<SharedString>) -> Label {
    Label::new(text).size(TextSize::XSmall).color(Color::Muted)
}

/// The screen with its caption underneath, as the detail pane shows them.
pub fn frame(screen: Div, text: impl Into<SharedString>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(Spacing::Small.pixels())
        .child(screen)
        .child(caption(text))
        .into_any_element()
}

/// Where a hit zone sits on the screen.
#[derive(Clone, Copy)]
pub enum Zone {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Zone {
    fn name(self) -> &'static str {
        match self {
            Zone::Left => "left",
            Zone::Right => "right",
            Zone::Top => "top",
            Zone::Bottom => "bottom",
            Zone::TopLeft => "top-left",
            Zone::TopRight => "top-right",
            Zone::BottomLeft => "bottom-left",
            Zone::BottomRight => "bottom-right",
        }
    }

    fn place<E: Styled>(self, zone: E) -> E {
        let edge = px(EDGE_HIT);
        let corner = px(CORNER_HIT);
        match self {
            Zone::Left => zone.top_0().bottom_0().left_0().w(edge),
            Zone::Right => zone.top_0().bottom_0().right_0().w(edge),
            Zone::Top => zone.left_0().right_0().top_0().h(edge),
            Zone::Bottom => zone.left_0().right_0().bottom_0().h(edge),
            Zone::TopLeft => zone.top_0().left_0().size(corner),
            Zone::TopRight => zone.top_0().right_0().size(corner),
            Zone::BottomLeft => zone.bottom_0().left_0().size(corner),
            Zone::BottomRight => zone.bottom_0().right_0().size(corner),
        }
    }
}

/// One clickable zone per value of a position enum, laid over `screen`.
/// Only a zone that is not the current value lights up on hover.
pub fn hit_zones<P>(
    mut screen: Div,
    id: &str,
    all: &[P],
    current: P,
    zone: fn(P) -> Zone,
    set: impl Fn(P, &mut App) + 'static,
    cx: &App,
) -> Div
where
    P: Copy + PartialEq + 'static,
{
    let colors = cx.theme().colors();
    let set = Rc::new(set);
    for &position in all {
        let set = set.clone();
        let zone = zone(position);
        let hit = div()
            .id(SharedString::from(format!("{id}-{}", zone.name())))
            .absolute()
            .cursor_pointer()
            .map(|hit| zone.place(hit))
            .when(position != current, |hit| {
                hit.hover(move |s| s.bg(colors.accent.fade(Opacity::Faint)))
            })
            .on_click(move |_, _, cx| set(position, cx));
        screen = screen.child(hit);
    }
    screen
}

/// Set one field of the section from a click on the preview, recording it
/// under `key` the way the field's own editor would.
pub fn set_field<S, V>(
    path: &ConfigPath<S>,
    key: &'static str,
    value: V,
    set: fn(&mut S, V),
    cx: &mut App,
) where
    S: Clone + 'static,
    V: Serialize,
{
    let mut section = path.read(cx).clone();
    let toml = to_value(&value);
    set(&mut section, value);
    path.set_raw(path.keys_for(key), toml, section, cx);
}
