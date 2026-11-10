//! Dock configuration.

use serde::{Deserialize, Serialize};
use ui::Spacing;

use crate::bar::config::BarPosition;

/// Which monitors show a dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DockMonitors {
    /// Only the primary display gets a dock (default).
    #[default]
    PrimaryOnly,
    /// Every display gets its own dock, scoped to that monitor's windows.
    All,
}

impl DockMonitors {
    pub const ALL: [Self; 2] = [Self::PrimaryOnly, Self::All];

    pub fn label(self) -> &'static str {
        match self {
            Self::PrimaryOnly => "Primary only",
            Self::All => "All monitors",
        }
    }
}

/// When the dock hides itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DockVisibility {
    /// Never hides.
    AlwaysVisible,
    /// Hides after a short delay once no window on the dock's monitor is
    /// focused; reveals on pointer-enter at the dock's edge, or as soon as
    /// focus returns to this monitor.
    #[default]
    IntelligentHide,
    /// Hides only while a window on the dock's monitor's current workspace
    /// covers the dock; comes back when the pointer reaches the dock's edge
    /// or nothing is in the way anymore. On compositors that report no
    /// window geometry (Niri), any window on that workspace counts as
    /// covering it.
    DodgeWindows,
}

impl DockVisibility {
    pub const ALL: [Self; 3] = [
        Self::AlwaysVisible,
        Self::IntelligentHide,
        Self::DodgeWindows,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::AlwaysVisible => "Always visible",
            Self::IntelligentHide => "Intelligent hide",
            Self::DodgeWindows => "Dodge windows",
        }
    }
}

/// Hover effect applied to a dock item under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DockHoverEffect {
    /// No hover effect.
    None,
    /// Icon rises under the cursor.
    #[default]
    Lift,
    /// Icon grows under the cursor.
    Magnify,
    /// Icon gets a themed accent-color ring under the cursor.
    Glow,
    /// Icon both grows and rises under the cursor.
    MagnifyLift,
}

impl DockHoverEffect {
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Lift,
        Self::Magnify,
        Self::Glow,
        Self::MagnifyLift,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Lift => "Lift",
            Self::Magnify => "Magnify",
            Self::Glow => "Glow",
            Self::MagnifyLift => "Magnify + lift",
        }
    }
}

/// How the dock meets the screen edge, like `bar.style`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DockStyle {
    /// Inset from the edge with rounded corners all around.
    #[default]
    Floating,
    /// Flush against the edge; only the corners facing the screen are rounded.
    Attached,
}

impl DockStyle {
    pub const ALL: [Self; 2] = [Self::Floating, Self::Attached];

    pub fn label(self) -> &'static str {
        match self {
            Self::Floating => "Floating",
            Self::Attached => "Attached",
        }
    }
}

/// Where the icons sit along the dock's edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DockAlign {
    /// Left, or top on a vertical dock.
    Start,
    #[default]
    Center,
    /// Right, or bottom on a vertical dock.
    End,
}

impl DockAlign {
    pub const ALL: [Self; 3] = [Self::Start, Self::Center, Self::End];

    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::Center => "Center",
            Self::End => "End",
        }
    }
}

/// Where the dock sits on its edge: what placement, the dodge rule and the
/// menus all work from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DockGeometry {
    pub position: BarPosition,
    /// Inset from the edge, and from the ends when aligned to one.
    pub gap: f32,
    pub align: DockAlign,
}

impl DockGeometry {
    /// The dock's offset along an edge `edge_length` long, the dock being
    /// `dock_length` of it.
    pub fn along(&self, edge_length: f32, dock_length: f32) -> f32 {
        match self.align {
            DockAlign::Start => self.gap,
            DockAlign::Center => (edge_length - dock_length).max(0.0) / 2.0,
            DockAlign::End => (edge_length - dock_length - self.gap).max(0.0),
        }
    }
}

