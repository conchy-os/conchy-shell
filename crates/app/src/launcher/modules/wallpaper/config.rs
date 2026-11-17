//! Wallpaper module configuration.

use serde::{Deserialize, Serialize};

/// The Material You scheme variant a wallpaper's colours are put through;
/// spelled like matugen's `--type`, so an existing config keeps working.
/// Anything unrecognised reads as the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WallpaperScheme {
    #[default]
    #[serde(rename = "scheme-tonal-spot", alias = "tonal-spot")]
    TonalSpot,
    #[serde(rename = "scheme-vibrant", alias = "vibrant")]
    Vibrant,
    #[serde(rename = "scheme-expressive", alias = "expressive")]
    Expressive,
    #[serde(rename = "scheme-content", alias = "content")]
    Content,
    #[serde(rename = "scheme-fidelity", alias = "fidelity")]
    Fidelity,
    #[serde(rename = "scheme-neutral", alias = "neutral")]
    Neutral,
    #[serde(rename = "scheme-monochrome", alias = "monochrome")]
    Monochrome,
    #[serde(rename = "scheme-rainbow", alias = "rainbow")]
    Rainbow,
    #[serde(rename = "scheme-fruit-salad", alias = "fruit-salad")]
    FruitSalad,
    #[serde(other)]
    Unknown,
}

impl WallpaperScheme {
    pub const ALL: [Self; 9] = [
        Self::TonalSpot,
        Self::Vibrant,
        Self::Expressive,
        Self::Content,
        Self::Fidelity,
        Self::Neutral,
        Self::Monochrome,
        Self::Rainbow,
        Self::FruitSalad,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::TonalSpot | Self::Unknown => "Tonal spot",
            Self::Vibrant => "Vibrant",
            Self::Expressive => "Expressive",
            Self::Content => "Content",
            Self::Fidelity => "Fidelity",
            Self::Neutral => "Neutral",
            Self::Monochrome => "Monochrome",
            Self::Rainbow => "Rainbow",
            Self::FruitSalad => "Fruit salad",
        }
    }

    /// matugen's `--type` for this scheme.
    pub fn matugen_type(self) -> &'static str {
        match self {
            Self::TonalSpot | Self::Unknown => "scheme-tonal-spot",
            Self::Vibrant => "scheme-vibrant",
            Self::Expressive => "scheme-expressive",
            Self::Content => "scheme-content",
            Self::Fidelity => "scheme-fidelity",
            Self::Neutral => "scheme-neutral",
            Self::Monochrome => "scheme-monochrome",
            Self::Rainbow => "scheme-rainbow",
            Self::FruitSalad => "scheme-fruit-salad",
        }
    }
}

/// Wallpaper module configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WallpaperConfig {
    pub prefix: String,
    pub directory: String,
    /// Cards per grid row.
    pub columns: usize,
    /// Generate dark and light themes from an applied wallpaper with
    /// matugen, offered above the grid. Never applied on its own.
    pub matugen: bool,
    /// How strongly the wallpaper's colours show: a Material You variant.
    pub matugen_scheme: WallpaperScheme,
    /// Retheme the shell from the wallpaper whenever it is set, and at startup
    /// while the active theme is the default or an earlier wallpaper's.
    pub auto_theme: bool,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            prefix: ";wp".into(),
            directory: "~/Pictures/Wallpapers".into(),
            columns: 3,
            matugen: true,
            matugen_scheme: WallpaperScheme::default(),
            auto_theme: true,
            width: None,
            height: None,
        }
    }
}
