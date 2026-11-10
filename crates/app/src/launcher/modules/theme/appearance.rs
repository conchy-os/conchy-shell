//! The theme's non-colour knobs - font size, corner radius, surface opacity
//! and the bar look - on their own page behind one row of the `~` list, so
//! the list stays about schemes. Enter steps a knob up, Shift+Enter down, the
//! action panel offers the presets; every change applies at once and is
//! saved to `theme.toml`, like a palette.

use gpui::{AnyElement, App, Context, FontWeight, div, prelude::*, px};
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, TextSize,
    Theme,
};

use super::apply_theme;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Detail, Item, List, Section};
use crate::launcher::page::LauncherPage;

/// One knob: how it reads, how it steps and which presets it offers.
struct Knob {
    id: &'static str,
    title: &'static str,
    subtitle: &'static str,
    icon: IconName,
    keywords: &'static str,
    get: fn(&Theme) -> f32,
    set: fn(&mut Theme, f32),
    /// `(label, value)`, in display order.
    presets: &'static [(&'static str, f32)],
    step: f32,
    range: (f32, f32),
    format: fn(f32) -> String,
}

const FONT: Knob = Knob {
    id: "appearance:font",
    title: "Font size",
    subtitle: "Base size every text size scales from",
    icon: IconName::FileText,
    keywords: "font text size scale",
    get: |theme| f32::from(theme.font_size),
    set: |theme, value| theme.font_size = px(value),
    presets: &[
        ("11 px", 11.0),
        ("12 px", 12.0),
        ("13 px", 13.0),
        ("14 px", 14.0),
        ("15 px", 15.0),
        ("16 px", 16.0),
    ],
    step: 1.0,
    range: (9.0, 24.0),
    format: |value| format!("{value:.0} px"),
};

const RADIUS: Knob = Knob {
    id: "appearance:radius",
    title: "Corner radius",
    subtitle: "Square at 0, rounder past 1; applies to every surface and chip",
    icon: IconName::Square,
    keywords: "corner radius round square blocky rounded shape",
    get: |theme| theme.radius_scale,
    set: Theme::set_radius_scale,
    presets: &[
        ("Square", 0.0),
        ("Soft", 0.5),
        ("Default", 1.0),
        ("Round", 1.5),
        ("Pill", 2.0),
    ],
    step: 0.25,
    range: (0.0, 3.0),
    format: |value| {
        RADIUS
            .presets
            .iter()
            .find(|(_, preset)| (preset - value).abs() < f32::EPSILON)
            .map_or_else(|| format!("{value:.2}×"), |(label, _)| label.to_string())
    },
};

const OPACITY: Knob = Knob {
    id: "appearance:opacity",
    title: "Surface opacity",
    subtitle: "Bar, panels, launcher, OSD, popups and dock; each can override it in config.toml",
    icon: IconName::Eye,
    keywords: "opacity transparent translucent blur alpha background",
    get: |theme| theme.opacity,
    set: Theme::set_opacity,
    presets: &[
        ("100%", 1.0),
        ("90%", 0.9),
        ("80%", 0.8),
        ("70%", 0.7),
        ("60%", 0.6),
        ("50%", 0.5),
    ],
    step: 0.05,
    range: (0.1, 1.0),
    format: |value| format!("{:.0}%", value * 100.),
};

const KNOBS: [&Knob; 3] = [&FONT, &RADIUS, &OPACITY];

/// The page the knobs live on.
pub(super) struct AppearancePage;

impl LauncherPage for AppearancePage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        List::new()
            .title("Appearance")
            .placeholder("Enter steps up, Shift+Enter down, Tab for presets")
            .section(Section::untitled().items(items(cx)))
            .into()
    }
}

/// The one row the `~` list keeps: opens [`AppearancePage`]. Its subtitle
/// carries the current values, so the list still answers at a glance.
pub(super) fn entry_row<V: 'static>(cx: &Context<V>) -> Item {
    let theme = cx.theme();
    let summary = format!(
        "{} · {} · {}",
        (FONT.format)((FONT.get)(theme)),
        (RADIUS.format)((RADIUS.get)(theme)),
        (OPACITY.format)((OPACITY.get)(theme)),
    );
    Item::new("appearance", "Appearance")
        .subtitle("Font size, corner radius, surface opacity and the bar look")
        .icon(IconName::Sliders)
        .keywords("appearance font size corner radius opacity look blocky rounded")
        .accessory(Accessory::text(summary))
        .action(
            Action::new("Open", |_, cx| Outcome::push(cx.new(|_| AppearancePage)))
                .icon(IconName::ChevronRight),
        )
}

/// The rows, in the order they are listed: the bar look first, since it
/// is what people come looking for, then the three knobs.
pub(super) fn items<V: 'static>(cx: &Context<V>) -> Vec<Item> {
    let theme = cx.theme();
    std::iter::once(look_row(cx))
        .chain(KNOBS.iter().map(|knob| knob.item(theme, cx)))
        .collect()
}

/// A pointer to the config editor's Look page, which sets the `[bar]`
/// keys and the radius scale together.
fn look_row<V: 'static>(cx: &Context<V>) -> Item {
    let bar = &crate::config::Config::global(cx).bar;
    let current = crate::bar::look::Look::current(bar, cx.theme().radius_scale)
        .map_or("Custom", |look| look.label());
    let prefix = crate::config::Config::global(cx)
        .launcher
        .modules
        .config
        .prefix
        .clone();
    Item::new("appearance:look", "Bar look")
        .subtitle("Blocky, rounded or floating - a preset for bar and corners")
        .icon(IconName::Sparkles)
        .keywords("look preset blocky rounded floating bar style")
        .accessory(Accessory::text(current))
        .action(
            Action::new("Choose a look", move |_, _| {
                Outcome::SetQuery(format!("{prefix}look"))
            })
            .icon(IconName::ChevronRight),
        )
}

