//! The equalizer's face: the response curve, with one draggable handle per
//! band. Press anywhere on a column to set its gain; drag to follow.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    App, Bounds, DragMoveEvent, ElementId, EntityId, IntoElement, MouseButton, PathBuilder, Pixels,
    Render, RenderOnce, Styled, Window, canvas, div, point, prelude::*, px, relative, size,
};
use services::equalizer::dsp::{self, BAND_COUNT, MAX_GAIN_DB};
use ui::{ActiveTheme, Color, Label, LabelCommon, Radius, TextSize, h_flex, v_flex};

const GRAPH_HEIGHT: f32 = 148.0;
/// Room above and below the ±12 dB lines for the handles.
const PAD: f32 = 10.0;
const HANDLE: f32 = 12.0;
/// Gain snaps to this many dB under the pointer.
const STEP: f32 = 0.5;
const CURVE_POINTS: usize = 96;
/// Grid lines, in dB.
const GRID: [f32; 5] = [-12.0, -6.0, 0.0, 6.0, 12.0];

/// Identifies the graph being dragged, so a drag from elsewhere cannot move
/// it.
#[derive(Clone, Render)]
pub struct GraphDrag(EntityId);

/// What the pointer is doing to the graph: the band it grabbed, and the gain
/// it is showing for it ahead of the service's confirmation.
#[derive(Default)]
struct Dragging {
    band: Option<usize>,
    gain: f32,
}

type OnChange = Rc<dyn Fn(usize, f32, &mut Window, &mut App)>;

#[derive(IntoElement)]
#[must_use = "EqGraph does nothing unless rendered"]
pub struct EqGraph {
    id: ElementId,
    gains: [f32; BAND_COUNT],
    selected: usize,
    /// The equalizer is in the audio path; off, the graph is drawn muted.
    active: bool,
    on_change: Option<OnChange>,
}

impl EqGraph {
    pub fn new(id: impl Into<ElementId>, gains: [f32; BAND_COUNT]) -> Self {
        Self {
            id: id.into(),
            gains,
            selected: 0,
            active: true,
            on_change: None,
        }
    }

    pub fn selected(mut self, band: usize) -> Self {
        self.selected = band;
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Called with a band and its new gain whenever the pointer sets one.
    pub fn on_change(mut self, handler: impl Fn(usize, f32, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

/// Vertical position of `gain` inside a graph `height` tall.
fn y_of(gain: f32, height: f32) -> f32 {
    let inner = height - 2.0 * PAD;
    PAD + (1.0 - (gain + MAX_GAIN_DB) / (2.0 * MAX_GAIN_DB)).clamp(0.0, 1.0) * inner
}

/// The gain at pointer height `y`, `top` and `height` being the graph's.
fn gain_at(y: Pixels, top: Pixels, height: Pixels) -> f32 {
    let inner = (f32::from(height) - 2.0 * PAD).max(1.0);
    let frac = ((f32::from(y - top) - PAD) / inner).clamp(0.0, 1.0);
    let gain = MAX_GAIN_DB - frac * 2.0 * MAX_GAIN_DB;
    ((gain / STEP).round() * STEP).clamp(-MAX_GAIN_DB, MAX_GAIN_DB)
}

/// The band under pointer `x`.
fn band_at(x: Pixels, left: Pixels, width: Pixels) -> usize {
    let frac = (f32::from(x - left) / f32::from(width).max(1.0)).clamp(0.0, 0.999_9);
    (frac * BAND_COUNT as f32) as usize
}

/// `+3`, `-2.5`, `0`.
pub fn gain_label(gain: f32) -> String {
    if gain.abs() < 0.05 {
        "0".to_string()
    } else if (gain - gain.round()).abs() < 0.05 {
        format!("{:+.0}", gain)
    } else {
        format!("{:+.1}", gain)
    }
}

impl RenderOnce for EqGraph {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = *cx.theme().colors();
        let dragging = window.use_keyed_state((self.id.clone(), "eq-drag"), cx, |_, _| {
            Dragging::default()
        });

        // A grabbed band shows where the pointer is, not where the service
        // last said it was.
        let mut gains = self.gains;
        if let Dragging {
            band: Some(band),
            gain,
        } = *dragging.read(cx)
        {
            gains[band] = gain;
        }
        let curve = dsp::curve(&gains, CURVE_POINTS);

        let accent = if self.active {
            colors.accent
        } else {
            colors.text_muted
        };
        let grid = colors.border_variant;
        let zero_line = colors.border;

        let bounds_cell: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(1.), px(1.)),
        }));

