//! Configuration for the config editor module.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigEditorConfig {
    pub prefix: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl Default for ConfigEditorConfig {
    fn default() -> Self {
        Self {
            prefix: ";cfg".to_string(),
            width: Some(800.0),
            height: Some(540.0),
        }
    }
}
