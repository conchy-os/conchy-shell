//! Control Center configuration.

use serde::{Deserialize, Serialize};

/// Power action commands for the Control Center.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerActionsConfig {
    pub sleep: String,
    pub reboot: String,
    pub poweroff: String,
}

impl Default for PowerActionsConfig {
    fn default() -> Self {
        Self {
            sleep: "loginctl suspend 2>/dev/null || zzz 2>/dev/null || systemctl suspend".to_string(),
            reboot: "loginctl reboot 2>/dev/null || reboot 2>/dev/null || systemctl reboot".to_string(),
            poweroff: "loginctl poweroff 2>/dev/null || poweroff 2>/dev/null || systemctl poweroff".to_string(),
        }
    }
}

/// Control Center configuration.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlCenterConfig {
    pub power_actions: PowerActionsConfig,
}
