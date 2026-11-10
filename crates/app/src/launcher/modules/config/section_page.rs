//! The generic editor page: a row per [`Field`] under its group heading, the
//! selected row's detail on the right. A nested row's detail is its child's
//! whole form, with every child action in the menu, so one row edits a
//! section without opening it.

use gpui::{
    AnyElement, App, AppContext, Context, Div, FontWeight, SharedString, Window, div, prelude::*,
    px,
};
use ui::{
    ActiveTheme, Button, ButtonCommon, ButtonStyle, Clickable, Color, Icon, IconName, IconSize,
    Label, LabelCommon, Radius, Slider, Spacing, Stepper, Switch, TextSize, h_flex, v_flex,
};

use super::choice_page::ChoicePage;
use super::schema::{ConfigPath, Editable, Editor, Field, Group};
use super::value_page::ValuePage;
use crate::config::{ConfigSignals, store};
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Detail, Item, List, Section};
use crate::launcher::page::LauncherPage;
use crate::state::watch;

/// Nested forms stop inlining children past this depth: a child of a child
/// shows its summary, its own row has the full form.
const MAX_FORM_DEPTH: usize = 1;

/// Form labels share one column so the controls line up; the pane is narrow,
/// so it is tighter than the metadata column.
const FORM_LABEL_WIDTH: f32 = 96.;

pub struct SectionPage<S: Editable> {
    path: ConfigPath<S>,
    title: &'static str,
    groups: Vec<Group<S>>,
}

impl<S: Editable> SectionPage<S> {
    pub fn new(path: ConfigPath<S>, cx: &mut Context<Self>) -> Self {
        Self::with_groups(path, S::title(), S::groups(), cx)
    }

