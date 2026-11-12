//! The shell's transient windows: panels (one at a time), the launcher and
//! the notification popups. [`ShellWindows`] tracks what is open; [`PanelHost`]
//! sizes a [`PanelSize::FitHeight`] panel to its content before painting it.

use std::collections::HashMap;
use std::rc::Rc;

use futures_signals::signal::{Mutable, Signal};
use gpui::{
    AnyElement, AnyWindowHandle, App, AvailableSpace, Bounds, Context, DisplayId, Entity, Global,
    Pixels, PlatformDisplay, Point, Render, Size, Window, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions, div, layer_shell::*, point, prelude::*, px, size,
};
use serde::{Deserialize, Serialize};
use ui::ActiveTheme;
use ui::patterns::PanelSurface;

use crate::config::BarPosition;
use crate::launcher::Launcher;

/// Behaviour shared by every panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelsConfig {
    /// Close the open panel when you click anywhere outside it. That click
    /// is not passed on to what is underneath.
    pub close_on_click_outside: bool,
    /// Opacity of every panel's surface (control center, notification
    /// center, tray menus, media panel), from 0.0 to 1.0. Unset, it follows
    /// `opacity` in `theme.kdl`.
    pub opacity: Option<f32>,
}

impl Default for PanelsConfig {
    fn default() -> Self {
        Self {
            close_on_click_outside: true,
            opacity: None,
        }
    }
}

/// The fill every panel paints with: the theme surface at `[panels] opacity`.
pub trait ShellPanel: Styled + Sized {
    fn shell_panel(self, cx: &App) -> Self;
}

impl<T: Styled> ShellPanel for T {
    fn shell_panel(self, cx: &App) -> Self {
        self.panel_surface_with(crate::config::Config::global(cx).panels.opacity, cx)
    }
}

// ============================================================================
// Panel sizing and placement
// ============================================================================

/// How a panel window is sized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PanelSize {
    /// Height follows the content, capped at `max_height` and the output.
    FitHeight { width: f32, max_height: f32 },
}

impl PanelSize {
    /// The size the window opens at: a fitted panel's cap.
    pub fn initial(&self) -> Size<Pixels> {
        match self {
            PanelSize::FitHeight { width, max_height } => size(px(*width), px(*max_height)),
        }
    }
}

/// Panel configuration for positioning and sizing.
#[derive(Clone, Debug)]
pub struct PanelConfig {
    pub size: PanelSize,
    pub anchor: Anchor,
    pub margin: (f32, f32, f32, f32), // top, right, bottom, left
    pub namespace: String,
}

impl Default for PanelConfig {
    fn default() -> Self {
        PanelConfig {
            size: PanelSize::FitHeight {
                width: 320.0,
                max_height: 400.0,
            },
            anchor: Anchor::TOP | Anchor::RIGHT,
            margin: (5.0, 5.0, 5.0, 5.0),
            namespace: "panel".to_string(),
        }
    }
}

/// Resolve panel anchor/margin from a click position, in window coordinates.
pub fn panel_placement_from_event(
    bar_position: BarPosition,
    position: Point<Pixels>,
    window: &Window,
    cx: &App,
    panel_size: Size<Pixels>,
) -> (Anchor, (f32, f32, f32, f32)) {
    let display_size = ShellWindows::display(cx, window)
        .map(|display| display.bounds().size)
        .unwrap_or_else(|| window.bounds().size);
    let display_bounds = Bounds {
        origin: point(px(0.), px(0.)),
        size: display_size,
    };
    // The panel keeps the bar's own gap from the bar and the screen edges,
    // so it reads as part of the same layout rather than stuck to a corner.
    let gap = px(crate::config::Config::global(cx).bar.panel_gap());
    let usable_bounds = Bounds {
        origin: point(gap, gap),
        size: size(
            (display_size.width - gap * 2.).max(px(0.)),
            (display_size.height - gap * 2.).max(px(0.)),
        ),
    };
    let click = position;
    panel_placement_from_click(
        bar_position,
        click,
        panel_size,
        display_bounds,
        usable_bounds,
    )
}

