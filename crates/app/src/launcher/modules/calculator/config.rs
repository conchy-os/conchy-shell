//! Calculator module configuration.

use serde::{Deserialize, Serialize};

/// Calculator module configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CalculatorConfig {
    pub prefix: String,
}

impl Default for CalculatorConfig {
    fn default() -> Self {
        Self { prefix: "=".into() }
    }
}
