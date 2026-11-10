//! System info module - one overview of CPU, memory, swap, temperature,
//! network and disks, following the sysinfo service live.
//!
//! `;sys` opens it; so does clicking the bar's SysInfo widget.

pub mod config;

use gpui::{App, Context, FontWeight, Hsla, IntoElement, Render, Window, div, prelude::*, px};
use services::{DiskInfo, SysInfoData};
use ui::patterns::PanelSurface;
use ui::{
    ActiveTheme, CircularProgress, Color, Icon, IconName, IconSize, Label, LabelCommon,
    ProgressBar, Spacing, TextSize, h_flex, v_flex,
};

use self::config::SysInfoConfig;
use crate::launcher::action::{Action, ActionPanel};
use crate::launcher::content::{Content, Custom};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};
use crate::state::{AppState, watch};

const SYSINFO_ICON: IconName = IconName::Cpu;

/// Width of the mount-point column in the disks table.
const MOUNT_WIDTH: f32 = 96.0;
/// Diameter of the CPU and memory rings.
const GAUGE_SIZE: f32 = 64.0;
const GAUGE_STROKE: f32 = 6.0;

pub fn descriptor(config: &SysInfoConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "sysinfo",
            prefix: config.prefix.clone().into(),
            name: "System Info",
            icon: SYSINFO_ICON,
            description: "CPU, memory, temperature, network and disks at a glance",
            default: false,
            in_help: true,
        },
        SysInfoModule::new,
    )
}

pub struct SysInfoModule {
    data: SysInfoData,
}

impl SysInfoModule {
    fn new(cx: &mut Context<Self>) -> Self {
        let subscriber = AppState::sysinfo(cx).clone();
        let data = subscriber.get();
        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });
        Self { data }
    }
}

impl LauncherPage for SysInfoModule {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let mut actions = ActionPanel::new();
        if let Some(ip) = &self.data.network.ip {
            actions = actions.action(Action::copy(ip.clone()).title("Copy IP"));
        }
        Custom::new(cx.entity()).actions(actions).into()
    }
}

impl Render for SysInfoModule {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let data = &self.data;
        let status = &cx.theme().colors().status;

        let memory_detail = format!(
            "{} used of {}",
            gb(data.memory_used_gb),
            gb(data.memory_total_gb)
        );
        let (temp_value, temp_color, temp_icon) = match data.temperature {
            Some(temp) => (
                format!("{temp}°C"),
                Color::Custom(status.from_temperature(temp)),
                if temp >= 70 {
                    IconName::Flame
                } else {
                    IconName::Thermometer
                },
            ),
            None => ("N/A".to_string(), Color::Muted, IconName::Thermometer),
        };
        let net = &data.network;
        let speeds = format!("↓ {}  ↑ {}", net.download_str(), net.upload_str());
        let ip = net.ip.clone().unwrap_or_else(|| "No IP".to_string());

        v_flex()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Medium.pixels())
            .child(
                h_flex()
                    .gap(Spacing::Medium.pixels())
                    .child(gauge(
                        "CPU",
                        cpu_icon(data.cpu_usage),
                        data.cpu_usage,
                        "Processor load".to_string(),
                        cx,
                    ))
                    .child(gauge(
                        "Memory",
                        IconName::MemoryStick,
                        data.memory_usage,
                        memory_detail,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(Spacing::Medium.pixels())
                    .child(stat(
                        "Swap",
                        IconName::ArrowDownUp,
                        format!("{}%", data.swap_usage),
                        Color::Custom(status.from_percentage(data.swap_usage)),
                        "Swap memory in use".to_string(),
                        cx,
                    ))
                    .child(stat(
                        "Temperature",
                        temp_icon,
                        temp_value,
                        temp_color,
                        "CPU package".to_string(),
                        cx,
                    ))
                    .child(stat(
                        "Network",
                        IconName::Network,
                        speeds,
                        Color::Default,
                        ip,
                        cx,
                    )),
            )
            .when(!data.disks.is_empty(), |el| {
                el.child(
                    v_flex()
                        .p(Spacing::Large.pixels())
                        .gap(Spacing::Small.pixels())
                        .panel_card(cx)
                        .child(card_title("Disks", IconName::HardDrive))
                        .children(data.disks.iter().map(|disk| disk_row(disk, cx))),
                )
            })
    }
}

