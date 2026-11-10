//! On-screen display: the pill that flashes up on a volume or brightness change.

use gpui::{App, Hsla, IntoElement, RenderOnce, Window, div, prelude::*, relative, rems};

use super::surface_radius;
use crate::{ActiveTheme, Color, Icon, IconName, IconSize, Radius, Spacing, TextSize};

/// Thickness of the level track.
const TRACK: Spacing = Spacing::Small;
/// Room for the widest value (`100%`), so the pill does not twitch between
/// 9% and 100%.
const VALUE_WIDTH: f32 = 2.8;

/// A level readout: icon, filled track, percentage.
#[derive(IntoElement)]
#[must_use = "OsdIndicator does nothing unless rendered"]
pub struct OsdIndicator {
    icon: IconName,
    level: u8,
    vertical: bool,
    fill: Option<Hsla>,
    icon_color: Option<Hsla>,
    background: Option<Hsla>,
}

impl OsdIndicator {
    pub fn new(icon: IconName, level: u8) -> Self {
        Self {
            icon,
            level,
            vertical: false,
            fill: None,
            icon_color: None,
            background: None,
        }
    }

    /// Surface fill in place of the theme's, e.g. at a configured opacity.
    pub fn background(mut self, background: Hsla) -> Self {
        self.background = Some(background);
        self
    }

    pub fn vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }

    /// Track colour.
    pub fn fill(mut self, fill: Hsla) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn icon_color(mut self, color: Hsla) -> Self {
        self.icon_color = Some(color);
        self
    }
}

impl RenderOnce for OsdIndicator {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors();
        let background = self.background.unwrap_or_else(|| theme.surface(None));
        let fill = self.fill.unwrap_or(colors.accent);
        let icon_color = self.icon_color.unwrap_or(colors.text);
        let track_bg = colors.elevated_surface_background;
        let text = colors.text;
        let vertical = self.vertical;
        let filled = (self.level as f32 / 100.0).min(1.0);

        let icon = Icon::new(self.icon)
            .size(IconSize::Large)
            .color(Color::Custom(icon_color));

        let value = div()
            .text_size(TextSize::Small.rems())
            .text_color(text)
            .child(format!("{}%", self.level));

        let track = div()
            .flex_1()
            .bg(track_bg)
            .rounded(Radius::Full.pixels())
            .overflow_hidden()
            .map(|el| {
                let bar = div().bg(fill).rounded(Radius::Full.pixels());
                if vertical {
                    // Grows upward, so the fill has to sit at the bottom of the track.
                    el.w(TRACK.pixels())
                        .flex()
                        .flex_col()
                        .justify_end()
                        .child(bar.w_full().h(relative(filled)))
                } else {
                    el.h(TRACK.pixels()).child(bar.h_full().w(relative(filled)))
                }
            });

        div()
            .size_full()
            .flex()
            .justify_center()
            .items_center()
            .gap(Spacing::Large.pixels())
            .bg(background)
            .border_1()
            .border_color(colors.border)
            .rounded(surface_radius())
            .map(|el| {
                if vertical {
                    el.flex_col()
                        .py(Spacing::Large.pixels())
                        .child(value)
                        .child(track)
                        .child(icon)
                } else {
                    el.px(Spacing::Large.pixels())
                        .child(icon)
                        .child(track)
                        .child(div().w(rems(VALUE_WIDTH)).text_right().child(value))
                }
            })
    }
}
