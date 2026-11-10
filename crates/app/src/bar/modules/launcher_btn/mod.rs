//! Launcher button widget for opening the application launcher.

mod config;
pub use config::LauncherBtnConfig;

use crate::launcher;
use gpui::{AnyElement, Context, Window, prelude::*};
use ui::{ButtonCommon, ButtonLike, ButtonStyle, Clickable, IconName};

use super::{BarCtx, BarWidget, BarWidgetDescriptor, BarWidgetMeta, boxed, style};
use crate::config::ActiveConfig;

pub const fn descriptor() -> BarWidgetDescriptor {
    BarWidgetDescriptor::new(
        BarWidgetMeta {
            name: "LauncherBtn",
            aliases: &["Launcher"],
            label: "Launcher button",
            description: "Opens the command launcher.",
            icon: IconName::MagnifyingGlass,
        },
        |cx| boxed(cx, LauncherBtn::new),
    )
}

/// A button widget that opens the launcher when clicked.
pub struct LauncherBtn;

const LAUNCHER_ICON: IconName = IconName::Layers;

impl LauncherBtn {
    /// Create a new launcher button.
    pub fn new(_cx: &mut Context<Self>) -> Self {
        LauncherBtn
    }

    fn render(&self, ctx: &BarCtx, cx: &mut Context<Self>) -> AnyElement {
        let config = &cx.config().bar.modules.launcher_btn;
        let icon = crate::icons::source_or(config.icon.as_ref(), LAUNCHER_ICON);

        // The chip around this button already paints the hover and owns the
        // padding, so the button itself stays transparent and unpadded.
        ButtonLike::new("launcher-btn")
            .style(ButtonStyle::Transparent)
            .on_click(cx.listener(move |_, _, _, cx| {
                launcher::toggle(None, cx);
            }))
            .child(style::icon(ctx, icon))
            .into_any_element()
    }
}

impl BarWidget for LauncherBtn {
    fn is_interactive(&self) -> bool {
        true
    }

    fn render_vertical(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, cx)
    }

    fn render_horizontal(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, cx)
    }
}
