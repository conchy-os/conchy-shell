//! Semantic color tokens used by every component.

use gpui::Hsla;

/// A semantic color reference that components use instead of raw [`Hsla`].
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum Color {
    /// Default text / foreground color.
    #[default]
    Default,
    /// Muted / secondary foreground.
    Muted,
    /// Placeholder foreground (e.g. empty input hint).
    Placeholder,
    /// Disabled foreground.
    Disabled,
    /// Accent foreground (links, emphasis).
    Accent,
    /// Selected foreground - used for items in the selected/active state.
    Selected,
    /// Primary accent foreground / icon / fill.
    Primary,
    /// Foreground on primary accent.
    OnPrimary,
    /// Tonal container fill for primary interactive elements.
    PrimaryContainer,
    /// Foreground on primary container.
    OnPrimaryContainer,
    /// Secondary accent foreground / icon / fill.
    Secondary,
    /// Foreground on secondary accent.
    OnSecondary,
    /// Tonal container fill for secondary surfaces.
    SecondaryContainer,
    /// Foreground on secondary container.
    OnSecondaryContainer,
    /// Tertiary accent foreground / icon / fill.
    Tertiary,
    /// Foreground on tertiary accent.
    OnTertiary,
    /// Tonal container fill for tertiary surfaces.
    TertiaryContainer,
    /// Foreground on tertiary container.
    OnTertiaryContainer,
    /// Tonal surface container - lowest elevation.
    SurfaceContainerLow,
    /// Tonal surface container - default elevation.
    SurfaceContainer,
    /// Tonal surface container - high elevation.
    SurfaceContainerHigh,
    /// Tonal surface container - highest elevation.
    SurfaceContainerHighest,
    /// High-contrast text / foreground on surface.
    OnSurface,
    /// Medium-contrast text / foreground on surface.
    OnSurfaceVariant,
    /// Hint or suggestion text.
    Hint,
    /// Visually hidden / strongly de-emphasized foreground.
    Hidden,
    /// Intentionally ignored item.
    Ignored,
    /// Success / positive status.
    Success,
    /// Warning / caution status.
    Warning,
    /// Error / destructive status.
    Error,
    /// Informational status.
    Info,
    /// Raw color, bypassing the theme.
    Custom(Hsla),
}

impl Color {
    /// Resolve this semantic color to an [`Hsla`] using the given [`ThemeColors`].
    pub fn hsla(&self, colors: &ThemeColors) -> Hsla {
        match self {
            Self::Default => colors.text,
            Self::Muted => colors.text_muted,
            Self::Placeholder => colors.text_placeholder,
            Self::Disabled => colors.text_disabled,
            Self::Accent | Self::Selected => colors.text_accent,
            Self::Primary => colors.primary,
            Self::OnPrimary => colors.on_primary,
            Self::PrimaryContainer => colors.primary_container,
            Self::OnPrimaryContainer => colors.on_primary_container,
            Self::Secondary => colors.secondary,
            Self::OnSecondary => colors.on_secondary,
            Self::SecondaryContainer => colors.secondary_container,
            Self::OnSecondaryContainer => colors.on_secondary_container,
            Self::Tertiary => colors.tertiary,
            Self::OnTertiary => colors.on_tertiary,
            Self::TertiaryContainer => colors.tertiary_container,
            Self::OnTertiaryContainer => colors.on_tertiary_container,
            Self::SurfaceContainerLow => colors.surface_container_low,
            Self::SurfaceContainer => colors.surface_container,
            Self::SurfaceContainerHigh => colors.surface_container_high,
            Self::SurfaceContainerHighest => colors.surface_container_highest,
            Self::OnSurface => colors.on_surface,
            Self::OnSurfaceVariant => colors.on_surface_variant,
            Self::Hint => colors.status.hint,
            Self::Hidden => colors.status.hidden,
            Self::Ignored => colors.status.ignored,
            Self::Success => colors.status.success,
            Self::Warning => colors.status.warning,
            Self::Error => colors.status.error,
            Self::Info => colors.status.info,
            Self::Custom(hsla) => *hsla,
        }
    }
}

