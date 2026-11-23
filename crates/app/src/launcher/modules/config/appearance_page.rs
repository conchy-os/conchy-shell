//! Appearance: fonts, text size and DPI, cursor theme and size, icon theme.

use gpui::{App, AppContext, Context, Entity, SharedString};
use services::appearance::{Change, Font, FontRole};
use ui::{Color, IconName};

use super::appearance_model::{AppearanceModel, Loaded, State};
use super::settings_form::{self as rows, Number};
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Detail, EmptyView, Form, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};

pub struct AppearancePage {
    model: Entity<AppearanceModel>,
}

impl AppearancePage {
    pub fn open(cx: &mut App) -> Page {
        Page::new(cx.new(Self::new))
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        let model = cx.new(AppearanceModel::new);
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        Self { model }
    }
}

impl FontRoleExt for FontRole {
    fn name(self) -> &'static str {
        match self {
            FontRole::Interface => "Interface font",
            FontRole::Document => "Document font",
            FontRole::Monospace => "Monospace font",
        }
    }

    fn about(self) -> &'static str {
        match self {
            FontRole::Interface => "Menus, buttons and window titles in GTK programs",
            FontRole::Document => "Text of documents in programs that ask for a document font",
            FontRole::Monospace => "Terminals, code and anything that must line up",
        }
    }

    fn id(self) -> &'static str {
        match self {
            FontRole::Interface => "interface",
            FontRole::Document => "document",
            FontRole::Monospace => "monospace",
        }
    }
}

trait FontRoleExt {
    fn name(self) -> &'static str;
    fn about(self) -> &'static str;
    fn id(self) -> &'static str;
}

fn font_of(loaded: &Loaded, role: FontRole) -> &Font {
    match role {
        FontRole::Interface => &loaded.settings.interface_font,
        FontRole::Document => &loaded.settings.document_font,
        FontRole::Monospace => &loaded.settings.monospace_font,
    }
}

impl LauncherPage for AppearancePage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let model = self.model.clone();
        let list = List::new()
            .show_detail(true)
            .click_selects(true)
            .title("Appearance")
            .placeholder("Search appearance settings…");

        let m = model.read(cx);
        let loaded = match &m.state {
            State::Loading => return list.loading(true).into(),
            State::Failed(err) => {
                return list
                    .empty(
                        EmptyView::new("Could not read the appearance settings")
                            .description(err.clone())
                            .icon(IconName::Palette),
                    )
                    .into();
            }
            State::Ready(loaded) => loaded,
        };

        let edit = || {
            ActionPanel::new()
                .action(Action::new("Edit", |_, _| Outcome::FocusDetail).icon(IconName::ChevronRight))
        };

        let font_items: Vec<Item> = [FontRole::Interface, FontRole::Document, FontRole::Monospace]
            .into_iter()
            .map(|role| {
                let font = font_of(loaded, role).clone();
                let (pick_model, size_model) = (model.clone(), model.clone());
                let detail_model = model.clone();
                Item::new(format!("font-{}", role.id()), role.name())
                    .icon(IconName::FileText)
                    .subtitle(font.description())
                    .keywords(format!("font typeface text {} {}", role.name(), font.family))
                    .accessory(Accessory::text(format!("{} pt", trim(font.size))))
                    .actions(
                        ActionPanel::new()
                            .action(
                                Action::new("Choose font…", move |_, cx| {
                                    Outcome::Push(FontPicker::open(pick_model.clone(), role, cx))
                                })
                                .icon(IconName::ChevronRight),
                            )
                            .action(
                                Action::new("Change size", move |_, _| {
                                    let _ = &size_model;
                                    Outcome::FocusDetail
                                })
                                .icon(IconName::Pencil),
                            ),
                    )
                    .detail(Detail::new().form(move |form, cx| {
                        font_form(form, &detail_model, role, cx)
                    }))
            })
            .collect();

        let text = {
            let detail_model = model.clone();
            Item::new("text", "Text size & DPI")
                .icon(IconName::Sliders)
                .subtitle(format!(
                    "{}× text, {} dpi",
                    trim(loaded.settings.text_scale),
                    loaded.settings.dpi
                ))
                .keywords("text scale scaling dpi font size zoom bigger smaller resolution")
                .actions(edit())
                .detail(Detail::new().form(move |form, cx| text_form(form, &detail_model, cx)))
        };

        let cursor = {
            let detail_model = model.clone();
            Item::new("cursor", "Cursor")
                .icon(IconName::Mouse)
                .subtitle(format!(
                    "{}, {} px",
                    loaded.settings.cursor_theme, loaded.settings.cursor_size
                ))
                .keywords("cursor pointer theme size mouse")
                .actions(edit())
                .detail(Detail::new().form(move |form, cx| cursor_form(form, &detail_model, cx)))
        };

        let icons = {
            let detail_model = model.clone();
            Item::new("icons", "Icons")
                .icon(IconName::Palette)
                .subtitle(loaded.settings.icon_theme.clone())
                .keywords("icons icon theme papirus adwaita")
                .actions(edit())
                .detail(Detail::new().form(move |form, cx| icons_form(form, &detail_model, cx)))
        };

        list.section(Section::new("Fonts").items(font_items))
            .section(Section::new("Display").item(text))
            .section(Section::new("Pointer & icons").items([cursor, icons]))
            .into()
    }
}

