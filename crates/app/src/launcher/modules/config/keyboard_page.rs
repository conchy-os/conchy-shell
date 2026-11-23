//! Keyboard: layouts and the key that switches them, repeat delay and rate.

use std::rc::Rc;

use gpui::{App, AppContext, Context, Entity, SharedString, div, prelude::*, px};
use services::input::{Change, InputBackend};
use services::xkb::{LayoutEntry, XkbCatalog, XkbSet};
use ui::{
    Color, IconButton, IconName, IconSize, Label, LabelCommon, Spacing, TextSize, h_flex,
    ButtonCommon, Clickable, Tooltip,
};

use super::form::{control_frame, labeled_row};
use super::input_model::{InputModel, State};
use super::select::select;
use super::settings_form::{self as rows, Number};
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Control, Detail, EmptyView, Form, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};

/// The row of the layouts and of typing, with their form beside it.
pub fn item(model: &Entity<InputModel>, cx: &App) -> Item {
    let summary = model.read(cx).loaded().map(|l| {
        let names: Vec<String> = l
            .settings
            .keyboard
            .xkb
            .layouts
            .iter()
            .map(|e| e.layout.to_uppercase())
            .collect();
        (names.join(" · "), l.settings.keyboard.repeat_delay, l.settings.keyboard.repeat_rate)
    });
    Item::new("keyboard", "Keyboard")
        .icon(IconName::Keyboard)
        .subtitle(summary.map_or_else(
            || "Layouts, switching keys, repeat delay and rate".to_string(),
            |(names, delay, rate)| format!("{names} • repeat after {delay} ms, {rate} per second"),
        ))
        .keywords("keyboard layout language switch xkb russian english caps lock alt shift repeat delay rate speed hold key typing")
        .actions(super::system::edit().action(add_layout_action(model)))
        .detail(Detail::new().form({
            let model = model.clone();
            move |form, cx| keyboard_form(form, &model, cx)
        }))
}

fn keyboard_form(form: &mut Form, model: &Entity<InputModel>, cx: &App) {
    layouts_form(form, model, cx);
    let m = model.read(cx);
    if m.backend != InputBackend::Unsupported && matches!(m.state, State::Ready(_)) {
        typing_form(form, model, cx);
    }
}

fn add_layout_action(model: &Entity<InputModel>) -> Action {
    let model = model.clone();
    Action::new("Add layout…", move |_, cx| {
        Outcome::Push(LayoutPicker::open(model.clone(), cx))
    })
    .icon(IconName::Plus)
    .shortcut("ctrl-n")
}

fn layouts_form(form: &mut Form, model: &Entity<InputModel>, cx: &App) {
    let m = model.read(cx);
    rows::title(
        form,
        "Layouts",
        "The first layout is the one a session starts with",
    );
    if m.backend == InputBackend::Unsupported {
        rows::note(form, m.backend.persistence_note(), Color::Warning);
        return;
    }
    match &m.state {
        State::Loading => {
            rows::note(form, "Reading the current settings…", Color::Muted);
            return;
        }
        State::Failed(err) => {
            rows::note(form, format!("Could not read the settings: {err}"), Color::Error);
            return;
        }
        State::Ready(_) => {}
    }
    let Some(loaded) = m.loaded() else { return };
    let xkb = loaded.settings.keyboard.xkb.clone();
    let catalog = loaded.catalog.clone();

    rows::heading(form, "In use");
    let count = xkb.layouts.len();
    for (index, entry) in xkb.layouts.iter().enumerate() {
        layout_row(form, model, &xkb, &catalog, index, entry, count, cx);
    }
    rows::action(
        form,
        "Add layout…",
        "Pick another language from the system's list",
        {
            let model = model.clone();
            move |_, cx| Outcome::Push(LayoutPicker::open(model.clone(), cx))
        },
        cx,
    );

    if count > 1 {
        rows::heading(form, "Switching");
        let options = catalog.switch_options();
        let mut labels: Vec<SharedString> = vec!["None".into()];
        labels.extend(options.iter().map(|o| SharedString::from(o.name.clone())));
        let selected = match xkb.switch_option() {
            None => Some(0),
            Some(current) => options.iter().position(|o| o.code == current).map(|i| i + 1),
        };
        let codes: Vec<String> = options.iter().map(|o| o.code.clone()).collect();
        rows::choice(
            form,
            "kb-switch",
            "Switch layouts with",
            "The keys that cycle through the layouts above",
            labels,
            selected,
            {
                let model = model.clone();
                let xkb = xkb.clone();
                move |ix, cx| {
                    let mut set = xkb.clone();
                    set.set_switch_option(ix.checked_sub(1).and_then(|i| codes.get(i)).map(String::as_str));
                    model.update(cx, |m, cx| m.change(Change::KeyboardLayouts(set), cx));
                }
            },
            cx,
        );
    }

    rows::heading(form, "Keys");
    let caps = CapsLock::of(&xkb);
    rows::choice(
        form,
        "kb-caps",
        "Caps Lock key",
        "What the Caps Lock key does",
        CapsLock::ALL.iter().map(|c| c.label().into()).collect(),
        CapsLock::ALL.iter().position(|c| *c == caps),
        {
            let model = model.clone();
            let xkb = xkb.clone();
            move |ix, cx| {
                let mut set = xkb.clone();
                CapsLock::ALL[ix].write(&mut set);
                model.update(cx, |m, cx| m.change(Change::KeyboardLayouts(set), cx));
            }
        },
        cx,
    );

    if let Some(err) = &m.error {
        rows::note(form, format!("Could not apply: {err}"), Color::Error);
    }
    rows::note(form, m.backend.persistence_note(), Color::Muted);
}

