//! Native Material You (Material Design 3) dynamic palette generation.
//!
//! Generates harmonized tonal color schemes from wallpapers or seed colors
//! in pure Rust using Google's Material Design 3 HCT algorithms, without any
//! external CLI binaries.

use std::path::Path;

use gpui::{Hsla, Rgba};
use image::GenericImageView;
use material_colors::color::Rgb;
use material_colors::dynamic_color::Variant;
use material_colors::image::extract_color;
use material_colors::scheme::Scheme;
use material_colors::theme::ThemeBuilder;
use serde::{Deserialize, Serialize};

use super::Appearance;
use super::base16::Base16Palette;
use super::colors::{StatusColors, ThemeColors};

/// Fully transparent color slot.
const TRANSPARENT: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0,
    a: 0.0,
};

fn rgb_to_hsla(rgb: Rgb) -> Hsla {
    Hsla::from(Rgba {
        r: rgb.red as f32 / 255.0,
        g: rgb.green as f32 / 255.0,
        b: rgb.blue as f32 / 255.0,
        a: 1.0,
    })
}

/// A complete Material You (M3) palette with primary, secondary, tertiary,
/// surface tiers and container roles.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MaterialPalette {
    pub appearance: Appearance,

    // Primary
    pub primary: Hsla,
    pub on_primary: Hsla,
    pub primary_container: Hsla,
    pub on_primary_container: Hsla,
    pub inverse_primary: Hsla,

    // Secondary
    pub secondary: Hsla,
    pub on_secondary: Hsla,
    pub secondary_container: Hsla,
    pub on_secondary_container: Hsla,

    // Tertiary
    pub tertiary: Hsla,
    pub on_tertiary: Hsla,
    pub tertiary_container: Hsla,
    pub on_tertiary_container: Hsla,

    // Error
    pub error: Hsla,
    pub on_error: Hsla,
    pub error_container: Hsla,
    pub on_error_container: Hsla,

    // Surfaces & Containers
    pub background: Hsla,
    pub on_background: Hsla,
    pub surface: Hsla,
    pub on_surface: Hsla,
    pub surface_variant: Hsla,
    pub on_surface_variant: Hsla,
    pub surface_dim: Hsla,
    pub surface_bright: Hsla,
    pub surface_container_lowest: Hsla,
    pub surface_container_low: Hsla,
    pub surface_container: Hsla,
    pub surface_container_high: Hsla,
    pub surface_container_highest: Hsla,

    // Outlines & Other
    pub outline: Hsla,
    pub outline_variant: Hsla,
    pub shadow: Hsla,
    pub scrim: Hsla,
    pub inverse_surface: Hsla,
    pub inverse_on_surface: Hsla,
}

impl MaterialPalette {
    /// Construct a [`MaterialPalette`] from an M3 [`Scheme`].
    pub fn from_m3_scheme(scheme: &Scheme, appearance: Appearance) -> Self {
        Self {
            appearance,
            primary: rgb_to_hsla(scheme.primary),
            on_primary: rgb_to_hsla(scheme.on_primary),
            primary_container: rgb_to_hsla(scheme.primary_container),
            on_primary_container: rgb_to_hsla(scheme.on_primary_container),
            inverse_primary: rgb_to_hsla(scheme.inverse_primary),

            secondary: rgb_to_hsla(scheme.secondary),
            on_secondary: rgb_to_hsla(scheme.on_secondary),
            secondary_container: rgb_to_hsla(scheme.secondary_container),
            on_secondary_container: rgb_to_hsla(scheme.on_secondary_container),

            tertiary: rgb_to_hsla(scheme.tertiary),
            on_tertiary: rgb_to_hsla(scheme.on_tertiary),
            tertiary_container: rgb_to_hsla(scheme.tertiary_container),
            on_tertiary_container: rgb_to_hsla(scheme.on_tertiary_container),

            error: rgb_to_hsla(scheme.error),
            on_error: rgb_to_hsla(scheme.on_error),
            error_container: rgb_to_hsla(scheme.error_container),
            on_error_container: rgb_to_hsla(scheme.on_error_container),

            background: rgb_to_hsla(scheme.background),
            on_background: rgb_to_hsla(scheme.on_background),
            surface: rgb_to_hsla(scheme.surface),
            on_surface: rgb_to_hsla(scheme.on_surface),
            surface_variant: rgb_to_hsla(scheme.surface_variant),
            on_surface_variant: rgb_to_hsla(scheme.on_surface_variant),
            surface_dim: rgb_to_hsla(scheme.surface_dim),
            surface_bright: rgb_to_hsla(scheme.surface_bright),
            surface_container_lowest: rgb_to_hsla(scheme.surface_container_lowest),
            surface_container_low: rgb_to_hsla(scheme.surface_container_low),
            surface_container: rgb_to_hsla(scheme.surface_container),
            surface_container_high: rgb_to_hsla(scheme.surface_container_high),
            surface_container_highest: rgb_to_hsla(scheme.surface_container_highest),

            outline: rgb_to_hsla(scheme.outline),
            outline_variant: rgb_to_hsla(scheme.outline_variant),
            shadow: rgb_to_hsla(scheme.shadow),
            scrim: rgb_to_hsla(scheme.scrim),
            inverse_surface: rgb_to_hsla(scheme.inverse_surface),
            inverse_on_surface: rgb_to_hsla(scheme.inverse_on_surface),
        }
    }

