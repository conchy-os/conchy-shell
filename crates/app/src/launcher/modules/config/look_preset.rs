//! Applying a bar [`Look`] from the editor and drawing it: the `[bar]` keys
//! become pending edits like any other change (Save writes them), the
//! theme's radius scale is written to `theme.toml` at once, like a scheme
//! switch.

use gpui::{AnyElement, App, FontWeight, Hsla, MouseButton, SharedString, div, prelude::*, px};
use toml_edit::Value;
use ui::patterns::{
    BarChip, BarEdge, BarRhythm, BarSegment, BarShape, BarSurface, ChipGeometry, card_radius,
};
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, TextSize,
    h_flex, v_flex,
};

use super::schema::{ConfigPath, to_value};
use crate::bar::config::BarConfig;
use crate::bar::look::Look;
use crate::launcher::modules::theme::apply_theme;

/// Apply `look`: every `[bar]` key it owns as its own pending edit (unset
/// ones removed, so defaults apply again), then the theme's radius scale.
pub fn apply_look(look: Look, path: &ConfigPath<BarConfig>, cx: &mut App) {
    let mut bar = path.read(cx).clone();
    look.apply(&mut bar);

    let set: [(&str, Value); 5] = [
        ("style", to_value(&bar.style)),
        ("show_border", to_value(&bar.show_border)),
        ("widget_background", to_value(&bar.widget_background)),
        ("widget_border", to_value(&bar.widget_border)),
        ("widget_stretch", to_value(&bar.widget_stretch)),
    ];
    for (key, value) in set {
        path.set_raw(path.keys_for(key), value, bar.clone(), cx);
    }
    match bar.padding {
        Some(padding) => path.set_raw(
            path.keys_for("padding"),
            to_value(&padding),
            bar.clone(),
            cx,
        ),
        None => path.remove_raw(path.keys_for("padding"), bar.clone(), cx),
    }
    for key in ["segments.gap", "segments.inset", "segments.padding"] {
        path.remove_raw(path.keys_for(key), bar.clone(), cx);
    }

    let mut theme = cx.theme().clone();
    theme.set_radius_scale(look.radius_scale());
    apply_theme(theme, cx);
}

/// The three looks stacked, each a card you click to apply it; the current
/// one is outlined in accent. This is the Look row's whole control.
pub fn look_preview(path: &ConfigPath<BarConfig>, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let current = Look::current(path.read(cx), cx.theme().radius_scale);
    v_flex()
        .w_full()
        .gap(Spacing::Medium.pixels())
        .children(Look::ALL.into_iter().map(|look| {
            let is_current = current == Some(look);
            let path = path.clone();
            v_flex()
                .id(SharedString::from(format!("look-{}", look.label())))
                .w_full()
                .gap(Spacing::XSmall.pixels())
                .p(Spacing::Medium.pixels())
                .rounded(card_radius())
                .border_1()
                .border_color(if is_current {
                    colors.accent
                } else {
                    colors.border_variant
                })
                .bg(colors.surface_background)
                .cursor_pointer()
                .hover(|s| s.bg(colors.element_hover))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    apply_look(look, &path, cx);
                })
                .child(
                    h_flex()
                        .gap(Spacing::Small.pixels())
                        .child(
                            Label::new(look.label())
                                .size(TextSize::Small)
                                .weight(FontWeight::SEMIBOLD)
                                .color(if is_current {
                                    Color::Accent
                                } else {
                                    Color::Default
                                }),
                        )
                        .child(
                            div().flex_1().min_w(px(0.)).child(
                                Label::new(look.description())
                                    .size(TextSize::XSmall)
                                    .color(Color::Muted)
                                    .truncate(),
                            ),
                        )
                        .child(if is_current {
                            Icon::new(IconName::Check)
                                .size(IconSize::Small)
                                .color(Color::Accent)
                                .into_any_element()
                        } else {
                            Label::new("Use")
                                .size(TextSize::XSmall)
                                .color(Color::Muted)
                                .into_any_element()
                        }),
                )
                .child(preview(look, cx))
        }))
        .into_any_element()
}

