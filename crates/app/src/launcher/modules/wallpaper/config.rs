//! Wallpaper module configuration.

use serde::{Deserialize, Serialize};

/// Wallpaper module configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WallpaperConfig {
    pub prefix: String,
    pub directory: String,
    /// Cards per grid row.
    pub columns: usize,
    /// Generate dark and light themes from an applied wallpaper with
    /// matugen, offered above the grid. Never applied on its own.
    pub matugen: bool,
    /// matugen `--type`, e.g. `scheme-tonal-spot`, `scheme-content`.
    pub matugen_scheme: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            prefix: ";wp".into(),
            directory: "~/Pictures/Wallpapers".into(),
            columns: 3,
            matugen: true,
            matugen_scheme: "scheme-tonal-spot".into(),
            width: None,
            height: None,
        }
    }
}
