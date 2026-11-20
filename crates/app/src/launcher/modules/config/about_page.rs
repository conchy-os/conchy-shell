use gpui::{App, Context, prelude::*};
use ui::IconName;

use super::about_detail::{SystemInfo, get_system_info};
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};

pub struct AboutPage {
    info: SystemInfo,
}

impl AboutPage {
    pub fn open(cx: &mut App) -> Page {
        Page::new(cx.new(|_| Self::new()))
    }

    pub fn new() -> Self {
        Self {
            info: get_system_info(),
        }
    }
}

impl LauncherPage for AboutPage {
    fn content(&mut self, _query: &str, _cx: &mut Context<Self>) -> Content {
        let info = &self.info;

        let mut list = List::new()
            .show_detail(true)
            .click_selects(true)
            .title("About System")
            .placeholder("Search system info…");

        // Action: Copy summary to clipboard
        let summary_text = format!(
            "OS: {}\nKernel: {}\nHost: {}\nCPU: {}\nMemory: {}\nCompositor: {}\nUptime: {}",
            info.os_name, info.kernel, info.hostname, info.cpu, info.memory, info.compositor, info.uptime
        );
        let summary_clone = summary_text.clone();
        let actions = ActionPanel::default().action(Action::new("Copy System Info", move |_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(summary_clone.clone()));
            Outcome::Stay
        }));

        list = list.actions(actions);

        // 1. Operating System section
        let os_items = vec![
            Item::new("os_name", "Operating System")
                .subtitle(info.os_name.clone())
                .icon(IconName::ScreenShare)
                .accessory(Accessory::text(info.os_name.clone()))
                .action(copy_action("Copy OS Name", &info.os_name)),
            Item::new("kernel", "Linux Kernel")
                .subtitle(info.kernel.clone())
                .icon(IconName::Sliders)
                .accessory(Accessory::text(info.kernel.clone()))
                .action(copy_action("Copy Kernel", &info.kernel)),
            Item::new("hostname", "Host Name")
                .subtitle(info.hostname.clone())
                .icon(IconName::Globe)
                .accessory(Accessory::text(info.hostname.clone()))
                .action(copy_action("Copy Hostname", &info.hostname)),
            Item::new("arch", "Architecture")
                .subtitle(std::env::consts::ARCH)
                .icon(IconName::Cpu)
                .accessory(Accessory::text(std::env::consts::ARCH))
                .action(copy_action("Copy Architecture", std::env::consts::ARCH)),
        ];
        list = list.section(Section::new("Operating System").items(os_items));

        // 2. Hardware section
        let hw_items = vec![
            Item::new("cpu", "Processor")
                .subtitle(info.cpu.clone())
                .icon(IconName::Cpu)
                .accessory(Accessory::text(info.cpu.clone()))
                .action(copy_action("Copy CPU Info", &info.cpu)),
            Item::new("memory", "Total Memory (RAM)")
                .subtitle(info.memory.clone())
                .icon(IconName::Layers)
                .accessory(Accessory::text(info.memory.clone()))
                .action(copy_action("Copy RAM Info", &info.memory)),
            Item::new("uptime", "System Uptime")
                .subtitle(format!("Running for {}", info.uptime))
                .icon(IconName::Clock)
                .accessory(Accessory::text(info.uptime.clone()))
                .action(copy_action("Copy Uptime", &info.uptime)),
        ];
        list = list.section(Section::new("Hardware").items(hw_items));

        // 3. Desktop Environment section
        let de_items = vec![
            Item::new("compositor", "Window Manager / Compositor")
                .subtitle(info.compositor.clone())
                .icon(IconName::Layout)
                .accessory(Accessory::text("Smithay / Niri"))
                .action(copy_action("Copy Compositor", &info.compositor)),
            Item::new("shell", "Desktop Shell")
                .subtitle("Conchy Shell (GPU-accelerated GPUI)")
                .icon(IconName::Settings)
                .accessory(Accessory::text("GPUI 60+ FPS"))
                .action(copy_action("Copy Shell", "Conchy Shell (GPUI)")),
            Item::new("display_server", "Display Protocol")
                .subtitle("Wayland Native")
                .icon(IconName::ScreenShare)
                .accessory(Accessory::text("Wayland"))
                .action(copy_action("Copy Protocol", "Wayland")),
        ];
        list = list.section(Section::new("Desktop Environment").items(de_items));

        list.into()
    }
}

fn copy_action(label: &'static str, text: &str) -> Action {
    let t = text.to_string();
    Action::new(label, move |_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(t.clone()));
        Outcome::Stay
    })
}