/// Resolve panel anchor/margin from a click position.
pub fn panel_placement_from_click(
    bar_position: BarPosition,
    click: Point<Pixels>,
    panel_size: Size<Pixels>,
    display_bounds: Bounds<Pixels>,
    usable_bounds: Bounds<Pixels>,
) -> (Anchor, (f32, f32, f32, f32)) {
    let anchor_horizontal = match bar_position {
        BarPosition::Left => Anchor::LEFT,
        BarPosition::Right => Anchor::RIGHT,
        BarPosition::Top | BarPosition::Bottom => {
            if click.x < display_bounds.center().x {
                Anchor::LEFT
            } else {
                Anchor::RIGHT
            }
        }
    };

    let anchor_vertical = match bar_position {
        BarPosition::Top => Anchor::TOP,
        BarPosition::Bottom => Anchor::BOTTOM,
        BarPosition::Left | BarPosition::Right => {
            if click.y < display_bounds.center().y {
                Anchor::TOP
            } else {
                Anchor::BOTTOM
            }
        }
    };

    let anchor = anchor_horizontal | anchor_vertical;
    let margin = margin_from_click(
        bar_position,
        anchor,
        click,
        panel_size,
        display_bounds,
        usable_bounds,
    );

    (anchor, margin)
}

fn margin_from_click(
    bar_position: BarPosition,
    anchor: Anchor,
    click: Point<Pixels>,
    panel_size: Size<Pixels>,
    display_bounds: Bounds<Pixels>,
    usable_bounds: Bounds<Pixels>,
) -> (f32, f32, f32, f32) {
    let display_origin = display_bounds.origin;
    let display_size = display_bounds.size;
    let usable_origin = usable_bounds.origin;
    let usable_size = usable_bounds.size;

    let click_x: f32 = (click.x - display_origin.x).into();
    let click_y: f32 = (click.y - display_origin.y).into();
    let panel_w: f32 = panel_size.width.into();
    let panel_h: f32 = panel_size.height.into();
    let disp_w: f32 = display_size.width.into();
    let disp_h: f32 = display_size.height.into();

    let avail_x = (disp_w - panel_w).max(0.0);
    let avail_y = (disp_h - panel_h).max(0.0);

    let inset_left: f32 = (usable_origin.x - display_origin.x).into();
    let inset_top: f32 = (usable_origin.y - display_origin.y).into();
    let inset_right: f32 =
        (display_origin.x + display_size.width - (usable_origin.x + usable_size.width)).into();
    let inset_bottom: f32 =
        (display_origin.y + display_size.height - (usable_origin.y + usable_size.height)).into();

    let mut min_x = inset_left;
    let mut max_x = disp_w - inset_right - panel_w;
    if max_x < min_x {
        min_x = 0.0;
        max_x = avail_x;
    }

    let mut min_y = inset_top;
    let mut max_y = disp_h - inset_bottom - panel_h;
    if max_y < min_y {
        min_y = 0.0;
        max_y = avail_y;
    }

    let mut origin_x = (click_x - (panel_w / 2.0)).clamp(min_x, max_x);
    let mut origin_y = (click_y - (panel_h / 2.0)).clamp(min_y, max_y);

    match bar_position {
        BarPosition::Top => {
            origin_y = min_y;
        }
        BarPosition::Bottom => {
            origin_y = max_y;
        }
        BarPosition::Left => {
            origin_x = min_x;
        }
        BarPosition::Right => {
            origin_x = max_x;
        }
    }

    let mut top = 0.0;
    let mut right = 0.0;
    let mut bottom = 0.0;
    let mut left = 0.0;

    if anchor.contains(Anchor::TOP) {
        top = origin_y;
    }
    if anchor.contains(Anchor::BOTTOM) {
        bottom = disp_h - (origin_y + panel_h);
    }
    if anchor.contains(Anchor::LEFT) {
        left = origin_x;
    }
    if anchor.contains(Anchor::RIGHT) {
        right = disp_w - (origin_x + panel_w);
    }

    (top, right, bottom, left)
}

