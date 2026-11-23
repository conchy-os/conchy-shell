//! Configuration for the equalizer module.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EqualizerConfig {
    /// Command prefix that opens the equalizer module.
    pub prefix: String,
}

impl Default for EqualizerConfig {
    fn default() -> Self {
        Self {
            prefix: ";equ".into(),
        }
    }
}
