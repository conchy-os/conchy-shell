//! Presets for the bar's overall look: one choice that sets the handful of
//! `[bar]` keys and the theme's `radius_scale` that together make the
//! shell blocky, rounded or floating. Widgets and their order are left
//! alone.

use super::config::{BarConfig, BarStyle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// Square corners everywhere; an attached bar whose chips and
    /// workspace cells sit flush and full height.
    Blocky,
    /// A transparent bar where every entry is a pill, chips bare inside.
    Rounded,
    /// The stock look: a floating bar with outlined, filled chips.
    Floating,
}

impl Look {
    pub const ALL: [Look; 3] = [Look::Blocky, Look::Rounded, Look::Floating];

    pub fn label(self) -> &'static str {
        match self {
            Look::Blocky => "Blocky",
            Look::Rounded => "Rounded",
            Look::Floating => "Floating",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Look::Blocky => "Square corners, attached bar, flush full-height segments",
            Look::Rounded => "Transparent bar, each entry a pill, square-free",
            Look::Floating => "Floating bar with outlined chips, the stock look",
        }
    }

    /// The theme's `radius_scale` that goes with the look.
    pub fn radius_scale(self) -> f32 {
        match self {
            Look::Blocky => 0.0,
            Look::Rounded | Look::Floating => 1.0,
        }
    }

    /// Set the `[bar]` keys the look owns. Everything else - widgets,
    /// size, position, modules - stays as it is.
    pub fn apply(self, bar: &mut BarConfig) {
        let (style, show_border, chrome, stretch, padding) = match self {
            Look::Blocky => (BarStyle::Attached, false, false, true, Some(0.0)),
            Look::Rounded => (BarStyle::Bubbles, true, false, false, None),
            Look::Floating => (BarStyle::Floating, true, true, false, None),
        };
        bar.style = style;
        bar.show_border = show_border;
        bar.widget_background = chrome;
        bar.widget_border = chrome;
        bar.widget_stretch = stretch;
        bar.padding = padding;
        bar.segments = Default::default();
    }

    /// Whether `bar` and `radius_scale` are exactly what [`apply`](Self::apply)
    /// would produce, so the editor can mark the current look.
    pub fn matches(self, bar: &BarConfig, radius_scale: f32) -> bool {
        let mut expected = bar.clone();
        self.apply(&mut expected);
        expected == *bar && (radius_scale - self.radius_scale()).abs() < f32::EPSILON
    }

    /// The look `bar` and the theme are in, if any.
    pub fn current(bar: &BarConfig, radius_scale: f32) -> Option<Look> {
        Self::ALL
            .into_iter()
            .find(|look| look.matches(bar, radius_scale))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each preset is recognisable after it is applied, and no other preset
    /// claims it.
    #[test]
    fn presets_round_trip_through_detection() {
        for look in Look::ALL {
            let mut bar = BarConfig::default();
            look.apply(&mut bar);
            assert_eq!(Look::current(&bar, look.radius_scale()), Some(look));
            // The scale is part of the look: blocky at scale 1 is not blocky.
            assert_eq!(Look::current(&bar, look.radius_scale() + 0.5), None);
        }
    }

    /// Blocky is the spec's reference A; rounded its reference B.
    #[test]
    fn presets_are_the_reference_configs() {
        let mut blocky = BarConfig::default();
        Look::Blocky.apply(&mut blocky);
        assert_eq!(blocky.style, BarStyle::Attached);
        assert!(blocky.widget_stretch && !blocky.show_border && !blocky.widget_border);
        assert_eq!(blocky.padding, Some(0.0));

        let mut rounded = BarConfig::default();
        Look::Rounded.apply(&mut rounded);
        assert_eq!(rounded.style, BarStyle::Bubbles);
        assert!(!rounded.widget_background && !rounded.widget_border && !rounded.widget_stretch);
        assert_eq!(rounded.padding, None);
    }

    /// Out of the box the shell is in the floating look, so the chooser
    /// marks it and the stock config needs no `[bar]` keys for it.
    #[test]
    fn the_default_config_is_the_floating_look() {
        let bar = BarConfig::default();
        assert_eq!(bar.style, BarStyle::Floating);
        assert_eq!(
            Look::current(&bar, ui::Theme::DEFAULT_RADIUS_SCALE),
            Some(Look::Floating)
        );
    }

    /// A preset never touches what the user placed in the bar.
    #[test]
    fn presets_leave_widgets_alone() {
        let mut bar = BarConfig {
            start: vec!["Clock".into()],
            size: 40.0,
            ..Default::default()
        };
        Look::Blocky.apply(&mut bar);
        assert_eq!(bar.start, vec!["Clock".into()]);
        assert_eq!(bar.size, 40.0);
    }
}
