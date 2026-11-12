//! Calendar module configuration.

use serde::{Deserialize, Serialize};

/// Calendar module configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarConfig {
    pub prefix: String,
}

impl Default for CalendarConfig {
    fn default() -> Self {
        Self {
            prefix: ";cal".into(),
        }
    }
}