    /// A page over part of a struct, as a bundle row opens.
    pub fn with_groups(
        path: ConfigPath<S>,
        title: &'static str,
        groups: Vec<Group<S>>,
        cx: &mut Context<Self>,
    ) -> Self {
        watch_revision(cx);
        Self {
            path,
            title,
            groups,
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

impl<S: Editable> LauncherPage for SectionPage<S> {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let mut list = List::new()
            .show_detail(true)
            .title(self.title)
            .placeholder("Search settings…")
            .actions(file_actions(ActionPanel::new()));
        for group in &self.groups {
            let mut items = group
                .fields
                .iter()
                .filter(|field| !field.editor.is_empty())
                .map(|field| field_item(&self.path, field, cx))
                .peekable();
            if items.peek().is_none() {
                continue;
            }
            let section = if group.title.is_empty() {
                Section::untitled()
            } else {
                Section::new(group.title)
            };
            list = list.section(section.items(items));
        }
        let shortcuts = S::shortcuts(cx);
        if !shortcuts.is_empty() {
            list = list.section(Section::new("More").items(shortcuts));
        }
        list.into()
    }

    fn render_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        unsaved_banner(cx)
    }
}

/// Save and discard, reachable from every row.
pub fn file_actions(panel: ActionPanel) -> ActionPanel {
    panel.section(
        "config.toml",
        [
            Action::new("Save", |_, cx| {
                store::save(cx);
                Outcome::Stay
            })
            .icon(IconName::Save)
            .shortcut("ctrl-s"),
            Action::new("Discard unsaved changes", |_, cx| {
                store::discard(cx);
                Outcome::Stay
            })
            .icon(IconName::RotateCcw)
            .shortcut("ctrl-shift-d")
            .destructive(),
        ],
    )
}

/// Shown while edits are only in memory.
pub fn unsaved_banner(cx: &App) -> Option<AnyElement> {
    let pending = store::pending();
    if pending == 0 {
        return None;
    }
    let colors = cx.theme().colors();
    let changes = if pending == 1 { "change" } else { "changes" };
    Some(
        header_card(
            h_flex()
                .w_full()
                .items_center()
                .gap(Spacing::Medium.pixels())
                .bg(colors.status.warning_background)
                .border_color(colors.status.warning_border)
                .child(
                    Icon::new(IconName::Save)
                        .size(IconSize::Small)
                        .color(Color::Warning),
                )
                .child(
                    Label::new(format!("{pending} unsaved {changes}"))
                        .size(TextSize::Small)
                        .color(Color::Warning),
                )
                .child(
                    div().flex_1().min_w(px(0.)).child(
                        Label::new("Ctrl+S saves, Ctrl+Shift+D discards")
                            .size(TextSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                    ),
                )
                // The same two actions for the mouse.
                .child(
                    Button::new("unsaved-discard", "Discard")
                        .style(ButtonStyle::Outlined)
                        .on_click(|_, _, cx| store::discard(cx)),
                )
                .child(
                    Button::new("unsaved-save", "Save")
                        .style(ButtonStyle::Filled)
                        .on_click(|_, _, cx| store::save(cx)),
                ),
        )
        .into_any_element(),
    )
}

/// A page header card with the launcher's card chrome; `body` sets its own
/// colours over the default surface.
pub fn header_card(body: Div) -> Div {
    div()
        .px(Spacing::Medium.pixels())
        .pt(Spacing::XSmall.pixels())
        .child(
            body.px(Spacing::Large.pixels())
                .py(Spacing::Medium.pixels())
                .rounded(Radius::Large.pixels())
                .border_1(),
        )
}

/// The row for a field. A nested child with a single setting is hoisted:
/// that setting becomes the row, so no page ever holds one field.
pub fn field_item<S: Editable>(path: &ConfigPath<S>, field: &Field<S>, cx: &App) -> Item {
    if let Editor::Nested {
        hoist: Some(hoist), ..
    } = &field.editor
    {
        return hoist(field, path, cx);
    }
    let section = path.read(cx);
    let key_path = path.key_string(field.key);
    let modified = field.is_modified(section);
    let value = field.display(section);

    let mut item = Item::new(key_path.clone(), field.label)
        .icon(field.icon.unwrap_or_else(|| field.editor.icon()))
        .keywords(key_path.clone());
    item = match (&field.editor, value.is_empty()) {
        // A nested row's subtitle is its child's values; the description
        // moves to the detail pane.
        (Editor::Nested { .. }, false) => item.subtitle(value),
        (_, false) => {
            if !field.description.is_empty() {
                item = item.subtitle(field.description);
            }
            item.accessory(Accessory::text(value))
        }
        (_, true) if !field.description.is_empty() => item.subtitle(field.description),
        _ => item,
    };
    if modified {
        item = item.accessory(Accessory::tag("modified").color(Color::Accent));
    }
    item.actions(field_actions(path, field, &key_path, modified))
        .detail(field_detail(path, field, section, &key_path))
}

/// [`field_item`] for the one field of `child`, titled after its parent.
pub fn hoisted_item<C: Editable>(
    parent: &Field<impl Editable>,
    child: &ConfigPath<C>,
    cx: &App,
) -> Item {
    let Some(field) = C::fields().pop() else {
        return Item::new(parent.key, parent.label);
    };
    let title: SharedString = format!("{} · {}", parent.label, field.label).into();
    let mut item = field_item(child, &field, cx);
    item.title = title;
    if let Some(icon) = parent.icon {
        item = item.icon(icon);
    }
    item
}

/// What Enter does for a field. `labeled` names the field, for menus that
/// list several fields' actions.
pub fn primary_action<S: Editable>(
    path: &ConfigPath<S>,
    field: &Field<S>,
    labeled: bool,
) -> Action {
    let title = |verb: &str| -> String {
        if labeled {
            format!("{verb} {}", field.label.to_lowercase())
        } else {
            verb.to_string()
        }
    };
    let path = path.clone();
    let field = field.clone();
    match &field.editor {
        Editor::Toggle(lens) => {
            let lens = *lens;
            Action::new(title("Toggle"), move |_, cx| {
                path.write(&field, |s| (lens.set)(s, !(lens.get)(s)), cx);
                Outcome::Stay
            })
            .icon(IconName::Check)
        }
        Editor::Choice { .. } => Action::new(title("Choose"), move |_, cx| {
            let page = cx.new(|cx| ChoicePage::new(path.clone(), field.clone(), cx));
            Outcome::push(page)
        })
        .icon(IconName::ListTree),
        Editor::Number { .. } | Editor::Text { .. } => Action::new(title("Edit"), move |_, cx| {
            let current = field.display(path.read(cx));
            let page = cx.new(|cx| ValuePage::new(path.clone(), field.clone(), cx));
            Outcome::push_with_input(page, current)
        })
        .icon(IconName::Pencil),
        Editor::Nested { open, .. } | Editor::Custom { open } => {
            let open = open.clone();
            Action::new(title("Open"), move |_, cx| {
                Outcome::Push(open(path.clone(), cx))
            })
            .icon(IconName::ChevronRight)
        }
        Editor::Bundle(fields) => {
            let fields = fields.clone();
            Action::new(title("Open"), move |_, cx| {
                let groups = vec![Group::new("", fields.to_vec())];
                let page =
                    cx.new(|cx| SectionPage::with_groups(path.clone(), field.label, groups, cx));
                Outcome::push(page)
            })
            .icon(IconName::ChevronRight)
        }
    }
}

fn field_actions<S: Editable>(
    path: &ConfigPath<S>,
    field: &Field<S>,
    key_path: &str,
    modified: bool,
) -> ActionPanel {
    let mut panel = ActionPanel::new().action(primary_action(path, field, false));
    match &field.editor {
        Editor::Nested { quick, .. } => panel = panel.section("Settings", quick(path)),
        Editor::Bundle(fields) => {
            let quick = fields
                .iter()
                .filter(|f| !f.editor.is_empty())
                .map(|f| primary_action(path, f, true));
            panel = panel.section("Settings", quick);
        }
        _ => {}
    }
    if !matches!(field.editor, Editor::Custom { .. }) {
        let path = path.clone();
        let field = field.clone();
        let mut reset = Action::new("Reset to default", move |_, cx| {
            path.reset(&field, cx);
            Outcome::Stay
        })
        .icon(IconName::RotateCcw)
        .shortcut("ctrl-r");
        if modified {
            reset = reset.destructive();
        }
        panel = panel.action(reset);
    }
    panel = panel.action(Action::copy(key_path.to_string()).title("Copy key"));
    file_actions(panel)
}

/// The detail pane: a small header (label, key, description), then the
/// controls, which get the room.
fn field_detail<S: Editable>(
    path: &ConfigPath<S>,
    field: &Field<S>,
    section: &S,
    key_path: &str,
) -> Detail {
    let path = path.clone();
    let field = field.clone();
    let key_path = key_path.to_string();
    let modified = field.is_modified(section);
    let default = if modified && !matches!(field.editor, Editor::Nested { .. }) {
        Some(field.display(&S::default()))
    } else {
        None
    };
    Detail::new().widget(move |cx| {
        let colors = cx.theme().colors();
        let mut header = v_flex().w_full().gap(Spacing::XXSmall.pixels()).child(
            h_flex()
                .w_full()
                .gap(Spacing::Medium.pixels())
                .items_baseline()
                .child(
                    Label::new(field.label)
                        .size(TextSize::Small)
                        .weight(FontWeight::SEMIBOLD),
                )
                .child(
                    div().flex_1().min_w(px(0.)).child(
                        Label::new(key_path.clone())
                            .size(TextSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                    ),
                ),
        );
        if !field.description.is_empty() {
            header = header.child(
                Label::new(field.description)
                    .size(TextSize::XSmall)
                    .color(Color::Muted),
            );
        }
        if let Some(default) = &default {
            header = header.child(
                Label::new(format!("Default {default}"))
                    .size(TextSize::XSmall)
                    .color(Color::Accent),
            );
        }
        let body = match &field.editor {
            Editor::Nested { form, .. } => form(&path, 1, cx),
            Editor::Bundle(fields) => render_form(&path, &[Group::new("", fields.to_vec())], 1, cx),
            // A preset with a preview is chosen on the preview itself.
            Editor::Choice {
                commit: Some(_), ..
            } if field.preview.is_some() => div().into_any_element(),
            Editor::Toggle(_) | Editor::Number { .. } | Editor::Choice { .. } => {
                field_widget(&path, &field, cx)
            }
            Editor::Text { .. } => h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(Spacing::Medium.pixels())
                .child(
                    div().flex_1().min_w(px(0.)).child(
                        Label::new(field.display(path.read(cx)))
                            .size(TextSize::Small)
                            .truncate(),
                    ),
                )
                .child(
                    Label::new("Enter to edit")
                        .size(TextSize::XSmall)
                        .color(Color::Muted),
                )
                .into_any_element(),
            Editor::Custom { .. } => Label::new("Enter to open")
                .size(TextSize::XSmall)
                .color(Color::Muted)
                .into_any_element(),
        };
        v_flex()
            .w_full()
            .gap(Spacing::Large.pixels())
            .child(header)
            .child(div().w_full().h(px(1.)).bg(colors.border_variant))
            .children(field.preview.as_ref().map(|preview| preview(&path, cx)))
            .child(body)
            .into_any_element()
    })
}

/// A child struct's fields as a form, group headings included. Children of
/// children inline until [`MAX_FORM_DEPTH`], then show their summary.
pub fn render_form<S: Editable>(
    path: &ConfigPath<S>,
    groups: &[Group<S>],
    depth: usize,
    cx: &App,
) -> AnyElement {
    let section = path.read(cx);
    let mut column = v_flex().w_full().gap(Spacing::Large.pixels());
    for group in groups {
        let fields: Vec<&Field<S>> = group
            .fields
            .iter()
            .filter(|f| !f.editor.is_empty())
            .collect();
        if fields.is_empty() {
            continue;
        }
        // Rows sit closer to each other than groups do; the heading is drawn
        // like a list section header.
        let mut block = v_flex().w_full().gap(Spacing::Small.pixels());
        // A lone bundle heads its own rows, so the group heading would repeat it.
        let headed_by_bundle = fields.len() == 1 && matches!(fields[0].editor, Editor::Bundle(_));
        if !group.title.is_empty() && !headed_by_bundle {
            block = block.child(form_heading(group.title));
        }
        for field in fields {
            block = block.child(form_row(path, field, section, depth, cx));
        }
        column = column.child(block);
    }
    column.into_any_element()
}

/// A group heading inside a form, in the list section header's dress.
pub fn form_heading(title: &str) -> Label {
    Label::new(SharedString::from(title.to_uppercase()))
        .size(TextSize::XSmall)
        .color(Color::Disabled)
}

/// `label` in the form's label column, `control` filling the rest.
fn labeled_row(label: Label, control: AnyElement) -> AnyElement {
    h_flex()
        .w_full()
        .items_center()
        .gap(Spacing::Medium.pixels())
        .child(
            div()
                .w(px(FORM_LABEL_WIDTH))
                .flex_none()
                .child(label.truncate()),
        )
        .child(h_flex().flex_1().min_w(px(0.)).justify_end().child(control))
        .into_any_element()
}

fn form_row<S: Editable>(
    path: &ConfigPath<S>,
    field: &Field<S>,
    section: &S,
    depth: usize,
    cx: &App,
) -> AnyElement {
    let label = Label::new(field.label).size(TextSize::Small);
    let muted = |text: String| {
        Label::new(text)
            .size(TextSize::Small)
            .color(Color::Muted)
            .truncate()
            .into_any_element()
    };
    let value = field.display(section);
    match &field.editor {
        Editor::Nested { form, .. } if depth < MAX_FORM_DEPTH => v_flex()
            .w_full()
            .gap(Spacing::Small.pixels())
            .child(label.weight(FontWeight::MEDIUM))
            .child(
                div()
                    .w_full()
                    .pl(Spacing::Large.pixels())
                    .border_l_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(form(path, depth + 1, cx)),
            )
            .into_any_element(),
        Editor::Bundle(fields) => v_flex()
            .w_full()
            .gap(Spacing::Small.pixels())
            .children(field.preview.as_ref().map(|preview| preview(path, cx)))
            .child(render_form(
                path,
                &[Group::new(field.label, fields.to_vec())],
                depth,
                cx,
            ))
            .into_any_element(),
        Editor::Toggle(_) | Editor::Choice { .. } | Editor::Number { .. } => {
            labeled_row(label, field_widget(path, field, cx))
        }
        // Text and custom fields edit from their own row; a deep nested one
        // shows its summary.
        Editor::Text { .. } if value.is_empty() => labeled_row(label, muted("Not set".into())),
        Editor::Custom { .. } => labeled_row(label, muted("Opens its own page".into())),
        _ => labeled_row(label, muted(value)),
    }
}

/// Mouse control for one field; handlers write through the store.
fn field_widget<S: Editable>(path: &ConfigPath<S>, field: &Field<S>, cx: &App) -> AnyElement {
    let section = path.read(cx);
    let id: SharedString = path.key_string(field.key).into();
    match &field.editor {
        Editor::Toggle(lens) => {
            let lens = *lens;
            let path = path.clone();
            let field = field.clone();
            Switch::new(id, (lens.get)(section))
                .on_click(move |_, _, cx| {
                    path.write(&field, |s| (lens.set)(s, !(lens.get)(s)), cx);
                })
                .into_any_element()
        }
        Editor::Number {
            lens,
            min,
            max,
            step,
            integer,
            ..
        } => {
            let lens = *lens;
            let value = (lens.get)(section);
            // The stepper shows the value for whole numbers; the slider only
            // when it stands alone.
            let slider = {
                let path = path.clone();
                let field = field.clone();
                Slider::new(id.clone(), value as f32)
                    .min(*min as f32)
                    .max(*max as f32)
                    .step(*step as f32)
                    .show_value(!*integer)
                    .on_change(move |value, _, cx| {
                        path.write(&field, |s| (lens.set)(s, f64::from(value)), cx);
                    })
            };
            let mut row = h_flex()
                .w_full()
                .items_center()
                .gap(Spacing::Medium.pixels())
                .child(div().flex_1().min_w(px(0.)).child(slider));
            if *integer {
                let path = path.clone();
                let field = field.clone();
                row = row.child(
                    div().flex_none().child(
                        Stepper::new(SharedString::from(format!("{id}-stepper")), value)
                            .min(*min)
                            .max(*max)
                            .step(*step)
                            .on_change(move |value, _, cx| {
                                path.write(&field, |s| (lens.set)(s, value.round()), cx);
                            }),
                    ),
                );
            }
            row.into_any_element()
        }
        Editor::Choice {
            current, options, ..
        } => {
            let selected = current(section);
            let labels: Vec<SharedString> = options.iter().map(|o| o.label.into()).collect();
            let path = path.clone();
            let field = field.clone();
            super::select::select(
                id,
                labels,
                (selected < options.len()).then_some(selected),
                move |ix, cx| field.choose(&path, ix, cx),
                cx,
            )
        }
        Editor::Text { .. } | Editor::Nested { .. } | Editor::Custom { .. } | Editor::Bundle(_) => {
            div().into_any_element()
        }
    }
}
