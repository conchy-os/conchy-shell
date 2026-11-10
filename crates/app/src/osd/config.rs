//! OSD module configuration.

use serde::{Deserialize, Serialize};

/// OSD screen position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsdPosition {
    Top,
    Bottom,
    Left,
    #[default]
    Right,
}

impl OsdPosition {
    pub const ALL: [Self; 4] = [Self::Top, Self::Bottom, Self::Left, Self::Right];

    pub fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }
}

/// OSD configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OsdConfig {
    pub enabled: Option<bool>,
    pub position: OsdPosition,
    /// Opacity of the indicator's surface, from 0.0 to 1.0. Unset, it
    /// follows `opacity` in `theme.toml`.
    pub opacity: Option<f32>,
}

impl Default for OsdConfig {
    fn default() -> Self {
        Self {
            enabled: Some(true),
            position: OsdPosition::Right,
            opacity: None,
        }
    }
}