/// The bar as `look` would draw it, at the preset's own radius scale so
/// the picture does not depend on the theme the user is leaving.
fn preview(look: Look, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let scale = look.radius_scale();
    let radius = |token: Radius| px(token.scaled(scale));
    let bar = px(32.);
    let rhythm = BarRhythm::derive(bar);
    let (chip_bg, chip_border): (Hsla, Option<Hsla>) = match look {
        Look::Floating => (colors.surface_background, Some(colors.border)),
        Look::Blocky | Look::Rounded => (gpui::transparent_black(), None),
    };
    let chip_geometry = |grouped: bool| match look {
        Look::Blocky => ChipGeometry::stretched(&rhythm, grouped, radius(Radius::Large)),
        Look::Rounded | Look::Floating => {
            ChipGeometry::derive(&rhythm, grouped, radius(Radius::Large))
        }
    };
    let chip = |content: AnyElement, grouped: bool| {
        BarChip::new(content)
            .geometry(chip_geometry(grouped))
            .background(chip_bg)
            .border(chip_border)
            .into_any_element()
    };
    let stat = |icon: IconName, text: &'static str| {
        h_flex()
            .gap(rhythm.breath)
            .child(Icon::new(icon).size(IconSize::Small))
            .child(Label::new(text).size(TextSize::Small))
            .into_any_element()
    };
    let cell_side = rhythm.chip() - rhythm.unit * 2.;
    let cell = |label: &'static str, active: bool| {
        let side = if look == Look::Blocky { bar } else { cell_side };
        div()
            .flex()
            .items_center()
            .justify_center()
            .h(side)
            .min_w(if active && look != Look::Blocky {
                px((f32::from(side) * 1.4).round())
            } else {
                side
            })
            .px(rhythm.unit)
            .rounded(match look {
                Look::Blocky => radius(Radius::Large),
                Look::Rounded => radius(Radius::Full).min(side / 2.),
                Look::Floating => px(Radius::Large.scaled(scale)) - rhythm.unit,
            })
            .bg(if active {
                colors.accent
            } else {
                colors.elevated_surface_background
            })
            .child(
                Label::new(label)
                    .size(TextSize::Small)
                    .color(Color::Custom(if active {
                        colors.background
                    } else {
                        colors.text
                    })),
            )
    };
    let cells = chip(
        h_flex()
            .gap(if look == Look::Blocky {
                px(1.)
            } else {
                rhythm.breath
            })
            .child(cell("1", false))
            .child(cell("2", true))
            .child(cell("3", false))
            .into_any_element(),
        true,
    );

    // Bubbles put the fill on the segments; the other looks on the bar.
    let (bar_fill, segment_fill, inset, gap, segment_padding) = match look {
        Look::Rounded => (
            gpui::transparent_black(),
            Some(colors.background),
            px(3.),
            px(6.),
            px(4.),
        ),
        Look::Blocky => (colors.background, None, px(0.), px(0.), rhythm.breath),
        Look::Floating => (colors.background, None, px(0.), px(0.), px(0.)),
    };
    let segment = |children: Vec<AnyElement>| {
        BarSegment::new(false)
            .fill(segment_fill)
            .border(if look == Look::Rounded {
                Some(colors.border)
            } else {
                None
            })
            .radius(px(Radius::Full.scaled(scale)).min((bar - inset * 2.) / 2.))
            .thickness(Some(bar - inset * 2.))
            .padding(segment_padding)
            .children(children)
            .into_any_element()
    };
    let shape = match look {
        Look::Floating => BarShape::Floating {
            radius: radius(Radius::XLarge),
        },
        Look::Blocky | Look::Rounded => BarShape::Attached,
    };

    div()
        .w_full()
        .p(Spacing::Medium.pixels())
        .rounded(Radius::Medium.pixels())
        .bg(colors.surface_background)
        .child(
            div().w_full().h(bar).child(
                BarSurface::new(BarEdge::Top)
                    .shape(shape)
                    .border(look == Look::Floating)
                    .background(bar_fill)
                    .padding(match look {
                        Look::Rounded => gap,
                        Look::Blocky => px(0.),
                        Look::Floating => rhythm.unit,
                    })
                    .inset(inset)
                    .segment_gap(gap)
                    .start([
                        segment(vec![chip(
                            Icon::new(IconName::Layers)
                                .size(IconSize::Small)
                                .into_any_element(),
                            false,
                        )]),
                        segment(vec![cells]),
                        segment(vec![chip(stat(IconName::Cpu, "12%"), false)]),
                    ])
                    .end([
                        segment(vec![chip(stat(IconName::Wifi, "Home"), false)]),
                        segment(vec![chip(stat(IconName::Clock, "09:41"), false)]),
                    ]),
            ),
        )
        .into_any_element()
}