// ============================================================================
// Fitting a window to its content
// ============================================================================

/// Frames a size is re-requested before painting at whatever was granted.
const MAX_RESIZE_ATTEMPTS: u8 = 4;

/// Resize handshake for a window that follows its content. Nothing is
/// painted until the first fit lands; later fits paint at the old size.
///
/// The request is repeated each frame: gpui applies every compositor
/// `configure`, and a stale one (the reply to the first commit) can undo a
/// single request.
#[derive(Debug, Default)]
pub struct WindowFit {
    requested: Option<Size<Pixels>>,
    attempts: u8,
    settled: bool,
}

impl WindowFit {
    /// Measure `content` at `width` and resize the window to it, within
    /// `max_height` and the output. Returns whether to paint this frame.
    pub fn fit_height(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        mut content: AnyElement,
        width: Pixels,
        max_height: Pixels,
        vertical_margin: Pixels,
    ) -> bool {
        let measured = content
            .layout_as_root(
                size(AvailableSpace::Definite(width), AvailableSpace::MaxContent),
                window,
                cx,
            )
            .height;

        let mut cap = max_height;
        if let Some(display) = ShellWindows::display(cx, window) {
            let usable = display.visible_bounds().size.height - vertical_margin;
            if usable > px(0.) {
                cap = cap.min(usable);
            }
        }
        let desired = size(width, measured.ceil().min(cap).max(px(1.)));
        self.request(window, desired)
    }

    /// Bring the window to `desired`. Returns whether to paint this frame.
    pub fn request(&mut self, window: &mut Window, desired: Size<Pixels>) -> bool {
        if window.viewport_size() == desired {
            self.settled = true;
            self.requested = Some(desired);
            self.attempts = 0;
            return true;
        }
        if self.requested != Some(desired) {
            self.requested = Some(desired);
            self.attempts = 0;
        }
        if self.attempts < MAX_RESIZE_ATTEMPTS {
            self.attempts += 1;
            window.resize(desired);
        } else {
            self.settled = true;
        }
        self.settled
    }

    /// The size the window was last asked to take.
    pub fn requested(&self) -> Option<Size<Pixels>> {
        self.requested
    }
}

/// Root view of a panel window; fits the window to the content if asked.
pub struct PanelHost<V: Render> {
    content: Entity<V>,
    size: PanelSize,
    vertical_margin: Pixels,
    fit: WindowFit,
}

impl<V: Render> Render for PanelHost<V> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paint = match self.size {
            PanelSize::FitHeight { width, max_height } => self.fit.fit_height(
                window,
                cx,
                self.content.clone().into_any_element(),
                px(width),
                px(max_height),
                self.vertical_margin,
            ),
        };
        div()
            .id("panel-host")
            .size_full()
            .when(paint, |el| el.child(self.content.clone()))
    }
}

// ============================================================================
// Registry
// ============================================================================

struct OpenPanel {
    id: String,
    handle: AnyWindowHandle,
}

/// What the shell has open besides its bar, dock and OSD.
#[derive(Default)]
pub struct ShellWindows {
    /// Only one panel is open at a time.
    panel: Option<OpenPanel>,
    /// Whether `panel` is set, for the frame windows that catch clicks outside it.
    panel_open: Mutable<bool>,
    launcher: Option<WindowHandle<Launcher>>,
    displays: HashMap<AnyWindowHandle, DisplayId>,
    osd: Option<crate::osd::OsdWindowState>,
    popups: HashMap<Option<DisplayId>, crate::notification::popup::PopupWindowState>,
}

impl Global for ShellWindows {}

impl ShellWindows {
    pub fn init(cx: &mut App) {
        cx.set_global(Self::default());
    }

    fn is_alive(cx: &App, handle: AnyWindowHandle) -> bool {
        cx.windows().contains(&handle)
    }