fn trim(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn footer(form: &mut Form, m: &AppearanceModel) {
    if let Some(err) = &m.notice {
        rows::note(form, format!("Could not apply: {err}"), Color::Error);
    }
}

fn font_form(form: &mut Form, model: &Entity<AppearanceModel>, role: FontRole, cx: &App) {
    let m = model.read(cx);
    rows::title(form, role.name(), role.about());
    let Some(loaded) = m.loaded() else { return };
    let font = font_of(loaded, role).clone();

    rows::heading(form, "Font");
    rows::note(
        form,
        format!("{} — press Enter on the list item to choose another family.", font.family),
        Color::Muted,
    );
    rows::number(
        form,
        Number {
            id: format!("font-size-{}", role.id()).into(),
            label: "Size",
            description: "In points",
            value: font.size,
            min: 6.0,
            max: 36.0,
            step: 0.5,
            unit: Some("pt"),
            default: 11.0,
        },
        {
            let (model, family) = (model.clone(), font.family.clone());
            move |size, cx| {
                let font = Font::new(family.clone(), size);
                model.update(cx, |m, cx| m.change(Change::Font(role, font), cx));
            }
        },
        cx,
    );
    footer(form, m);
}

fn text_form(form: &mut Form, model: &Entity<AppearanceModel>, cx: &App) {
    let m = model.read(cx);
    rows::title(
        form,
        "Text size & DPI",
        "Make text bigger or smaller everywhere without touching the screen's scale",
    );
    let Some(loaded) = m.loaded() else { return };
    let s = &loaded.settings;

    rows::heading(form, "Text");
    rows::number(
        form,
        Number {
            id: "text-scale".into(),
            label: "Text scaling",
            description: "Multiplies every font's size in GTK 4 and libadwaita programs",
            value: s.text_scale,
            min: 0.5,
            max: 3.0,
            step: 0.05,
            unit: Some("×"),
            default: 1.0,
        },
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::TextScale(v), cx))
        },
        cx,
    );
    rows::number(
        form,
        Number {
            id: "font-dpi".into(),
            label: "Font DPI",
            description: "Dots per inch fonts are drawn at in GTK 3 and X11 programs",
            value: f64::from(s.dpi),
            min: 48.0,
            max: 288.0,
            step: 4.0,
            unit: Some("dpi"),
            default: 96.0,
        },
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::Dpi(v as u32), cx))
        },
        cx,
    );
    rows::note(
        form,
        "Programs that are already open may need a restart to pick the change up.",
        Color::Muted,
    );
    footer(form, m);
}

