//! Bar - the edge-anchored status surface, the segments in it and the chips
//! inside those.
//!
//! Shown at both orientations, because that is where bar work goes wrong:
//! the same widget has to read as a pill on a horizontal bar and as a stack
//! on a vertical one. The last two examples are the spec's reference bars:
//! blocky (square, flush, full-height) and rounded (pills with gaps) - one
//! model, two configs.

use gpui::{AnyElement, Bounds, Edges, Hsla, Pixels, div, point, size};
use ui::patterns::{
    BarChip, BarEdge, BarRhythm, BarSegment, BarShape, BarSurface, ChipGeometry, ScreenFrame,
};

use crate::prelude::*;

const BAR: Pixels = px(32.);

/// Width of the vertical example bar and what it leaves a segment.
const VERTICAL_BAR: Pixels = px(44.);
const VERTICAL_THICKNESS: Pixels = px(41.);

/// Stand-in widgets. The real ones are entities wired to services; what the
/// pattern sees is an element. Vertical chips take their geometry from the
/// vertical bar's own rhythm, as the shell derives it.
fn chip(icon: IconName, label: &'static str, vertical: bool, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    BarChip::new(content(icon, label, vertical))
        .vertical(vertical)
        .when(vertical, |chip| {
            chip.geometry(ChipGeometry::derive(
                &BarRhythm::derive(VERTICAL_THICKNESS),
                false,
                Radius::Small.pixels(),
            ))
        })
        .background(colors.surface_background)
        .border(Some(colors.border))
        .hover(Some(colors.element_hover))
        .into_any_element()
}

fn content(icon: IconName, label: &'static str, vertical: bool) -> AnyElement {
    if vertical {
        v_flex()
            .items_center()
            .gap(px(2.))
            .whitespace_nowrap()
            .child(Icon::new(icon).size(IconSize::Small))
            .child(Label::new(label).size(TextSize::XSmall))
            .into_any_element()
    } else {
        h_flex()
            .items_center()
            .gap(px(4.))
            .child(Icon::new(icon).size(IconSize::Small))
            .child(Label::new(label).size(TextSize::Small))
            .into_any_element()
    }
}

fn group(icons: &[IconName], vertical: bool, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let dots = icons
        .iter()
        .map(|icon| Icon::new(*icon).size(IconSize::Small).into_any_element());

    BarChip::new(
        div()
            .flex()
            .when(vertical, |el| el.flex_col())
            .items_center()
            .gap(px(3.))
            .children(dots),
    )
    .vertical(vertical)
    .grouped(true)
    .when(vertical, |chip| {
        chip.geometry(ChipGeometry::derive(
            &BarRhythm::derive(VERTICAL_THICKNESS),
            true,
            Radius::Small.pixels(),
        ))
    })
    .background(colors.surface_background)
    .border(Some(colors.border))
    .into_any_element()
}

/// An invisible segment: what every style but bubbles wraps its entries in.
fn flush(widgets: impl IntoIterator<Item = AnyElement>) -> AnyElement {
    BarSegment::new(false).children(widgets).into_any_element()
}

fn horizontal_bar(cx: &App) -> AnyElement {
    div()
        .w_full()
        .h(BAR)
        .child(
            BarSurface::new(BarEdge::Top)
                .border(true)
                .padding(BarRhythm::derive(ChipGeometry::STANDARD_THICKNESS).unit)
                .start([flush([
                    chip(IconName::Layout, "1", false, cx),
                    group(&[IconName::Cpu, IconName::MemoryStick], false, cx),
                ])])
                .center([flush([chip(IconName::Layers, "zed - bar.rs", false, cx)])])
                .end([flush([
                    chip(IconName::Volume, "62%", false, cx),
                    chip(IconName::Battery, "84%", false, cx),
                    chip(IconName::Clock, "09:41", false, cx),
                ])]),
        )
        .into_any_element()
}

