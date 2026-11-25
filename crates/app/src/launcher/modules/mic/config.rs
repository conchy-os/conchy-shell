//! Configuration for the microphone module.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MicConfig {
    /// Command prefix that opens the microphone module.
    pub prefix: String,
}

impl Default for MicConfig {
    fn default() -> Self {
        Self {
            prefix: ";mic".into(),
        }
    }
}
