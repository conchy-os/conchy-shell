//! Configuration for services status module.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServicesConfig {
    pub prefix: String,
}

impl Default for ServicesConfig {
    fn default() -> Self {
        Self {
            prefix: ";s".to_string(),
        }
    }
}
