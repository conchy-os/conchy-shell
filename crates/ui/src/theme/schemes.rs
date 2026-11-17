//! A named, selectable Base16 palette.

use gpui::{Hsla, SharedString};

use super::base16::Base16Palette;

/// A named Base16 scheme - built in, from Stylix, or fetched from a Tinted
/// Theming repository.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeScheme {
    /// Display name.
    pub name: SharedString,
    /// Short description (typically `"<provider> — <author>"`).
    pub description: SharedString,
    /// The 16 colors this scheme is made of.
    pub palette: Base16Palette,
}

impl ThemeScheme {
    pub fn new(
        name: impl Into<SharedString>,
        description: impl Into<SharedString>,
        palette: Base16Palette,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            palette,
        }
    }

    /// Colors for the preview strip on a scheme card: the surface ramp
    /// followed by the eight accents.
    pub fn preview_colors(&self) -> Vec<Hsla> {
        let p = &self.palette;
        vec![
            p.base00, p.base01, p.base02, p.base05, p.base08, p.base09, p.base0a, p.base0b,
            p.base0c, p.base0d, p.base0e, p.base0f,
        ]
    }
}

/// The schemes that ship with the shell, available before any theme
/// repository has been cloned.
pub fn builtin_schemes() -> Vec<ThemeScheme> {
    vec![
        ThemeScheme::new(
            "Catppuccin Mocha",
            "Soothing pastel dark theme with vibrant accents",
            Base16Palette::from_hex(&[
                "#1e1e2e", "#181825", "#313244", "#45475a", "#585b70", "#cdd6f4", "#f5e0dc",
                "#b4befe", "#f38ba8", "#fab387", "#f9e2af", "#a6e3a1", "#94e2d5", "#89b4fa",
                "#cba6f7", "#f2cdcd",
            ])
            .expect("Catppuccin Mocha palette must parse"),
        ),
        ThemeScheme::new(
            "Tokyo Night",
            "Clean dark theme celebrating the lights of Downtown Tokyo",
            Base16Palette::from_hex(&[
                "#1a1b26", "#16161e", "#2f3549", "#444b6a", "#787c99", "#a9b1d6", "#c0caf5",
                "#c0caf5", "#f7768e", "#ff9e64", "#e0af68", "#9ece6a", "#7dcfff", "#7aa2f7",
                "#bb9af7", "#db4b4b",
            ])
            .expect("Tokyo Night palette must parse"),
        ),
        ThemeScheme::new(
            "Catppuccin Latte",
            "Crisp, accessible high-contrast light theme",
            Base16Palette::from_hex(&[
                "#eff1f5", "#e6e9ef", "#ccd0da", "#bcc0cc", "#acb0be", "#4c4f69", "#5c5f77",
                "#6c6f85", "#d20f39", "#fe640b", "#df8e1d", "#40a02b", "#179299", "#1e66f5",
                "#8839ef", "#dd7878",
            ])
            .expect("Catppuccin Latte palette must parse"),
        ),
        ThemeScheme::new(
            "Nord",
            "Arctic, north-bluish clean palette",
            Base16Palette::from_hex(&[
                "#2e3440", "#3b4252", "#434c5e", "#4c566a", "#d8dee9", "#e5e9f0", "#eceff4",
                "#8fbcbb", "#bf616a", "#d08770", "#ebcb8b", "#a3be8c", "#88c0d0", "#81a1c1",
                "#b48ead", "#5e81ac",
            ])
            .expect("Nord palette must parse"),
        ),
        ThemeScheme::new(
            "Default Dark",
            "Built-in — the canonical Base16 dark scheme",
            Base16Palette::default(),
        ),
    ]
}
