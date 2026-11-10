//! What the bar tells a widget about its place: orientation, the ink it may
//! draw with, the chip it sits in and the sizes it lays out with. Built once
//! per bar from the config and theme and handed to every widget, so no
//! widget reads `theme.colors` or picks a literal pixel itself.
//!
//! Every size derives from the bar's [`BarRhythm`]: one unit from the
//! thickness a segment has across the bar, and everything else a small
//! multiple of it (the table on `BarRhythm` has the ratios).

use gpui::{Hsla, Pixels, px};
use ui::patterns::{BarRhythm, ChipGeometry};
use ui::{IconSize, Radius, StatusColors, TextSize, Theme, inset_radius};

use crate::bar::config::{BarConfig, BarStyle};

/// Colours a widget draws with. From the theme today; the hook for a
/// segment's own fill later, which is why widgets never read the theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    /// Labels, icons and values.
    pub text: Hsla,
    /// Secondary text: a date part, an artist, an empty workspace's number.
    pub muted: Hsla,
    /// The one highlight colour: the active workspace, an unread badge.
    pub accent: Hsla,
    /// Text drawn on top of `accent`.
    pub on_accent: Hsla,
    /// Fill of something present but not active: an occupied workspace.
    pub occupied: Hsla,
    /// Fill of a cell under the pointer.
    pub hover: Hsla,
    /// Attention colours, used only past a threshold.
    pub status: StatusColors,
}

impl Ink {
    pub fn from_theme(theme: &Theme) -> Self {
        let colors = theme.colors();
        Self {
            text: colors.text,
            muted: colors.text_muted,
            accent: colors.accent,
            on_accent: colors.background,
            occupied: colors.elevated_surface_background,
            hover: colors.element_hover,
            status: colors.status,
        }
    }
}

/// The chrome around a widget: the chip's box and its colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChipStyle {
    /// A chip holding one icon and a value.
    pub single: ChipGeometry,
    /// A chip holding several items (workspaces, tray).
    pub group: ChipGeometry,
    pub background: Hsla,
    pub border: Option<Hsla>,
    /// Resting fill of an interactive chip under the pointer.
    pub hover: Hsla,
}

/// Size of a workspace cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellMetrics {
    pub width: Pixels,
    pub width_active: Pixels,
    pub height: Pixels,
    /// Concentric with the chip around it.
    pub radius: Pixels,
}

/// The sizes every widget lays out with, all on the bar's rhythm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarMetrics {
    pub icon: IconSize,
    pub label: TextSize,
    /// Gap between an icon and its value, and between stacked stats.
    pub gap: Pixels,
    /// Gap between the icons of a widget that shows several (settings) and
    /// the visual distance between tray icons.
    pub icon_gap: Pixels,
    /// Gap between the cells of a grouped widget.
    pub group_gap: Pixels,
    /// Gap between the lines of a stacked title.
    pub line_gap: Pixels,
    /// Hit target of a tray item: its icon plus a unit each side.
    pub tray_item: Pixels,
    /// Gap between tray items, so their icons sit `icon_gap` apart.
    pub tray_gap: Pixels,
    pub cell: CellMetrics,
    /// Length of the hairline between a dense widget's sections.
    pub divider: Pixels,
    /// Width every line of a vertical stack is centred in.
    pub text_line: Pixels,
    /// Widest a horizontal title grows before it truncates.
    pub title_width: Pixels,
    /// Characters a title keeps before it is cut at a word boundary.
    pub title_chars: usize,
}

impl BarMetrics {
    /// Characters that fit `title_width` at the base font, roughly.
    const CHARS_PER_EM: f32 = 1.8;

