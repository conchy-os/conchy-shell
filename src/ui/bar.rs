use crate::config::ShellConfig;
use crate::state::ShellState;
use tracing::info;

/// Logical representation of the Conchy Status Bar.
///
/// Designed to be rendered by any Wayland UI backend (e.g. Smithay Client Toolkit,
/// Slint, Iced, Relm4 / GTK4 Layer Shell).
pub struct BarModel {
    pub config: ShellConfig,
    pub state: ShellState,
}

impl BarModel {
    pub fn new(config: ShellConfig, state: ShellState) -> Self {
        Self { config, state }
    }

    /// Renders a text-based ASCII representation of the status bar, perfect for
    /// CLI preview, unit testing, and headless status printing.
    pub fn render_preview(&self) -> String {
        // Left section: Workspaces
        let mut ws_repr = String::new();
        for ws in &self.state.workspaces {
            let label = ws.name.clone().unwrap_or_else(|| ws.idx.to_string());
            if ws.is_focused {
                ws_repr.push_str(&format!(" [{}] ", label));
            } else if ws.is_active {
                ws_repr.push_str(&format!(" ({}) ", label));
            } else if ws.is_urgent {
                ws_repr.push_str(&format!(" !{}! ", label));
            } else {
                ws_repr.push_str(&format!("  {}  ", label));
            }
        }
        if ws_repr.is_empty() {
            ws_repr = " [1]   2   3 ".to_string();
        }

        // Center section: Active Window Title
        let title = self
            .state
            .active_window_title()
            .unwrap_or("Conchy Desktop");
        let truncated_title = if title.chars().count() > 40 {
            let truncated: String = title.chars().take(37).collect();
            format!("{}...", truncated)
        } else {
            title.to_string()
        };

        // Right section: Status & Clock
        let time_str = chrono_stub();
        let right_section = format!("CPU: 12% | RAM: 3.2G | {}", time_str);

        format!(
            "|{}| {:^40} |{}|",
            ws_repr.trim(),
            truncated_title,
            right_section
        )
    }

    pub fn print_bar(&self) {
        let bar = self.render_preview();
        info!("BAR: {}", bar);
    }
}

fn chrono_stub() -> String {
    // Current UTC/local time format without requiring external heavy time crate
    // (Classmates can plug in chrono or time crates)
    let now = std::time::SystemTime::now();
    let since_epoch = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs = since_epoch % 60;
    let mins = (since_epoch / 60) % 60;
    let hours = (since_epoch / 3600) % 24;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}