    /// A panel or the launcher is open and holds the keyboard, so the
    /// compositor reports no focused toplevel.
    pub fn holds_keyboard_focus(cx: &App) -> bool {
        let Some(this) = cx.try_global::<Self>() else {
            return false;
        };
        this.panel
            .as_ref()
            .is_some_and(|panel| Self::is_alive(cx, panel.handle))
            || this
                .launcher
                .is_some_and(|handle| Self::is_alive(cx, handle.into()))
    }

    /// Remember the launcher window. Liveness is checked on use.
    pub fn set_launcher(cx: &mut App, handle: Option<WindowHandle<Launcher>>) {
        cx.default_global::<Self>().launcher = handle;
    }

    pub fn take_launcher(cx: &mut App) -> Option<WindowHandle<Launcher>> {
        cx.default_global::<Self>().launcher.take()
    }

    pub fn record_display(cx: &mut App, handle: AnyWindowHandle, display_id: Option<DisplayId>) {
        if let Some(display_id) = display_id {
            let open = cx.windows();
            let displays = &mut cx.default_global::<Self>().displays;
            displays.retain(|handle, _| open.contains(handle));
            displays.insert(handle, display_id);
        }
    }

    pub fn display_for_window(cx: &App, window: &Window) -> Option<DisplayId> {
        cx.try_global::<Self>()?
            .displays
            .get(&window.window_handle())
            .copied()
    }

    /// The output `window` was opened on. Prefer this to `window.display()`,
    /// which is unreliable for layer-shell windows.
    pub fn display(cx: &App, window: &Window) -> Option<Rc<dyn PlatformDisplay>> {
        Self::display_for_window(cx, window)
            .and_then(|display_id| cx.find_display(display_id))
            .or_else(|| window.display(cx))
    }

    /// How deep the shell's own exclusive zones are on each edge: the bar,
    /// the frame's strips and, `with_dock`, a dock that reserves its space.
    /// The compositor lays a new layer surface out inside these and counts
    /// its margins from there, which gpui's display bounds do not know.
    pub fn reserved_edges(cx: &App, with_dock: bool) -> gpui::Edges<Pixels> {
        use ui::patterns::frame::edge_mut;
        let config = crate::config::Config::global(cx);
        let mut edges = gpui::Edges::all(px(0.));
        let add = |edges: &mut gpui::Edges<Pixels>, edge: BarPosition, depth: f32| {
            let slot = edge_mut(edges, edge.edge());
            *slot += px(depth);
        };
        add(&mut edges, config.bar.position, config.bar.depth());
        for (edge, thickness) in crate::frame::strips(config) {
            add(&mut edges, edge, thickness);
        }
        if with_dock && config.dock.enabled && config.dock.reserve_space {
            let dock = &config.dock;
            add(
                &mut edges,
                dock.position,
                crate::dock::thickness(dock) + dock.edge_gap(),
            );
        }
        edges
    }

    pub(crate) fn osd(cx: &App) -> Option<crate::osd::OsdWindowState> {
        cx.try_global::<Self>()?.osd.clone()
    }

    pub(crate) fn set_osd(cx: &mut App, state: Option<crate::osd::OsdWindowState>) {
        cx.default_global::<Self>().osd = state;
    }

    pub(crate) fn take_popups(
        cx: &mut App,
    ) -> HashMap<Option<DisplayId>, crate::notification::popup::PopupWindowState> {
        std::mem::take(&mut cx.default_global::<Self>().popups)
    }

    pub(crate) fn set_popups(
        cx: &mut App,
        popups: HashMap<Option<DisplayId>, crate::notification::popup::PopupWindowState>,
    ) {
        cx.default_global::<Self>().popups = popups;
    }

    /// The ID of the open panel, if any.
    pub fn active_panel_id(cx: &App) -> Option<String> {
        let this = cx.try_global::<Self>()?;
        let panel = this.panel.as_ref()?;
        Self::is_alive(cx, panel.handle).then(|| panel.id.clone())
    }

