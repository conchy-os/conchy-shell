//! Theme module configuration.

use serde::{Deserialize, Serialize};

/// Themes module configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemesConfig {
    pub prefix: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl Default for ThemesConfig {
    fn default() -> Self {
        Self {
            prefix: "~".into(),
            width: None,
            height: None,
        }
    }
}
