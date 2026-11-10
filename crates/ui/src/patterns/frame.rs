//! Screen frame: a band around the work area with rounded inner corners.
//! Shell surfaces on an edge - the bar, a taskbar dock - become that side
//! of the frame: the frame's hole stops at them and it leaves their strips
//! unpainted, so a translucent surface never doubles up.

use gpui::{
    App, Bounds, Edges, FillOptions, FillRule, Hsla, IntoElement, PathBuilder, PathStyle, Pixels,
    RenderOnce, Window, canvas, point, prelude::*, px, size,
};

use super::BarEdge;

#[derive(IntoElement)]
#[must_use = "ScreenFrame does nothing unless rendered"]
pub struct ScreenFrame {
    depths: Edges<Pixels>,
    cutouts: Vec<Bounds<Pixels>>,
    radius: Pixels,
    background: Hsla,
    border: Option<Hsla>,
}

impl ScreenFrame {
    pub fn new(background: Hsla) -> Self {
        Self {
            depths: Edges::all(px(0.)),
            cutouts: Vec::new(),
            radius: px(0.),
            background,
            border: None,
        }
    }

    /// How deep the frame is on each edge: the band's thickness, or the
    /// shell surface sitting there.
    pub fn depths(mut self, depths: Edges<Pixels>) -> Self {
        self.depths = depths;
        self
    }

    /// Strips another surface paints, which the frame leaves alone.
    pub fn cutouts(mut self, cutouts: Vec<Bounds<Pixels>>) -> Self {
        self.cutouts = cutouts;
        self
    }

    /// Radius of the hole's corners.
    pub fn radius(mut self, radius: Pixels) -> Self {
        self.radius = radius;
        self
    }

    /// Outline the hole.
    pub fn border(mut self, border: Option<Hsla>) -> Self {
        self.border = border;
        self
    }
}

/// `bounds` without the bar's strip on `edge`.
pub fn outside_bar(bounds: Bounds<Pixels>, edge: BarEdge, depth: Pixels) -> Bounds<Pixels> {
    let mut depths = Edges::all(px(0.));
    *edge_mut(&mut depths, edge) = depth;
    frame_hole(bounds, depths)
}

/// The work area a frame `depths` deep on each edge leaves open.
pub fn frame_hole(bounds: Bounds<Pixels>, depths: Edges<Pixels>) -> Bounds<Pixels> {
    Bounds {
        origin: point(bounds.origin.x + depths.left, bounds.origin.y + depths.top),
        size: size(
            (bounds.size.width - depths.left - depths.right).max(px(0.)),
            (bounds.size.height - depths.top - depths.bottom).max(px(0.)),
        ),
    }
}

/// The strip `depth` deep along `edge` of `bounds`, shortened at each end by
/// `insets` on the two adjacent edges: where a surface on `edge` sits once
/// the compositor has laid it out beside what those edges hold.
pub fn edge_strip(
    bounds: Bounds<Pixels>,
    edge: BarEdge,
    depth: Pixels,
    insets: Edges<Pixels>,
) -> Bounds<Pixels> {
    let (left, top) = (bounds.origin.x, bounds.origin.y);
    let (width, height) = (bounds.size.width, bounds.size.height);
    let across = |a: Pixels, b: Pixels, extent: Pixels| (a, (extent - a - b).max(px(0.)));
    match edge {
        BarEdge::Top | BarEdge::Bottom => {
            let (x, w) = across(insets.left, insets.right, width);
            let y = if edge == BarEdge::Top {
                top
            } else {
                top + height - depth
            };
            Bounds::new(point(left + x, y), size(w, depth))
        }
        BarEdge::Left | BarEdge::Right => {
            let (y, h) = across(insets.top, insets.bottom, height);
            let x = if edge == BarEdge::Left {
                left
            } else {
                left + width - depth
            };
            Bounds::new(point(x, top + y), size(depth, h))
        }
    }
}

pub fn edge_mut(edges: &mut Edges<Pixels>, edge: BarEdge) -> &mut Pixels {
    match edge {
        BarEdge::Top => &mut edges.top,
        BarEdge::Right => &mut edges.right,
        BarEdge::Bottom => &mut edges.bottom,
        BarEdge::Left => &mut edges.left,
    }
}

