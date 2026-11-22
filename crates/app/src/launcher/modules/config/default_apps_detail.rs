//! The preview beside the "Default applications" row: what each everyday role
//! opens with right now.

use gpui::{App, FontWeight, div, prelude::*, px};
use services::mimeapps::{Layers, Source, Store, find_desktop_file};
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, TextSize,
    h_flex, v_flex,
};

use super::default_apps_page::{Choice, ROLES, role_choice, tilde};
use crate::launcher::content::Detail;
use crate::state::AppState;

pub fn default_apps_detail(_cx: &App) -> Detail {
    Detail::new().widget(move |cx| {
        let colors = cx.theme().colors();
        let store = Store::standard().ok();
        let layers = store.as_ref().and_then(|store| Layers::load(store).ok());

        let header = v_flex()
            .w_full()
            .gap(Spacing::Small.pixels())
            .child(
                h_flex()
                    .w_full()
                    .gap(Spacing::Small.pixels())
                    .items_center()
                    .child(Icon::new(IconName::ExternalLink).size(IconSize::Medium))
                    .child(
                        Label::new("Default applications")
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
                                Label::new("mimeapps.list")
                                    .size(TextSize::XSmall)
                                    .color(Color::Muted),
                            ),
                    ),
            )
            .child(
                Label::new("Which program opens links, folders, images and other files")
                    .size(TextSize::Small)
                    .color(Color::Muted),
            );

        let Some(layers) = layers else {
            return v_flex()
                .w_full()
                .gap(Spacing::Large.pixels())
                .child(header)
                .child(
                    Label::new("mimeapps.list could not be read")
                        .size(TextSize::Small)
                        .color(Color::Warning),
                )
                .into_any_element();
        };

        // Only the files are asked here: scanning every desktop entry on each
        // redraw would be too much. A type with no default shows as automatic.
        let apps = AppState::applications(cx).all();
        let name_of = |id: &str| -> String {
            find_desktop_file(id)
                .and_then(|path| apps.iter().find(|app| app.desktop_file == path))
                .map_or_else(
                    || id.trim_end_matches(".desktop").to_string(),
                    |a| a.name.clone(),
                )
        };
        let installed = |id: &str| find_desktop_file(id).is_some();

        let rows = ROLES.iter().map(|role| {
            let choice = role_choice(role, |mime| {
                layers.configured(mime, installed).map(|(id, _)| id)
            });
            let overridden = role.mimes.iter().any(|mime| {
                matches!(
                    layers.configured(mime, installed),
                    Some((_, Source::Override(_)))
                )
            });
            let (text, color) = match &choice {
                Choice::One(id) => (name_of(id), Color::Default),
                Choice::Mixed(ids) => (
                    format!(
                        "Mixed: {}",
                        ids.iter()
                            .map(|id| name_of(id))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    Color::Warning,
                ),
                Choice::Unset => ("Automatic".to_string(), Color::Muted),
            };
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(Spacing::Medium.pixels())
                .p(Spacing::Medium.pixels())
                .rounded(Radius::Medium.pixels())
                .bg(colors.surface_background)
                .border_1()
                .border_color(colors.border_variant)
                .child(
                    h_flex()
                        .gap(Spacing::Medium.pixels())
                        .items_center()
                        .child(
                            Icon::new(role.icon)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                        .child(
                            Label::new(role.name)
                                .size(TextSize::Default)
                                .weight(FontWeight::SEMIBOLD),
                        ),
                )
                .child(
                    Label::new(if overridden {
                        format!("{text} (overridden)")
                    } else {
                        text
                    })
                    .size(TextSize::Small)
                    .color(color),
                )
        });

        let path = store
            .as_ref()
            .map(|store| tilde(store.path()))
            .unwrap_or_default();
        v_flex()
            .w_full()
            .gap(Spacing::Large.pixels())
            .child(header)
            .child(div().w_full().h(px(1.)).bg(colors.border_variant))
            .child(
                v_flex()
                    .w_full()
                    .gap(Spacing::Small.pixels())
                    .child(
                        Label::new("OPENS WITH")
                            .size(TextSize::XSmall)
                            .color(Color::Disabled),
                    )
                    .children(rows),
            )
            .child(
                Label::new(path)
                    .size(TextSize::XSmall)
                    .color(Color::Disabled),
            )
            .into_any_element()
    })
}
