//! Services module - inspect and control the shell's background services.
//!
//! `;s` lists every service beside the selected one's detail; Enter pushes
//! that service's page: restart, stop/start, and its start mode.

pub mod config;

use std::time::Duration;

use gpui::{AnyElement, App, Context, Window, div, prelude::*, px};
use services::{ServiceMode, ServiceStatus};
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, TextSize,
};

use self::config::ServicesConfig;
use crate::config::State;
use crate::icons;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{
    Accessory, Content, Detail, EmptyView, Item, List, Metadata, Section,
};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};
use crate::state::{AppState, watch};

/// Icon for the services module itself, and for any service without a more
/// specific one.
const SERVICES_ICON: IconName = IconName::Gauge;

/// How often the resident-memory figure in the header is refreshed.
const RSS_INTERVAL: Duration = Duration::from_secs(1);

pub fn descriptor(config: &ServicesConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "services",
            prefix: config.prefix.clone().into(),
            name: "Services",
            icon: SERVICES_ICON,
            description: "Inspect, restart and configure services",
            default: false,
            in_help: true,
        },
        ServicesModule::new,
    )
}

/// Snapshot of one service.
#[derive(Clone)]
struct ServiceInfo {
    name: &'static str,
    icon: IconName,
    status: ServiceStatus,
    mode: ServiceMode,
    memory: usize,
    controllable: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ServiceAction {
    Start,
    Stop,
    Restart,
    SetMode(ServiceMode),
}

impl ServiceAction {
    fn label(&self) -> String {
        match self {
            ServiceAction::Start => "Start service".to_string(),
            ServiceAction::Stop => "Stop service".to_string(),
            ServiceAction::Restart => "Restart service".to_string(),
            ServiceAction::SetMode(mode) => format!("Start mode: {}", mode.label()),
        }
    }

    fn description(&self) -> &'static str {
        match self {
            ServiceAction::Start => "Bring the service up now",
            ServiceAction::Stop => "Tear down the background work",
            ServiceAction::Restart => "Stop the current run and start a fresh one",
            ServiceAction::SetMode(ServiceMode::Eager) => "Start with the shell",
            ServiceAction::SetMode(ServiceMode::Lazy) => "Start on first use",
            ServiceAction::SetMode(ServiceMode::Off) => "Never start",
        }
    }

    fn icon(&self) -> IconName {
        match self {
            ServiceAction::Start => IconName::Play,
            ServiceAction::Stop => IconName::Stop,
            ServiceAction::Restart => IconName::Refresh,
            ServiceAction::SetMode(_) => IconName::Settings,
        }
    }

    fn for_service(service: &ServiceInfo) -> Vec<ServiceAction> {
        if !service.controllable {
            return Vec::new();
        }
        let mut actions = if service.status.is_operational() {
            vec![ServiceAction::Restart, ServiceAction::Stop]
        } else {
            vec![ServiceAction::Start]
        };
        actions.extend(ServiceMode::ALL.map(ServiceAction::SetMode));
        actions
    }

    fn run(self, name: &str, cx: &mut App) {
        let Some(service) = AppState::managed_services(cx)
            .into_iter()
            .find(|service| service.name() == name)
        else {
            return;
        };
        match self {
            ServiceAction::Start => service.start(),
            ServiceAction::Stop => service.stop(),
            ServiceAction::Restart => service.restart(),
            ServiceAction::SetMode(mode) => {
                service.lifecycle().set_mode(mode);
                // Turning a running service off stops it now; the other modes
                // only decide what happens at the next startup.
                if mode == ServiceMode::Off {
                    service.stop();
                }
                State::set_service_mode(name, mode, cx);
            }
        }
    }
}

fn snapshot(cx: &App) -> Vec<ServiceInfo> {
    AppState::managed_services(cx)
        .into_iter()
        .map(|service| ServiceInfo {
            name: service.name(),
            icon: icon_for(service.name()),
            status: service.status(),
            mode: service.mode(),
            memory: service.memory_bytes(),
            controllable: service.controllable(),
        })
        .collect()
}

