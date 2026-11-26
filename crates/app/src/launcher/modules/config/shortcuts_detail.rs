use gpui::{AnyElement, App, FontWeight, SharedString, div, prelude::*, px};
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, Switch,
    TextSize, h_flex, v_flex,
};

use super::shortcuts_page::describe;
use crate::config::ConfigSignals;
use crate::launcher::content::Detail;
use crate::shortcuts::{self, Bind, LAUNCHER_ACTION, Store};

pub fn shortcuts_detail(_cx: &App) -> Detail {
    Detail::new().widget(move |cx| {
        let colors = cx.theme().colors();
        let store = Store::standard().ok();
        let shortcuts = store
            .as_ref()
            .and_then(|s| s.load().ok())
            .unwrap_or_default();
        let tap = shortcuts.tap();
        let is_super_launcher = tap
            .as_ref()
            .is_some_and(|b| b.action.trim() == LAUNCHER_ACTION);

        let header = v_flex()
            .w_full()
            .gap(Spacing::Small.pixels())
            .child(
                h_flex()
                    .w_full()
                    .gap(Spacing::Small.pixels())
                    .items_center()
                    .child(Icon::new(IconName::Keyboard).size(IconSize::Medium))
                    .child(
                        Label::new("Shortcuts")
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
                                Label::new("shortcuts.kdl")
                                    .size(TextSize::XSmall)
                                    .color(Color::Muted),
                            ),
                    ),
            )
            .child(
                Label::new("Super key behavior and window manager keybindings")
                    .size(TextSize::Small)
                    .color(Color::Muted),
            );

        let super_card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .rounded(Radius::Large.pixels())
            .bg(colors.surface_background)
            .border_1()
            .border_color(colors.border_variant)
            .gap(Spacing::Small.pixels())
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        v_flex()
                            .gap(px(2.))
                            .child(
                                Label::new("Super key tap")
                                    .size(TextSize::Default)
                                    .weight(FontWeight::SEMIBOLD),
                            )
                            .child(
                                Label::new(
                                    "Tap Super (Windows key) on its own to open the launcher",
                                )
                                .size(TextSize::Small)
                                .color(Color::Muted),
                            ),
                    )
                    .child(Switch::new("super-tap-toggle", is_super_launcher).on_click(
                        move |_, _, cx| {
                            if let Ok(store) = Store::standard()
                                && let Ok(mut sc) = store.load()
                            {
                                if is_super_launcher {
                                    sc.remove(shortcuts::SUPER_KEY);
                                } else {
                                    sc.put(Bind::launcher_tap(true));
                                }
                                if store.save(&sc).is_ok() {
                                    ConfigSignals::publish(cx);
                                }
                            }
                        },
                    )),
            );

        let non_tap_binds: Vec<&Bind> = shortcuts
            .binds
            .iter()
            .filter(|b| !(b.is_tap() && b.key.eq_ignore_ascii_case(shortcuts::SUPER_KEY)))
            .collect();

        let binds_list =
            v_flex()
                .w_full()
                .gap(Spacing::Small.pixels())
                .children(non_tap_binds.into_iter().map(|bind| {
                    let key_parts = bind.key.split('+');
                    let keycaps = h_flex().gap(px(4.)).items_center().children(
                        key_parts.enumerate().flat_map(|(ix, part)| {
                            let mut elems: Vec<AnyElement> = Vec::new();
                            if ix > 0 {
                                elems.push(
                                    Label::new("+")
                                        .size(TextSize::XSmall)
                                        .color(Color::Muted)
                                        .into_any_element(),
                                );
                            }
                            elems.push(
                                div()
                                    .px(Spacing::Small.pixels())
                                    .py(px(2.))
                                    .rounded(Radius::Small.pixels())
                                    .bg(colors.element_background)
                                    .border_1()
                                    .border_color(colors.border)
                                    .child(
                                        Label::new(SharedString::from(part.to_string()))
                                            .size(TextSize::XSmall)
                                            .weight(FontWeight::SEMIBOLD),
                                    )
                                    .into_any_element(),
                            );
                            elems
                        }),
                    );

                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .p(Spacing::Medium.pixels())
                        .rounded(Radius::Medium.pixels())
                        .bg(colors.surface_background)
                        .border_1()
                        .border_color(colors.border_variant)
                        .child(
                            h_flex()
                                .gap(Spacing::Medium.pixels())
                                .items_center()
                                .child(keycaps)
                                .child(
                                    Label::new(describe(&bind.action))
                                        .size(TextSize::Small)
                                        .color(Color::Default),
                                ),
                        )
                }));

        v_flex()
            .w_full()
            .gap(Spacing::Large.pixels())
            .child(header)
            .child(div().w_full().h(px(1.)).bg(colors.border_variant))
            .child(super_card)
            .child(
                v_flex()
                    .w_full()
                    .gap(Spacing::Small.pixels())
                    .child(
                        Label::new(SharedString::from("CONFIGURED SHORTCUTS"))
                            .size(TextSize::XSmall)
                            .color(Color::Disabled),
                    )
                    .child(binds_list),
            )
            .into_any_element()
    })
}
