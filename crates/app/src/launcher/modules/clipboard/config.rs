//! Clipboard module configuration.

use serde::{Deserialize, Serialize};

/// Clipboard module configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipboardConfig {
    pub prefix: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self {
            prefix: ";c".into(),
            width: None,
            height: None,
        }
    }
}
