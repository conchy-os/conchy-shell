//! Spacing, typography, radius and opacity tokens.

use std::sync::atomic::{AtomicU32, Ordering};

use gpui::{Hsla, Pixels, Rems, px, rems};

use super::Appearance;

/// gpui's default rem size.
const NOMINAL_REM_PX: f32 = 16.0;

/// Semantic spacing step used for gaps, padding, and margins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Spacing {
    /// 0px
    None,
    /// 2px
    XXSmall,
    /// 4px
    XSmall,
    /// 6px
    Small,
    /// 8px
    Medium,
    /// 12px
    Large,
    /// 16px
    XLarge,
    /// 20px
    XXLarge,
    /// 24px
    XXXLarge,
}

impl Spacing {
    pub const fn pixels(self) -> Pixels {
        px(self.value())
    }

    /// The raw pixel count.
    pub const fn value(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::XXSmall => 2.0,
            Self::XSmall => 4.0,
            Self::Small => 6.0,
            Self::Medium => 8.0,
            Self::Large => 12.0,
            Self::XLarge => 16.0,
            Self::XXLarge => 20.0,
            Self::XXXLarge => 24.0,
        }
    }
}

/// Semantic text size, expressed as a ratio of the configured base font size.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextSize {
    /// 0.77 rem (~10px at a 13px base)
    XSmall,
    /// 0.85 rem (~11px at a 13px base)
    Small,
    /// 1 rem - the configured base font size
    #[default]
    Default,
    /// 1.08 rem (~14px at a 13px base)
    Medium,
    /// 1.23 rem (~16px at a 13px base)
    Large,
    /// 1.38 rem (~18px at a 13px base)
    XLarge,
}

impl TextSize {
    pub fn rems(self) -> Rems {
        rems(self.ratio())
    }

    /// This size's multiplier over the base font size.
    pub const fn ratio(self) -> f32 {
        match self {
            Self::XSmall => 0.77,
            Self::Small => 0.85,
            Self::Default => 1.0,
            Self::Medium => 1.08,
            Self::Large => 1.23,
            Self::XLarge => 1.38,
        }
    }

    /// Resolve to absolute pixels against an explicit base font size.
    pub fn pixels(self, base: Pixels) -> Pixels {
        base * self.ratio()
    }
}

/// Semantic icon size.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum IconSize {
    /// 10px - status dots and other decorations.
    Indicator,
    /// 12px
    XSmall,
    /// 14px
    Small,
    /// 16px
    #[default]
    Medium,
    /// 18px
    Large,
    /// 20px
    XLarge,
    /// 48px
    XXLarge,
    /// A caller-specified size in [`Rems`], bypassing the preset ladder.
    Custom(Rems),
    /// A caller-specified size in absolute pixels, which does *not* scale
    /// with the window's rem size.
    Exact(f32),
    /// The whole of the parent, for an icon inside a box that is sized -
    /// or resized on hover - by its container.
    Fill,
}

impl IconSize {
    pub fn pixels(self) -> Pixels {
        match self {
            // `Custom` is authored in rems; converting back assumes the
            // nominal 16px rem. Prefer `rems()` when going through a
            // rem-aware API.
            Self::Custom(r) => px(r.0 * NOMINAL_REM_PX),
            Self::Exact(p) => px(p),
            other => px(other.value()),
        }
    }

    /// Size in rems, so icons scale with the window's rem size the way text does.
    pub fn rems(self) -> Rems {
        match self {
            Self::Custom(r) => r,
            Self::Exact(p) => rems(p / NOMINAL_REM_PX),
            other => rems(other.value() / NOMINAL_REM_PX),
        }
    }

    /// The raw pixel count of a preset; `Fill` has none and reads as `Medium`.
    pub const fn value(self) -> f32 {
        match self {
            Self::Custom(_) | Self::Fill => NOMINAL_REM_PX,
            Self::Exact(p) => p,
            Self::Indicator => 10.0,
            Self::XSmall => 12.0,
            Self::Small => 14.0,
            Self::Medium => 16.0,
            Self::Large => 18.0,
            Self::XLarge => 20.0,
            Self::XXLarge => 48.0,
        }
    }
}

/// Semantic border radius.
///
/// Every value is multiplied by the theme's `radius_scale`, so `0` is square
/// everywhere (pills and circles included) and `>1` is rounder.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Radius {
    /// 0px
    None,
    /// 2px
    XSmall,
    /// 4px
    Small,
    /// 6px
    #[default]
    Medium,
    /// 8px
    Large,
    /// 12px
    XLarge,
    /// 16px
    XXLarge,
    /// Fully rounded (pill).
    Full,
}

