//! Application configuration stored as a GPUI global.

mod persistence;
mod signals;
mod state;
pub mod store;
mod theme;

use std::collections::BTreeMap;
use std::path::PathBuf;

use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use services::{FileWatcher, LocalSlot, ServiceMode};
use ui::{StoredTheme, Theme};

pub use signals::ConfigSignals;
pub use state::State;

use crate::state::AppState;

pub use crate::bar::config::{BarConfig, BarPosition, BarStyle, ModulesConfig};
pub use crate::control_center::ControlCenterConfig;
pub use crate::dock::DockConfig;
pub use crate::frame::FrameConfig;
pub use crate::launcher::config::LauncherConfig;
pub use crate::notification::{NotificationConfig, NotificationPopupPosition};
pub use crate::osd::{OsdConfig, OsdPosition};
pub use crate::windows::PanelsConfig;

/// Root application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub bar: BarConfig,
    pub launcher: LauncherConfig,
    pub osd: OsdConfig,
    pub notification: NotificationConfig,
    pub control_center: ControlCenterConfig,
    pub dock: DockConfig,
    pub frame: FrameConfig,
    pub panels: PanelsConfig,
    /// Startup mode per service, keyed by the lowercased service name
    /// (`audio`, `tray`, ...). Services absent from the map start eagerly.
    /// Setting a mode from `;s` writes `state.toml`, which then overrides
    /// this per service.
    pub services: BTreeMap<String, ServiceMode>,
    /// Watch config.toml for changes and hot-reload (requires restart to change).
    pub watch_config: bool,
    /// Watch theme.toml for changes and hot-reload (requires restart to change).
    pub watch_theme: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bar: BarConfig::default(),
            launcher: LauncherConfig::default(),
            osd: OsdConfig::default(),
            notification: NotificationConfig::default(),
            control_center: ControlCenterConfig::default(),
            dock: DockConfig::default(),
            frame: FrameConfig::default(),
            panels: PanelsConfig::default(),
            services: BTreeMap::new(),
            watch_config: true,
            watch_theme: true,
        }
    }
}

impl Global for Config {}

impl Config {
    /// Initialize the global config.
    ///
    /// A file we can't parse costs the session its settings, but never the
    /// file itself: the shell persists its own values to `state.toml` (see
    /// [`State`]), the config editor edits `config.toml` one key at a time
    /// through `toml_edit` (see [`store`]) and refuses a file that does not
    /// parse, and both watchers keep the last good value until the next save
    /// fixes the file. `gpuishell --validate` reports where the parse gave up.
    pub fn init(cx: &mut App) {
        let config = match persistence::load_or_create() {
            Ok(config) => config,
            Err(err) => {
                tracing::warn!("Failed to load config, using defaults: {err:#}");
                Config::default()
            }
        };

        let theme = match theme::persistence::load_theme() {
            Ok(theme) => theme,
            Err(err) => {
                tracing::warn!("Failed to load theme, using defaults: {err:#}");
                Theme::default()
            }
        };

        Theme::set(theme, cx);
        cx.set_global(config);
        State::init(cx);
        ConfigSignals::init(cx);
        Self::start_hot_reload(cx);
        Self::start_conchy_config_listener(cx);
    }

    /// Get the global config.
    #[inline(always)]
    pub fn global(cx: &App) -> &Config {
        cx.global::<Config>()
    }

    /// Reload config from disk and replace the global config. A file that
    /// stopped parsing mid-edit leaves the running config alone.
    pub fn reload(cx: &mut App) {
        match persistence::load() {
            Ok(config) => {
                *cx.global_mut::<Config>() = config;
                ConfigSignals::publish(cx);
                AppState::notification(cx).clear_local(LocalSlot::ConfigError);
            }
            Err(err) => {
                tracing::warn!("Failed to reload config from disk: {err:#}");
                notify_parse_failure(LocalSlot::ConfigError, "config.toml", &err, cx);
            }
        }
    }

    /// Reload theme from disk and replace the global theme.
    fn reload_theme(cx: &mut App) {
        match theme::persistence::load_theme() {
            Ok(theme) => {
                Theme::set(theme, cx);
                AppState::notification(cx).clear_local(LocalSlot::ThemeError);
            }
            Err(err) => {
                tracing::warn!("Failed to reload theme from disk: {err:#}");
                notify_parse_failure(LocalSlot::ThemeError, "theme.toml", &err, cx);
            }
        }
    }

    /// Persist current global theme colors to `theme.toml`.
    pub fn save_theme(cx: &App) -> anyhow::Result<()> {
        theme::persistence::save_theme(Theme::global(cx))
    }

    /// Persist a provided theme colors to `theme.toml`.
    pub fn save_theme_value(theme: &Theme) -> anyhow::Result<()> {
        theme::persistence::save_theme(theme)
    }

