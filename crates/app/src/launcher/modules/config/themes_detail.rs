use gpui::{App, ElementId, FontWeight, SharedString, div, prelude::*, px};
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, Switch,
    TextSize, builtin_schemes, h_flex, v_flex,
};

use super::schema::{ConfigPath, Editable, Field};
use crate::config::Config;
use crate::launcher::config::{LauncherConfig, ModulesConfig};
use crate::launcher::content::Detail;
use crate::launcher::modules::theme::{apply_palette, apply_theme, palette_is_active};
use crate::launcher::modules::wallpaper::config::{WallpaperConfig, WallpaperScheme};
use crate::launcher::modules::wallpaper::{
    GenerationStatus, generated_palettes, generation_status, palette_chip, regenerate_from_current,
};

fn wallpaper_path() -> ConfigPath<WallpaperConfig> {
    ConfigPath::root()
        .then(
            "launcher",
            |c: &Config| &c.launcher,
            |c: &mut Config| &mut c.launcher,
        )
        .then(
            "modules",
            |l: &LauncherConfig| &l.modules,
            |l: &mut LauncherConfig| &mut l.modules,
        )
        .then(
            "wallpaper",
            |m: &ModulesConfig| &m.wallpaper,
            |m: &mut ModulesConfig| &mut m.wallpaper,
        )
}

fn wallpaper_field(key: &str) -> Option<Field<WallpaperConfig>> {
    WallpaperConfig::fields().into_iter().find(|f| f.key == key)
}

/// Material You from the wallpaper: follow switch, scheme variant and the
/// generated dark and light palettes, ready to apply.
fn wallpaper_card(cx: &App) -> gpui::AnyElement {
    let colors = cx.theme().colors();
    let path = wallpaper_path();
    let config = path.read(cx).clone();

    let follow = {
        let path = path.clone();
        let on = config.auto_theme;
        Switch::new("wallpaper-follow", on).on_click(move |_, _, cx| {
            if let Some(field) = wallpaper_field("auto_theme") {
                path.write(&field, |s| s.auto_theme = !on, cx);
            }
            if !on {
                regenerate_from_current(true, cx);
            }
        })
    };

    let schemes =
        h_flex()
            .w_full()
            .flex_wrap()
            .gap(px(4.))
            .children(WallpaperScheme::ALL.into_iter().map(|scheme| {
                let selected = scheme == config.matugen_scheme;
                let path = path.clone();
                div()
                    .id(SharedString::from(format!(
                        "wp-scheme-{}",
                        scheme.matugen_type()
                    )))
                    .px(Spacing::Medium.pixels())
                    .py(px(4.))
                    .rounded(Radius::Small.pixels())
                    .text_size(TextSize::Small.rems())
                    .cursor_pointer()
                    .when(selected, |this| {
                        this.bg(colors.element_selected)
                            .text_color(colors.text_accent)
                            .font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!selected, |this| {
                        this.text_color(colors.text_muted)
                            .hover(|s| s.bg(colors.element_hover).text_color(colors.text))
                    })
                    .on_click(move |_, _, cx| {
                        if let Some(field) = wallpaper_field("matugen_scheme") {
                            path.write(&field, |s| s.matugen_scheme = scheme, cx);
                        }
                        let apply = path.read(cx).auto_theme;
                        regenerate_from_current(apply, cx);
                    })
                    .child(scheme.label())
            }));

    let status_line = |text: SharedString, color: Color| {
        Label::new(text)
            .size(TextSize::XSmall)
            .color(color)
            .into_any_element()
    };
    let generate_button = |label: &'static str| {
        div()
            .id("themes-wallpaper-generate")
            .cursor_pointer()
            .text_size(TextSize::Small.rems())
            .text_color(colors.text_accent)
            .on_click(|_, _, cx| regenerate_from_current(false, cx))
            .child(label)
            .into_any_element()
    };
    let generated = match (generated_palettes(cx), generation_status(cx)) {
        (_, GenerationStatus::Running(name)) => {
            status_line(format!("Generating from {name}…").into(), Color::Muted)
        }
        (_, GenerationStatus::Failed(err)) => h_flex()
            .w_full()
            .items_center()
            .gap(Spacing::Medium.pixels())
            .child(status_line(err, Color::Error))
            .child(div().flex_1())
            .child(generate_button("Retry"))
            .into_any_element(),
        (Some((name, palettes)), _) => h_flex()
            .w_full()
            .items_center()
            .gap(Spacing::Medium.pixels())
            .child(
                Label::new(SharedString::from(format!("From {name}")))
                    .size(TextSize::XSmall)
                    .color(Color::Muted),
            )
            .child(div().flex_1())
            .child(palette_chip(
                "themes-wallpaper-dark",
                "Dark",
                palettes.dark,
                palettes.material_dark,
                name.clone(),
                cx,
            ))
            .child(palette_chip(
                "themes-wallpaper-light",
                "Light",
                palettes.light,
                palettes.material_light,
                name,
                cx,
            ))
            .into_any_element(),
        (None, _) => generate_button("Generate from the current wallpaper"),
    };

    v_flex()
        .w_full()
        .p(Spacing::Large.pixels())
        .rounded(Radius::Large.pixels())
        .bg(colors.surface_background)
        .border_1()
        .border_color(colors.border_variant)
        .gap(Spacing::Medium.pixels())
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(Spacing::Medium.pixels())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .gap(px(2.))
                        .child(
                            Label::new("Theme follows wallpaper")
                                .size(TextSize::Default)
                                .weight(FontWeight::SEMIBOLD),
                        )
                        .child(
                            Label::new("Material You colours generated from your wallpaper")
                                .size(TextSize::Small)
                                .color(Color::Muted),
                        ),
                )
                .child(follow),
        )
        .child(schemes)
        .child(generated)
        .into_any_element()
}