/// The theme's `radius_scale`, as f32 bits. Process-wide rather than a
/// theme argument so the ~100 `Radius::X.pixels()` call sites, many of them
/// in closures without a context, stay as they are; `Theme::set` keeps it in
/// step with the global theme.
static RADIUS_SCALE: AtomicU32 = AtomicU32::new(1.0f32.to_bits());

/// The multiplier every [`Radius`] currently resolves through.
pub fn radius_scale() -> f32 {
    f32::from_bits(RADIUS_SCALE.load(Ordering::Relaxed))
}

pub(crate) fn set_radius_scale(scale: f32) {
    RADIUS_SCALE.store(scale.to_bits(), Ordering::Relaxed);
}

impl Radius {
    /// The largest scale the theme accepts; past it every corner is a pill.
    pub const MAX_SCALE: f32 = 4.0;

    pub fn pixels(self) -> Pixels {
        px(self.value())
    }

    /// The pixel count after the theme scale.
    pub fn value(self) -> f32 {
        self.scaled(radius_scale())
    }

    /// The pixel count before scaling.
    pub const fn base(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::XSmall => 2.0,
            Self::Small => 4.0,
            Self::Medium => 6.0,
            Self::Large => 8.0,
            Self::XLarge => 12.0,
            Self::XXLarge => 16.0,
            Self::Full => 9999.0,
        }
    }

    /// `base` at an explicit scale; what [`value`](Self::value) does with
    /// the theme's.
    pub fn scaled(self, scale: f32) -> f32 {
        self.base() * scale.clamp(0.0, Self::MAX_SCALE)
    }

    /// The radius of something `padding` inside a corner of this radius,
    /// so the two curves share a centre. Never below zero.
    pub fn inset(self, padding: Pixels) -> Pixels {
        inset_radius(self.pixels(), padding)
    }
}

/// `outer` minus `padding`, floored at zero: the inner radius that keeps a
/// nested corner concentric with the one around it.
pub fn inset_radius(outer: Pixels, padding: Pixels) -> Pixels {
    (outer - padding).max(px(0.))
}

/// [`inset_radius`] for a card or row inside a padded surface: concentric,
/// but never sharper than [`Radius::Large`] while the surface itself is
/// rounded, so a panel padded by its own radius keeps soft, round cards.
pub fn nested_radius(outer: Pixels, padding: Pixels) -> Pixels {
    inset_radius(outer, padding).max(Radius::Large.pixels().min(outer))
}

/// Semantic alpha for UI states. These fade text, borders and fills within a
/// surface; the surface's own translucency is `Theme::surface` and is never
/// multiplied into content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Opacity {
    /// 0: a gradient's transparent end.
    Hidden,
    /// 0.08: a drop shadow's ink.
    Shadow,
    /// 0.15: a tint behind text (subtle button fill, error background, hover wash).
    Faint,
    /// 0.3: a quiet border or focus ring.
    Subtle,
    /// 0.5: a selected fill, a secondary border, an underline.
    Muted,
    /// 0.7: a disabled control, a strong border tint.
    Soft,
}

impl Opacity {
    pub const fn value(self) -> f32 {
        match self {
            Self::Hidden => 0.0,
            Self::Shadow => 0.08,
            Self::Faint => 0.15,
            Self::Subtle => 0.3,
            Self::Muted => 0.5,
            Self::Soft => 0.7,
        }
    }

    /// Borders need more alpha on a light surface to read at all, shadows
    /// less; the rest are the same either way.
    pub const fn on(self, appearance: Appearance) -> f32 {
        match (self, appearance) {
            (Self::Shadow, Appearance::Light) => 0.04,
            (Self::Subtle, Appearance::Light) => 0.45,
            (Self::Muted, Appearance::Light) => 0.65,
            _ => self.value(),
        }
    }
}

/// `color.fade(Opacity::Muted)` in place of `color.opacity(0.5)`.
pub trait Fade {
    fn fade(self, opacity: Opacity) -> Self;
    /// [`fade`](Self::fade) with the light/dark variant of the token.
    fn fade_on(self, opacity: Opacity, appearance: Appearance) -> Self;
}

impl Fade for Hsla {
    fn fade(self, opacity: Opacity) -> Self {
        self.opacity(opacity.value())
    }

