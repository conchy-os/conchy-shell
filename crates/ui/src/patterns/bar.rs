//! Status bar chrome: the edge-anchored surface and the chip each widget sits in.

use gpui::{
    AnyElement, App, FontWeight, Hsla, IntoElement, Pixels, RenderOnce, Window, div, prelude::*, px,
};

use crate::{ActiveTheme, Radius, Spacing, TextSize};

/// Which screen edge the bar is anchored to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarEdge {
    Top,
    Bottom,
    Left,
    Right,
}

impl BarEdge {
    pub fn is_vertical(self) -> bool {
        matches!(self, BarEdge::Left | BarEdge::Right)
    }
}

/// How the bar meets the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BarShape {
    /// Flush with the screen edge; the border is the screen-facing hairline.
    Attached,
    /// Inset from the edges; rounded and bordered all around.
    Floating { radius: Pixels },
    /// One side of a [`ScreenFrame`](super::ScreenFrame), which draws the
    /// border, so the bar draws none.
    Framed,
}

/// Where a section sits along the bar's long axis.
#[derive(Clone, Copy)]
enum Align {
    Start,
    Center,
    End,
}

/// The bar itself: a full-bleed surface with three widget sections.
#[derive(IntoElement)]
#[must_use = "BarSurface does nothing unless rendered"]
pub struct BarSurface {
    edge: BarEdge,
    shape: BarShape,
    border: bool,
    background: Option<Hsla>,
    padding: Pixels,
    inset: Pixels,
    segment_gap: Pixels,
    start: Vec<AnyElement>,
    center: Vec<AnyElement>,
    end: Vec<AnyElement>,
}

impl BarSurface {
    /// A vertical bar keeps this much between its long edges and the
    /// segments, on top of any inset, so full-width chips clear the hairline.
    pub const VERTICAL_EDGE_PADDING: Pixels = px(1.);

    pub fn new(edge: BarEdge) -> Self {
        Self {
            edge,
            shape: BarShape::Attached,
            border: false,
            background: None,
            padding: px(0.),
            inset: px(0.),
            segment_gap: px(0.),
            start: Vec::new(),
            center: Vec::new(),
            end: Vec::new(),
        }
    }

    pub fn shape(mut self, shape: BarShape) -> Self {
        self.shape = shape;
        self
    }

    /// Draw the border the shape calls for.
    pub fn border(mut self, border: bool) -> Self {
        self.border = border;
        self
    }

    /// Fill in place of the theme background, e.g. a translucent one.
    pub fn background(mut self, background: Hsla) -> Self {
        self.background = Some(background);
        self
    }

    /// Padding along the long axis.
    pub fn padding(mut self, padding: Pixels) -> Self {
        self.padding = padding;
        self
    }

    /// Padding across the bar, between its long edges and the segments.
    pub fn inset(mut self, inset: Pixels) -> Self {
        self.inset = inset;
        self
    }

    /// Space between neighbouring segments in a section.
    pub fn segment_gap(mut self, gap: Pixels) -> Self {
        self.segment_gap = gap;
        self
    }

    pub fn start(mut self, widgets: impl IntoIterator<Item = AnyElement>) -> Self {
        self.start.extend(widgets);
        self
    }

    pub fn center(mut self, widgets: impl IntoIterator<Item = AnyElement>) -> Self {
        self.center.extend(widgets);
        self
    }

    pub fn end(mut self, widgets: impl IntoIterator<Item = AnyElement>) -> Self {
        self.end.extend(widgets);
        self
    }
}

impl RenderOnce for BarSurface {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors();
        let vertical = self.edge.is_vertical();
        let edge = self.edge;
        // A floating bar's border runs all around; only an attached one has an edge hairline.
        let border = self.border && self.shape == BarShape::Attached;

        let root = div()
            .size_full()
            .flex()
            .text_size(TextSize::Small.rems())
            .font_weight(FontWeight::MEDIUM)
            .text_color(colors.text)
            .bg(self.background.unwrap_or_else(|| theme.surface(None)))
            .border_color(colors.border)
            .when_some(
                match self.shape {
                    BarShape::Floating { radius } => Some(radius),
                    BarShape::Attached | BarShape::Framed => None,
                },
                |el, radius| el.rounded(radius).when(self.border, |el| el.border_1()),
            );

