//! The bar widget contract and the host that renders one.
//!
//! A widget is an entity with two render methods, one per orientation, that
//! take the [`BarCtx`] the bar hands it. It declares its chrome (shell and
//! interactivity) and the host draws it: the widget never builds a chip.
//! Each widget directory exposes a [`BarWidgetDescriptor`]; `mod.rs` lists
//! them once.

use gpui::{AnyElement, App, AppContext, Context, Entity, IntoElement, Window};
use ui::IconName;
use ui::patterns::BarChip;

use super::ctx::BarCtx;

/// The chip a widget asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarWidgetShell {
    /// Bare content: the widget has nothing to show.
    None,
    /// One icon and a value.
    Standard,
    /// Several items side by side (workspaces, tray).
    Group,
}

/// What a bar widget renders. `Self` is an entity, so handlers come from
/// `cx.listener`; it never implements `Render`, the host does.
pub(crate) trait BarWidget: Sized + 'static {
    fn render_vertical(
        &mut self,
        ctx: &BarCtx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement;

    fn render_horizontal(
        &mut self,
        ctx: &BarCtx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement;

    /// Clickable: the chip paints a hover and a pointer cursor.
    fn is_interactive(&self) -> bool {
        false
    }

    fn shell(&self) -> BarWidgetShell {
        BarWidgetShell::Standard
    }
}

/// What the config calls a widget, and how the editor shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarWidgetMeta {
    /// The spelling the default config uses.
    pub name: &'static str,
    /// Older or shorter spellings that still parse.
    pub aliases: &'static [&'static str],
    pub label: &'static str,
    pub description: &'static str,
    pub icon: IconName,
}

impl BarWidgetMeta {
    pub fn matches(&self, name: &str) -> bool {
        self.name == name || self.aliases.contains(&name)
    }
}

/// A widget the bar can build by name.
pub struct BarWidgetDescriptor {
    pub meta: BarWidgetMeta,
    build: fn(&mut App) -> Box<dyn DynWidget>,
}

impl BarWidgetDescriptor {
    /// `build` is the widget's `new`, wrapped: `|cx| Box::new(cx.new(Clock::new))`.
    pub(crate) const fn new(
        meta: BarWidgetMeta,
        build: fn(&mut App) -> Box<dyn DynWidget>,
    ) -> Self {
        Self { meta, build }
    }

    pub(crate) fn build(&'static self, ctx: BarCtx, cx: &mut App) -> Widget {
        let widget = (self.build)(cx);
        let host = cx.new(|cx| {
            widget.observe(cx);
            WidgetHost { widget, ctx }
        });
        Widget { host }
    }
}

/// `Box::new(cx.new(W::new))` as a descriptor's `build`.
pub(crate) fn boxed<W: BarWidget>(
    cx: &mut App,
    new: fn(&mut Context<W>) -> W,
) -> Box<dyn DynWidget> {
    Box::new(cx.new(new))
}

/// A widget entity with its type erased, so one host type serves them all.
pub(crate) trait DynWidget {
    fn render(&self, ctx: &BarCtx, window: &mut Window, cx: &mut App) -> AnyElement;
    fn chrome(&self, cx: &App) -> (BarWidgetShell, bool);
    /// Re-render the host when the widget notifies; the widget is not a view.
    fn observe(&self, cx: &mut Context<WidgetHost>);
}

impl<W: BarWidget> DynWidget for Entity<W> {
    fn render(&self, ctx: &BarCtx, window: &mut Window, cx: &mut App) -> AnyElement {
        self.update(cx, |widget, cx| {
            if ctx.vertical {
                widget.render_vertical(ctx, window, cx)
            } else {
                widget.render_horizontal(ctx, window, cx)
            }
        })
    }

    fn chrome(&self, cx: &App) -> (BarWidgetShell, bool) {
        let widget = self.read(cx);
        (widget.shell(), widget.is_interactive())
    }

    fn observe(&self, cx: &mut Context<WidgetHost>) {
        cx.observe(self, |_, _, cx| cx.notify()).detach();
    }
}

/// The view the bar places: owns the widget's [`BarCtx`] and draws its chip.
pub(crate) struct WidgetHost {
    widget: Box<dyn DynWidget>,
    ctx: BarCtx,
}

impl WidgetHost {
    fn set_ctx(&mut self, ctx: &BarCtx, cx: &mut Context<Self>) {
        if self.ctx != *ctx {
            self.ctx = ctx.clone();
            cx.notify();
        }
    }
}

impl gpui::Render for WidgetHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ctx = self.ctx.clone();
        let (shell, interactive) = self.widget.chrome(cx);
        let content = self.widget.render(&ctx, window, cx);
        match shell {
            BarWidgetShell::None => content,
            BarWidgetShell::Standard | BarWidgetShell::Group => BarChip::new(content)
                .vertical(ctx.vertical)
                .geometry(if shell == BarWidgetShell::Group {
                    ctx.chip.group
                } else {
                    ctx.chip.single
                })
                .background(ctx.chip.background)
                .border(ctx.chip.border)
                .hover(interactive.then_some(ctx.chip.hover))
                .into_any_element(),
        }
    }
}

/// A placed widget: the host that renders it.
pub struct Widget {
    host: Entity<WidgetHost>,
}

impl Widget {
    pub fn render(&self) -> AnyElement {
        self.host.clone().into_any_element()
    }

    /// Hand the widget a new placement; a no-op when nothing changed.
    pub fn set_ctx(&self, ctx: &BarCtx, cx: &mut App) {
        self.host.update(cx, |host, cx| host.set_ctx(ctx, cx));
    }
}
