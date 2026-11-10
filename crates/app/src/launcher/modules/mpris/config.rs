//! Configuration for the media (MPRIS) module.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MprisConfig {
    /// Command prefix that opens the media module.
    pub prefix: String,
}

impl Default for MprisConfig {
    fn default() -> Self {
        Self {
            prefix: ";m".into(),
        }
    }
}
