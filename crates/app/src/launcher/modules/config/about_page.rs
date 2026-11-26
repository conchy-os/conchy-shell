//! About: the form beside the "About System" row. What the computer is, and
//! one key to copy it all for a bug report.

use gpui::{App, SharedString};
use ui::IconName;

use super::about_detail::SystemInfo;
use super::settings_form as rows;
use crate::launcher::action::Outcome;
use crate::launcher::content::{Detail, Form, Item};

pub fn item(info: &std::rc::Rc<SystemInfo>) -> Item {
    let info = info.clone();
    Item::new("about", "About System")
        .subtitle("System specifications, OS, and hardware details")
        .icon(IconName::Info)
        .keywords("about system info specs hardware cpu ram memory kernel os")
        .actions(super::system::edit())
        .detail(Detail::new().form(move |form, cx| about_form(form, &info, cx)))
}

fn about_form(form: &mut Form, info: &SystemInfo, cx: &App) {
    rows::title(
        form,
        "About System",
        "What this computer is and what it runs",
    );

    rows::heading(form, "Operating system");
    rows::info(form, "System", info.os_name.clone());
    rows::info(form, "Kernel", info.kernel.clone());
    rows::info(form, "Host name", info.hostname.clone());
    rows::info(form, "Architecture", std::env::consts::ARCH);

    rows::heading(form, "Hardware");
    rows::info(form, "Processor", info.cpu.clone());
    rows::info(form, "Memory", info.memory.clone());
    rows::info(form, "Uptime", info.uptime.clone());

    rows::heading(form, "Desktop");
    rows::info(form, "Compositor", info.compositor.clone());
    rows::info(form, "Shell", "Conchy Shell");
    rows::info(form, "Display protocol", "Wayland");

    let summary: SharedString = format!(
        "OS: {}\nKernel: {}\nHost: {}\nCPU: {}\nMemory: {}\nCompositor: {}\nUptime: {}",
        info.os_name,
        info.kernel,
        info.hostname,
        info.cpu,
        info.memory,
        info.compositor,
        info.uptime
    )
    .into();
    rows::heading(form, "Share");
    rows::action(
        form,
        "Copy system info",
        "Put the lines above on the clipboard",
        move |_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(summary.to_string()));
            Outcome::Stay
        },
        cx,
    );
}
