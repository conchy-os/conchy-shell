use gpui::{App, FontWeight, div, prelude::*, px};
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, TextSize,
    h_flex, v_flex,
};

use crate::launcher::content::Detail;

pub struct SystemInfo {
    pub os_name: String,
    pub kernel: String,
    pub hostname: String,
    pub cpu: String,
    pub memory: String,
    pub uptime: String,
    pub compositor: String,
}

pub fn get_system_info() -> SystemInfo {
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "Linux".to_string());

    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "localhost".to_string());

    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1))
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "Unknown CPU".to_string());

    let memory = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|kb| kb.parse::<u64>().ok())
                .map(|kb| format!("{:.1} GB", kb as f64 / 1_048_576.0))
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let uptime = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|info| {
            info.split_whitespace()
                .next()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|secs| {
                    let total_mins = (secs / 60.0) as u64;
                    let hours = total_mins / 60;
                    let mins = total_mins % 60;
                    if hours > 0 {
                        format!("{}h {}m", hours, mins)
                    } else {
                        format!("{}m", mins)
                    }
                })
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let os_name = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|content| {
            content.lines().find(|l| l.starts_with("PRETTY_NAME=")).map(|l| {
                l.trim_start_matches("PRETTY_NAME=")
                    .trim_matches('"')
                    .to_string()
            })
        })
        .unwrap_or_else(|| "Conchy OS (Linux)".to_string());

    SystemInfo {
        os_name,
        kernel,
        hostname,
        cpu,
        memory,
        uptime,
        compositor: "Conchy WM / Wayland".to_string(),
    }
}

pub fn about_detail(_cx: &App) -> Detail {
    Detail::new().widget(move |cx| {
        let theme = cx.theme();
        let colors = theme.colors();
        let info = get_system_info();

        let header = v_flex()
            .w_full()
            .gap(Spacing::Small.pixels())
            .child(
                h_flex()
                    .w_full()
                    .gap(Spacing::Small.pixels())
                    .items_center()
                    .child(Icon::new(IconName::Info).size(IconSize::Medium))
                    .child(
                        Label::new("About System")
                            .size(TextSize::Medium)
                            .weight(FontWeight::BOLD),
                    )
                    .child(
                        div()
                            .px(Spacing::Small.pixels())
                            .py(px(2.))
                            .rounded(Radius::Small.pixels())
                            .bg(colors.element_background)
                            .child(
                                Label::new("Conchy OS")
                                    .size(TextSize::XSmall)
                                    .color(Color::Custom(colors.status.info)),
                            ),
                    ),
            )
            .child(
                Label::new("System hardware, operating system, and desktop environment specifications")
                    .size(TextSize::Small)
                    .color(Color::Muted),
            );

        let system_card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .rounded(Radius::Large.pixels())
            .bg(colors.surface_background)
            .border_1()
            .border_color(colors.border_variant)
            .gap(Spacing::Medium.pixels())
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap(Spacing::Medium.pixels())
                            .items_center()
                            .child(
                                div()
                                    .p(Spacing::Small.pixels())
                                    .rounded(Radius::Medium.pixels())
                                    .bg(colors.element_background)
                                    .child(
                                        Icon::new(IconName::ScreenShare)
                                            .size(IconSize::Medium)
                                            .color(Color::Custom(colors.accent)),
                                    ),
                            )
                            .child(
                                v_flex()
                                    .child(
                                        Label::new(info.os_name.clone())
                                            .size(TextSize::Medium)
                                            .weight(FontWeight::BOLD),
                                    )
                                    .child(
                                        Label::new(format!("Kernel {}", info.kernel))
                                            .size(TextSize::Small)
                                            .color(Color::Muted),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .px(Spacing::Medium.pixels())
                            .py(px(4.))
                            .rounded(Radius::Full.pixels())
                            .bg(colors.element_background)
                            .child(
                                Label::new(format!("Up {}", info.uptime))
                                    .size(TextSize::XSmall)
                                    .color(Color::Muted),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap(Spacing::Small.pixels())
                    .pt(Spacing::Small.pixels())
                    .border_t_1()
                    .border_color(colors.border_variant)
                    .child(spec_row("Host", &info.hostname, cx))
                    .child(spec_row("Processor", &info.cpu, cx))
                    .child(spec_row("Memory", &info.memory, cx))
                    .child(spec_row("Compositor", &info.compositor, cx))
                    .child(spec_row("Shell", "Conchy Shell (GPUI 60+ FPS)", cx)),
            );

        let hint = Label::new("Press Enter to view complete hardware and environment details")
            .size(TextSize::Small)
            .color(Color::Muted);

        v_flex()
            .w_full()
            .gap(Spacing::Large.pixels())
            .child(header)
            .child(system_card)
            .child(hint)
            .into_any_element()
    })
}

fn spec_row(label: &'static str, val: &str, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    h_flex()
        .w_full()
        .justify_between()
        .items_center()
        .py(px(2.))
        .child(
            Label::new(label)
                .size(TextSize::Small)
                .color(Color::Muted),
        )
        .child(
            Label::new(val.to_string())
                .size(TextSize::Small)
                .color(Color::Custom(colors.text)),
        )
}
