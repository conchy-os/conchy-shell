//! Workspaces module configuration.

use serde::{Deserialize, Serialize};

/// Workspaces module configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspacesConfig {
    pub prefix: String,
}

impl Default for WorkspacesConfig {
    fn default() -> Self {
        Self {
            prefix: ";ws".into(),
        }
    }
}