fn vertical_bar(cx: &App) -> AnyElement {
    div()
        .w(VERTICAL_BAR)
        .h(px(320.))
        .child(
            BarSurface::new(BarEdge::Left)
                .border(true)
                .start([
                    chip(IconName::Layout, "1", true, cx),
                    group(&[IconName::Cpu, IconName::MemoryStick], true, cx),
                ])
                .end([
                    chip(IconName::Volume, "62", true, cx),
                    chip(IconName::Clock, "9:41", true, cx),
                ]),
        )
        .into_any_element()
}

fn floating_bar(cx: &App) -> AnyElement {
    div()
        .w_full()
        .h(px(48.))
        .p(px(8.))
        .child(
            BarSurface::new(BarEdge::Top)
                .shape(BarShape::Floating {
                    radius: Radius::XLarge.pixels(),
                })
                .border(true)
                .padding(BarRhythm::derive(ChipGeometry::STANDARD_THICKNESS).unit)
                .start([chip(IconName::Layout, "1", false, cx)])
                .end([chip(IconName::Clock, "09:41", false, cx)]),
        )
        .into_any_element()
}

/// A bubbles bar with the spec's default segment knobs: gap 6, inset 3,
/// padding 4, pill radius.
fn bubbles_bar(cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let inset = px(3.);
    let bubble = || {
        BarSegment::new(false)
            .fill(Some(cx.theme().surface(None)))
            .border(Some(colors.border))
            .radius(Radius::Full.pixels())
            .thickness(Some(BAR - inset * 2.))
            .padding(px(4.))
    };
    div()
        .w_full()
        .h(BAR)
        .bg(colors.surface_background)
        .child(
            BarSurface::new(BarEdge::Top)
                .background(gpui::transparent_black())
                .padding(px(8.))
                .inset(inset)
                .segment_gap(px(6.))
                .start([
                    bubble()
                        .child(chip(IconName::Layout, "1", false, cx))
                        .into_any_element(),
                    bubble()
                        .child(chip(IconName::Cpu, "12%", false, cx))
                        .child(chip(IconName::MemoryStick, "38%", false, cx))
                        .into_any_element(),
                ])
                .end([bubble()
                    .child(chip(IconName::Clock, "09:41", false, cx))
                    .into_any_element()]),
        )
        .into_any_element()
}

fn framed_screen(cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    div()
        .relative()
        .w(px(480.))
        .h(px(270.))
        .bg(colors.surface_background)
        .child(
            div().absolute().size_full().child(
                // The bar is the top side; it is laid out inside the two
                // side bands, so the frame paints the corners under it.
                ScreenFrame::new(cx.theme().surface(None))
                    .depths(Edges {
                        top: BAR,
                        ..Edges::all(px(6.))
                    })
                    .cutouts(vec![Bounds::new(
                        point(px(6.), px(0.)),
                        size(px(480. - 12.), BAR),
                    )])
                    .radius(Radius::XXLarge.pixels())
                    .border(Some(colors.border)),
            ),
        )
        .child(
            div().absolute().left(px(6.)).right(px(6.)).h(BAR).child(
                BarSurface::new(BarEdge::Top)
                    .shape(BarShape::Framed)
                    .padding(BarRhythm::derive(ChipGeometry::STANDARD_THICKNESS).unit)
                    .start([chip(IconName::Layout, "1", false, cx)])
                    .end([chip(IconName::Clock, "09:41", false, cx)]),
            ),
        )
        .into_any_element()
}

/// A chip with no background or border - what `widget_background = false`
/// and `widget_border = false` in the bar config look like.
fn bare_chips(cx: &App) -> AnyElement {
    let transparent: Option<Hsla> = None;
    div()
        .flex()
        .items_center()
        .child(
            BarChip::new(
                h_flex()
                    .items_center()
                    .gap(px(4.))
                    .child(Icon::new(IconName::Wifi).size(IconSize::Small))
                    .child(Label::new("Home").size(TextSize::Small)),
            )
            .border(transparent),
        )
        .child(
            BarChip::new(Label::new("09:41").size(TextSize::Small))
                .hover(Some(cx.theme().colors().element_hover)),
        )
        .into_any_element()
}

