//! Appearance: fonts, text size and DPI, cursor theme and size, icon theme.

use gpui::{App, AppContext, Context, Entity, SharedString};
use services::appearance::{Change, Font, FontRole};
use ui::{Color, IconName};

use super::appearance_model::{AppearanceModel, Loaded, State};
use super::settings_form::{self as rows, Number};
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Detail, EmptyView, Form, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};

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

/// The row of fonts, text size, cursor and icons, with their form beside it.
pub fn item(model: &Entity<AppearanceModel>, cx: &App) -> Item {
    let subtitle = match &model.read(cx).state {
        State::Loading => "Reading the current settings…".to_string(),
        State::Failed(_) => "Could not read the appearance settings".to_string(),
        State::Ready(loaded) => format!(
            "{} {} pt • {}× text • {}",
            loaded.settings.interface_font.family,
            trim(loaded.settings.interface_font.size),
            trim(loaded.settings.text_scale),
            loaded.settings.icon_theme
        ),
    };
    let model = model.clone();
    Item::new("appearance", "Appearance")
        .icon(IconName::Palette)
        .subtitle(subtitle)
        .keywords("appearance fonts font typeface text scale scaling dpi size zoom cursor pointer theme icons icon papirus adwaita")
        .actions(super::system::edit())
        .detail(Detail::new().form(move |form, cx| appearance_form(form, &model, cx)))
}

fn appearance_form(form: &mut Form, model: &Entity<AppearanceModel>, cx: &App) {
    let m = model.read(cx);
    match &m.state {
        State::Loading => {
            rows::title(form, "Appearance", "");
            rows::note(form, "Reading the current settings…", Color::Muted);
            return;
        }
        State::Failed(err) => {
            rows::title(form, "Appearance", "");
            rows::note(
                form,
                format!("Could not read the settings: {err}"),
                Color::Error,
            );
            return;
        }
        State::Ready(_) => {}
    }
    for role in [FontRole::Interface, FontRole::Document, FontRole::Monospace] {
        font_form(form, model, role, cx);
    }
    text_form(form, model, cx);
    cursor_form(form, model, cx);
    icons_form(form, model, cx);
    footer(form, m);
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
    rows::info(form, "Family", font.family.clone());
    rows::action(
        form,
        "Choose another family…",
        "Pick from the fonts installed on this computer",
        {
            let model = model.clone();
            move |_, cx| Outcome::Push(FontPicker::open(model.clone(), role, cx))
        },
        cx,
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
            themes
                .iter()
                .map(|t| SharedString::from(t.clone()))
                .collect(),
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
            themes
                .iter()
                .map(|t| SharedString::from(t.clone()))
                .collect(),
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
            item.actions(
                ActionPanel::new().action(Action::new("Use this font", move |_, cx| {
                    let font = Font::new(family.clone(), size);
                    model.update(cx, |m, cx| m.change(Change::Font(role, font), cx));
                    Outcome::Pop
                })),
            )
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
