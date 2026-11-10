//! Tray module configuration.

use serde::{Deserialize, Serialize};

/// Tray module configuration.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrayConfig {
    /// Size in px of each tray icon. Unset, tray icons are as big as the
    /// bar's other icons, which follows `bar.size`.
    pub icon_size: Option<f32>,
}