    /// Whether the panel with this ID is open.
    #[allow(dead_code)]
    pub fn is_panel_open(cx: &App, panel_id: &str) -> bool {
        Self::active_panel_id(cx).as_deref() == Some(panel_id)
    }

    /// Open this panel on `window`'s output, or close it if it is the one
    /// open. Any other panel closes first. Returns true if opened.
    pub fn toggle_panel<V: Render + 'static>(
        cx: &mut App,
        window: &Window,
        panel_id: &str,
        config: PanelConfig,
        build: impl FnOnce(&mut Context<V>) -> V + 'static,
    ) -> bool {
        let display_id = Self::display_for_window(cx, window);
        Self::toggle_panel_on_display(cx, panel_id, config, display_id, build)
    }

    /// [`Self::toggle_panel`] on an explicitly selected output.
    pub fn toggle_panel_on_display<V: Render + 'static>(
        cx: &mut App,
        panel_id: &str,
        config: PanelConfig,
        display_id: Option<DisplayId>,
        build: impl FnOnce(&mut Context<V>) -> V + 'static,
    ) -> bool {
        if let Some(open) = Self::set_panel(cx, None) {
            // A panel that already closed itself does not count as a toggle.
            let was_open = cx
                .update_window(open.handle, |_, window, _cx| {
                    window.remove_window();
                })
                .is_ok();
            if was_open && open.id == panel_id {
                return false;
            }
        }

        let initial = config.size.initial();
        let vertical_margin = px(config.margin.0 + config.margin.2);
        let window_options = WindowOptions {
            display_id,
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::new(px(0.), px(0.)),
                size: initial,
            })),
            app_id: Some(format!("conchy-shell-panel-{}", panel_id)),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: config.namespace,
                layer: Layer::Overlay,
                anchor: config.anchor,
                exclusive_zone: None,
                margin: Some((
                    px(config.margin.0),
                    px(config.margin.1),
                    px(config.margin.2),
                    px(config.margin.3),
                )),
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                ..Default::default()
            }),
            focus: true,
            ..Default::default()
        };

        let size = config.size;
        let Ok(handle) = cx.open_window(window_options, move |window, cx| {
            window.set_rem_size(cx.theme().font_size);
            let handle = window.window_handle();
            cx.new(|cx| {
                // Panels also close themselves (Escape, picking an entry).
                cx.on_release(move |_, cx| Self::forget_panel(cx, handle))
                    .detach();
                PanelHost {
                    content: cx.new(build),
                    size,
                    vertical_margin,
                    fit: WindowFit::default(),
                }
            })
        }) else {
            return false;
        };

        Self::record_display(cx, handle.into(), display_id);
        Self::set_panel(
            cx,
            Some(OpenPanel {
                id: panel_id.to_string(),
                handle: handle.into(),
            }),
        );
        true
    }

    /// Replace the tracked panel, returning the previous one.
    fn set_panel(cx: &mut App, panel: Option<OpenPanel>) -> Option<OpenPanel> {
        let this = cx.default_global::<Self>();
        this.panel_open.set_neq(panel.is_some());
        std::mem::replace(&mut this.panel, panel)
    }

    /// Stop tracking `handle` once its window is gone.
    fn forget_panel(cx: &mut App, handle: AnyWindowHandle) {
        let this = cx.default_global::<Self>();
        if this
            .panel
            .as_ref()
            .is_some_and(|open| open.handle == handle)
        {
            Self::set_panel(cx, None);
        }
    }

    /// Whether a panel is open, as a signal.
    pub fn panel_open_signal(cx: &mut App) -> impl Signal<Item = bool> + use<> {
        cx.default_global::<Self>().panel_open.signal()
    }

    /// Close the open panel, if any.
    pub fn close_panel(cx: &mut App) {
        if let Some(open) = Self::set_panel(cx, None) {
            // A panel closing itself is mid-update and `update_window` would
            // fail, leaving it open but untracked; remove it once that ends.
            cx.defer(move |cx| {
                let _ = cx.update_window(open.handle, |_, window, _cx| {
                    window.remove_window();
                });
            });
        }
    }
}