impl Knob {
    fn item<V: 'static>(&'static self, theme: &Theme, cx: &Context<V>) -> Item {
        let current = (self.get)(theme);
        let step = |title: &'static str, icon: IconName, direction: f32| {
            Action::listener(title, cx, move |_, _, cx| {
                let mut theme = cx.theme().clone();
                let value = (self.get)(&theme) + self.step * direction;
                (self.set)(
                    &mut theme,
                    snap(value, self.step).clamp(self.range.0, self.range.1),
                );
                apply_theme(theme, cx);
                cx.notify();
                Outcome::Stay
            })
            .icon(icon)
        };
        let presets = self.presets.iter().map(|(label, value)| {
            let value = *value;
            Action::listener(*label, cx, move |_, _, cx| {
                let mut theme = cx.theme().clone();
                (self.set)(&mut theme, value);
                apply_theme(theme, cx);
                cx.notify();
                Outcome::Stay
            })
            .icon(if (value - current).abs() < f32::EPSILON {
                IconName::Check
            } else {
                IconName::ChevronRight
            })
        });
        let actions = ActionPanel::new()
            .action(step("Increase", IconName::ChevronUp, 1.0))
            .action(step("Decrease", IconName::ChevronDown, -1.0))
            .section("Presets", presets);

        Item::new(self.id, self.title)
            .subtitle(self.subtitle)
            .icon(self.icon)
            .keywords(self.keywords)
            .accessory(Accessory::text((self.format)(current)))
            .actions(actions)
            .detail(Detail::new().widget(preview))
    }
}

/// Keep stepped values on the step grid, so 0.3 + 0.25 reads 0.5, not 0.55.
fn snap(value: f32, step: f32) -> f32 {
    (value / step).round() * step
}

/// A miniature of the shell drawn with the live tokens: a bar with two
/// chips over a panel card, so all three knobs show at once.
fn preview(cx: &App) -> AnyElement {
    let theme = cx.theme();
    let colors = theme.colors();
    let chip = |icon: IconName, text: &'static str| {
        div()
            .flex()
            .items_center()
            .gap(Spacing::XSmall.pixels())
            .px(Spacing::Medium.pixels())
            .py(Spacing::XSmall.pixels())
            .rounded(Radius::Large.pixels())
            .bg(colors.surface_background)
            .border_1()
            .border_color(colors.border)
            .child(Icon::new(icon).size(IconSize::Small))
            .child(Label::new(text).size(TextSize::Small))
    };
    let summary = format!(
        "{} · {} · {}",
        (FONT.format)((FONT.get)(theme)),
        (RADIUS.format)((RADIUS.get)(theme)),
        (OPACITY.format)((OPACITY.get)(theme)),
    );

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(Spacing::Medium.pixels())
        .child(
            // Something behind the sample so its opacity shows.
            div()
                .w_full()
                .p(Spacing::Medium.pixels())
                .rounded(Radius::Large.pixels())
                .bg(colors.accent)
                .flex()
                .flex_col()
                .gap(Spacing::Medium.pixels())
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(Spacing::Medium.pixels())
                        .py(Spacing::XSmall.pixels())
                        .rounded(Radius::XLarge.pixels())
                        .bg(theme.surface(None))
                        .border_1()
                        .border_color(colors.border)
                        .child(chip(IconName::Layers, "1"))
                        .child(chip(IconName::Clock, "09:41")),
                )
                .child(
                    div()
                        .w_full()
                        .p(Spacing::Large.pixels())
                        .rounded(Radius::Large.pixels())
                        .bg(theme.surface(None))
                        .border_1()
                        .border_color(colors.border)
                        .flex()
                        .flex_col()
                        .gap(Spacing::XSmall.pixels())
                        .child(
                            Label::new("Panel")
                                .size(TextSize::Default)
                                .weight(FontWeight::SEMIBOLD),
                        )
                        .child(
                            Label::new("Text at the base font size, in a panel surface.")
                                .size(TextSize::Small)
                                .color(Color::Muted),
                        )
                        .child(
                            div()
                                .mt(Spacing::XSmall.pixels())
                                .px(Spacing::Medium.pixels())
                                .py(Spacing::XSmall.pixels())
                                .rounded(Radius::Full.pixels())
                                .bg(colors.accent)
                                .w(px(96.))
                                .child(
                                    Label::new("Pill")
                                        .size(TextSize::XSmall)
                                        .color(Color::Custom(colors.background)),
                                ),
                        ),
                ),
        )
        .child(
            Label::new(summary)
                .size(TextSize::XSmall)
                .color(Color::Muted),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_stays_on_the_grid_and_in_range() {
        assert_eq!(snap(0.3 + 0.25, 0.25), 0.5);
        assert_eq!(snap(1.0 - 0.25, 0.25), 0.75);
        assert_eq!(snap(13.0 + 1.0, 1.0), 14.0);
        let (lo, hi) = RADIUS.range;
        assert_eq!(snap(hi + RADIUS.step, RADIUS.step).clamp(lo, hi), hi);
    }

    #[test]
    fn the_radius_reads_as_its_preset_name() {
        assert_eq!((RADIUS.format)(0.0), "Square");
        assert_eq!((RADIUS.format)(1.0), "Default");
        assert_eq!((RADIUS.format)(0.75), "0.75×");
        assert_eq!((OPACITY.format)(0.85), "85%");
        assert_eq!((FONT.format)(13.0), "13 px");
    }
}