/// Muted icon-and-title line every card opens with.
fn card_title(title: &str, icon: IconName) -> impl IntoElement {
    h_flex()
        .gap(Spacing::Small.pixels())
        .child(Icon::new(icon).size(IconSize::Small).color(Color::Muted))
        .child(
            Label::new(title.to_string())
                .size(TextSize::Small)
                .color(Color::Muted),
        )
}

/// A card with a ring gauge beside the title and one detail line.
fn gauge(title: &str, icon: IconName, usage: u32, detail: String, cx: &App) -> impl IntoElement {
    let color = cx.theme().colors().status.from_percentage(usage);
    h_flex()
        .flex_1()
        .p(Spacing::Large.pixels())
        .gap(Spacing::Large.pixels())
        .panel_card(cx)
        .child(ring(usage, color, cx))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(Spacing::XSmall.pixels())
                .child(card_title(title, icon))
                .child(Label::new(detail).size(TextSize::Small)),
        )
}

/// A ring with the percentage in its centre.
fn ring(usage: u32, color: Hsla, cx: &App) -> impl IntoElement {
    div()
        .relative()
        .flex_none()
        .size(px(GAUGE_SIZE))
        .child(
            CircularProgress::new(usage.min(100) as f32, 100., px(GAUGE_SIZE))
                .stroke_width(px(GAUGE_STROKE))
                .progress_color(color)
                .bg_color(cx.theme().colors().elevated_surface_background),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Label::new(format!("{usage}%"))
                        .size(TextSize::Small)
                        .weight(FontWeight::BOLD)
                        .color(Color::Custom(color)),
                ),
        )
}

/// A small card with one value and an optional line under it.
fn stat(
    title: &str,
    icon: IconName,
    value: String,
    color: Color,
    sub: String,
    cx: &App,
) -> impl IntoElement {
    v_flex()
        .flex_1()
        .p(Spacing::Large.pixels())
        .gap(Spacing::XSmall.pixels())
        .panel_card(cx)
        .child(card_title(title, icon))
        .child(
            Label::new(value)
                .size(TextSize::Medium)
                .weight(FontWeight::MEDIUM)
                .color(color),
        )
        .child(Label::new(sub).size(TextSize::XSmall).color(Color::Muted))
}

fn disk_row(disk: &DiskInfo, cx: &App) -> impl IntoElement {
    let color = cx
        .theme()
        .colors()
        .status
        .from_percentage(disk.usage_percent);
    h_flex()
        .gap(Spacing::Medium.pixels())
        .items_center()
        .child(
            div()
                .w(px(MOUNT_WIDTH))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(Label::new(disk.mount_point.clone()).size(TextSize::Small)),
        )
        .child(div().flex_1().child(bar(disk.usage_percent, color, cx)))
        .child(
            div().w(px(40.)).flex().justify_end().child(
                Label::new(format!("{}%", disk.usage_percent))
                    .size(TextSize::Small)
                    .weight(FontWeight::MEDIUM)
                    .color(Color::Custom(color)),
            ),
        )
        .child(
            div().w(px(132.)).flex().justify_end().child(
                Label::new(format!("{} / {}", gb(disk.used_gb), gb(disk.total_gb)))
                    .size(TextSize::XSmall)
                    .color(Color::Muted),
            ),
        )
}

fn bar(usage: u32, color: Hsla, cx: &App) -> ProgressBar {
    ProgressBar::new(usage.min(100) as f32, 100.)
        .fg_color(color)
        .bg_color(cx.theme().colors().elevated_surface_background)
}

fn cpu_icon(usage: u32) -> IconName {
    if usage >= 90 {
        IconName::Flame
    } else {
        IconName::Cpu
    }
}

fn gb(value: f64) -> String {
    format!("{value:.1} GB")
}