        let root = if vertical {
            root.flex_col()
                .items_center()
                .px(Self::VERTICAL_EDGE_PADDING + self.inset)
                .py(Spacing::Medium.pixels())
                .when(border && edge == BarEdge::Left, |el| el.border_r_1())
                .when(border && edge == BarEdge::Right, |el| el.border_l_1())
        } else {
            root.items_center()
                .px(self.padding)
                .py(self.inset)
                .when(border && edge == BarEdge::Top, |el| el.border_b_1())
                .when(border && edge == BarEdge::Bottom, |el| el.border_t_1())
        };

        let gap = self.segment_gap;
        root.child(section(vertical, Align::Start, gap, self.start))
            .child(section(vertical, Align::Center, gap, self.center))
            .child(section(vertical, Align::End, gap, self.end))
    }
}

fn section(
    vertical: bool,
    align: Align,
    gap: Pixels,
    segments: Vec<AnyElement>,
) -> impl IntoElement {
    if vertical {
        // A vertical bar sizes to its widgets, so only the centre section
        // claims the leftover space; start and end stay at their content
        // height and hug their ends.
        div()
            .flex()
            .w_full()
            .flex_col()
            .items_center()
            .gap(gap)
            .map(|el| match align {
                Align::Start => el.justify_start(),
                Align::Center => el.flex_1().justify_center(),
                Align::End => el.justify_end(),
            })
            .children(segments)
    } else {
        div()
            .flex()
            .h_full()
            .items_center()
            .flex_1()
            .gap(gap)
            .map(|el| match align {
                Align::Start => el.justify_start(),
                Align::Center => el.justify_center().overflow_hidden(),
                Align::End => el.justify_end(),
            })
            .children(segments)
    }
}

/// One entry of a bar section: the box one or more widgets share. With no
/// fill, border or padding it is invisible, which is how an attached bar
/// lays its widgets out; a bubbles bar gives it the bar's fill and a pill
/// radius. The bar hands it an explicit thickness across the bar so what is
/// inside can be sized exactly.
#[derive(IntoElement)]
#[must_use = "BarSegment does nothing unless rendered"]
pub struct BarSegment {
    vertical: bool,
    fill: Option<Hsla>,
    border: Option<Hsla>,
    radius: Pixels,
    thickness: Option<Pixels>,
    padding: Pixels,
    children: Vec<AnyElement>,
}

impl BarSegment {
    pub fn new(vertical: bool) -> Self {
        Self {
            vertical,
            fill: None,
            border: None,
            radius: px(0.),
            thickness: None,
            padding: px(0.),
            children: Vec::new(),
        }
    }

    pub fn fill(mut self, fill: Option<Hsla>) -> Self {
        self.fill = fill;
        self
    }

    pub fn border(mut self, border: Option<Hsla>) -> Self {
        self.border = border;
        self
    }

    pub fn radius(mut self, radius: Pixels) -> Self {
        self.radius = radius;
        self
    }

    /// Size across the bar; `None` fills the section.
    pub fn thickness(mut self, thickness: Option<Pixels>) -> Self {
        self.thickness = thickness;
        self
    }

    /// Room inside the segment's two ends.
    pub fn padding(mut self, padding: Pixels) -> Self {
        self.padding = padding;
        self
    }
}

impl ParentElement for BarSegment {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for BarSegment {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_center()
            .map(|el| {
                if self.vertical {
                    el.flex_col()
                        .py(self.padding)
                        .map(|el| match self.thickness {
                            Some(thickness) => el.w(thickness),
                            None => el.w_full(),
                        })
                } else {
                    el.px(self.padding).map(|el| match self.thickness {
                        Some(thickness) => el.h(thickness),
                        None => el.h_full(),
                    })
                }
            })
            .rounded(self.radius)
            .when_some(self.fill, |el, fill| el.bg(fill))
            .when_some(self.border, |el, color| el.border_1().border_color(color))
            .children(self.children)
    }
}