/// One layout of the set: its name, its variant, and buttons to move or drop it.
#[allow(clippy::too_many_arguments)]
fn layout_row(
    form: &mut Form,
    model: &Entity<InputModel>,
    xkb: &XkbSet,
    catalog: &Rc<XkbCatalog>,
    index: usize,
    entry: &LayoutEntry,
    count: usize,
    cx: &App,
) {
    let name = catalog
        .layout_name(&entry.layout)
        .map_or_else(|| entry.layout.clone(), str::to_string);
    let variants: Vec<_> = catalog.variants_of(&entry.layout).collect();

    let mut labels: Vec<SharedString> = vec!["Default".into()];
    labels.extend(variants.iter().map(|v| {
        // `us: English (US, intl.)` → the part after the layout's own name.
        SharedString::from(
            v.name
                .split_once('(')
                .map_or(v.name.as_str(), |(_, rest)| rest.trim_end_matches(')'))
                .to_string(),
        )
    }));
    let codes: Vec<String> = variants.iter().map(|v| v.code.clone()).collect();
    let selected = if entry.variant.is_empty() {
        Some(0)
    } else {
        codes.iter().position(|c| *c == entry.variant).map(|i| i + 1)
    };

    let set_variant = {
        let model = model.clone();
        let xkb = xkb.clone();
        let codes = codes.clone();
        Rc::new(move |ix: usize, cx: &mut App| {
            let mut set = xkb.clone();
            set.layouts[index].variant = ix
                .checked_sub(1)
                .and_then(|i| codes.get(i).cloned())
                .unwrap_or_default();
            model.update(cx, |m, cx| m.change(Change::KeyboardLayouts(set), cx));
        })
    };
    let move_up = {
        let model = model.clone();
        let xkb = xkb.clone();
        Rc::new(move |cx: &mut App| {
            if index == 0 {
                return;
            }
            let mut set = xkb.clone();
            set.layouts.swap(index, index - 1);
            model.update(cx, |m, cx| m.change(Change::KeyboardLayouts(set), cx));
        })
    };
    let remove = {
        let model = model.clone();
        let xkb = xkb.clone();
        Rc::new(move |cx: &mut App| {
            if xkb.layouts.len() < 2 {
                return;
            }
            let mut set = xkb.clone();
            set.layouts.remove(index);
            // One layout left has nothing to switch between.
            if set.layouts.len() < 2 {
                set.set_switch_option(None);
            }
            model.update(cx, |m, cx| m.change(Change::KeyboardLayouts(set), cx));
        })
    };

    let id = format!("kb-layout-{index}");
    let widget = if variants.is_empty() {
        Label::new(SharedString::from(entry.layout.to_uppercase()))
            .size(TextSize::Small)
            .color(Color::Muted)
            .into_any_element()
    } else {
        let set_variant = set_variant.clone();
        select(
            SharedString::from(format!("{id}-variant")),
            labels.clone(),
            selected,
            move |ix, cx| set_variant(ix, cx),
            cx,
        )
    };
    let buttons = h_flex()
        .gap(px(2.))
        .when(index > 0, |row| {
            let move_up = move_up.clone();
            row.child(
                IconButton::new(SharedString::from(format!("{id}-up")), IconName::ArrowUp)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Move up"))
                    .on_click(move |_, _, cx| move_up(cx)),
            )
        })
        .when(count > 1, |row| {
            let remove = remove.clone();
            row.child(
                IconButton::new(SharedString::from(format!("{id}-remove")), IconName::Trash)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Remove"))
                    .on_click(move |_, _, cx| remove(cx)),
            )
        });
    let row = labeled_row(
        Label::new(SharedString::from(format!(
            "{}{}",
            if index == 0 { "★ " } else { "" },
            name
        )))
        .size(TextSize::Small),
        h_flex()
            .gap(Spacing::Small.pixels())
            .items_center()
            .child(div().child(widget))
            .child(buttons)
            .into_any_element(),
    );

    // Keyboard: ← → pick the variant, Tab opens move and remove.
    let step = |forward: bool| {
        let set_variant = set_variant.clone();
        let total = labels.len();
        Action::new(if forward { "Next variant" } else { "Previous variant" }, move |_, cx| {
            if total > 1 {
                let current = selected.unwrap_or(0);
                let next = if forward {
                    (current + 1) % total
                } else {
                    (current + total - 1) % total
                };
                set_variant(next, cx);
            }
            Outcome::Stay
        })
    };
    let mut panel = ActionPanel::new().action(step(true).icon(IconName::ListTree));
    if index > 0 {
        let move_up = move_up.clone();
        panel = panel.action(
            Action::new("Move up", move |_, cx| {
                move_up(cx);
                Outcome::Stay
            })
            .icon(IconName::ArrowUp),
        );
    }
    if count > 1 {
        let remove = remove.clone();
        panel = panel.action(
            Action::new("Remove layout", move |_, cx| {
                remove(cx);
                Outcome::Stay
            })
            .icon(IconName::Trash)
            .destructive(),
        );
    }
    let control = Control::new(panel).step(step(false), step(true));
    form.control(control, |focused| {
        control_frame(
            row,
            if index == 0 {
                "The default layout. ← → change its variant."
            } else {
                "← → change its variant. Tab for move and remove."
            },
            focused,
            0,
            cx,
        )
    });
}