    fn start_hot_reload(cx: &mut App) {
        let config = cx.global::<Config>();
        let watch_config = config.watch_config;
        let watch_theme = config.watch_theme;

        // Start config file watcher
        if watch_config {
            let config_path = match persistence::config_path() {
                Ok(path) => path,
                Err(err) => {
                    tracing::warn!("Failed to determine config path for hot reload: {err:#}");
                    return;
                }
            };

            let mut rx = FileWatcher::watch(config_path);

            cx.spawn(async move |cx| {
                while rx.recv().await.is_some() {
                    // A reload would throw away edits the user has not saved
                    // or discarded yet.
                    if store::has_pending() {
                        continue;
                    }
                    cx.update(|cx| {
                        tracing::info!("Config file changed, reloading");
                        Config::reload(cx);
                        cx.refresh_windows();
                    });
                }
            })
            .detach();
        }

        // Start theme file watcher
        if watch_theme {
            let theme_path = match theme::persistence::theme_path() {
                Ok(path) => path,
                Err(err) => {
                    tracing::warn!("Failed to determine theme path for hot reload: {err:#}");
                    return;
                }
            };

            let mut rx = FileWatcher::watch(theme_path);

            cx.spawn(async move |cx| {
                while rx.recv().await.is_some() {
                    cx.update(|cx| {
                        tracing::info!("Theme file changed, reloading");
                        Self::reload_theme(cx);
                        cx.refresh_windows();
                    });
                }
            })
            .detach();
        }
    }

    fn start_conchy_config_listener(cx: &mut App) {
        cx.spawn(async move |cx| {
            let connection = match zbus::Connection::session().await {
                Ok(conn) => conn,
                Err(err) => {
                    tracing::debug!("No D-Bus session for conchy-config: {err:#}");
                    return;
                }
            };

            let client = match conchy_config::ConfigClient::new(&connection).await {
                Ok(c) => c,
                Err(err) => {
                    tracing::debug!("org.conchy.Config service not running: {err:#}");
                    return;
                }
            };

            tracing::info!("Connected to org.conchy.Config D-Bus daemon");

            // Query initial values for channel "shell" if available
            if let Ok(properties) = client.list("shell").await {
                for (k, v) in properties {
                    let _ = cx.update(|cx| {
                        Config::apply_conchy_property(&k, &v, cx);
                    });
                }
            }

            // Listen for PropertyChanged signals
            if let Ok(mut stream) = client.receive_property_changed().await {
                use futures_util::StreamExt;
                while let Some(signal) = stream.next().await {
                    if let Ok(args) = signal.args() {
                        if args.channel == "shell" {
                            tracing::info!(
                                "D-Bus Config update: [{}] {} = {}",
                                args.channel,
                                args.key,
                                args.value
                            );
                            let _ = cx.update(|cx| {
                                Config::apply_conchy_property(&args.key, &args.value, cx);
                                cx.refresh_windows();
                            });
                        }
                    }
                }
            }
        })
        .detach();
    }

    /// Apply a key-value property from Conchy Config D-Bus.
    pub fn apply_conchy_property(key: &str, value: &str, cx: &mut App) {
        let mut config = cx.global::<Config>().clone();
        match key {
            "bar.position" => match value.to_lowercase().as_str() {
                "bottom" => config.bar.position = BarPosition::Bottom,
                "left" => config.bar.position = BarPosition::Left,
                "right" => config.bar.position = BarPosition::Right,
                _ => config.bar.position = BarPosition::Top,
            },
            "bar.height" | "bar.size" => {
                if let Ok(h) = value.parse::<f32>() {
                    config.bar.size = h;
                }
            }
            "bar.style" => match value.to_lowercase().as_str() {
                "attached" | "blocky" => config.bar.style = BarStyle::Attached,
                "bubbles" => config.bar.style = BarStyle::Bubbles,
                "frame" => config.bar.style = BarStyle::Frame,
                _ => config.bar.style = BarStyle::Floating,
            },
            "dock.enabled" => {
                config.dock.enabled = value == "true";
            }
            "osd.enabled" => {
                config.osd.enabled = Some(value == "true");
            }
            "clock.format" => {
                config.bar.modules.clock.format_horizontal = value.to_string();
            }
            other => {
                tracing::debug!("Unhandled conchy-config property: {}", other);
            }
        }
        *cx.global_mut::<Config>() = config;
        ConfigSignals::publish(cx);
    }
}

/// Trait for accessing active app configuration from `App`.
pub trait ActiveConfig {
    fn config(&self) -> &Config;
}

impl ActiveConfig for App {
    #[inline(always)]
    fn config(&self) -> &Config {
        Config::global(self)
    }
}

/// Parse every config file without applying it, printing each verdict and any
/// parse diagnostic. `false` when one of them failed, so `--validate` can exit
/// non-zero - the point is to check a file before the shell reloads it.
pub fn validate() -> bool {
    fn check<T: serde::de::DeserializeOwned>(path: anyhow::Result<PathBuf>) -> bool {
        let verdict = path.and_then(|path| {
            let verdict = match persistence::parse_toml::<T>(&path)? {
                Some(_) => "ok",
                None => "absent, defaults apply",
            };
            println!("{}: {verdict}", path.display());
            Ok(())
        });

        match verdict {
            Ok(()) => true,
            Err(err) => {
                eprintln!("{err:#}");
                false
            }
        }
    }

    // `&`, not `&&`: report every file rather than stopping at the first bad one.
    check::<Config>(persistence::config_path())
        & check::<StoredTheme>(theme::persistence::theme_path())
        & check::<State>(state::state_path())
}

