//! Configuration for the system info module.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SysInfoConfig {
    /// Command prefix that opens the system info module.
    pub prefix: String,
}

impl Default for SysInfoConfig {
    fn default() -> Self {
        Self {
            prefix: ";sys".into(),
        }
    }
}
