//! The screen frame: a band around the work area with rounded inner
//! corners, as one full-output layer per display under the panels. The bar
//! and a taskbar dock on an edge become that side of the frame; a floating
//! bar or dock sits inside it. While a panel is open the layer also catches
//! clicks outside it to close it; its input region is empty otherwise, so
//! it never gets in the way.
//!
//! The frame keeps windows off its bands through [`FrameExclusion`] strips,
//! because the full-output layer itself must ignore exclusive zones. The
//! strips open before the bar and the dock, so those are laid out inside
//! the bands; a surface merged into the frame reserves its own edge.

use futures::StreamExt;
use futures_signals::map_ref;
use futures_signals::signal::SignalExt;
use gpui::{
    App, Bounds, Context, DisplayId, Edges, Pixels, Size, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, div, layer_shell::*, point, prelude::*, px,
};
use serde::{Deserialize, Serialize};
use ui::patterns::ScreenFrame;
use ui::patterns::frame::{edge_mut, edge_strip, outside_bar};
use ui::{ActiveTheme, Radius};

use crate::config::{ActiveConfig, BarPosition, BarStyle, Config, ConfigSignals};
use crate::dock::DockStyle;
use crate::state::{display_extent, record_window_display, watch};
use crate::windows::ShellWindows;

/// The screen frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameConfig {
    pub enabled: bool,
    /// Width in px of the band on the edges no shell surface sits on.
    pub thickness: f32,
    /// Radius in px of the work area's corners. Unset, it follows the
    /// theme's `radius_scale` (16px at scale 1); set, it is used as is.
    pub radius: Option<f32>,
    /// Opacity of the band, from 0.0 to 1.0. Unset, it matches the bar's
    /// background, so a merged bar and frame read as one surface.
    pub opacity: Option<f32>,
}

impl Default for FrameConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            thickness: 6.0,
            radius: None,
            opacity: None,
        }
    }
}

impl FrameConfig {
    /// The token the corners follow when `radius` is unset.
    pub const RADIUS: Radius = Radius::XXLarge;

    pub fn radius(&self) -> Pixels {
        self.radius
            .map_or_else(|| Self::RADIUS.pixels(), |radius| px(radius.max(0.)))
    }
}

impl Config {
    /// The frame to draw, if any. `bar.style = "frame"` is the old spelling
    /// of an attached bar with the frame on, with its geometry under `bar.frame`.
    pub fn screen_frame(&self) -> Option<FrameConfig> {
        if self.frame.enabled {
            Some(self.frame.clone())
        } else if self.bar.style == BarStyle::Frame {
            Some(FrameConfig {
                enabled: true,
                thickness: self.bar.frame.thickness,
                radius: self.bar.frame.radius,
                opacity: None,
            })
        } else {
            None
        }
    }

    /// Whether the bar is a side of the frame: attached along its edge.
    pub fn bar_is_frame_side(&self) -> bool {
        matches!(self.bar.style, BarStyle::Attached | BarStyle::Frame)
    }

    /// Whether the dock is a side of the frame: a reserved strip along an
    /// edge the bar does not already hold.
    pub fn dock_is_frame_side(&self) -> bool {
        let dock = &self.dock;
        dock.style == DockStyle::Attached
            && dock.stretch
            && dock.reserve_space
            && !(self.bar_is_frame_side() && dock.position == self.bar.position)
    }
}

/// Which edges the frame paints as bands, and how deep each edge is.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sides {
    depths: Edges<Pixels>,
    bar: Option<BarPosition>,
    dock: Option<BarPosition>,
}

impl Sides {
    fn new(config: &Config, frame: &FrameConfig) -> Self {
        let mut depths = Edges::all(px(frame.thickness.max(0.)));
        let bar = config.bar_is_frame_side().then_some(config.bar.position);
        if let Some(edge) = bar {
            *edge_mut(&mut depths, edge.edge()) = px(config.bar.size);
        }
        let dock = config.dock_is_frame_side().then_some(config.dock.position);
        if let Some(edge) = dock {
            *edge_mut(&mut depths, edge.edge()) = px(crate::dock::thickness(&config.dock));
        }
        Self { depths, bar, dock }
    }