    pub fn derive(rhythm: &BarRhythm, vertical: bool, chip_radius: Pixels, theme: &Theme) -> Self {
        let chip = rhythm.chip();
        let icon = rhythm.icon();
        let cell_side = (chip - rhythm.unit * 2.).max(px(0.));
        // Titles get 36 ems; that is the 460px they had at the 13px base.
        let title_width = theme.font_size * 36.;
        Self {
            icon: IconSize::Exact(f32::from(icon)),
            label: if vertical {
                TextSize::XSmall
            } else {
                TextSize::Small
            },
            gap: rhythm.breath,
            icon_gap: rhythm.icon_gap(),
            // Cells are fills; a unit apart they merge into one shape.
            group_gap: rhythm.breath,
            line_gap: px(1.),
            tray_item: icon + rhythm.unit * 2.,
            tray_gap: (rhythm.icon_gap() - rhythm.unit * 2.).max(px(0.)),
            cell: CellMetrics {
                width: cell_side,
                // The active cell stretches into a pill.
                width_active: px((f32::from(cell_side) * 1.4).round()),
                height: cell_side,
                radius: inset_radius(chip_radius, rhythm.unit),
            },
            divider: chip - rhythm.breath * 2.,
            // The chip's full width: a wide value overflows it centred
            // rather than wrapping.
            text_line: chip,
            title_width,
            title_chars: (f32::from(title_width) / f32::from(theme.font_size) * Self::CHARS_PER_EM)
                as usize,
        }
    }
}

/// The box each section entry draws around its widgets. Every style renders
/// segments; only bubbles give them a fill, a border and room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentStyle {
    pub fill: Option<Hsla>,
    pub border: Option<Hsla>,
    pub radius: Pixels,
    /// Across the bar; what [`BarConfig::segment_thickness`] leaves.
    pub thickness: Pixels,
    /// Inside the segment's two ends.
    pub padding: Pixels,
    /// Between segments.
    pub gap: Pixels,
    /// Between a segment and the bar's long edges.
    pub inset: Pixels,
}

impl SegmentStyle {
    pub fn new(bar: &BarConfig, theme: &Theme) -> Self {
        let spacing = bar.segment_spacing();
        let bubbles = bar.style == BarStyle::Bubbles;
        let thickness = px(bar.segment_thickness());
        Self {
            fill: bubbles.then(|| bar.background_color(theme)),
            border: (bubbles && bar.show_border).then_some(theme.colors().border),
            // A pill, which is as round as half its thickness lets it be.
            radius: Radius::Full.pixels().min(thickness / 2.),
            thickness,
            padding: px(spacing.padding),
            gap: px(spacing.gap),
            inset: px(spacing.inset),
        }
    }
}

/// Everything a widget needs from the bar to render.
#[derive(Clone, Debug, PartialEq)]
pub struct BarCtx {
    pub vertical: bool,
    pub ink: Ink,
    pub chip: ChipStyle,
    pub metrics: BarMetrics,
    pub segment: SegmentStyle,
    pub rhythm: BarRhythm,
    /// Padding at the bar's two ends: `[bar] padding`, or a unit so the
    /// first chip sits one breath from the end like every other.
    pub end_padding: Pixels,
}