    /// Convert this Material You palette into [`ThemeColors`].
    pub fn into_colors(self) -> ThemeColors {
        let status = StatusColors {
            info: self.secondary,
            info_background: self.secondary_container,
            info_border: self.secondary.alpha(0.3),
            success: self.tertiary,
            success_background: self.tertiary_container,
            success_border: self.tertiary.alpha(0.3),
            warning: self.secondary_container,
            warning_background: self.secondary_container.alpha(0.3),
            warning_border: self.secondary.alpha(0.4),
            error: self.error,
            error_background: self.error_container,
            error_border: self.error.alpha(0.35),
            hint: self.primary,
            hint_background: self.primary_container,
            hint_border: self.primary.alpha(0.25),
            hidden: self.outline_variant,
            hidden_background: self.surface_container_lowest,
            hidden_border: self.outline_variant.alpha(0.2),
            ignored: self.outline_variant,
            ignored_background: self.surface_container_lowest,
            ignored_border: self.outline_variant.alpha(0.2),
        };

        ThemeColors {
            background: self.background,
            surface_background: self.surface_container,
            elevated_surface_background: self.surface_container_high,

            border: self.outline_variant,
            border_variant: self.outline_variant.alpha(0.6),
            border_focused: self.primary,
            border_selected: self.primary,
            border_disabled: self.outline_variant.alpha(0.3),
            border_transparent: TRANSPARENT,

            text: self.on_surface,
            text_muted: self.on_surface_variant,
            text_placeholder: self.outline,
            text_disabled: self.on_surface.alpha(0.38),
            text_accent: self.primary,

            icon: self.on_surface,
            icon_muted: self.on_surface_variant,
            icon_disabled: self.on_surface.alpha(0.38),
            icon_accent: self.primary,

            element_background: self.surface_container,
            element_hover: self.surface_container_high,
            element_active: self.surface_container_highest,
            element_selected: self.primary_container,
            element_disabled: self.surface_container_low,

            ghost_element_background: TRANSPARENT,
            ghost_element_hover: self.on_surface.alpha(0.08),
            ghost_element_active: self.on_surface.alpha(0.12),
            ghost_element_selected: self.primary_container.alpha(0.6),
            ghost_element_disabled: TRANSPARENT,

            status,
            accent: self.primary,

            primary: self.primary,
            on_primary: self.on_primary,
            primary_container: self.primary_container,
            on_primary_container: self.on_primary_container,

            secondary: self.secondary,
            on_secondary: self.on_secondary,
            secondary_container: self.secondary_container,
            on_secondary_container: self.on_secondary_container,

            tertiary: self.tertiary,
            on_tertiary: self.on_tertiary,
            tertiary_container: self.tertiary_container,
            on_tertiary_container: self.on_tertiary_container,

            surface_container_low: self.surface_container_low,
            surface_container: self.surface_container,
            surface_container_high: self.surface_container_high,
            surface_container_highest: self.surface_container_highest,
            on_surface: self.on_surface,
            on_surface_variant: self.on_surface_variant,
        }
    }

    /// Convert this M3 palette to a backward-compatible [`Base16Palette`].
    pub fn into_base16(self) -> Base16Palette {
        Base16Palette {
            base00: self.background,
            base01: self.surface_container,
            base02: self.surface_container_high,
            base03: self.outline_variant,
            base04: self.on_surface_variant,
            base05: self.on_surface,
            base06: self.surface_bright,
            base07: self.surface_container_lowest,
            base08: self.error,
            base09: self.tertiary,
            base0a: self.secondary_container,
            base0b: self.tertiary_container,
            base0c: self.outline,
            base0d: self.primary,
            base0e: self.secondary,
            base0f: self.inverse_primary,
        }
    }
}