pub fn themes_detail(_cx: &App) -> Detail {
    Detail::new().widget(move |cx| {
        let colors = cx.theme().colors();
        let current_theme = cx.theme().clone();

        let header = v_flex()
            .w_full()
            .gap(Spacing::Small.pixels())
            .child(
                h_flex()
                    .w_full()
                    .gap(Spacing::Small.pixels())
                    .items_center()
                    .child(Icon::new(IconName::Palette).size(IconSize::Medium))
                    .child(
                        Label::new("Themes & Appearance")
                            .size(TextSize::Medium)
                            .weight(FontWeight::BOLD),
                    )
                    .child(
                        div()
                            .px(Spacing::Small.pixels())
                            .py(px(2.))
                            .rounded(Radius::Small.pixels())
                            .bg(colors.element_background)
                            .child(
                                Label::new("theme.kdl")
                                    .size(TextSize::XSmall)
                                    .color(Color::Muted),
                            ),
                    ),
            )
            .child(
                Label::new("Desktop themes, color palettes, corner radius and opacity")
                    .size(TextSize::Small)
                    .color(Color::Muted),
            );

        // Active palette strip
        let palette = current_theme.palette;
        let swatch_colors = [
            palette.base00,
            palette.base01,
            palette.base02,
            palette.base03,
            palette.base04,
            palette.base05,
            palette.base06,
            palette.base07,
            palette.base08,
            palette.base09,
            palette.base0a,
            palette.base0b,
            palette.base0c,
            palette.base0d,
            palette.base0e,
            palette.base0f,
        ];
        let swatches =
            h_flex()
                .w_full()
                .gap(px(4.))
                .children(swatch_colors.into_iter().enumerate().map(|(ix, col)| {
                    div()
                        .id(ElementId::NamedInteger("swatch".into(), ix as u64))
                        .flex_1()
                        .h(px(18.))
                        .rounded(Radius::Small.pixels())
                        .bg(col)
                        .border_1()
                        .border_color(colors.border)
                }));

        let active_theme_card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .rounded(Radius::Large.pixels())
            .bg(colors.surface_background)
            .border_1()
            .border_color(colors.border_variant)
            .gap(Spacing::Medium.pixels())
            .child(
                h_flex().w_full().items_center().justify_between().child(
                    v_flex()
                        .gap(px(2.))
                        .child(
                            Label::new("Current Palette")
                                .size(TextSize::Default)
                                .weight(FontWeight::SEMIBOLD),
                        )
                        .child(
                            Label::new("Base16 color palette across all shell elements")
                                .size(TextSize::Small)
                                .color(Color::Muted),
                        ),
                ),
            )
            .child(swatches);

        // Appearance controls
        let radius_presets: [(&'static str, f32); 5] = [
            ("Square", 0.0),
            ("Soft", 0.5),
            ("Default", 1.0),
            ("Round", 1.5),
            ("Pill", 2.0),
        ];
        let current_radius = current_theme.radius_scale;
        let radius_row =
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .child(
                    v_flex()
                        .gap(px(2.))
                        .child(
                            Label::new("Corner radius")
                                .size(TextSize::Small)
                                .weight(FontWeight::MEDIUM),
                        )
                        .child(
                            Label::new("Radius scale for cards and chips")
                                .size(TextSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
                .child(
                    h_flex()
                        .gap(px(4.))
                        .p(px(3.))
                        .rounded(Radius::Medium.pixels())
                        .bg(colors.element_background)
                        .children(radius_presets.into_iter().enumerate().map(
                            |(ix, (lbl, val))| {
                                let is_sel = (current_radius - val).abs() < 0.15;
                                div()
                                    .id(ElementId::NamedInteger("radius-btn".into(), ix as u64))
                                    .px(Spacing::Medium.pixels())
                                    .py(px(4.))
                                    .rounded(Radius::Small.pixels())
                                    .text_size(TextSize::Small.rems())
                                    .cursor_pointer()
                                    .when(is_sel, |this| {
                                        this.bg(colors.element_selected)
                                            .text_color(colors.text_accent)
                                            .font_weight(FontWeight::SEMIBOLD)
                                    })
                                    .when(!is_sel, |this| {
                                        this.text_color(colors.text_muted).hover(|s| {
                                            s.bg(colors.element_hover).text_color(colors.text)
                                        })
                                    })
                                    .on_click(move |_, _, cx| {
                                        let mut t = cx.theme().clone();
                                        t.set_radius_scale(val);
                                        apply_theme(t, cx);
                                    })
                                    .child(lbl)
                            },
                        )),
                );

        let opacity_presets: [(&'static str, f32); 5] = [
            ("50%", 0.5),
            ("70%", 0.7),
            ("80%", 0.8),
            ("90%", 0.9),
            ("100%", 1.0),
        ];
        let current_opacity = current_theme.opacity;
        let opacity_row =
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .child(
                    v_flex()
                        .gap(px(2.))
                        .child(
                            Label::new("Surface opacity")
                                .size(TextSize::Small)
                                .weight(FontWeight::MEDIUM),
                        )
                        .child(
                            Label::new("Base opacity for shell surfaces")
                                .size(TextSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
                .child(
                    h_flex()
                        .gap(px(4.))
                        .p(px(3.))
                        .rounded(Radius::Medium.pixels())
                        .bg(colors.element_background)
                        .children(opacity_presets.into_iter().enumerate().map(
                            |(ix, (lbl, val))| {
                                let is_sel = (current_opacity - val).abs() < 0.05;
                                div()
                                    .id(ElementId::NamedInteger("opacity-btn".into(), ix as u64))
                                    .px(Spacing::Medium.pixels())
                                    .py(px(4.))
                                    .rounded(Radius::Small.pixels())
                                    .text_size(TextSize::Small.rems())
                                    .cursor_pointer()
                                    .when(is_sel, |this| {
                                        this.bg(colors.element_selected)
                                            .text_color(colors.text_accent)
                                            .font_weight(FontWeight::SEMIBOLD)
                                    })
                                    .when(!is_sel, |this| {
                                        this.text_color(colors.text_muted).hover(|s| {
                                            s.bg(colors.element_hover).text_color(colors.text)
                                        })
                                    })
                                    .on_click(move |_, _, cx| {
                                        let mut t = cx.theme().clone();
                                        t.set_opacity(val);
                                        apply_theme(t, cx);
                                    })
                                    .child(lbl)
                            },
                        )),
                );

        let appearance_card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .rounded(Radius::Large.pixels())
            .bg(colors.surface_background)
            .border_1()
            .border_color(colors.border_variant)
            .gap(Spacing::Large.pixels())
            .child(radius_row)
            .child(div().w_full().h(px(1.)).bg(colors.border_variant))
            .child(opacity_row);

        // Schemes list
        let schemes = builtin_schemes();
        let schemes_list =
            v_flex()
                .w_full()
                .gap(Spacing::Small.pixels())
                .children(schemes.into_iter().map(|scheme| {
                    let is_active = palette_is_active(&scheme.palette, &current_theme);
                    let p = scheme.palette;
                    let preview_colors = scheme.preview_colors();

                    let name = scheme.name.clone();
                    v_flex()
                        .id(SharedString::from(format!("scheme-{}", scheme.name)))
                        .w_full()
                        .p(Spacing::Medium.pixels())
                        .rounded(Radius::Medium.pixels())
                        .bg(colors.surface_background)
                        .border_1()
                        .border_color(if is_active {
                            colors.accent
                        } else {
                            colors.border_variant
                        })
                        .cursor_pointer()
                        .hover(|s| s.bg(colors.element_hover))
                        .on_click(move |_, _, cx| {
                            apply_palette(name.clone(), p, cx);
                        })
                        .gap(Spacing::Small.pixels())
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .child(
                                    h_flex()
                                        .gap(Spacing::Small.pixels())
                                        .items_center()
                                        .child(
                                            Label::new(scheme.name.clone())
                                                .size(TextSize::Small)
                                                .weight(FontWeight::SEMIBOLD)
                                                .color(if is_active {
                                                    Color::Accent
                                                } else {
                                                    Color::Default
                                                }),
                                        )
                                        .child(
                                            Label::new(scheme.description.clone())
                                                .size(TextSize::XSmall)
                                                .color(Color::Muted),
                                        ),
                                )
                                .child(if is_active {
                                    Icon::new(IconName::Check)
                                        .size(IconSize::Small)
                                        .color(Color::Accent)
                                        .into_any_element()
                                } else {
                                    Label::new("Use")
                                        .size(TextSize::XSmall)
                                        .color(Color::Muted)
                                        .into_any_element()
                                }),
                        )
                        .child(h_flex().w_full().h(px(12.)).gap(px(2.)).children(
                            preview_colors.into_iter().map(|col| {
                                div()
                                    .flex_1()
                                    .h_full()
                                    .rounded(Radius::Small.pixels())
                                    .bg(col)
                            }),
                        ))
                }));

        v_flex()
            .w_full()
            .gap(Spacing::Large.pixels())
            .child(header)
            .child(div().w_full().h(px(1.)).bg(colors.border_variant))
            .child(active_theme_card)
            .child(wallpaper_card(cx))
            .child(appearance_card)
            .child(
                v_flex()
                    .w_full()
                    .gap(Spacing::Small.pixels())
                    .child(
                        Label::new(SharedString::from("SCHEMES"))
                            .size(TextSize::XSmall)
                            .color(Color::Disabled),
                    )
                    .child(schemes_list),
            )
            .into_any_element()
    })
}