/// Reference A: an attached bar with no border, `widget_stretch = true` and
/// bare chips, at `radius_scale = 0`. Chips fill the bar's thickness and
/// touch; workspace cells are full-height squares.
fn blocky_bar(cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let rhythm = BarRhythm::derive(BAR);
    let geometry = ChipGeometry::stretched(&rhythm, false, Radius::Large.pixels());
    let stretched = |content: AnyElement, fill: Hsla| {
        BarChip::new(content)
            .geometry(geometry)
            .background(fill)
            .into_any_element()
    };
    let cell = |label: &'static str, active: bool, occupied: bool| {
        div()
            .flex()
            .items_center()
            .justify_center()
            .size(BAR)
            .rounded(Radius::Small.pixels())
            .bg(if active {
                colors.accent
            } else if occupied {
                colors.elevated_surface_background
            } else {
                gpui::transparent_black()
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
    let cells = BarChip::new(
        h_flex()
            .child(cell("1", false, true))
            .child(cell("2", true, true))
            .child(cell("3", false, false))
            .child(cell("4", false, false)),
    )
    .geometry(ChipGeometry::stretched(
        &rhythm,
        true,
        Radius::Large.pixels(),
    ))
    .into_any_element();
    let icons = |names: &[IconName]| {
        h_flex()
            .gap(px(8.))
            .children(
                names
                    .iter()
                    .map(|name| Icon::new(*name).size(IconSize::Medium)),
            )
            .into_any_element()
    };

    div()
        .w_full()
        .h(BAR)
        .child(
            BarSurface::new(BarEdge::Top)
                .border(false)
                .start([flush([
                    stretched(
                        Icon::new(IconName::Layers)
                            .size(IconSize::Medium)
                            .color(Color::Custom(colors.background))
                            .into_any_element(),
                        colors.accent,
                    ),
                    cells,
                    stretched(
                        content(IconName::Cpu, "12%", false),
                        gpui::transparent_black(),
                    ),
                ])])
                .center([flush([stretched(
                    content(
                        IconName::Globe,
                        "FIREFOX  gpui-shell/crates/app/src/bar",
                        false,
                    ),
                    gpui::transparent_black(),
                )])])
                .end([
                    flush([stretched(
                        content(IconName::Play, "Boards of Canada", false),
                        colors.elevated_surface_background,
                    )]),
                    flush([stretched(
                        content(IconName::Bell, "3", false),
                        colors.status.error_background,
                    )]),
                    flush([stretched(
                        icons(&[
                            IconName::Wifi,
                            IconName::Volume,
                            IconName::Bluetooth,
                            IconName::Battery,
                        ]),
                        gpui::transparent_black(),
                    )]),
                    flush([stretched(
                        content(IconName::Clock, "MON 19 OCT  14:32", false),
                        colors.surface_background,
                    )]),
                ]),
        )
        .into_any_element()
}

/// Reference B: a bubbles bar with bare chips at `radius_scale = 1`. Each
/// entry is a pill, groups share one, and the launcher is an accent disc.
fn rounded_bar(cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let inset = px(3.);
    let thickness = BAR - inset * 2.;
    let pill = || {
        BarSegment::new(false)
            .fill(Some(cx.theme().surface(None)))
            .radius(Radius::Full.pixels())
            .thickness(Some(thickness))
            .padding(px(4.))
    };
    let bare = |content: AnyElement| BarChip::new(content).into_any_element();
    let dot = |active: bool| {
        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(18.))
            .w(if active { px(30.) } else { px(18.) })
            .rounded(Radius::Full.pixels())
            .bg(if active {
                colors.accent
            } else {
                colors.elevated_surface_background
            })
            .when(active, |el| {
                el.child(
                    Label::new("3")
                        .size(TextSize::XSmall)
                        .color(Color::Custom(colors.background)),
                )
            })
    };
    let icons = |names: &[IconName]| {
        h_flex()
            .gap(px(8.))
            .children(
                names
                    .iter()
                    .map(|name| Icon::new(*name).size(IconSize::Small)),
            )
            .into_any_element()
    };

    div()
        .w_full()
        .h(BAR)
        .bg(colors.surface_background)
        .child(
            BarSurface::new(BarEdge::Top)
                .background(gpui::transparent_black())
                .padding(px(8.))
                .inset(inset)
                .segment_gap(px(6.))
                .start([
                    BarSegment::new(false)
                        .fill(Some(colors.accent))
                        .radius(Radius::Full.pixels())
                        .thickness(Some(thickness))
                        .padding(px(6.))
                        .child(
                            Icon::new(IconName::Layers)
                                .size(IconSize::Small)
                                .color(Color::Custom(colors.background)),
                        )
                        .into_any_element(),
                    pill()
                        .child(bare(
                            h_flex()
                                .gap(px(4.))
                                .child(dot(false))
                                .child(dot(false))
                                .child(dot(true))
                                .child(dot(false))
                                .into_any_element(),
                        ))
                        .into_any_element(),
                    pill()
                        .child(bare(content(IconName::Cpu, "12%", false)))
                        .child(bare(content(IconName::MemoryStick, "38%", false)))
                        .into_any_element(),
                ])
                .center([pill()
                    .child(bare(content(
                        IconName::Terminal,
                        "Ghostty  ~/src/gpui-shell",
                        false,
                    )))
                    .into_any_element()])
                .end([
                    pill()
                        .child(bare(icons(&[
                            IconName::Inbox,
                            IconName::Folder,
                            IconName::Moon,
                        ])))
                        .child(bare(
                            Label::new("US").size(TextSize::Small).into_any_element(),
                        ))
                        .into_any_element(),
                    pill()
                        .child(bare(icons(&[
                            IconName::Wifi,
                            IconName::Volume,
                            IconName::Bluetooth,
                        ])))
                        .child(bare(content(IconName::Battery, "99%", false)))
                        .into_any_element(),
                    pill()
                        .child(bare(content(
                            IconName::Calendar,
                            "Mon 19 Oct  14:32",
                            false,
                        )))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(20.))
                                .rounded(Radius::Full.pixels())
                                .bg(colors.status.error)
                                .child(
                                    Icon::new(IconName::Bell)
                                        .size(IconSize::XSmall)
                                        .color(Color::Custom(colors.background)),
                                ),
                        )
                        .into_any_element(),
                ]),
        )
        .into_any_element()
}

