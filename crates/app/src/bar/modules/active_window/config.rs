//! Active window module configuration.

use serde::{Deserialize, Serialize};

/// Active window module configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActiveWindowConfig {
    pub max_length: usize,
    pub show_app_icon: bool,
    /// `strftime` format shown on a horizontal bar while no window is focused.
    pub idle_format_horizontal: String,
    /// The same for a vertical bar; each line is one row.
    pub idle_format_vertical: String,
}

impl Default for ActiveWindowConfig {
    fn default() -> Self {
        Self {
            max_length: 64,
            show_app_icon: true,
            idle_format_horizontal: "%a, %d %b %Y".into(),
            idle_format_vertical: "%d\n%m".into(),
        }
    }
}