/// Redraw the page whenever any service changes status.
fn watch_statuses<V: 'static>(cx: &mut Context<V>) {
    let signals: Vec<_> = AppState::managed_services(cx)
        .into_iter()
        .map(|service| service.status_signal())
        .collect();
    for signal in signals {
        watch(cx, signal, |_, _, cx| cx.notify());
    }
}

/// A service action that runs on page `V` and redraws it, since a mode
/// change fires no status signal.
fn service_action<V: 'static>(
    action: ServiceAction,
    name: &'static str,
    title: String,
    cx: &Context<V>,
) -> Action {
    Action::listener(title, cx, move |_, _, cx| {
        action.run(name, cx);
        cx.notify();
        Outcome::Stay
    })
    .icon(action.icon())
}

pub struct ServicesModule {
    rss: Option<usize>,
}

impl ServicesModule {
    fn new(cx: &mut Context<Self>) -> Self {
        watch_statuses(cx);
        cx.spawn(async move |this, cx| {
            loop {
                let rss = cx.background_spawn(async { process_rss_bytes() }).await;
                let alive = this.update(cx, |this, cx| {
                    this.rss = rss;
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
                cx.background_executor().timer(RSS_INTERVAL).await;
            }
        })
        .detach();
        Self { rss: None }
    }

    fn item(service: &ServiceInfo, cx: &Context<Self>) -> Item {
        let name = service.name;
        let mut actions = ActionPanel::new().action(
            Action::push("Manage", move |cx| ServicePage::new(name, cx)).shortcut("ctrl-o"),
        );
        let quick = ServiceAction::for_service(service)
            .into_iter()
            .filter(|action| !matches!(action, ServiceAction::SetMode(_)))
            .map(|action| {
                let shortcut = match action {
                    ServiceAction::Restart => "ctrl-r",
                    _ => "ctrl-s",
                };
                service_action(action, name, action.label(), cx).shortcut(shortcut)
            });
        actions = actions.section("Service", quick);
        if let Some(error) = service.status.error_message() {
            actions = actions.action(Action::copy(error.to_string()).title("Copy Error"));
        }

        Item::new(name, name)
            .icon(service.icon)
            .keywords(format!(
                "{} {}",
                service.status.label(),
                service.mode.label()
            ))
            .accessory(
                Accessory::tag(service.status.label())
                    .with_icon(icons::service_status_icon(&service.status))
                    .color(status_color(&service.status)),
            )
            .detail(service_detail(service))
            .actions(actions)
    }
}

impl LauncherPage for ServicesModule {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let services = snapshot(cx);
        List::new()
            .show_detail(true)
            .placeholder("Search services...")
            .section(
                Section::new("Services")
                    .subtitle(services.len().to_string())
                    .items(services.iter().map(|service| Self::item(service, cx))),
            )
            .into()
    }

    fn render_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let services = snapshot(cx);
        let running = services
            .iter()
            .filter(|service| service.status.is_operational())
            .count();
        let errors = services
            .iter()
            .filter(|service| matches!(service.status, ServiceStatus::Error(_)))
            .count();
        let retained: usize = services.iter().map(|service| service.memory).sum();

        let mut summary = format!(
            "{}/{} running · {} retained",
            running,
            services.len(),
            format_bytes(retained)
        );
        if let Some(rss) = self.rss {
            summary.push_str(&format!(" · {} resident", format_bytes(rss)));
        }
        let color = if errors > 0 {
            Color::Error
        } else if running == services.len() {
            Color::Success
        } else {
            Color::Warning
        };
        Some(summary_card(summary, color, None, cx))
    }
}

/// One service's actions, pushed from the list.
pub struct ServicePage {
    name: &'static str,
}

impl ServicePage {
    fn new(name: &'static str, cx: &mut Context<Self>) -> Self {
        watch_statuses(cx);
        Self { name }
    }

    fn info(&self, cx: &App) -> Option<ServiceInfo> {
        snapshot(cx)
            .into_iter()
            .find(|service| service.name == self.name)
    }
}