pub struct BarStory;

impl Render for BarStory {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap(Spacing::XLarge.pixels())
            .child(example_group(
                "Bar",
                vec![
                    example("Horizontal", horizontal_bar(cx)).description(
                        "Start, centre and end sections; only the screen edge is bordered.",
                    ),
                    example("Vertical", vertical_bar(cx))
                        .description("Chips stack, radii tighten, labels drop a size."),
                    example("Floating", floating_bar(cx))
                        .description("Inset from the edges, rounded and bordered all around."),
                    example("Bubbles", bubbles_bar(cx)).description(
                        "A transparent bar; each entry is a pill segment, groups share one.",
                    ),
                    example("Frame", framed_screen(cx)).description(
                        "The bar carried around the screen, with rounded inner corners.",
                    ),
                    example("Chip without chrome", bare_chips(cx)).description(
                        "Background and border are the app's call, so both are optional.",
                    ),
                ],
            ))
            .child(example_group(
                "Reference looks",
                vec![
                    example("Blocky (reference A)", blocky_bar(cx)).description(
                        "attached, no border, widget_stretch, radius_scale 0: flush full-height \
                         segments and square cells. Set Radius to 0 in the sidebar.",
                    ),
                    example("Rounded (reference B)", rounded_bar(cx)).description(
                        "bubbles, bare chips, radius_scale 1: pills with gaps and a small inset \
                         from the bar's edges.",
                    ),
                ],
            ))
    }
}

pub fn build(_window: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_cx| BarStory).into()
}
