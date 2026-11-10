//! Status bar using layer shell for Wayland.
//!
//! This module provides a configurable shell bar for any screen edge.

use futures::StreamExt;
use futures_signals::signal::SignalExt;
use gpui::{
    AnyElement, App, Bounds, Context, DisplayId, Size, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, layer_shell::*, point, prelude::*, px,
};
use ui::ActiveTheme;
use ui::patterns::{BarSegment, BarShape, BarSurface};

use super::config::{BarItem, BarPosition, BarStyle};
use super::modules::{self, BarCtx, Widget};
use crate::config::{ActiveConfig, ConfigSignals};
use crate::state::watch;

/// One of the bar's three widget lists, kept with the entries it was built from.
struct Section {
    items: Vec<BarItem>,
    /// One list per entry: what shares a bubble.
    groups: Vec<Vec<Widget>>,
}

impl Section {
    fn new(items: &[BarItem], ctx: &BarCtx, cx: &mut Context<Bar>) -> Self {
        Self {
            items: items.to_vec(),
            groups: items
                .iter()
                .map(|item| modules::build_many(item.names(), ctx, cx))
                .filter(|group| !group.is_empty())
                .collect(),
        }
    }

    /// Rebuild only when the list changed, so untouched widgets keep their state.
    fn sync(&mut self, items: &[BarItem], ctx: &BarCtx, cx: &mut Context<Bar>) {
        if self.items != items {
            *self = Self::new(items, ctx, cx);
        }
    }

    /// Hand every widget its current placement.
    fn place(&self, ctx: &BarCtx, cx: &mut Context<Bar>) {
        for widget in self.groups.iter().flatten() {
            widget.set_ctx(ctx, cx);
        }
    }

    /// One segment per entry, in every style; the style only decides what
    /// the segment paints.
    fn render(&self, ctx: &BarCtx) -> Vec<AnyElement> {
        let segment = ctx.segment;
        self.groups
            .iter()
            .map(|group| {
                BarSegment::new(ctx.vertical)
                    .fill(segment.fill)
                    .border(segment.border)
                    .radius(segment.radius)
                    .thickness(Some(segment.thickness))
                    .padding(segment.padding)
                    .children(group.iter().map(Widget::render))
                    .into_any_element()
            })
            .collect()
    }
}

/// The main bar view.
struct Bar {
    position: BarPosition,
    start: Section,
    center: Section,
    end: Section,
}

impl Bar {
    /// Create a bar with configuration.
    fn new(cx: &mut Context<Self>) -> Self {
        let config = cx.config().bar.clone();
        let ctx = BarCtx::new(&config, cx.theme());
        let bar = ConfigSignals::global(cx).bar.signal_cloned();
        watch(cx, bar, |this, bar, cx| {
            let ctx = BarCtx::new(&bar, cx.theme());
            this.start.sync(&bar.start, &ctx, cx);
            this.center.sync(&bar.center, &ctx, cx);
            this.end.sync(&bar.end, &ctx, cx);
            cx.notify();
        });

        Self {
            position: config.position,
            start: Section::new(&config.start, &ctx, cx),
            center: Section::new(&config.center, &ctx, cx),
            end: Section::new(&config.end, &ctx, cx),
        }
    }
}

impl Render for Bar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(cx.theme().font_size);

        // Config and theme changes both land here, so this is where every
        // widget learns about its new placement.
        let ctx = BarCtx::new(&cx.config().bar, cx.theme());
        for section in [&self.start, &self.center, &self.end] {
            section.place(&ctx, cx);
        }

        let bar = &cx.config().bar;
        // A side of the screen frame draws no hairline of its own: the frame
        // outlines the whole work area.
        let framed = cx.config().screen_frame().is_some() && cx.config().bar_is_frame_side();
        let shape = match bar.style {
            BarStyle::Attached | BarStyle::Frame if framed => BarShape::Framed,
            BarStyle::Attached | BarStyle::Bubbles | BarStyle::Frame => BarShape::Attached,
            BarStyle::Floating => BarShape::Floating {
                radius: bar.floating.radius(),
            },
        };

        // With bubbles the fill and border move onto the segments.
        let bubbles = bar.style == BarStyle::Bubbles;
        let fill = if bubbles {
            gpui::transparent_black()
        } else {
            bar.background_color(cx.theme())
        };

        BarSurface::new(self.position.edge())
            .shape(shape)
            .background(fill)
            .border(bar.show_border && !bubbles)
            .padding(ctx.end_padding)
            .inset(ctx.segment.inset)
            .segment_gap(ctx.segment.gap)
            .start(self.start.render(&ctx))
            .center(self.center.render(&ctx))
            .end(self.end.render(&ctx))
    }
}