/// The bar's spacing grid, derived from the thickness a segment has across
/// it. One unit sets every other distance, so a bar of any size reads as one
/// rhythm:
///
/// | name        | ratio                  | 32px bar |
/// |-------------|------------------------|----------|
/// | `unit`      | thickness / 16, 1..=4  | 2        |
/// | `breath`    | 2 × unit               | 4        |
/// | chip        | thickness − 2 × breath | 24       |
/// | chip ends   | 2 × breath (single), breath + unit (group), unit when the chip draws no chrome | 8 / 6 / 2 |
/// | icon        | chip × 2 / 3           | 16       |
/// | cell        | chip − 2 × unit        | 20       |
/// | icon gap    | 2 × breath             | 8        |
///
/// `breath` is the space above a chip, between two chips (one `unit` of
/// margin each) and between the first chip and the bar's end (its `unit` of
/// padding plus the chip's margin). `unit` is also the step inner corners
/// shrink by to stay concentric.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarRhythm {
    /// What a segment gets across the bar.
    pub thickness: Pixels,
    pub unit: Pixels,
    pub breath: Pixels,
}

impl BarRhythm {
    pub fn derive(thickness: Pixels) -> Self {
        let unit = px((f32::from(thickness) / 16.).round().clamp(1., 4.));
        Self {
            thickness,
            unit,
            breath: unit * 2.,
        }
    }

    /// Cross-axis size of a chip that breathes on both sides.
    pub fn chip(&self) -> Pixels {
        (self.thickness - self.breath * 2.).max(px(0.))
    }

    /// Icon size for this bar, in whole pixels.
    pub fn icon(&self) -> Pixels {
        px((f32::from(self.chip()) * 2. / 3.).round())
    }

    /// Distance between the icons of a widget that shows several.
    pub fn icon_gap(&self) -> Pixels {
        self.breath * 2.
    }
}

/// The box a [`BarChip`] draws, in the bar's own axes: *main* runs along the
/// bar, *cross* across it. The bar derives one per orientation from its
/// [`BarRhythm`]; [`standard`](Self::standard) is the shape at the default
/// bar size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChipGeometry {
    /// Padding at the chip's two ends along the bar.
    pub padding_main: Pixels,
    /// Padding between the content and the bar's long edges.
    pub padding_cross: Pixels,
    /// Fixed thickness across the bar; `None` sizes to the content.
    pub thickness: Option<Pixels>,
    pub radius: Pixels,
    /// Breathing room around the chip.
    pub margin: Pixels,
}

impl ChipGeometry {
    /// A chip on the rhythm's grid: `breath` from the bar's long edges, a
    /// `unit` of margin at each end so two chips sit `breath` apart, and
    /// content padded by a `unit` across. `grouped` chips (workspaces, tray)
    /// hold items with their own padding, so their ends are tighter.
    pub fn derive(rhythm: &BarRhythm, grouped: bool, radius: Pixels) -> Self {
        Self {
            padding_main: if grouped {
                rhythm.breath + rhythm.unit
            } else {
                rhythm.breath * 2.
            },
            padding_cross: rhythm.unit,
            thickness: Some(rhythm.chip()),
            radius,
            margin: rhythm.unit,
        }
    }

    /// A chip that fills its segment: the full thickness, no margin and no
    /// cross padding, so neighbours sit flush. The ends keep their padding
    /// so content still breathes along the bar.
    pub fn stretched(rhythm: &BarRhythm, grouped: bool, radius: Pixels) -> Self {
        Self {
            padding_cross: px(0.),
            thickness: Some(rhythm.thickness),
            margin: px(0.),
            ..Self::derive(rhythm, grouped, radius)
        }
    }

    /// The chip of a 32px bar, for stories and callers without a bar.
    pub fn standard(vertical: bool, grouped: bool) -> Self {
        let rhythm = BarRhythm::derive(Self::STANDARD_THICKNESS);
        let radius = if vertical {
            Radius::Small.pixels()
        } else {
            Radius::Large.pixels()
        };
        Self::derive(&rhythm, grouped, radius)
    }

    /// What a 32px bar leaves a segment after its hairline.
    pub const STANDARD_THICKNESS: Pixels = px(31.);
}

