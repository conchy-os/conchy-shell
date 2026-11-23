//! The page of the config module: the sections in a list on the left, the
//! selected one's form on the right. Enter (or →) on a section moves the
//! keyboard into its form; Esc brings it back to the list. The system
//! sections (network, sound, displays…) come from [`System`], the shell's own
//! from `shell.kdl`.

use gpui::{App, Context, FontWeight, div, prelude::*, px};
use ui::{
    ActiveTheme, Button, ButtonCommon, ButtonStyle, Clickable, Color, IconName, Label,
    LabelCommon, Radius, Spacing, TextSize, h_flex, v_flex,
};

use super::form::{add_field, add_groups, reset_action};
use super::schema::{ConfigPath, Editable, Editor, Field, Group};
use super::system::System;
use super::{shortcuts_detail, shortcuts_page, themes_detail};
use crate::config::{ActiveConfig, Config, ConfigSignals};
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Detail, Item, List, Section};
use crate::launcher::page::LauncherPage;
use crate::state::watch;

pub struct ConfigPage {
    path: ConfigPath<Config>,
    groups: Vec<Group<Config>>,
    system: System,
}

impl ConfigPage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        watch_revision(cx);
        Self {
            path: ConfigPath::root(),
            groups: Config::groups(),
            system: System::new(cx),
        }
    }
}

/// Redraw whenever any edit, save or reload lands.
pub fn watch_revision<V: 'static>(cx: &mut Context<V>) {
    watch(
        cx,
        ConfigSignals::global(cx).revision.signal(),
        |_, _, cx| cx.notify(),
    );
}

impl LauncherPage for ConfigPage {
    /// Two groups: how things look (the appearance settings, themes and the
    /// shell's own sections) and the system (hardware, session, shortcuts).
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let mut appearance = self.system.appearance_items(cx);
        appearance.push(themes_item(cx));
        let mut system = self.system.system_items(cx);
        system.push(shortcuts_item(cx));

        for group in &self.groups {
            let items = group
                .fields
                .iter()
                .filter(|field| !field.editor.is_empty())
                .map(|field| section_item(&self.path, field, cx));
            if group.title == "Desktop Shell" {
                appearance.extend(items);
            } else {
                system.extend(items);
            }
        }
        system.push(self.system.about_item());

        List::new()
            .show_detail(true)
            .click_selects(true)
            .title(Config::title())
            .menu(false)
            .placeholder("Search settings…")
            .section(Section::new("Appearance").items(appearance))
            .section(Section::new("System").items(system))
            .into()
    }
}

fn shortcuts_item(cx: &App) -> Item {
    Item::new("shortcuts", "Shortcuts")
        .subtitle("Keyboard shortcuts")
        .icon(IconName::Keyboard)
        .keywords("shortcuts keybinds hotkeys keys super bindings")
        .action(
            Action::new("Open shortcuts", |_, cx| {
                Outcome::Push(shortcuts_page::ShortcutsPage::open(cx))
            })
            .icon(IconName::ChevronRight),
        )
        .detail(shortcuts_detail::shortcuts_detail(cx))
}

fn themes_item(cx: &App) -> Item {
    let prefix = cx.config().launcher.modules.themes.prefix.clone();
    Item::new("themes", "Themes")
        .subtitle("Desktop themes and colors")
        .icon(IconName::Palette)
        .keywords("theme colors palette")
        .action(
            Action::new("Open themes", move |_, _| Outcome::SetQuery(prefix.clone()))
                .icon(IconName::ChevronRight),
        )
        .detail(themes_detail::themes_detail(cx))
}

/// The list row of a section; its form is the detail pane.
fn section_item(path: &ConfigPath<Config>, field: &Field<Config>, cx: &App) -> Item {
    let config = path.read(cx);
    let key_path = path.key_string(field.key);
    let modified = field.is_modified(config);
    let summary = field.display(config);

    let mut item = Item::new(key_path.clone(), field.label)
        .icon(field.icon.unwrap_or_else(|| field.editor.icon()))
        .keywords(key_path);
    if modified {
        item = item.accessory(Accessory::tag("modified").color(Color::Accent));
    }
    if !field.description.is_empty() {
        item = item.subtitle(field.description);
    } else if !summary.is_empty() {
        item = item.subtitle(summary);
    }

    let panel = ActionPanel::new()
        .action(
            Action::new("Edit", |_, _| Outcome::FocusDetail).icon(IconName::ChevronRight),
        )
        .action(reset_action(path, field, modified));
    item.actions(panel).detail(section_form(path, field))
}

/// The detail pane: a small header (label, key, description), then the
/// section's fields.
fn section_form(path: &ConfigPath<Config>, field: &Field<Config>) -> Detail {
    let path = path.clone();
    let field = field.clone();
    Detail::new().form(move |form, cx| {
        form.row(form_header(&path, &field, cx));
        match &field.editor {
            Editor::Nested { inline, .. } => inline(form, &path, 0, cx),
            Editor::Bundle(fields) => {
                add_groups(form, &path, &[Group::new("", fields.to_vec())], 0, cx);
            }
            _ => add_field(form, &path, &field, 0, cx),
        }
    })
}

fn form_header(path: &ConfigPath<Config>, field: &Field<Config>, cx: &App) -> gpui::AnyElement {
    let colors = cx.theme().colors();
    let modified = field.is_modified(path.read(cx));
    let mut header = v_flex()
        .w_full()
        .pb(Spacing::Medium.pixels())
        .gap(Spacing::Small.pixels())
        .child(
            h_flex()
                .w_full()
                .gap(Spacing::Medium.pixels())
                .items_center()
                .justify_between()
                .child(
                    h_flex()
                        .gap(Spacing::Small.pixels())
                        .items_center()
                        .child(
                            Label::new(field.label)
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
                                    Label::new(path.key_string(field.key))
                                        .size(TextSize::XSmall)
                                        .color(Color::Muted),
                                ),
                        ),
                )
                .when(modified, |el| {
                    let (path, field) = (path.clone(), field.clone());
                    el.child(
                        Button::new("reset-section", "Reset to defaults")
                            .style(ButtonStyle::Subtle)
                            .on_click(move |_, _, cx| path.reset(&field, cx)),
                    )
                }),
        );
    if !field.description.is_empty() {
        header = header.child(
            Label::new(field.description)
                .size(TextSize::Small)
                .color(Color::Muted),
        );
    }
    header
        .child(div().w_full().h(px(1.)).bg(colors.border_variant))
        .into_any_element()
}