impl BarCtx {
    pub fn new(bar: &BarConfig, theme: &Theme) -> Self {
        let vertical = bar.is_vertical();
        let colors = theme.colors();
        let segment = SegmentStyle::new(bar, theme);
        // From the bar, not the segment: a bubbles inset must not shrink the
        // icons and chips, it only takes from the room around them.
        let rhythm = bar.rhythm();
        let chip_radius = chip_radius(bar, &segment, &rhythm);
        let bare = !bar.widget_background && !bar.widget_border;
        let geometry = |grouped: bool| {
            let mut chip = if bar.widget_stretch {
                ChipGeometry::stretched(&rhythm, grouped, chip_radius)
            } else {
                ChipGeometry::derive(&rhythm, grouped, chip_radius)
            };
            // A chip never outgrows a pill that is inset from the bar.
            chip.thickness = chip.thickness.map(|t| t.min(segment.thickness));
            if bar.widget_stretch && grouped {
                // Cells are their own boxes; a flush strip needs no ends.
                chip.padding_main = px(0.);
            } else if bare {
                // Nothing draws the chip, so its ends would only read as a
                // gap: keep a unit, and neighbours sit a breath apart.
                chip.padding_main = rhythm.unit;
            }
            chip
        };
        let mut metrics = BarMetrics::derive(&rhythm, vertical, chip_radius, theme);
        if bar.style == BarStyle::Bubbles && !bar.widget_stretch {
            // Inside a pill the cells are pills too (square at scale 0).
            metrics.cell.radius = Radius::Full.pixels().min(metrics.cell.height / 2.);
        }
        if bar.widget_stretch {
            // Square cells as tall as the segment, a hairline apart so two
            // occupied neighbours do not fuse into one block.
            metrics.cell = CellMetrics {
                width: segment.thickness,
                width_active: segment.thickness,
                height: segment.thickness,
                radius: chip_radius,
            };
            metrics.group_gap = px(1.);
        }
        let end_padding = match bar.padding {
            Some(padding) => px(padding.max(0.)),
            None if bar.style == BarStyle::Bubbles => segment.gap,
            None if bar.widget_stretch => px(0.),
            None => rhythm.unit,
        };
        Self {
            vertical,
            ink: Ink::from_theme(theme),
            chip: ChipStyle {
                single: geometry(false),
                group: geometry(true),
                background: if bar.widget_background {
                    colors.surface_background
                } else {
                    colors.border_transparent
                },
                border: bar.widget_border.then_some(if vertical {
                    colors.border_variant
                } else {
                    colors.border
                }),
                hover: if bar.widget_background {
                    colors.elevated_surface_background
                } else {
                    colors.element_hover
                },
            },
            metrics,
            segment,
            rhythm,
            end_padding,
        }
    }

    /// The colour of the hairline between a dense widget's sections; gone
    /// with the chip border, as before.
    pub fn divider_color(&self) -> Hsla {
        self.chip.border.unwrap_or(gpui::transparent_black())
    }
}