fn typing_form(form: &mut Form, model: &Entity<InputModel>, cx: &App) {
    let m = model.read(cx);
    rows::title(
        form,
        "Typing",
        "What happens when a key is held down",
    );
    if m.backend == InputBackend::Unsupported {
        rows::note(form, m.backend.persistence_note(), Color::Warning);
        return;
    }
    let Some(loaded) = m.loaded() else {
        let text = match &m.state {
            State::Failed(err) => format!("Could not read the settings: {err}"),
            _ => "Reading the current settings…".to_string(),
        };
        rows::note(form, text, Color::Muted);
        return;
    };
    let k = &loaded.settings.keyboard;

    rows::heading(form, "Key repeat");
    rows::number(
        form,
        Number {
            id: "kb-delay".into(),
            label: "Delay",
            description: "How long a key is held before it starts repeating",
            value: f64::from(k.repeat_delay),
            min: 100.0,
            max: 1500.0,
            step: 10.0,
            unit: Some("ms"),
            default: 600.0,
        },
        {
            let model = model.clone();
            move |v, cx| {
                model.update(cx, |m, cx| m.change(Change::KeyboardRepeatDelay(v as u32), cx))
            }
        },
        cx,
    );
    rows::number(
        form,
        Number {
            id: "kb-rate".into(),
            label: "Rate",
            description: "How many characters a held key types per second",
            value: f64::from(k.repeat_rate),
            min: 1.0,
            max: 100.0,
            step: 1.0,
            unit: Some("per second"),
            default: 25.0,
        },
        {
            let model = model.clone();
            move |v, cx| {
                model.update(cx, |m, cx| m.change(Change::KeyboardRepeatRate(v as u32), cx))
            }
        },
        cx,
    );
    if let Some(err) = &m.error {
        rows::note(form, format!("Could not apply: {err}"), Color::Error);
    }
    rows::note(form, m.backend.persistence_note(), Color::Muted);
}

// -- Caps Lock -----------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum CapsLock {
    Default,
    Escape,
    Ctrl,
    Off,
}

impl CapsLock {
    const ALL: [CapsLock; 4] = [Self::Default, Self::Escape, Self::Ctrl, Self::Off];

    fn label(self) -> &'static str {
        match self {
            Self::Default => "Caps Lock",
            Self::Escape => "Escape",
            Self::Ctrl => "Ctrl",
            Self::Off => "Nothing",
        }
    }

    fn option(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Escape => Some("caps:escape"),
            Self::Ctrl => Some("ctrl:nocaps"),
            Self::Off => Some("caps:none"),
        }
    }

    fn of(set: &XkbSet) -> Self {
        Self::ALL
            .into_iter()
            .find(|c| c.option().is_some_and(|o| set.options.iter().any(|x| x == o)))
            .unwrap_or(Self::Default)
    }

    fn write(self, set: &mut XkbSet) {
        set.options.retain(|o| {
            !Self::ALL
                .iter()
                .any(|c| c.option().is_some_and(|known| known == o))
        });
        if let Some(option) = self.option() {
            set.options.push(option.to_string());
        }
    }
}