/// The pill a bar widget lives in, plus the breathing room around it.
#[derive(IntoElement)]
#[must_use = "BarChip does nothing unless rendered"]
pub struct BarChip {
    content: AnyElement,
    vertical: bool,
    grouped: bool,
    geometry: Option<ChipGeometry>,
    background: Hsla,
    border: Option<Hsla>,
    hover: Option<Hsla>,
}

impl BarChip {
    pub fn new(content: impl IntoElement) -> Self {
        Self {
            content: content.into_any_element(),
            vertical: false,
            grouped: false,
            geometry: None,
            background: gpui::transparent_black(),
            border: None,
            hover: None,
        }
    }

    pub fn vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }

    /// Wider inner padding, for widgets that hold several items (workspaces,
    /// tray) rather than one icon and a value.
    pub fn grouped(mut self, grouped: bool) -> Self {
        self.grouped = grouped;
        self
    }

    /// The box to draw, in place of [`ChipGeometry::standard`].
    pub fn geometry(mut self, geometry: ChipGeometry) -> Self {
        self.geometry = Some(geometry);
        self
    }

    pub fn background(mut self, background: Hsla) -> Self {
        self.background = background;
        self
    }

    pub fn border(mut self, border: Option<Hsla>) -> Self {
        self.border = border;
        self
    }

    /// Setting a hover colour also makes the chip look clickable.
    pub fn hover(mut self, hover: Option<Hsla>) -> Self {
        self.hover = hover;
        self
    }
}

impl RenderOnce for BarChip {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let vertical = self.vertical;
        let hover = self.hover;
        let geometry = self
            .geometry
            .unwrap_or_else(|| ChipGeometry::standard(vertical, self.grouped));

        let chip = div()
            .map(|el| {
                if vertical {
                    el.w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .py(geometry.padding_main)
                        .px(geometry.padding_cross)
                } else {
                    el.flex()
                        .items_center()
                        .px(geometry.padding_main)
                        .py(geometry.padding_cross)
                }
            })
            .when_some(geometry.thickness, |el, thickness| {
                if vertical {
                    el.w(thickness)
                } else {
                    el.h(thickness)
                }
            })
            .justify_center()
            .rounded(geometry.radius)
            .bg(self.background)
            .when_some(self.border, |el, color| el.border_1().border_color(color))
            .when_some(hover, |el, color| {
                el.cursor_pointer().hover(move |style| style.bg(color))
            })
            .child(self.content);

        if vertical {
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .py(geometry.margin)
                .child(chip)
        } else {
            div().mx(geometry.margin).my(geometry.margin).child(chip)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rhythm table in the docs, pinned: a 32px bar with a hairline
    /// gives unit 2, breath 4, a 23px chip and 16px icons.
    #[test]
    fn rhythm_of_the_default_bar() {
        let rhythm = BarRhythm::derive(px(31.));
        assert_eq!((rhythm.unit, rhythm.breath), (px(2.), px(4.)));
        assert_eq!(rhythm.chip(), px(23.));
        assert_eq!(rhythm.icon(), px(15.));
        assert_eq!(rhythm.icon_gap(), px(8.));

        let borderless = BarRhythm::derive(px(32.));
        assert_eq!(borderless.chip(), px(24.));
        assert_eq!(borderless.icon(), px(16.));

        // The unit is clamped: a tiny bar still breathes, a huge one does
        // not drown its content.
        assert_eq!(BarRhythm::derive(px(12.)).unit, px(1.));
        assert_eq!(BarRhythm::derive(px(120.)).unit, px(4.));
    }

    #[test]
    fn chip_geometry_follows_the_rhythm() {
        let rhythm = BarRhythm::derive(px(32.));
        let single = ChipGeometry::derive(&rhythm, false, px(8.));
        assert_eq!(
            (
                single.padding_main,
                single.padding_cross,
                single.thickness,
                single.margin
            ),
            (px(8.), px(2.), Some(px(24.)), px(2.))
        );
        assert_eq!(
            ChipGeometry::derive(&rhythm, true, px(8.)).padding_main,
            px(6.)
        );

        let flush = ChipGeometry::stretched(&rhythm, false, px(0.));
        assert_eq!(flush.thickness, Some(px(32.)));
        assert_eq!((flush.margin, flush.padding_cross), (px(0.), px(0.)));
        assert_eq!(flush.padding_main, single.padding_main);
    }
}