fn rounded_rect(builder: &mut PathBuilder, rect: Bounds<Pixels>, radius: Pixels) {
    let r = radius
        .min(rect.size.width / 2.)
        .min(rect.size.height / 2.)
        .max(px(0.));
    let radii = point(r, r);
    let (left, top) = (rect.origin.x, rect.origin.y);
    let (right, bottom) = (left + rect.size.width, top + rect.size.height);

    builder.move_to(point(left + r, top));
    builder.line_to(point(right - r, top));
    builder.arc_to(radii, px(0.), false, true, point(right, top + r));
    builder.line_to(point(right, bottom - r));
    builder.arc_to(radii, px(0.), false, true, point(right - r, bottom));
    builder.line_to(point(left + r, bottom));
    builder.arc_to(radii, px(0.), false, true, point(left, bottom - r));
    builder.line_to(point(left, top + r));
    builder.arc_to(radii, px(0.), false, true, point(left + r, top));
    builder.close();
}

impl RenderOnce for ScreenFrame {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let hole = frame_hole(bounds, self.depths);
                if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
                    return;
                }

                // Even-odd: the hole and each cut-out, none overlapping,
                // are taken out of the whole screen.
                let mut fill = PathBuilder::fill().with_style(PathStyle::Fill(
                    FillOptions::default().with_fill_rule(FillRule::EvenOdd),
                ));
                rounded_rect(&mut fill, bounds, px(0.));
                rounded_rect(&mut fill, hole, self.radius);
                for cutout in &self.cutouts {
                    if cutout.size.width > px(0.) && cutout.size.height > px(0.) {
                        rounded_rect(&mut fill, *cutout, px(0.));
                    }
                }
                if let Ok(path) = fill.build() {
                    window.paint_path(path, self.background);
                }

                if let Some(color) = self.border {
                    // Centre the hairline on the hole's edge pixel, not between two.
                    let line = Bounds {
                        origin: point(hole.origin.x + px(0.5), hole.origin.y + px(0.5)),
                        size: size(hole.size.width - px(1.), hole.size.height - px(1.)),
                    };
                    let mut stroke = PathBuilder::stroke(px(1.));
                    rounded_rect(&mut stroke, line, self.radius - px(0.5));
                    if let Ok(path) = stroke.build() {
                        window.paint_path(path, color);
                    }
                }
            },
        )
        .size_full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Bounds<Pixels> {
        Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(1000.), px(600.)),
        }
    }

    #[test]
    fn the_hole_is_the_screen_less_each_edges_depth() {
        let depths = Edges {
            top: px(6.),
            right: px(6.),
            bottom: px(6.),
            left: px(40.),
        };
        let hole = frame_hole(screen(), depths);
        assert_eq!(hole.origin, point(px(40.), px(6.)));
        assert_eq!(hole.size, size(px(954.), px(588.)));

        let hole = frame_hole(
            screen(),
            Edges {
                bottom: px(32.),
                ..Edges::all(px(4.))
            },
        );
        assert_eq!(hole.origin, point(px(4.), px(4.)));
        assert_eq!(hole.size, size(px(992.), px(564.)));
    }

    #[test]
    fn outside_bar_is_the_screen_minus_one_strip() {
        let rest = outside_bar(screen(), BarEdge::Top, px(48.));
        assert_eq!(rest.origin, point(px(0.), px(48.)));
        assert_eq!(rest.size, size(px(1000.), px(552.)));
    }

    /// A surface's strip is shortened by what its neighbouring edges hold.
    #[test]
    fn an_edge_strip_stops_short_of_its_neighbours() {
        let insets = Edges {
            top: px(32.),
            right: px(6.),
            bottom: px(0.),
            left: px(6.),
        };
        let dock = edge_strip(screen(), BarEdge::Bottom, px(60.), insets);
        assert_eq!(dock.origin, point(px(6.), px(540.)));
        assert_eq!(dock.size, size(px(988.), px(60.)));

        let bar = edge_strip(screen(), BarEdge::Left, px(32.), insets);
        assert_eq!(bar.origin, point(px(0.), px(32.)));
        assert_eq!(bar.size, size(px(32.), px(568.)));
    }
}