// -- picking a layout ----------------------------------------------------------

struct LayoutPicker {
    model: Entity<InputModel>,
}

impl LayoutPicker {
    fn open(model: Entity<InputModel>, cx: &mut App) -> Page {
        Page::new(cx.new(|_| Self { model }))
    }
}

impl LauncherPage for LayoutPicker {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let list = List::new()
            .title("Add layout")
            .placeholder("Search languages…");
        let Some(loaded) = self.model.read(cx).loaded() else {
            return list
                .empty(EmptyView::new("The keyboard settings are not loaded").icon(IconName::Keyboard))
                .into();
        };
        let catalog = loaded.catalog.clone();
        let used = loaded.settings.keyboard.xkb.clone();
        if catalog.layouts.is_empty() {
            return list
                .empty(
                    EmptyView::new("No layout list found")
                        .description("/usr/share/X11/xkb/rules/evdev.lst could not be read.")
                        .icon(IconName::Keyboard),
                )
                .into();
        }
        let items = catalog.layouts.iter().map(|layout| {
            let has_variants = catalog.variants_of(&layout.code).next().is_some();
            let added = used.layouts.iter().any(|e| e.layout == layout.code);
            let (model, code) = (self.model.clone(), layout.code.clone());
            let mut item = Item::new(layout.code.clone(), layout.name.clone())
                .subtitle(layout.code.clone())
                .icon(IconName::Languages)
                .keywords(format!("{} {}", layout.code, layout.name));
            if added {
                item = item.accessory(Accessory::tag("in use").color(Color::Accent));
            }
            item.actions(ActionPanel::new().action(Action::new(
                if has_variants { "Choose variant…" } else { "Add" },
                {
                    let catalog = catalog.clone();
                    move |_, cx| {
                        if has_variants {
                            Outcome::Push(VariantPicker::open(
                                model.clone(),
                                catalog.clone(),
                                code.clone(),
                                cx,
                            ))
                        } else {
                            add_layout(&model, LayoutEntry::new(code.clone(), ""), cx);
                            Outcome::Pop
                        }
                    }
                },
            )))
        });
        list.section(Section::new("Layouts").items(items)).into()
    }
}

fn add_layout(model: &Entity<InputModel>, entry: LayoutEntry, cx: &mut App) {
    let Some(mut set) = model.read(cx).loaded().map(|l| l.settings.keyboard.xkb.clone()) else {
        return;
    };
    if set.contains(&entry.layout, &entry.variant) {
        return;
    }
    set.layouts.push(entry);
    model.update(cx, |m, cx| m.change(Change::KeyboardLayouts(set), cx));
}

struct VariantPicker {
    model: Entity<InputModel>,
    catalog: Rc<XkbCatalog>,
    layout: String,
}

impl VariantPicker {
    fn open(
        model: Entity<InputModel>,
        catalog: Rc<XkbCatalog>,
        layout: String,
        cx: &mut App,
    ) -> Page {
        Page::new(cx.new(|_| Self { model, catalog, layout }))
    }
}

impl LauncherPage for VariantPicker {
    fn content(&mut self, _query: &str, _cx: &mut Context<Self>) -> Content {
        let name = self
            .catalog
            .layout_name(&self.layout)
            .unwrap_or(&self.layout)
            .to_string();
        let choose = |model: Entity<InputModel>, layout: String, variant: String| {
            Action::new("Add", move |_, cx| {
                add_layout(&model, LayoutEntry::new(layout.clone(), variant.clone()), cx);
                // Past the variants and the layouts.
                Outcome::PopMany(2)
            })
        };
        let mut items = vec![
            Item::new("default", "Default")
                .subtitle(name.clone())
                .icon(IconName::Languages)
                .actions(ActionPanel::new().action(choose(
                    self.model.clone(),
                    self.layout.clone(),
                    String::new(),
                ))),
        ];
        items.extend(self.catalog.variants_of(&self.layout).map(|v| {
            Item::new(v.code.clone(), v.name.clone())
                .subtitle(v.code.clone())
                .icon(IconName::Languages)
                .actions(ActionPanel::new().action(choose(
                    self.model.clone(),
                    self.layout.clone(),
                    v.code.clone(),
                )))
        }));
        List::new()
            .title(format!("{name}: variant"))
            .placeholder("Search variants…")
            .section(Section::new("Variants").items(items))
            .into()
    }
}