    fn fade_on(self, opacity: Opacity, appearance: Appearance) -> Self {
        self.opacity(opacity.on(appearance))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_is_monotonically_increasing() {
        let steps = [
            Spacing::None,
            Spacing::XXSmall,
            Spacing::XSmall,
            Spacing::Small,
            Spacing::Medium,
            Spacing::Large,
            Spacing::XLarge,
            Spacing::XXLarge,
            Spacing::XXXLarge,
        ];
        for window in steps.windows(2) {
            assert!(
                window[0].pixels() < window[1].pixels(),
                "{:?} ({:?}) should be less than {:?} ({:?})",
                window[0],
                window[0].pixels(),
                window[1],
                window[1].pixels(),
            );
        }
    }

    #[test]
    fn radius_is_monotonically_increasing() {
        let steps = [
            Radius::None,
            Radius::XSmall,
            Radius::Small,
            Radius::Medium,
            Radius::Large,
            Radius::XLarge,
            Radius::XXLarge,
            Radius::Full,
        ];
        for window in steps.windows(2) {
            assert!(
                window[0].base() < window[1].base(),
                "{:?} should be smaller than {:?}",
                window[0],
                window[1],
            );
        }
    }

    /// Scale zero is square everywhere - the pill included - and the scale
    /// never goes negative or past the cap.
    #[test]
    fn radius_scale_applies_to_every_token() {
        for radius in [Radius::XSmall, Radius::XLarge, Radius::Full] {
            assert_eq!(radius.scaled(0.0), 0.0, "{radius:?}");
            assert_eq!(radius.scaled(-1.0), 0.0, "{radius:?}");
            assert_eq!(radius.scaled(1.0), radius.base(), "{radius:?}");
            assert_eq!(radius.scaled(1.5), radius.base() * 1.5, "{radius:?}");
            assert_eq!(
                radius.scaled(99.0),
                radius.base() * Radius::MAX_SCALE,
                "{radius:?}"
            );
        }
    }

    #[test]
    fn inset_radius_stays_concentric_and_never_negative() {
        assert_eq!(inset_radius(px(12.), px(4.)), px(8.));
        assert_eq!(inset_radius(px(2.), px(4.)), px(0.));
    }

    /// A nested corner is concentric when there is room, floors at the
    /// large token otherwise, and stays square inside a square surface.
    #[test]
    fn nested_radius_floors_at_large_unless_square() {
        let large = Radius::Large.pixels();
        assert_eq!(nested_radius(px(24.), px(12.)), px(12.));
        assert_eq!(nested_radius(px(12.), px(12.)), large);
        assert_eq!(nested_radius(px(0.), px(12.)), px(0.));
        assert_eq!(nested_radius(px(2.), px(0.)), px(2.));
    }

    #[test]
    fn opacity_tokens_are_distinct_and_ordered() {
        let steps = [
            Opacity::Hidden,
            Opacity::Shadow,
            Opacity::Faint,
            Opacity::Subtle,
            Opacity::Muted,
            Opacity::Soft,
        ];
        for window in steps.windows(2) {
            assert!(window[0].value() < window[1].value(), "{window:?}");
        }
        assert!(Opacity::Subtle.on(Appearance::Light) > Opacity::Subtle.value());
        assert_eq!(Opacity::Faint.on(Appearance::Light), Opacity::Faint.value());
    }

    #[test]
    fn text_size_is_monotonically_increasing() {
        let steps = [
            TextSize::XSmall,
            TextSize::Small,
            TextSize::Default,
            TextSize::Medium,
            TextSize::Large,
            TextSize::XLarge,
        ];
        for window in steps.windows(2) {
            assert!(
                window[0].ratio() < window[1].ratio(),
                "{:?} should be smaller than {:?}",
                window[0],
                window[1],
            );
        }
    }

    #[test]
    fn text_size_default_is_exactly_the_base() {
        assert_eq!(TextSize::Default.ratio(), 1.0);
        assert_eq!(TextSize::Default.pixels(px(13.0)), px(13.0));
    }

    /// The ratios must keep reproducing the scale the shell shipped before
    /// (`FontSizes::new(13.0)`), or every module silently reflows.
    #[test]
    fn text_size_reproduces_the_previous_font_scale() {
        let base = px(13.0);
        for (size, expected) in [
            (TextSize::XSmall, 10.01),
            (TextSize::Small, 11.05),
            (TextSize::Default, 13.0),
            (TextSize::Medium, 14.04),
            (TextSize::Large, 15.99),
            (TextSize::XLarge, 17.94),
        ] {
            let got: f32 = size.pixels(base).into();
            assert!(
                (got - expected).abs() < 0.01,
                "{size:?}: got {got}, expected {expected}"
            );
        }
    }
}
