//! Live figures for the services detail pane: what a service is currently
//! reporting, not how the shell manages it.
//!
//! Only a running service is read. The state accessors start a lazy service
//! on first use, so reading a stopped one here would turn it on just because
//! it was highlighted in the list.

use std::time::Duration;

use gpui::App;
use services::{
    ActiveConnectionInfo, BluetoothState, ConnectivityState, ServiceStatus, WallpaperEngine,
};
use services::mpris::PlaybackStatus;
use services::upower::PowerProfile;
use ui::Color;

use crate::launcher::content::Metadata;
use crate::state::AppState;

/// The rows a service adds under the common ones, empty while it is not
/// running or has nothing to say.
pub fn metrics(name: &str, status: &ServiceStatus, cx: &App) -> Vec<Metadata> {
    if !matches!(status, ServiceStatus::Active) {
        return Vec::new();
    }
    match name {
        "Audio" => audio(cx),
        "Network" => network(cx),
        "Bluetooth" => bluetooth(cx),
        "UPower" => upower(cx),
        "MPRIS" => mpris(cx),
        "Notifications" => notifications(cx),
        "Tray" => tray(cx),
        "Sysinfo" => sysinfo(cx),
        "Privacy" => privacy(cx),
        "Wallpaper" => wallpaper(cx),
        "Brightness" => brightness(cx),
        "Clipboard" => clipboard(cx),
        "Vpnx" => vpnx(cx),
        _ => Vec::new(),
    }
}

fn row(title: &str, text: impl Into<String>) -> Metadata {
    Metadata::label(title.to_string(), text.into())
}

fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn audio(cx: &App) -> Vec<Metadata> {
    let data = AppState::audio(cx).get();
    let level = |volume: u8, muted: bool| {
        if muted {
            format!("{volume}% · muted")
        } else {
            format!("{volume}%")
        }
    };
    vec![
        row("Output", level(data.sink_volume, data.sink_muted)),
        row("Input", level(data.source_volume, data.source_muted)),
    ]
}

fn network(cx: &App) -> Vec<Metadata> {
    let data = AppState::network(cx).get();
    let connectivity = match data.connectivity {
        ConnectivityState::Full => "Online",
        ConnectivityState::Portal => "Captive portal",
        ConnectivityState::Loss => "Limited",
        ConnectivityState::None => "Offline",
        ConnectivityState::Unknown => "Unknown",
    };
    let color = match data.connectivity {
        ConnectivityState::Full => Color::Success,
        ConnectivityState::Portal | ConnectivityState::Loss => Color::Warning,
        ConnectivityState::None => Color::Error,
        ConnectivityState::Unknown => Color::Muted,
    };
    let mut rows = vec![Metadata::label("Connectivity", connectivity).color(color)];
    for connection in &data.active_connections {
        let (kind, detail) = match connection {
            ActiveConnectionInfo::Wired { speed, .. } => ("Wired", format!("{speed} Mbit/s")),
            ActiveConnectionInfo::WiFi { strength, .. } => ("Wi-Fi", format!("{strength}%")),
            ActiveConnectionInfo::Vpn { .. } => ("VPN", "active".to_string()),
        };
        rows.push(row(kind, format!("{} · {detail}", connection.name())));
    }
    rows.push(row("Wi-Fi radio", if data.wifi_enabled { "On" } else { "Off" }));
    if data.wifi_enabled {
        rows.push(row(
            "Networks in range",
            data.wireless_access_points.len().to_string(),
        ));
    }
    rows
}

fn bluetooth(cx: &App) -> Vec<Metadata> {
    let data = AppState::bluetooth(cx).get();
    let adapter = match data.state {
        BluetoothState::Active => "On",
        BluetoothState::Inactive => "Off",
        BluetoothState::Unavailable => "No adapter",
    };
    let mut rows = vec![row("Adapter", adapter)];
    if data.discovering {
        rows.push(row("Scanning", "Yes"));
    }
    rows.push(row(
        "Paired",
        plural(data.paired_devices().count(), "device", "devices"),
    ));
    let connected: Vec<_> = data.connected_devices().collect();
    if connected.is_empty() {
        rows.push(row("Connected", "None"));
    }
    for device in connected {
        let text = match device.battery {
            Some(battery) => format!("{} · {battery}%", device.name),
            None => device.name.clone(),
        };
        rows.push(row("Connected", text));
    }
    rows
}

fn duration(value: Duration) -> String {
    let minutes = value.as_secs() / 60;
    if minutes >= 60 {
        format!("{}h {:02}m", minutes / 60, minutes % 60)
    } else {
        format!("{minutes}m")
    }
}

fn upower(cx: &App) -> Vec<Metadata> {
    let data = AppState::upower(cx).get();
    let mut rows = Vec::new();
    match &data.battery {
        Some(battery) => {
            rows.push(row(
                "Battery",
                format!("{}% · {:?}", battery.percentage, battery.state),
            ));
            if let Some(left) = battery.time_to_empty.filter(|time| !time.is_zero()) {
                rows.push(row("Time left", duration(left)));
            }
            if let Some(until) = battery.time_to_full.filter(|time| !time.is_zero()) {
                rows.push(row("Until full", duration(until)));
            }
            if let Some(rate) = battery.energy_rate.filter(|rate| *rate > 0.0) {
                rows.push(row("Power draw", format!("{rate:.1} W")));
            }
            if let Some(capacity) = battery.capacity {
                rows.push(row("Health", format!("{capacity:.0}%")));
            }
        }
        None => rows.push(row("Battery", "None")),
    }
    if let Some(on_battery) = data.on_battery {
        rows.push(row("Power source", if on_battery { "Battery" } else { "AC" }));
    }
    if data.power_profiles_available {
        let profile = match data.power_profile {
            PowerProfile::Balanced => "Balanced",
            PowerProfile::Performance => "Performance",
            PowerProfile::PowerSaver => "Power saver",
            PowerProfile::Unknown => "Unknown",
        };
        rows.push(row("Power profile", profile));
    }
    rows
}

