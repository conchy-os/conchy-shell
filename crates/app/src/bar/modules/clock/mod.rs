//! Clock widget that displays the current date and time.

mod config;
pub use config::ClockConfig;

use chrono::Local;
use gpui::{AnyElement, Context, Window, prelude::*};
use std::time::Duration;
use ui::IconName;

use super::{BarCtx, BarWidget, BarWidgetDescriptor, BarWidgetMeta, boxed, style};
use crate::config::ActiveConfig;

pub const fn descriptor() -> BarWidgetDescriptor {
    BarWidgetDescriptor::new(
        BarWidgetMeta {
            name: "Clock",
            aliases: &[],
            label: "Clock",
            description: "Current time and date.",
            icon: IconName::Clock,
        },
        |cx| boxed(cx, Clock::new),
    )
}

/// A clock widget that updates every second.
pub struct Clock;

impl Clock {
    /// Create a new clock widget that auto-updates.
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                let _ = this.update(cx, |_, cx| cx.notify());
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        })
        .detach();

        Clock
    }

    fn formatted(format: &str) -> String {
        Local::now().format(format).to_string()
    }

    /// One entry per non-empty line of the vertical format, falling back to
    /// the horizontal format when it yields nothing.
    fn lines(vertical_format: &str, horizontal_fallback: &str) -> Vec<String> {
        let lines: Vec<String> = Self::formatted(vertical_format)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        if lines.is_empty() {
            vec![Self::formatted(horizontal_fallback)]
        } else {
            lines
        }
    }
}

impl BarWidget for Clock {
    fn render_vertical(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let config = &cx.config().bar.modules.clock;
        let lines = Self::lines(&config.format_vertical, &config.format_horizontal);

        style::stack(ctx)
            .id("clock")
            .gap(ctx.metrics.gap)
            .children(lines.into_iter().enumerate().map(|(idx, line)| {
                // Hours and minutes carry the time; anything after (seconds,
                // a date) is secondary and muted.
                let color = if idx < 2 { ctx.ink.text } else { ctx.ink.muted };
                style::text_line(ctx, style::label_in(ctx, line, color))
            }))
            .into_any_element()
    }

    fn render_horizontal(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let config = &cx.config().bar.modules.clock;

        style::stack(ctx)
            .id("clock")
            .gap(ctx.metrics.gap)
            .child(style::label(
                ctx,
                Self::formatted(&config.format_horizontal),
            ))
            .into_any_element()
    }
}