/// Returns window options for the bar.
pub fn window_options(
    // config: &BarConfig,
    display_id: Option<DisplayId>,
    cx: &App,
) -> WindowOptions {
    // The bar spans the screen on its long axis, so it needs the real
    // logical screen extent. `PlatformDisplay::bounds()` does not give it:
    // gpui derives those from the integer `wl_output.scale`, and Wayland's
    // core output protocol carries no fractional scale, so a screen at 1.5
    // advertises 2 and gpui reports two thirds of the true size - which
    // left the bar covering 75% of the screen.
    //
    // Requesting 0 on the spanned axis and letting the compositor stretch
    // between the anchors would be the protocol-blessed alternative, but
    // gpui sets a `wp_viewport` destination from these bounds before the
    // configure arrives, and a zero there is a protocol error.
    let display_size = crate::state::display_extent(display_id, cx);
    let config = cx.config();
    // A floating bar keeps `gap` from every edge, so it is that much shorter at both ends.
    let gap = match config.bar.style {
        BarStyle::Floating => config.bar.floating.gap.max(0.),
        BarStyle::Attached | BarStyle::Frame | BarStyle::Bubbles => 0.,
    };
    let length = |extent: gpui::Pixels| (extent - px(gap * 2.)).max(px(1.));
    let (window_size, anchor) = match config.bar.position {
        BarPosition::Left => (
            Size::new(px(config.bar.size), length(display_size.height)),
            Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
        ),
        BarPosition::Right => (
            Size::new(px(config.bar.size), length(display_size.height)),
            Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
        ),
        BarPosition::Top => (
            Size::new(length(display_size.width), px(config.bar.size)),
            Anchor::LEFT | Anchor::RIGHT | Anchor::TOP,
        ),
        BarPosition::Bottom => (
            Size::new(length(display_size.width), px(config.bar.size)),
            Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
        ),
    };

    WindowOptions {
        display_id,
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.), px(0.)),
            size: window_size,
        })),
        app_id: Some("conchy-shell-bar".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "bar".to_string(),
            layer: Layer::Top,
            anchor,
            // Compositors add the margin on the anchored edge to this zone.
            exclusive_zone: Some(px(config.bar.size)),
            margin: (gap > 0.).then(|| (px(gap), px(gap), px(gap), px(gap))),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Initialize the bar using the current global config.
pub fn init(cx: &mut App) {
    // Only the anchor, exclusive zone and margins need new windows; `Bar`
    // applies the rest in place.
    let layout = ConfigSignals::global(cx)
        .bar
        .signal_ref(|bar| {
            let gap = (bar.style == BarStyle::Floating).then_some(bar.floating.gap);
            (bar.position, bar.size, gap)
        })
        .dedupe()
        .to_stream();

    cx.spawn(async move |cx| {
        // Small delay to allow Wayland to enumerate displays
        cx.background_executor()
            .timer(std::time::Duration::from_millis(100))
            .await;

        tracing::info!("Bar opened");

        // Frame strips first, so a floating bar sits inside the frame; the
        // dock last, so it sits beside the bar. See `reload`.
        cx.update(|cx| {
            crate::frame::reopen(cx);
            open_all_bar_windows(cx);
            crate::dock::reload(cx);
        });

        let mut layout = layout.skip(1);
        while layout.next().await.is_some() {
            tracing::info!("Bar layout changed; reopening bar windows");
            cx.update(reload);
        }
    })
    .detach();
}

/// Open the bar with custom configuration.
pub fn open_with_config(display_id: Option<DisplayId>, cx: &mut App) -> bool {
    match cx.open_window(window_options(display_id, cx), move |window, cx| {
        crate::state::record_window_display(window.window_handle(), display_id, cx);
        cx.new(Bar::new)
    }) {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!("Failed to open bar window: {}", err);
            false
        }
    }
}

fn open_all_bar_windows(cx: &mut App) -> usize {
    let mut opened = 0usize;
    let displays = cx.displays();
    let filter = cx.config().bar.monitors.clone();

    if displays.is_empty() {
        tracing::info!("No displays found, opening bar on default display");
        if open_with_config(None, cx) {
            opened += 1;
        }
    } else {
        tracing::info!("Opening bar on {} displays", displays.len());
        for d in displays {
            if let Some(target) = &filter {
                let target_uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, target.as_bytes());
                if let Ok(u) = d.uuid() {
                    if u != target_uuid {
                        continue;
                    }
                }
            }
            tracing::info!("Opening bar on display {:?}", d.id());
            if open_with_config(Some(d.id()), cx) {
                opened += 1;
            }
        }
    }
    opened
}

/// Rebuild all bar windows using the latest config.
/// Reopen the shell's edge windows in layout order. Compositors hand out
/// exclusive zones in creation order (niri) or by layer (Hyprland), so the
/// frame's strips go first, then the bar, then the dock: each is laid out
/// inside what came before it.
pub fn reload(cx: &mut App) {
    let old_windows: Vec<_> = cx
        .windows()
        .into_iter()
        .filter(|h| h.downcast::<Bar>().is_some())
        .collect();

    crate::frame::reopen(cx);
    let opened = open_all_bar_windows(cx);
    if opened == 0 {
        tracing::warn!("Bar reload skipped closing old windows because no new bar window opened");
        return;
    }

    for handle in old_windows {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
    crate::dock::reload(cx);
}