    fn is_band(&self, edge: BarPosition) -> bool {
        self.bar != Some(edge) && self.dock != Some(edge)
    }

    /// The strips the bar and the dock paint themselves. The bar opens
    /// before the dock, so the dock is laid out beside the bar as well as
    /// the bands, while the bar only clears the bands.
    fn cutouts(&self, bounds: Bounds<Pixels>) -> Vec<Bounds<Pixels>> {
        let depth = |edge: BarPosition| *edge_mut(&mut self.depths.clone(), edge.edge());
        let mut cutouts = Vec::new();
        if let Some(bar) = self.bar {
            let mut insets = self.depths;
            for edge in BarPosition::ALL {
                if !self.is_band(edge) {
                    *edge_mut(&mut insets, edge.edge()) = px(0.);
                }
            }
            cutouts.push(edge_strip(bounds, bar.edge(), depth(bar), insets));
        }
        if let Some(dock) = self.dock {
            let mut insets = self.depths;
            *edge_mut(&mut insets, dock.edge()) = px(0.);
            cutouts.push(edge_strip(bounds, dock.edge(), depth(dock), insets));
        }
        cutouts
    }
}

/// What forces the frame windows to be reopened.
#[derive(Clone, Debug, PartialEq)]
struct Layout {
    /// The full-output layer has a job: drawing, or catching outside clicks.
    enabled: bool,
    /// Thickness of the exclusion strip on each edge that gets one.
    strips: Vec<(BarPosition, f32)>,
}

impl Layout {
    fn new(config: &Config) -> Self {
        let frame = config.screen_frame();
        let strips = match &frame {
            Some(frame) if frame.thickness > 0. => {
                let sides = Sides::new(config, frame);
                BarPosition::ALL
                    .into_iter()
                    .filter(|edge| sides.is_band(*edge))
                    .map(|edge| (edge, frame.thickness))
                    .collect()
            }
            _ => Vec::new(),
        };
        Self {
            enabled: frame.is_some() || config.panels.close_on_click_outside,
            strips,
        }
    }
}

/// The exclusion strips the frame opens: one per band edge, with its depth.
pub fn strips(config: &Config) -> Vec<(BarPosition, f32)> {
    Layout::new(config).strips
}

struct Frame {
    panel_open: bool,
    input_region: Option<Vec<Bounds<Pixels>>>,
}

impl Frame {
    fn new(cx: &mut Context<Self>) -> Self {
        let panel_open = ShellWindows::panel_open_signal(cx);
        watch(cx, panel_open, |this, open, cx| {
            this.panel_open = open;
            cx.notify();
        });
        let signals = ConfigSignals::global(cx);
        let (bar, dock, frame) = (
            signals.bar.signal_cloned(),
            signals.dock.signal_cloned(),
            signals.frame.signal_cloned(),
        );
        watch(cx, bar, |_, _, cx| cx.notify());
        watch(cx, dock, |_, _, cx| cx.notify());
        watch(cx, frame, |_, _, cx| cx.notify());

        Self {
            panel_open: false,
            input_region: None,
        }
    }
}

impl Render for Frame {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = cx.config();
        let bar = &config.bar;
        let edge = bar.position.edge();
        let viewport = Bounds {
            origin: point(px(0.), px(0.)),
            size: window.viewport_size(),
        };

        // The bar keeps its own clicks, so a click on another widget swaps panels.
        let region = if self.panel_open && config.panels.close_on_click_outside {
            vec![outside_bar(viewport, edge, px(bar.depth()))]
        } else {
            Vec::new()
        };
        if self.input_region.as_ref() != Some(&region) {
            window.set_input_region(Some(&region));
            self.input_region = Some(region);
        }

        let frame = config.screen_frame().map(|frame| {
            let theme = cx.theme();
            let sides = Sides::new(config, &frame);
            let fill = match frame.opacity {
                Some(_) => theme.surface(frame.opacity),
                None => bar.background_color(theme),
            };
            ScreenFrame::new(fill)
                .depths(sides.depths)
                .cutouts(sides.cutouts(viewport))
                .radius(frame.radius())
                .border(bar.show_border.then_some(theme.colors().border))
        });