fn cursor_form(form: &mut Form, model: &Entity<AppearanceModel>, cx: &App) {
    let m = model.read(cx);
    rows::title(form, "Cursor", "The mouse pointer's look and size");
    let Some(loaded) = m.loaded() else { return };
    let s = &loaded.settings;

    rows::heading(form, "Pointer");
    if loaded.cursor_themes.is_empty() {
        rows::note(form, "No cursor themes were found.", Color::Muted);
    } else {
        let themes = loaded.cursor_themes.clone();
        rows::choice(
            form,
            "cursor-theme",
            "Theme",
            "The set of pointer shapes",
            themes.iter().map(|t| SharedString::from(t.clone())).collect(),
            themes.iter().position(|t| *t == s.cursor_theme),
            {
                let model = model.clone();
                move |ix, cx| {
                    if let Some(theme) = themes.get(ix) {
                        let theme = theme.clone();
                        model.update(cx, |m, cx| m.change(Change::CursorTheme(theme), cx));
                    }
                }
            },
            cx,
        );
    }
    rows::number(
        form,
        Number {
            id: "cursor-size".into(),
            label: "Size",
            description: "In pixels; 24 is usual, 32 to 48 for high-resolution screens",
            value: f64::from(s.cursor_size),
            min: 16.0,
            max: 96.0,
            step: 4.0,
            unit: Some("px"),
            default: 24.0,
        },
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::CursorSize(v as u32), cx))
        },
        cx,
    );
    rows::note(
        form,
        "The compositor and new programs use it at once; programs already open keep the old one.",
        Color::Muted,
    );
    footer(form, m);
}

fn icons_form(form: &mut Form, model: &Entity<AppearanceModel>, cx: &App) {
    let m = model.read(cx);
    rows::title(form, "Icons", "The icon set programs draw their icons from");
    let Some(loaded) = m.loaded() else { return };

    rows::heading(form, "Theme");
    if loaded.icon_themes.is_empty() {
        rows::note(form, "No icon themes were found.", Color::Muted);
    } else {
        let themes = loaded.icon_themes.clone();
        rows::choice(
            form,
            "icon-theme",
            "Icon theme",
            "Applies to GTK and Qt programs; the shell's own icons stay as they are",
            themes.iter().map(|t| SharedString::from(t.clone())).collect(),
            themes.iter().position(|t| *t == loaded.settings.icon_theme),
            {
                let model = model.clone();
                move |ix, cx| {
                    if let Some(theme) = themes.get(ix) {
                        let theme = theme.clone();
                        model.update(cx, |m, cx| m.change(Change::IconTheme(theme), cx));
                    }
                }
            },
            cx,
        );
    }
    footer(form, m);
}

// -- picking a font ------------------------------------------------------------

struct FontPicker {
    model: Entity<AppearanceModel>,
    role: FontRole,
}

impl FontPicker {
    fn open(model: Entity<AppearanceModel>, role: FontRole, cx: &mut App) -> Page {
        Page::new(cx.new(|_| Self { model, role }))
    }
}

impl LauncherPage for FontPicker {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let list = List::new()
            .title(self.role.name())
            .placeholder("Search fonts…");
        let Some(loaded) = self.model.read(cx).loaded() else {
            return list
                .empty(EmptyView::new("The fonts are not loaded yet").icon(IconName::FileText))
                .into();
        };
        let current = font_of(loaded, self.role).clone();
        let fonts = loaded.fonts.clone();
        if fonts.all.is_empty() {
            return list
                .empty(
                    EmptyView::new("No fonts found")
                        .description("`fc-list` listed nothing; is fontconfig installed?")
                        .icon(IconName::FileText),
                )
                .into();
        }

        let (model, role) = (self.model.clone(), self.role);
        let item_of = move |family: &String| {
            let (model, family) = (model.clone(), family.clone());
            let is_current = family == current.family;
            let mut item = Item::new(format!("font:{family}"), family.clone())
                .icon(IconName::FileText)
                .keywords(family.clone());
            if is_current {
                item = item.accessory(Accessory::tag("current").color(Color::Accent));
            }
            let size = current.size;
            item.actions(ActionPanel::new().action(Action::new("Use this font", move |_, cx| {
                let font = Font::new(family.clone(), size);
                model.update(cx, |m, cx| m.change(Change::Font(role, font), cx));
                Outcome::Pop
            })))
        };

        if role == FontRole::Monospace && !fonts.monospace.is_empty() {
            let rest: Vec<&String> = fonts
                .all
                .iter()
                .filter(|f| !fonts.monospace.contains(f))
                .collect();
            list.section(Section::new("Monospace").items(fonts.monospace.iter().map(&item_of)))
                .section(Section::new("Other fonts").items(rest.into_iter().map(&item_of)))
                .into()
        } else {
            list.section(Section::new("Fonts").items(fonts.all.iter().map(&item_of)))
                .into()
        }
    }
}
