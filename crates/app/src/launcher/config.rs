use serde::{Deserialize, Serialize};

use super::modules::{
    apps::config::AppsConfig, calculator::config::CalculatorConfig,
    clipboard::config::ClipboardConfig, config::config::ConfigEditorConfig,
    help::config::HelpConfig, mpris::config::MprisConfig, services::config::ServicesConfig,
    shell::config::ShellConfig, sysinfo::config::SysInfoConfig, theme::config::ThemesConfig,
    wallpaper::config::WallpaperConfig, web::config::WebConfig,
    workspaces::config::WorkspacesConfig,
};

/// Launcher window configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherConfig {
    pub width: f32,
    pub height: f32,
    pub margin_top: f32,
    pub margin_right: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    /// Opacity of the launcher surface, from 0.0 to 1.0. Unset, it follows
    /// `opacity` in `theme.toml`.
    pub opacity: Option<f32>,
    /// Per-module settings, `[launcher.modules.<id>]`.
    #[serde(alias = "views")]
    pub modules: ModulesConfig,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            width: 600.0,
            height: 450.0,
            margin_top: 100.0,
            margin_right: 0.0,
            margin_bottom: 0.0,
            margin_left: 0.0,
            opacity: None,
            modules: ModulesConfig::default(),
        }
    }
}

/// Configuration for launcher modules.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModulesConfig {
    pub apps: AppsConfig,
    pub shell: ShellConfig,
    pub web: WebConfig,
    pub workspaces: WorkspacesConfig,
    pub wallpaper: WallpaperConfig,
    pub themes: ThemesConfig,
    pub services: ServicesConfig,
    pub sysinfo: SysInfoConfig,
    pub mpris: MprisConfig,
    pub calculator: CalculatorConfig,
    pub clipboard: ClipboardConfig,
    pub config: ConfigEditorConfig,
    pub help: HelpConfig,
}