        div()
            .id("frame")
            .size_full()
            .on_any_mouse_down(|_, _, cx| ShellWindows::close_panel(cx))
            .children(frame)
    }
}

/// An invisible strip that reserves the frame's thickness on one edge.
struct FrameExclusion {
    input_cleared: bool,
}

impl Render for FrameExclusion {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if !self.input_cleared {
            window.set_input_region(Some(&[]));
            self.input_cleared = true;
        }
        div()
    }
}

fn layer_window(
    display_id: Option<DisplayId>,
    size: Size<Pixels>,
    app_id: &str,
    options: LayerShellOptions,
) -> WindowOptions {
    WindowOptions {
        display_id,
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.), px(0.)),
            size,
        })),
        app_id: Some(app_id.to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            keyboard_interactivity: KeyboardInteractivity::None,
            ..options
        }),
        focus: false,
        ..Default::default()
    }
}

fn open_frame(display_id: Option<DisplayId>, cx: &mut App) -> bool {
    let options = layer_window(
        display_id,
        display_extent(display_id, cx),
        "conchy-shell-frame",
        LayerShellOptions {
            namespace: "frame".to_string(),
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            // Span the whole output, bar included.
            exclusive_zone: Some(px(-1.)),
            ..Default::default()
        },
    );
    match cx.open_window(options, move |window, cx| {
        record_window_display(window.window_handle(), display_id, cx);
        cx.new(Frame::new)
    }) {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!("Failed to open frame window: {err}");
            false
        }
    }
}

fn open_strip(
    display_id: Option<DisplayId>,
    edge: BarPosition,
    thickness: f32,
    cx: &mut App,
) -> bool {
    let extent = display_extent(display_id, cx);
    let (size, anchor) = match edge {
        BarPosition::Left => (
            Size::new(px(thickness), extent.height),
            Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
        ),
        BarPosition::Right => (
            Size::new(px(thickness), extent.height),
            Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
        ),
        BarPosition::Top => (
            Size::new(extent.width, px(thickness)),
            Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
        ),
        BarPosition::Bottom => (
            Size::new(extent.width, px(thickness)),
            Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        ),
    };
    let options = layer_window(
        display_id,
        size,
        "conchy-shell-frame-exclusion",
        LayerShellOptions {
            namespace: "frame-exclusion".to_string(),
            // Hyprland lays out exclusive zones by layer, niri by creation
            // order: Bottom, opened first, puts the strips before the bar
            // and the dock either way, so those sit inside the bands.
            layer: Layer::Bottom,
            anchor,
            exclusive_zone: Some(px(thickness)),
            ..Default::default()
        },
    );
    match cx.open_window(options, |_, cx| {
        cx.new(|_| FrameExclusion {
            input_cleared: false,
        })
    }) {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!("Failed to open frame exclusion strip: {err}");
            false
        }
    }
}

fn open_all(layout: &Layout, cx: &mut App) {
    if !layout.enabled {
        return;
    }
    let displays: Vec<_> = cx
        .displays()
        .iter()
        .map(|display| Some(display.id()))
        .collect();
    let displays = if displays.is_empty() {
        vec![None]
    } else {
        displays
    };

    for display_id in displays {
        open_frame(display_id, cx);
        for (edge, thickness) in &layout.strips {
            open_strip(display_id, *edge, *thickness, cx);
        }
    }
}