/// Status color group: the diagnostic / informational flavors, each with a
/// base foreground variant plus a background and border for status surfaces
/// (inline banners, toast chrome, battery/temperature warnings).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusColors {
    pub info: Hsla,
    pub info_background: Hsla,
    pub info_border: Hsla,

    pub success: Hsla,
    pub success_background: Hsla,
    pub success_border: Hsla,

    pub warning: Hsla,
    pub warning_background: Hsla,
    pub warning_border: Hsla,

    pub error: Hsla,
    pub error_background: Hsla,
    pub error_border: Hsla,

    /// Hint or suggestion text.
    pub hint: Hsla,
    pub hint_background: Hsla,
    pub hint_border: Hsla,

    /// Strongly de-emphasized - present but should not draw the eye.
    pub hidden: Hsla,
    pub hidden_background: Hsla,
    pub hidden_border: Hsla,

    /// Items intentionally ignored.
    pub ignored: Hsla,
    pub ignored_background: Hsla,
    pub ignored_border: Hsla,
}

impl StatusColors {
    /// The attention colour a usage percentage earns, if any: warning from
    /// 70, error from 90, nothing below. For readouts that should stay
    /// neutral until something is actually wrong.
    pub fn usage_attention(&self, value: u32) -> Option<Hsla> {
        match value {
            90.. => Some(self.error),
            70.. => Some(self.warning),
            _ => None,
        }
    }

    /// [`usage_attention`](Self::usage_attention) for a temperature in
    /// degrees Celsius: warning from 70, error from 85.
    pub fn temperature_attention(&self, temp: i32) -> Option<Hsla> {
        match temp {
            85.. => Some(self.error),
            70.. => Some(self.warning),
            _ => None,
        }
    }
}

/// The semantic color palette powering every component.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeColors {
    // Surfaces
    /// Window / root background.
    pub background: Hsla,
    /// Grounded surface (bar, panel, sidebar).
    pub surface_background: Hsla,
    /// Elevated surface (popover, dropdown, card).
    pub elevated_surface_background: Hsla,

    // Borders
    /// Default border color.
    pub border: Hsla,
    /// Subtle border used for dividers between related content.
    pub border_variant: Hsla,
    /// Border for keyboard focus ring.
    pub border_focused: Hsla,
    /// Border for the active / selected state.
    pub border_selected: Hsla,
    /// Border for disabled elements.
    pub border_disabled: Hsla,
    /// A fully transparent border.
    pub border_transparent: Hsla,

    // Foreground text
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_placeholder: Hsla,
    pub text_disabled: Hsla,
    pub text_accent: Hsla,

    // Foreground icons
    pub icon: Hsla,
    pub icon_muted: Hsla,
    pub icon_disabled: Hsla,
    pub icon_accent: Hsla,

    // Filled (opaque) interactive element backgrounds
    pub element_background: Hsla,
    pub element_hover: Hsla,
    pub element_active: Hsla,
    pub element_selected: Hsla,
    pub element_disabled: Hsla,

    // Ghost (transparent) interactive element backgrounds
    /// Resting background for a ghost element.
    pub ghost_element_background: Hsla,
    pub ghost_element_hover: Hsla,
    pub ghost_element_active: Hsla,
    pub ghost_element_selected: Hsla,
    pub ghost_element_disabled: Hsla,

    // Status / semantic
    pub status: StatusColors,

    /// Primary accent color.
    pub accent: Hsla,

    // Expressive / Material You semantic accents & containers
    /// Primary accent color (synonym of `accent` for clarity).
    pub primary: Hsla,
    /// High-contrast foreground on primary accent.
    pub on_primary: Hsla,
    /// Tonal container fill for primary interactive elements.
    pub primary_container: Hsla,
    /// High-contrast foreground on primary container.
    pub on_primary_container: Hsla,

    /// Secondary accent color for supplementary actions and toggles.
    pub secondary: Hsla,
    /// Foreground on secondary accent.
    pub on_secondary: Hsla,
    /// Tonal container for secondary surfaces.
    pub secondary_container: Hsla,
    /// Foreground on secondary container.
    pub on_secondary_container: Hsla,

    /// Tertiary accent color for highlights, badges, and calendar markers.
    pub tertiary: Hsla,
    /// Foreground on tertiary accent.
    pub on_tertiary: Hsla,
    /// Tonal container for tertiary surfaces.
    pub tertiary_container: Hsla,
    /// Foreground on tertiary container.
    pub on_tertiary_container: Hsla,

    // Tonal Surface Layers
    pub surface_container_low: Hsla,
    pub surface_container: Hsla,
    pub surface_container_high: Hsla,
    pub surface_container_highest: Hsla,
    pub on_surface: Hsla,
    pub on_surface_variant: Hsla,
}