/// Dark and Light variants derived from an image or seed color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallpaperMaterialPalettes {
    pub dark: MaterialPalette,
    pub light: MaterialPalette,
}

impl WallpaperMaterialPalettes {
    /// Derive Material You dark and light palettes from an image file.
    /// Runs in-process in pure Rust in under 50 milliseconds.
    pub fn from_wallpaper(wallpaper_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Self::from_wallpaper_with(wallpaper_path, Variant::TonalSpot)
    }

    /// Like [`Self::from_wallpaper`], with the given scheme variant.
    pub fn from_wallpaper_with(
        wallpaper_path: impl AsRef<Path>,
        variant: Variant,
    ) -> anyhow::Result<Self> {
        let path = wallpaper_path.as_ref();
        let img = image::open(path)
            .map_err(|e| anyhow::anyhow!("Failed to open wallpaper {:?}: {e}", path))?;

        // Shrink to 128x128 for instant quantization; a cheap filter is plenty
        let resized = img.thumbnail(128, 128);
        let pixels: Vec<Rgb> = resized
            .pixels()
            .map(|(_, _, p)| Rgb::new(p[0], p[1], p[2]))
            .collect();

        if pixels.is_empty() {
            return Err(anyhow::anyhow!("Wallpaper contains no pixels"));
        }

        let source = extract_color(&pixels);
        Ok(Self::from_source_rgb_with(source, variant))
    }

    /// Derive palettes directly from a source RGB seed.
    pub fn from_source_rgb(source: Rgb) -> Self {
        Self::from_source_rgb_with(source, Variant::TonalSpot)
    }

    /// Derive palettes from a source RGB seed with the given scheme variant.
    pub fn from_source_rgb_with(source: Rgb, variant: Variant) -> Self {
        let theme = ThemeBuilder::with_source(source).variant(variant).build();
        Self {
            dark: MaterialPalette::from_m3_scheme(&theme.schemes.dark, Appearance::Dark),
            light: MaterialPalette::from_m3_scheme(&theme.schemes.light, Appearance::Light),
        }
    }
}

/// The variant a matugen-style scheme name (`scheme-content`, `fruit-salad`,
/// …) stands for; unknown names get the Material default.
pub fn variant_from_scheme(name: &str) -> Variant {
    let name = name.trim().to_ascii_lowercase();
    match name.strip_prefix("scheme-").unwrap_or(&name) {
        "monochrome" => Variant::Monochrome,
        "neutral" => Variant::Neutral,
        "vibrant" => Variant::Vibrant,
        "expressive" => Variant::Expressive,
        "fidelity" => Variant::Fidelity,
        "content" => Variant::Content,
        "rainbow" => Variant::Rainbow,
        "fruit-salad" | "fruit_salad" => Variant::FruitSalad,
        _ => Variant::TonalSpot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_names_map_to_variants() {
        assert!(matches!(
            variant_from_scheme("scheme-content"),
            Variant::Content
        ));
        assert!(matches!(
            variant_from_scheme("Fruit-Salad"),
            Variant::FruitSalad
        ));
        assert!(matches!(
            variant_from_scheme("scheme-tonal-spot"),
            Variant::TonalSpot
        ));
        assert!(matches!(variant_from_scheme("junk"), Variant::TonalSpot));
    }

    #[test]
    fn variants_change_the_palette() {
        let seed = Rgb::new(50, 120, 200);
        let tonal = WallpaperMaterialPalettes::from_source_rgb(seed);
        let vibrant = WallpaperMaterialPalettes::from_source_rgb_with(seed, Variant::Vibrant);
        assert_ne!(tonal.dark.secondary, vibrant.dark.secondary);
    }

    #[test]
    fn seed_color_generates_valid_palettes() {
        let seed = Rgb::new(50, 120, 200);
        let palettes = WallpaperMaterialPalettes::from_source_rgb(seed);

        assert_eq!(palettes.dark.appearance, Appearance::Dark);
        assert_eq!(palettes.light.appearance, Appearance::Light);

        let colors_dark = palettes.dark.into_colors();
        assert_eq!(colors_dark.primary, palettes.dark.primary);
        assert_eq!(colors_dark.secondary, palettes.dark.secondary);
        assert_eq!(colors_dark.tertiary, palettes.dark.tertiary);

        let base16_dark = palettes.dark.into_base16();
        assert_eq!(base16_dark.base00, palettes.dark.background);
        assert_eq!(base16_dark.base0d, palettes.dark.primary);
    }
}
