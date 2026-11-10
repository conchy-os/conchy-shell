//! Chrome for the shell's floating surfaces.

use gpui::{App, Pixels, Styled};

use crate::{ActiveTheme, Radius, Spacing, nested_radius};

/// The two surface levels the shell paints with.
pub trait PanelSurface: Styled + Sized {
    /// A floating panel: the control center, a bar popup, a notification.
    /// Filled at the theme's surface opacity.
    fn panel_surface(self, cx: &App) -> Self {
        self.panel_surface_with(None, cx)
    }

    /// [`panel_surface`](Self::panel_surface) at `opacity`, when a surface
    /// has its own setting (`[panels] opacity`, `[notification] opacity`).
    fn panel_surface_with(self, opacity: Option<f32>, cx: &App) -> Self {
        let theme = cx.theme();
        let colors = theme.colors();
        self.bg(theme.surface(opacity))
            .border_1()
            .border_color(colors.border)
            .rounded(surface_radius())
    }

    /// A card *inside* a panel - one slider, one expanded section - padded
    /// from the panel's edge by [`surface_padding`].
    fn panel_card(self, cx: &App) -> Self {
        let colors = cx.theme().colors();
        self.bg(colors.surface_background)
            .border_1()
            .border_color(colors.border_variant)
            .rounded(card_radius())
    }
}

impl<T: Styled> PanelSurface for T {}

/// Every top-level shell surface - floating bar, panel, launcher, OSD,
/// notification popup, dock - shares this corner.
pub fn surface_radius() -> Pixels {
    Radius::XLarge.pixels()
}

/// The inset a surface keeps between its edge and its content.
pub fn surface_padding() -> Pixels {
    Spacing::Large.pixels()
}

/// The corner of a card or row sitting [`surface_padding`] inside a surface:
/// concentric with it, never sharper than the large token.
pub fn card_radius() -> Pixels {
    nested_radius(surface_radius(), surface_padding())
}
