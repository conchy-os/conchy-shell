//! Clipboard module configuration.

use serde::{Deserialize, Serialize};

/// Clipboard module configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipboardConfig {
    pub prefix: String,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self {
            prefix: ";c".into(),
        }
    }
}