fn mpris(cx: &App) -> Vec<Metadata> {
    let data = AppState::mpris(cx).get();
    let mut rows = vec![row("Players", data.players.len().to_string())];
    for player in &data.players {
        let name = player
            .identity
            .clone()
            .unwrap_or_else(|| player.service.clone());
        let state = match player.state {
            PlaybackStatus::Playing => "Playing",
            PlaybackStatus::Paused => "Paused",
            PlaybackStatus::Stopped => "Stopped",
        };
        let track = player
            .metadata
            .as_ref()
            .map(ToString::to_string)
            .filter(|track| !track.is_empty());
        rows.push(row(
            &name,
            match track {
                Some(track) => format!("{state} · {track}"),
                None => state.to_string(),
            },
        ));
    }
    rows
}

fn notifications(cx: &App) -> Vec<Metadata> {
    let data = AppState::notification(cx).get();
    vec![
        row("Stored", data.notifications.len().to_string()),
        row("Unread", data.unread_count.to_string()),
        row("Do not disturb", yes_no(data.dnd)),
    ]
}

fn tray(cx: &App) -> Vec<Metadata> {
    let data = AppState::tray(cx).get();
    let mut rows = vec![row("Items", data.len().to_string())];
    for item in &data.items {
        let name = item
            .title
            .clone()
            .filter(|title| !title.is_empty())
            .or_else(|| item.id.clone())
            .unwrap_or_else(|| item.name.clone());
        rows.push(row("Item", name));
    }
    rows
}

fn sysinfo(cx: &App) -> Vec<Metadata> {
    let data = AppState::sysinfo(cx).get();
    let mut rows = vec![
        row("CPU", format!("{}%", data.cpu_usage)),
        row(
            "Memory",
            format!(
                "{:.1} / {:.1} GB · {}%",
                data.memory_used_gb, data.memory_total_gb, data.memory_usage
            ),
        ),
        row("Swap", format!("{}%", data.swap_usage)),
    ];
    if let Some(temperature) = data.temperature {
        rows.push(row("Temperature", format!("{temperature}°C")));
    }
    if let Some(ip) = &data.network.ip {
        rows.push(row("Local IP", ip.clone()));
    }
    rows.push(row(
        "Network",
        format!(
            "↓ {} KB/s  ↑ {} KB/s",
            data.network.download_speed, data.network.upload_speed
        ),
    ));
    for disk in &data.disks {
        rows.push(row(
            &disk.mount_point,
            format!(
                "{:.0} / {:.0} GB · {}%",
                disk.used_gb, disk.total_gb, disk.usage_percent
            ),
        ));
    }
    rows
}

fn privacy(cx: &App) -> Vec<Metadata> {
    let data = AppState::privacy(cx).get();
    let titled = |title: &str, active: bool| {
        if active {
            Metadata::label(title.to_string(), "In use").color(Color::Error)
        } else {
            Metadata::label(title.to_string(), "Idle")
        }
    };
    vec![
        titled("Microphone", data.microphone_access()),
        titled("Camera", data.webcam_access()),
        titled("Screen sharing", data.screenshare_access()),
    ]
}

fn wallpaper(cx: &App) -> Vec<Metadata> {
    let data = AppState::wallpaper(cx).get();
    let engine = match data.engine {
        WallpaperEngine::Awww => "awww",
        WallpaperEngine::Hyprpaper => "hyprpaper",
    };
    let current = data
        .current
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "None".to_string());
    vec![row("Engine", engine), row("Current", current)]
}

fn brightness(cx: &App) -> Vec<Metadata> {
    let data = AppState::brightness(cx).get();
    vec![row(
        "Level",
        format!("{}% ({} / {})", data.percentage(), data.current, data.max),
    )]
}

fn clipboard(cx: &App) -> Vec<Metadata> {
    let data = AppState::clipboard(cx).get();
    vec![row("History", plural(data.entries.len(), "entry", "entries"))]
}

fn vpnx(cx: &App) -> Vec<Metadata> {
    let data = AppState::vpnx(cx).get();
    let mut rows = vec![
        row("Tunnel", format!("{:?}", data.state)),
        row("Mode", format!("{:?}", data.mode)),
    ];
    if let Some(mode) = data.effective_mode {
        rows.push(row("Effective mode", format!("{mode:?}")));
    }
    if !data.groups.is_empty() {
        rows.push(row("Proxy groups", data.groups.len().to_string()));
    }
    if !data.last_error.is_empty() {
        rows.push(Metadata::label("Last error", data.last_error.clone()).color(Color::Error));
    }
    for warning in &data.warnings {
        rows.push(Metadata::label("Warning", warning.clone()).color(Color::Warning));
    }
    rows
}
