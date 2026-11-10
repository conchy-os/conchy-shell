//! SysInfo widget showing CPU and memory usage in the bar.
//!
//! Clicking the widget opens the launcher's system info module.

use gpui::{AnyElement, Context, Hsla, Window, prelude::*};
use services::SysInfoData;
use ui::{ButtonCommon, ButtonLike, ButtonStyle, Clickable, IconName};

mod config;
pub use config::SysInfoConfig;

use super::{BarCtx, BarWidget, BarWidgetDescriptor, BarWidgetMeta, boxed, style};
use crate::config::ActiveConfig;
use crate::launcher;
use crate::state::AppState;
use crate::state::watch;

pub const fn descriptor() -> BarWidgetDescriptor {
    BarWidgetDescriptor::new(
        BarWidgetMeta {
            name: "SysInfo",
            aliases: &[],
            label: "System info",
            description: "CPU, memory and temperature readouts.",
            icon: IconName::Cpu,
        },
        |cx| boxed(cx, SysInfo::new),
    )
}

/// SysInfo widget showing CPU and memory usage in the bar.
pub struct SysInfo {
    data: SysInfoData,
}

struct SysInfoStat {
    icon: IconName,
    text: String,
    color: Hsla,
}

impl SysInfo {
    /// Create a new SysInfo widget.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscriber = AppState::sysinfo(cx).clone();
        let initial_data = subscriber.get();

        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });

        SysInfo { data: initial_data }
    }

    fn cpu_icon(&self) -> IconName {
        if self.data.cpu_usage >= 90 {
            IconName::Flame
        } else {
            IconName::Cpu
        }
    }

    fn memory_icon(&self) -> IconName {
        if self.data.memory_usage >= 90 {
            IconName::ArrowDownUp
        } else {
            IconName::MemoryStick
        }
    }

    fn stats(&self, ctx: &BarCtx, config: &SysInfoConfig) -> Vec<SysInfoStat> {
        let mut stats = Vec::new();

        // Neutral until a reading deserves attention, like the battery.
        let usage_ink = |usage: u32| {
            ctx.ink
                .status
                .usage_attention(usage)
                .unwrap_or(ctx.ink.text)
        };

        if config.show_cpu {
            let cpu_usage = self.data.cpu_usage;
            stats.push(SysInfoStat {
                icon: self.cpu_icon(),
                text: style::compact_percent(ctx, cpu_usage),
                color: usage_ink(cpu_usage),
            });
        }

        if config.show_memory {
            let memory_usage = self.data.memory_usage;
            stats.push(SysInfoStat {
                icon: self.memory_icon(),
                text: style::compact_percent(ctx, memory_usage),
                color: usage_ink(memory_usage),
            });
        }

        if config.show_temp
            && let Some(temp) = self.data.temperature
        {
            stats.push(SysInfoStat {
                icon: if temp >= 70 {
                    IconName::Flame
                } else {
                    IconName::Thermometer
                },
                text: if ctx.vertical {
                    temp.to_string()
                } else {
                    format!("{temp}°C")
                },
                color: ctx
                    .ink
                    .status
                    .temperature_attention(temp)
                    .unwrap_or(ctx.ink.text),
            });
        }

        stats
    }

    fn render(&self, ctx: &BarCtx, cx: &mut Context<Self>) -> AnyElement {
        let stats = self.stats(ctx, &cx.config().bar.modules.sysinfo);

        ButtonLike::new("sysinfo-widget")
            .style(ButtonStyle::Transparent)
            .on_click(cx.listener(|_, _, _, cx| {
                let prefix = cx.config().launcher.modules.sysinfo.prefix.clone();
                launcher::toggle(Some(prefix), cx);
            }))
            .child(
                style::stack(ctx)
                    .justify_center()
                    .gap(ctx.metrics.gap)
                    .children(
                        stats
                            .into_iter()
                            .map(|stat| style::stat(ctx, stat.icon, stat.text, stat.color)),
                    ),
            )
            .into_any_element()
    }
}

impl BarWidget for SysInfo {
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
