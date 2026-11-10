//! Configuration for the config editor module.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigEditorConfig {
    pub prefix: String,
}

impl Default for ConfigEditorConfig {
    fn default() -> Self {
        Self {
            prefix: ";cfg".to_string(),
        }
    }
}