/// Dock configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockConfig {
    /// Whether the dock is enabled.
    pub enabled: bool,
    /// Screen edge the dock is anchored to. Independent of `bar.position`.
    pub position: BarPosition,
    /// Floating off the edge, or attached to it.
    pub style: DockStyle,
    /// Span the whole edge instead of fitting the icons.
    pub stretch: bool,
    /// Where the icons sit along the edge: the dock itself when fitted, the
    /// icons inside it when stretched.
    pub align: DockAlign,
    /// A button at the start of the dock that opens the launcher, with the
    /// bar's `LauncherBtn` icon.
    pub launcher: bool,
    /// Reserve the dock's space, as the bar does, so windows never cover it.
    /// Such a dock never hides; `visibility` is ignored.
    pub reserve_space: bool,
    /// Which monitors get a dock.
    pub monitors: DockMonitors,
    /// When the dock hides itself.
    pub visibility: DockVisibility,
    /// Hover effect applied to dock items.
    pub hover_effect: DockHoverEffect,
    /// Icon size in px.
    pub icon_size: f32,
    /// Opacity of the dock surface, from 0.0 to 1.0. Unset, it follows
    /// `opacity` in `theme.toml`.
    pub opacity: Option<f32>,
    /// Pinned app identifiers (desktop file names, e.g. `"firefox.desktop"`).
    /// Pinning from the UI writes `state.toml`, which then overrides this.
    pub pinned: Vec<String>,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position: BarPosition::Bottom,
            style: DockStyle::default(),
            stretch: false,
            align: DockAlign::default(),
            launcher: false,
            reserve_space: false,
            monitors: DockMonitors::default(),
            visibility: DockVisibility::default(),
            hover_effect: DockHoverEffect::default(),
            icon_size: 40.0,
            opacity: None,
            pinned: Vec::new(),
        }
    }
}

impl DockConfig {
    /// Distance from the screen edge to the dock: the floating inset, or
    /// nothing when attached. Also the gap a stretched dock keeps at its ends.
    pub fn edge_gap(&self) -> f32 {
        match self.style {
            DockStyle::Floating => Spacing::Medium.value(),
            DockStyle::Attached => 0.0,
        }
    }

    pub fn geometry(&self) -> DockGeometry {
        DockGeometry {
            position: self.position,
            gap: self.edge_gap(),
            align: self.align,
        }
    }

    /// The visibility rule that runs: a dock the compositor keeps windows
    /// off has nothing to hide from.
    pub fn effective_visibility(&self) -> DockVisibility {
        if self.reserve_space {
            DockVisibility::AlwaysVisible
        } else {
            self.visibility
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DockAlign, DockConfig, DockGeometry, DockHoverEffect, DockStyle, DockVisibility};
    use crate::bar::config::BarPosition;

    #[test]
    fn layout_keys_default_to_the_floating_fitted_dock() {
        let config: DockConfig = toml::from_str("").unwrap();
        assert_eq!(config.style, DockStyle::Floating);
        assert!(!config.stretch && !config.reserve_space && !config.launcher);
        assert_eq!(config.align, DockAlign::Center);
        assert_eq!(config.edge_gap(), 8.0);

        let taskbar: DockConfig = toml::from_str("align = \"start\"\nlauncher = true\n").unwrap();
        assert_eq!(taskbar.align, DockAlign::Start);
        assert!(taskbar.launcher);

        let attached: DockConfig =
            toml::from_str("style = \"attached\"\nstretch = true\nreserve_space = true\n").unwrap();
        assert_eq!(attached.style, DockStyle::Attached);
        assert!(attached.stretch && attached.reserve_space);
        assert_eq!(attached.edge_gap(), 0.0);
    }

    /// Start and end keep the gap from their end; centre ignores it.
    #[test]
    fn alignment_places_the_dock_along_its_edge() {
        let geometry = |align| DockGeometry {
            position: BarPosition::Bottom,
            gap: 8.0,
            align,
        };
        assert_eq!(geometry(DockAlign::Start).along(1920.0, 200.0), 8.0);
        assert_eq!(geometry(DockAlign::Center).along(1920.0, 200.0), 860.0);
        assert_eq!(geometry(DockAlign::End).along(1920.0, 200.0), 1712.0);
        // Stretched, the dock already spans the edge less its gaps.
        assert_eq!(geometry(DockAlign::Center).along(1920.0, 1904.0), 8.0);
    }

    #[test]
    fn a_dock_that_reserves_space_never_hides() {
        let config: DockConfig =
            toml::from_str("reserve_space = true\nvisibility = \"dodge-windows\"\n").unwrap();
        assert_eq!(config.visibility, DockVisibility::DodgeWindows);
        assert_eq!(config.effective_visibility(), DockVisibility::AlwaysVisible);
        assert_eq!(
            DockConfig::default().effective_visibility(),
            DockVisibility::IntelligentHide
        );
    }

    #[test]
    fn hover_effect_deserializes_magnify_lift_from_kebab_case() {
        let config: DockConfig = toml::from_str("hover_effect = \"magnify-lift\"").unwrap();

        assert_eq!(config.hover_effect, DockHoverEffect::MagnifyLift);
    }

    #[test]
    fn visibility_defaults_to_intelligent_hide() {
        let config: DockConfig = toml::from_str("").unwrap();

        assert_eq!(config.visibility, DockVisibility::IntelligentHide);
        let explicit: DockConfig = toml::from_str("visibility = \"always-visible\"").unwrap();
        assert_eq!(explicit.visibility, DockVisibility::AlwaysVisible);
    }
}