impl LauncherPage for ServicePage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let list = List::new().title(self.name).empty(
            EmptyView::new("Nothing to control")
                .description("This service runs with the shell and can't be stopped")
                .icon(SERVICES_ICON),
        );
        let Some(service) = self.info(cx) else {
            return list.into();
        };
        let items = ServiceAction::for_service(&service)
            .into_iter()
            .map(|action| {
                let current = action == ServiceAction::SetMode(service.mode);
                let item = Item::new(action.label(), action.label())
                    .subtitle(action.description())
                    .icon(action.icon())
                    .action(service_action(action, self.name, "Run".into(), cx));
                if current {
                    item.accessory(Accessory::tag("current").color(Color::Accent))
                } else {
                    item
                }
            });
        list.section(Section::new("Actions").items(items)).into()
    }

    fn render_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let service = self.info(cx)?;
        let summary = format!(
            "{} · {} · {} · {}",
            service.name,
            service.status.label(),
            service.mode.label(),
            format_bytes(service.memory)
        );
        let error = service.status.error_message().map(String::from);
        Some(summary_card(
            summary,
            status_color(&service.status),
            error,
            cx,
        ))
    }
}

fn service_detail(service: &ServiceInfo) -> Detail {
    let mut markdown = format!("## {}\n", service.name);
    if let Some(error) = service.status.error_message() {
        markdown.push_str(&format!("\n> {error}\n"));
    }
    Detail::new()
        .markdown(markdown)
        .metadata(
            Metadata::label("Status", service.status.label())
                .icon(icons::service_status_icon(&service.status))
                .color(status_color(&service.status)),
        )
        .metadata(Metadata::label("Start mode", service.mode.label()))
        .metadata(Metadata::label("Retained", format_bytes(service.memory)))
        .metadata(Metadata::label(
            "Controllable",
            if service.controllable { "Yes" } else { "No" },
        ))
}

fn summary_card(summary: String, color: Color, error: Option<String>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .p(Spacing::Medium.pixels())
        .child(
            div()
                .px(Spacing::Large.pixels())
                .py(Spacing::Medium.pixels())
                .bg(theme.colors.surface_background)
                .rounded(Radius::Large.pixels())
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(Spacing::Medium.pixels())
                        .child(Icon::new(SERVICES_ICON).size(IconSize::Small).color(color))
                        .child(Label::new(summary).size(TextSize::Small).color(color)),
                )
                .children(
                    error.map(|error| Label::new(error).size(TextSize::XSmall).color(Color::Error)),
                ),
        )
        .into_any_element()
}

fn status_color(status: &ServiceStatus) -> Color {
    match status {
        ServiceStatus::Active => Color::Success,
        ServiceStatus::Initializing => Color::Info,
        ServiceStatus::Error(_) => Color::Error,
        ServiceStatus::Stopped => Color::Muted,
        ServiceStatus::Unavailable => Color::Disabled,
    }
}

fn icon_for(name: &str) -> IconName {
    match name {
        "Audio" => IconName::Volume,
        "Network" => IconName::Globe,
        "Bluetooth" => IconName::Bluetooth,
        "UPower" => IconName::BatteryFull,
        "MPRIS" => IconName::Music,
        "Notifications" => IconName::Bell,
        "Tray" => IconName::Menu,
        "Sysinfo" => IconName::Cpu,
        "Privacy" => IconName::Eye,
        "Wallpaper" => IconName::Image,
        "Brightness" => IconName::Sun,
        "Compositor" => IconName::Layout,
        _ => SERVICES_ICON,
    }
}

fn format_bytes(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    let bytes = bytes as f64;

    if bytes >= KB * KB {
        format!("{:.1} MB", bytes / (KB * KB))
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{bytes:.0} B")
    }
}

/// Resident set size of the whole shell process.
///
/// Services share one heap, so this is the only true memory figure available;
/// per-service numbers are the state each one retains.
fn process_rss_bytes() -> Option<usize> {
    // ponytail: 4 KiB pages, true for every Linux target this shell runs on.
    // Read `sysconf(_SC_PAGESIZE)` if that ever stops holding.
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let resident: usize = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(resident * 4096)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_formatted_by_magnitude() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MB");
    }
}