/// Close and reopen the frame windows. The bar reopens itself and the dock
/// right after, see [`crate::bar::reload`], so they stay inside the bands.
pub fn reopen(cx: &mut App) {
    let old_windows: Vec<_> = cx
        .windows()
        .into_iter()
        .filter(|handle| {
            handle.downcast::<Frame>().is_some() || handle.downcast::<FrameExclusion>().is_some()
        })
        .collect();
    for handle in old_windows {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
    open_all(&Layout::new(cx.config()), cx);
}

pub fn init(cx: &mut App) {
    let signals = ConfigSignals::global(cx);
    let layout = map_ref! {
        let _bar = signals.bar.signal_cloned(),
        let _dock = signals.dock.signal_cloned(),
        let _frame = signals.frame.signal_cloned(),
        let _panels = signals.panels.signal_cloned() => ()
    }
    .to_stream();

    // The first open comes from the bar, see [`reopen`]. Later, a change
    // to the strips reopens everything through the bar, in order.
    cx.spawn(async move |cx| {
        let mut current = cx.update(|cx| Layout::new(cx.config()));
        let mut layout = Box::pin(layout.skip(1));
        while layout.next().await.is_some() {
            let next = cx.update(|cx| Layout::new(cx.config()));
            if next != current {
                current = next;
                cx.update(crate::bar::reload);
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BarConfig, DockConfig};
    use crate::windows::PanelsConfig;

    fn shell(bar: BarConfig, dock: DockConfig, frame: FrameConfig) -> Config {
        Config {
            bar,
            dock,
            frame,
            panels: PanelsConfig {
                close_on_click_outside: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn frame() -> FrameConfig {
        FrameConfig {
            enabled: true,
            ..Default::default()
        }
    }

    fn taskbar() -> DockConfig {
        DockConfig {
            position: BarPosition::Bottom,
            style: DockStyle::Attached,
            stretch: true,
            reserve_space: true,
            hover_effect: crate::dock::DockHoverEffect::None,
            ..Default::default()
        }
    }

    fn screen() -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(0.)), Size::new(px(1000.), px(600.)))
    }

    /// An attached bar and a taskbar dock are sides; the other edges are
    /// bands with strips; the frame paints neither surface's strip.
    #[test]
    fn the_bar_and_a_taskbar_dock_are_sides_of_the_frame() {
        let config = shell(
            BarConfig {
                position: BarPosition::Top,
                style: BarStyle::Attached,
                size: 32.,
                ..Default::default()
            },
            taskbar(),
            frame(),
        );
        let sides = Sides::new(&config, &config.frame);
        assert_eq!(sides.bar, Some(BarPosition::Top));
        assert_eq!(sides.dock, Some(BarPosition::Bottom));
        assert_eq!(sides.depths.top, px(32.));
        assert_eq!(
            sides.depths.bottom,
            px(crate::dock::thickness(&config.dock))
        );
        assert_eq!((sides.depths.left, sides.depths.right), (px(6.), px(6.)));

        let cutouts = sides.cutouts(screen());
        assert_eq!(
            cutouts[0],
            Bounds::new(point(px(6.), px(0.)), Size::new(px(988.), px(32.)))
        );
        let dock = px(crate::dock::thickness(&config.dock));
        assert_eq!(cutouts[1].origin, point(px(6.), px(600.) - dock));
        assert_eq!(cutouts[1].size, Size::new(px(988.), dock));

        let layout = Layout::new(&config);
        assert_eq!(
            layout.strips,
            vec![(BarPosition::Left, 6.), (BarPosition::Right, 6.)]
        );
    }

    /// A floating bar and a floating dock sit inside a frame with four bands.
    #[test]
    fn floating_surfaces_leave_the_frame_whole() {
        let config = shell(
            BarConfig {
                style: BarStyle::Floating,
                ..Default::default()
            },
            DockConfig::default(),
            frame(),
        );
        let sides = Sides::new(&config, &config.frame);
        assert_eq!(sides.depths, Edges::all(px(6.)));
        assert!(sides.cutouts(screen()).is_empty());
        assert_eq!(Layout::new(&config).strips.len(), 4);
    }

    /// A bar and a dock on the same edge: the bar is the side, the dock is
    /// not merged, and the old `bar.style = "frame"` still means a frame.
    #[test]
    fn the_legacy_frame_style_and_a_shared_edge_resolve() {
        let config = shell(
            BarConfig {
                position: BarPosition::Bottom,
                style: BarStyle::Frame,
                ..Default::default()
            },
            taskbar(),
            FrameConfig::default(),
        );
        let frame = config.screen_frame().expect("the legacy style is a frame");
        assert_eq!(frame.thickness, config.bar.frame.thickness);
        let sides = Sides::new(&config, &frame);
        assert_eq!(sides.bar, Some(BarPosition::Bottom));
        assert_eq!(sides.dock, None);

        let off = shell(
            BarConfig::default(),
            DockConfig::default(),
            FrameConfig::default(),
        );
        assert!(off.screen_frame().is_none());
        assert!(!Layout::new(&off).enabled);
    }
}