/// Notify about anything [`Config::init`] could not parse. Called once the
/// notification service exists - `init` runs before it, so it has nobody to
/// tell. Re-reading two small files beats threading the errors through.
pub fn report_load_errors(cx: &App) {
    if let Err(err) = persistence::load() {
        notify_parse_failure(LocalSlot::ConfigError, "config.toml", &err, cx);
    }

    if let Err(err) = theme::persistence::load_theme() {
        notify_parse_failure(LocalSlot::ThemeError, "theme.toml", &err, cx);
    }
}

/// Tell the user, where they will actually see it, that a file did not load.
/// The warning in the log scrolls past unread; this is the visible half of the
/// same message, and it stays until the file parses again.
fn notify_parse_failure(slot: LocalSlot, file: &str, err: &anyhow::Error, cx: &App) {
    AppState::notification(cx).post_local(slot, format!("{file} was not loaded"), one_line(err));
}

/// Flatten a parse diagnostic into one line. The caret block toml draws is
/// noise in a notification card, and the reason - `unknown variant ...` - is
/// the line underneath it.
pub(super) fn one_line(err: &anyhow::Error) -> String {
    format!("{err:#}")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !is_snippet(line))
        .collect::<Vec<_>>()
        .join(" - ")
}

/// A line of the `2 | position = "sideways"` block toml prints under an error.
fn is_snippet(line: &str) -> bool {
    line.starts_with('|')
        || line.contains("^^^")
        || line
            .split_once(" | ")
            .is_some_and(|(number, _)| number.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use anyhow::Context;

    use super::*;

    /// Hand-written service modes stay readable from `config.toml`; the
    /// overlay that `state.toml` puts on top of them is tested in `state`.
    #[test]
    fn service_modes_round_trip_through_toml() {
        let mut config = Config::default();
        config
            .services
            .insert("tray".to_string(), ServiceMode::Lazy);
        config
            .services
            .insert("bluetooth".to_string(), ServiceMode::Off);

        let encoded = toml::to_string_pretty(&config).expect("config serializes");
        let decoded: Config = toml::from_str(&encoded).expect("config parses back");

        assert_eq!(decoded.services.get("tray"), Some(&ServiceMode::Lazy));
        assert_eq!(decoded.services.get("bluetooth"), Some(&ServiceMode::Off));
        assert_eq!(decoded.services.get("audio"), None);
    }

    /// What lands in the notification card: one line, and the reason the parse
    /// failed has to survive - it is the only part the user can act on.
    #[test]
    fn a_parse_diagnostic_flattens_to_one_readable_line() {
        let err = toml::from_str::<Config>("[bar]\nposition = \"sideways\"\n")
            .map_err(anyhow::Error::from)
            .context("Failed to parse /tmp/config.toml")
            .expect_err("that position does not exist");

        let line = one_line(&err);

        assert_eq!(line.lines().count(), 1, "{line}");
        assert!(line.contains("Failed to parse /tmp/config.toml"), "{line}");
        assert!(line.contains("line 2, column 12"), "{line}");
        assert!(line.contains("unknown variant `sideways`"), "{line}");
        assert!(!line.contains("^^^"), "{line}");
        assert!(!line.contains("position = "), "{line}");
    }

    /// Parses the config actually on this machine, if there is one. Skips
    /// silently when there is not, so CI stays hermetic.
    #[test]
    fn the_live_config_on_this_machine_parses() {
        let Ok(path) = super::persistence::config_path() else {
            return;
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return;
        };

        if let Err(err) = toml::from_str::<Config>(&raw) {
            panic!("live config at {} does not parse: {err}", path.display());
        }
    }

    /// Configs written before the SVG icon switch still hold Nerd Font
    /// glyphs. One of those must not fail the parse - doing so drops every
    /// other setting in the file, which is how a `position = "top"` bar
    /// silently came back vertical.
    #[test]
    fn a_stale_icon_glyph_does_not_discard_the_rest_of_the_config() {
        let raw = concat!(
            "[bar]\n",
            "position = \"top\"\n",
            "\n",
            "[bar.modules.launcher_btn]\n",
            "icon = \"\u{f003b}\"\n",
            "\n",
            "[notification.icons]\n",
            "bell = \"\u{f009a}\"\n",
        );

        let config: Config = toml::from_str(raw).expect("config parses despite stale icons");

        assert_eq!(config.bar.position, crate::bar::config::BarPosition::Top);
        assert_eq!(config.bar.modules.launcher_btn.icon, None);
        assert_eq!(config.notification.icons.bell, None);
    }
}