/// A chip's corner, concentric with whatever corner it sits inside: the
/// floating bar's, a bubble's; on its own otherwise.
fn chip_radius(bar: &BarConfig, segment: &SegmentStyle, rhythm: &BarRhythm) -> Pixels {
    match bar.style {
        BarStyle::Floating => inset_radius(bar.floating.radius(), rhythm.breath + segment.inset),
        BarStyle::Bubbles => inset_radius(segment.radius, rhythm.breath),
        BarStyle::Attached | BarStyle::Frame => Radius::Large.pixels(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bar::config::BarPosition;

    fn top(size: f32) -> BarConfig {
        BarConfig {
            position: BarPosition::Top,
            size,
            show_border: false,
            ..Default::default()
        }
    }

    /// A bar without widget chrome hands out invisible fills, not missing
    /// ones, so a widget never has to special-case it.
    #[test]
    fn chrome_toggles_only_change_colours() {
        let theme = Theme::default();
        let plain = BarCtx::new(
            &BarConfig {
                widget_background: false,
                widget_border: false,
                ..Default::default()
            },
            &theme,
        );
        assert_eq!(plain.chip.background.a, 0.0);
        assert_eq!(plain.chip.border, None);
        assert_eq!(plain.divider_color().a, 0.0);

        let chrome = BarCtx::new(&BarConfig::default(), &theme);
        assert!(chrome.chip.border.is_some());
    }

    /// Everything scales with the bar: a 32px bar and a 64px bar have the
    /// same proportions, in whole pixels.
    #[test]
    fn metrics_follow_the_bar_size() {
        let theme = Theme::default();
        let small = BarCtx::new(&top(32.), &theme);
        let large = BarCtx::new(&top(64.), &theme);

        assert_eq!(small.rhythm.unit, px(2.));
        assert_eq!(small.chip.single.thickness, Some(px(24.)));
        assert_eq!(small.metrics.icon, IconSize::Exact(16.));
        assert_eq!(small.metrics.cell.height, px(20.));
        assert_eq!(small.end_padding, px(2.));

        assert_eq!(large.rhythm.unit, px(4.));
        assert_eq!(large.chip.single.thickness, Some(px(48.)));
        assert_eq!(large.metrics.icon, IconSize::Exact(32.));
        assert_eq!(large.metrics.cell.height, px(40.));
    }

    /// Corners nest: a chip in a floating bar is the bar's radius minus the
    /// breath around it; a cell is a unit tighter than its chip.
    #[test]
    fn radii_are_concentric() {
        let theme = Theme::default();
        let floating = BarCtx::new(
            &BarConfig {
                style: BarStyle::Floating,
                ..top(32.)
            },
            &theme,
        );
        let bar_radius = floating.segment.inset + floating.rhythm.breath;
        assert_eq!(
            floating.chip.single.radius,
            Radius::XLarge.pixels() - bar_radius
        );
        assert_eq!(
            floating.metrics.cell.radius,
            floating.chip.single.radius - floating.rhythm.unit
        );
    }

    /// Only bubbles paint their segments; the rest keep them invisible so
    /// a floating or attached bar lays out exactly as before.
    #[test]
    fn only_bubbles_paint_segments() {
        let theme = Theme::default();
        let floating = BarCtx::new(&BarConfig::default(), &theme);
        assert_eq!(floating.segment.fill, None);
        assert_eq!(floating.segment.border, None);
        assert_eq!(floating.segment.gap, px(0.));
        assert_eq!(floating.segment.padding, px(0.));

        let bubbles = BarCtx::new(
            &BarConfig {
                style: BarStyle::Bubbles,
                ..Default::default()
            },
            &theme,
        );
        assert!(bubbles.segment.fill.is_some());
        assert!(bubbles.segment.border.is_some());
        assert_eq!(bubbles.segment.gap, px(6.));
        assert_eq!(bubbles.end_padding, px(6.));
    }

    /// A bubbles inset takes room from around the chips, never from the
    /// chips: icons and cells match the attached bar's, and a chip is
    /// clamped to its pill. Bare chips keep only a unit at their ends.
    #[test]
    fn bubbles_keep_the_bars_rhythm() {
        let theme = Theme::default();
        let attached = BarCtx::new(&top(32.), &theme);
        let bubbles = BarCtx::new(
            &BarConfig {
                style: BarStyle::Bubbles,
                widget_background: false,
                widget_border: false,
                ..top(32.)
            },
            &theme,
        );
        assert_eq!(bubbles.rhythm, attached.rhythm);
        assert_eq!(bubbles.metrics.icon, attached.metrics.icon);
        assert_eq!(bubbles.metrics.cell.height, attached.metrics.cell.height);
        assert_eq!(bubbles.segment.thickness, px(26.));
        assert_eq!(bubbles.chip.single.thickness, Some(px(24.)));
        assert_eq!(bubbles.chip.single.padding_main, bubbles.rhythm.unit);
        assert_eq!(attached.chip.single.padding_main, px(8.));
        // Cells in a pill are pills; at scale 0 they would be square.
        assert_eq!(
            bubbles.metrics.cell.radius,
            bubbles.metrics.cell.height / 2.
        );

        let tiny = BarCtx::new(
            &BarConfig {
                style: BarStyle::Bubbles,
                size: 20.,
                ..top(20.)
            },
            &theme,
        );
        // 20 − 2 × 3 = 14 of pill; the chip cannot be the 16 the rhythm wants.
        assert_eq!(tiny.chip.single.thickness, Some(px(14.)));
    }

    /// Stretched chips fill the segment with no margin and the workspace
    /// cells become touching squares of the same thickness.
    #[test]
    fn stretch_fills_the_segment() {
        let theme = Theme::default();
        let bar = BarConfig {
            widget_stretch: true,
            ..top(32.)
        };
        let ctx = BarCtx::new(&bar, &theme);
        let thickness = px(bar.size);
        assert_eq!(ctx.chip.single.thickness, Some(thickness));
        assert_eq!(ctx.chip.single.margin, px(0.));
        assert_eq!(ctx.chip.single.padding_cross, px(0.));
        assert_eq!(ctx.metrics.cell.width, thickness);
        assert_eq!(ctx.metrics.cell.width_active, thickness);
        assert_eq!(ctx.metrics.group_gap, px(1.));
        assert_eq!(ctx.end_padding, px(0.));
    }
}