        let drawing = {
            let bounds_cell = bounds_cell.clone();
            canvas(
                move |bounds, _, _| bounds_cell.set(bounds),
                move |bounds, _, window, _| {
                    let height = f32::from(bounds.size.height);
                    let line = |window: &mut Window, y: f32, color| {
                        let mut path = PathBuilder::stroke(px(1.));
                        path.move_to(point(bounds.origin.x, bounds.origin.y + px(y)));
                        path.line_to(point(
                            bounds.origin.x + bounds.size.width,
                            bounds.origin.y + px(y),
                        ));
                        if let Ok(path) = path.build() {
                            window.paint_path(path, color);
                        }
                    };
                    for db in GRID {
                        line(
                            window,
                            y_of(db, height),
                            if db == 0.0 { zero_line } else { grid },
                        );
                    }

                    let mut path = PathBuilder::stroke(px(2.));
                    for (i, db) in curve.iter().enumerate() {
                        let x = bounds.origin.x
                            + bounds.size.width * (i as f32 / (CURVE_POINTS - 1) as f32);
                        let y = bounds.origin.y + px(y_of(*db, height));
                        if i == 0 {
                            path.move_to(point(x, y));
                        } else {
                            path.line_to(point(x, y));
                        }
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, accent);
                    }
                },
            )
            .absolute()
            .size_full()
        };

        let handles = (0..BAND_COUNT).map(|band| {
            let selected = band == self.selected;
            div()
                .absolute()
                .left(relative((band as f32 + 0.5) / BAND_COUNT as f32))
                .ml(px(-HANDLE / 2.0))
                .top(px(y_of(gains[band], GRAPH_HEIGHT) - HANDLE / 2.0))
                .size(px(HANDLE))
                .rounded(Radius::Full.pixels())
                .bg(if selected { accent } else { colors.background })
                .border_2()
                .border_color(accent)
        });

        // One stalk per band, so the columns read as sliders.
        let stalks = (0..BAND_COUNT).map(|band| {
            let top = y_of(gains[band], GRAPH_HEIGHT).min(y_of(0.0, GRAPH_HEIGHT));
            let bottom = y_of(gains[band], GRAPH_HEIGHT).max(y_of(0.0, GRAPH_HEIGHT));
            div()
                .absolute()
                .left(relative((band as f32 + 0.5) / BAND_COUNT as f32))
                .ml(px(-1.5))
                .top(px(top))
                .h(px((bottom - top).max(0.0)))
                .w(px(3.))
                .rounded(Radius::Full.pixels())
                .bg(accent)
                .opacity(0.35)
        });

        let gain_row = h_flex().w_full().children((0..BAND_COUNT).map(|band| {
            div()
                .flex_1()
                .flex()
                .justify_center()
                .child(
                    Label::new(gain_label(gains[band]))
                        .size(TextSize::XSmall)
                        .color(if band == self.selected {
                            Color::Accent
                        } else {
                            Color::Muted
                        }),
                )
        }));
        let freq_row = h_flex().w_full().children(dsp::LABELS.iter().enumerate().map(
            |(band, label)| {
                div().flex_1().flex().justify_center().child(
                    Label::new(*label)
                        .size(TextSize::XSmall)
                        .color(if band == self.selected {
                            Color::Accent
                        } else {
                            Color::Muted
                        }),
                )
            },
        ));

        let mut area = div()
            .id(self.id.clone())
            .relative()
            .w_full()
            .h(px(GRAPH_HEIGHT))
            .child(drawing)
            .children(stalks)
            .children(handles);

        if let Some(on_change) = self.on_change {
            let drag_id = dragging.entity_id();

            let set = {
                let on_change = on_change.clone();
                let bounds_cell = bounds_cell.clone();
                let dragging = dragging.clone();
                move |position: gpui::Point<Pixels>,
                      grab: bool,
                      window: &mut Window,
                      cx: &mut App| {
                    let bounds = bounds_cell.get();
                    let band = if grab {
                        band_at(position.x, bounds.origin.x, bounds.size.width)
                    } else {
                        match dragging.read(cx).band {
                            Some(band) => band,
                            None => return,
                        }
                    };
                    let gain = gain_at(position.y, bounds.origin.y, bounds.size.height);
                    let changed = dragging.update(cx, |state, cx| {
                        if state.band == Some(band) && state.gain == gain {
                            return false;
                        }
                        state.band = Some(band);
                        state.gain = gain;
                        cx.notify();
                        true
                    });
                    if changed {
                        on_change(band, gain, window, cx);
                    }
                }
            };

            let down = set.clone();
            let moved = set;
            let end = dragging.clone();
            area = area
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    down(event.position, true, window, cx);
                })
                .on_drag(GraphDrag(drag_id), |drag, _, _, cx| {
                    cx.stop_propagation();
                    cx.new(|_| drag.clone())
                })
                .on_drag_move(move |event: &DragMoveEvent<GraphDrag>, window, cx| {
                    if event.drag(cx).0 != drag_id {
                        return;
                    }
                    moved(event.event.position, false, window, cx);
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    end.update(cx, |state, cx| {
                        if state.band.take().is_some() {
                            cx.notify();
                        }
                    });
                });
        }

        v_flex()
            .w_full()
            .gap(px(2.))
            .child(gain_row)
            .child(area)
            .child(freq_row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_labels_are_short() {
        assert_eq!(gain_label(0.0), "0");
        assert_eq!(gain_label(3.0), "+3");
        assert_eq!(gain_label(-2.5), "-2.5");
        assert_eq!(gain_label(12.0), "+12");
    }

    #[test]
    fn the_pointer_maps_to_a_gain_and_back() {
        let height = px(GRAPH_HEIGHT);
        // The top handle position is +12, the bottom -12, the middle 0.
        assert_eq!(gain_at(px(PAD), px(0.), height), MAX_GAIN_DB);
        assert_eq!(gain_at(height - px(PAD), px(0.), height), -MAX_GAIN_DB);
        assert_eq!(gain_at(height / 2.0, px(0.), height), 0.0);
        // Beyond the ends it clamps.
        assert_eq!(gain_at(px(-50.), px(0.), height), MAX_GAIN_DB);
        assert_eq!(gain_at(px(500.), px(0.), height), -MAX_GAIN_DB);

        for gain in [-12.0, -4.5, 0.0, 3.0, 12.0] {
            let y = px(y_of(gain, GRAPH_HEIGHT));
            assert!((gain_at(y, px(0.), height) - gain).abs() <= STEP, "{gain}");
        }
    }

    #[test]
    fn the_pointer_maps_to_a_band() {
        let width = px(1000.);
        assert_eq!(band_at(px(0.), px(0.), width), 0);
        assert_eq!(band_at(px(99.), px(0.), width), 0);
        assert_eq!(band_at(px(100.), px(0.), width), 1);
        assert_eq!(band_at(px(550.), px(0.), width), 5);
        assert_eq!(band_at(px(1000.), px(0.), width), BAND_COUNT - 1);
        assert_eq!(band_at(px(-30.), px(0.), width), 0);
        // The graph does not have to start at the window's edge.
        assert_eq!(band_at(px(350.), px(250.), width), 1);
    }
}
