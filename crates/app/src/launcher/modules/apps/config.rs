//! Apps module configuration.

use serde::{Deserialize, Serialize};

/// Apps module configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppsConfig {
    pub prefix: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl Default for AppsConfig {
    fn default() -> Self {
        Self {
            prefix: "@".into(),
            width: None,
            height: None,
        }
    }
}
