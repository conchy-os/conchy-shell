//! The settings form: a section's fields as stacked rows, nested children
//! inline under their own headings. Each control row comes with the keyboard's
//! way into it, so the launcher can move between them.

use gpui::{AnyElement, App, ElementId, FontWeight, IntoElement, SharedString, div, prelude::*, px};
use ui::{
    ActiveTheme, Color, IconName, Label, LabelCommon, Radius, Slider, Spacing, Stepper, Switch,
    TextSize, h_flex, v_flex,
};

use super::schema::{ConfigPath, Editable, Editor, Field, Group};
use super::value_page::ValuePage;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Control, Form};

/// Form labels share one column so the controls line up; the pane is narrow,
/// so it is tighter than the metadata column.
const FORM_LABEL_WIDTH: f32 = 140.;

/// Width of the rail a nested child's rows sit behind, one per level.
const RAIL_WIDTH: f32 = 17.;

/// Add the fields of the section at `path` to `form`, `depth` levels of
/// nested children down.
pub fn add_groups<S: Editable>(
    form: &mut Form,
    path: &ConfigPath<S>,
    groups: &[Group<S>],
    depth: usize,
    cx: &App,
) {
    for group in groups {
        let fields: Vec<&Field<S>> = group
            .fields
            .iter()
            .filter(|field| !field.editor.is_empty())
            .collect();
        if fields.is_empty() && group.preview.is_none() {
            continue;
        }
        // A lone bundle heads its own rows, so the group heading would repeat it.
        let headed_by_bundle = fields.len() == 1 && matches!(fields[0].editor, Editor::Bundle(_));
        if !group.title.is_empty() && !headed_by_bundle {
            let heading = form_heading(group.title);
            add_heading(form, heading, depth, cx);
        }
        if let Some(preview) = &group.preview {
            form.row(indent(depth, preview(path, cx), cx));
        }
        for field in fields {
            add_field(form, path, field, depth, cx);
        }
    }
}

/// One field: a control row, or the rows of a nested child or a bundle.
pub fn add_field<S: Editable>(
    form: &mut Form,
    path: &ConfigPath<S>,
    field: &Field<S>,
    depth: usize,
    cx: &App,
) {
    match &field.editor {
        Editor::Nested { inline, .. } => {
            let heading = Label::new(field.label)
                .size(TextSize::Small)
                .weight(FontWeight::MEDIUM);
            add_heading(form, heading, depth, cx);
            inline(form, path, depth + 1, cx);
        }
        Editor::Bundle(fields) => {
            if let Some(preview) = &field.preview {
                form.row(indent(depth, preview(path, cx), cx));
            }
            let group = Group::new(field.label, fields.to_vec());
            add_groups(form, path, &[group], depth, cx);
        }
        _ => {
            let Some(control) = control(path, field, cx) else {
                return;
            };
            let row = field_row(path, field, cx);
            let description = field.description;
            form.control(control, |focused| {
                control_frame(row, description, focused, depth, cx)
            });
        }
    }
}

/// A heading with room above it, unless it opens the form.
fn add_heading(form: &mut Form, heading: Label, depth: usize, cx: &App) {
    let above = if form.is_empty() {
        Spacing::XSmall
    } else {
        Spacing::XLarge
    };
    let row = div()
        .w_full()
        .pt(above.pixels())
        .pb(Spacing::XSmall.pixels())
        .child(heading);
    form.row(indent(depth, row, cx));
}

/// A group heading inside a form, in the list section header's dress.
pub fn form_heading(title: &str) -> Label {
    Label::new(SharedString::from(title.to_uppercase()))
        .size(TextSize::XSmall)
        .color(Color::Disabled)
}

/// `content` behind one rail per level.
fn indent(depth: usize, content: impl IntoElement, cx: &App) -> AnyElement {
    if depth == 0 {
        return content.into_any_element();
    }
    let rail = cx.theme().colors().border_variant;
    h_flex()
        .w_full()
        .items_stretch()
        .children((0..depth).map(|_| {
            div()
                .flex_none()
                .w(px(1.))
                .mr(px(RAIL_WIDTH - 1.))
                .ml(px(3.))
                .bg(rail)
        }))
        .child(div().flex_1().min_w(px(0.)).child(content))
        .into_any_element()
}

/// The control's row, lit while it has the keyboard, with what it does
/// underneath.
pub(super) fn control_frame(
    content: AnyElement,
    description: &'static str,
    focused: bool,
    depth: usize,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors();
    let row = v_flex()
        .w_full()
        .gap(px(2.))
        .px(Spacing::Small.pixels())
        .py(px(3.))
        .rounded(Radius::Small.pixels())
        .when(focused, |el| el.bg(colors.element_selected))
        .child(content)
        .when(focused && !description.is_empty(), |el| {
            el.child(
                Label::new(description)
                    .size(TextSize::XSmall)
                    .color(Color::Muted),
            )
        });
    indent(depth, row, cx)
}

/// What Enter does for a field; `None` for a field that is not one control.
pub fn primary_action<S: Editable>(path: &ConfigPath<S>, field: &Field<S>) -> Option<Action> {
    let path = path.clone();
    let field = field.clone();
    Some(match &field.editor {
        Editor::Toggle(lens) => {
            let lens = *lens;
            Action::new("Toggle", move |_, cx| {
                path.write(&field, |s| (lens.set)(s, !(lens.get)(s)), cx);
                Outcome::Stay
            })
            .icon(IconName::Check)
        }
        Editor::Choice {
            current, options, ..
        } => {
            let current = current.clone();
            let options = options.clone();
            Action::new("Cycle", move |_, cx| {
                let next = (current(path.read(cx)) + 1) % options.len();
                field.choose(&path, next, cx);
                Outcome::Stay
            })
            .icon(IconName::ListTree)
        }
        // Typing the number beats stepping through a range: a step that
        // passes the end would have to wrap or stop, and neither is a value
        // the user asked for. Stepping is on the arrows, bounded.
        Editor::Number { .. } | Editor::Text { .. } => Action::new("Edit", move |_, cx| {
            let current = field.display(path.read(cx));
            let page = cx.new(|cx| ValuePage::new(path.clone(), field.clone(), cx));
            Outcome::push_with_input(page, current)
        })
        .icon(IconName::Pencil),
        Editor::Custom { open } => {
            let open = open.clone();
            Action::new("Open", move |_, cx| Outcome::Push(open(path.clone(), cx)))
                .icon(IconName::ChevronRight)
        }
        Editor::Nested { .. } | Editor::Bundle(_) => return None,
    })
}

/// The keyboard's way into one control: Enter, Ctrl+R to reset, and what ← → do.
fn control<S: Editable>(path: &ConfigPath<S>, field: &Field<S>, cx: &App) -> Option<Control> {
    let primary = primary_action(path, field)?;
    let modified = field.is_modified(path.read(cx));
    let mut panel = ActionPanel::new().action(primary.clone());
    let mut step = None;
    match &field.editor {
        Editor::Choice {
            current, options, ..
        } => {
            let (current, options) = (current.clone(), options.clone());
            let (field, path) = (field.clone(), path.clone());
            let back = Action::new("Previous option", move |_, cx| {
                let cur = current(path.read(cx));
                let prev = if cur == 0 {
                    options.len().saturating_sub(1)
                } else {
                    cur - 1
                };
                field.choose(&path, prev, cx);
                Outcome::Stay
            });
            step = Some((back, primary));
        }
        Editor::Number {
            lens,
            min,
            max,
            step: by,
            integer,
            ..
        } => {
            let nudge = |label: &'static str, direction: f64| {
                let (lens, min, max, by, integer) = (*lens, *min, *max, *by, *integer);
                let (field, path) = (field.clone(), path.clone());
                Action::new(label, move |_, cx| {
                    let cur = (lens.get)(path.read(cx));
                    let mut next = (cur + direction * by).clamp(min, max);
                    if integer {
                        next = next.round();
                    }
                    path.write(&field, |s| (lens.set)(s, next), cx);
                    Outcome::Stay
                })
            };
            step = Some((nudge("Decrease", -1.0), nudge("Increase", 1.0)));
        }
        _ => {}
    }
    if !matches!(field.editor, Editor::Custom { .. }) {
        panel = panel.action(reset_action(path, field, modified));
    }
    Some(match step {
        Some((back, forward)) => Control::new(panel).step(back, forward),
        None => Control::new(panel),
    })
}

/// Put a field's default back.
pub fn reset_action<S: Editable>(path: &ConfigPath<S>, field: &Field<S>, modified: bool) -> Action {
    let (path, field) = (path.clone(), field.clone());
    let reset = Action::new("Reset to default", move |_, cx| {
        path.reset(&field, cx);
        Outcome::Stay
    })
    .icon(IconName::RotateCcw)
    .shortcut("ctrl-r");
    if modified { reset.destructive() } else { reset }
}

/// `label` in the form's label column, `control` filling the rest.
pub(super) fn labeled_row(label: Label, control: AnyElement) -> AnyElement {
    h_flex()
        .w_full()
        .items_center()
        .gap(Spacing::Medium.pixels())
        .child(
            div()
                .min_w(px(FORM_LABEL_WIDTH))
                .flex_none()
                .child(label),
        )
        .child(h_flex().flex_1().min_w(px(0.)).justify_end().child(control))
        .into_any_element()
}

/// The row of a leaf field: its label and the mouse control for it.
fn field_row<S: Editable>(path: &ConfigPath<S>, field: &Field<S>, cx: &App) -> AnyElement {
    let section = path.read(cx);
    let label = Label::new(field.label).size(TextSize::Small);
    let muted = |text: String| {
        Label::new(text)
            .size(TextSize::Small)
            .color(Color::Muted)
            .truncate()
            .into_any_element()
    };
    match &field.editor {
        Editor::Toggle(lens) => {
            let lens = *lens;
            let (path, field) = (path.clone(), field.clone());
            let id: SharedString = path.key_string(field.key).into();
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(Spacing::Medium.pixels())
                .child(div().flex_1().min_w(px(0.)).child(label))
                .child(
                    Switch::new(id, (lens.get)(section)).on_click(move |_, _, cx| {
                        path.write(&field, |s| (lens.set)(s, !(lens.get)(s)), cx);
                    }),
                )
                .into_any_element()
        }
        // A preset with a preview is chosen on the preview itself.
        Editor::Choice {
            commit: Some(_), ..
        } if field.preview.is_some() => field
            .preview
            .as_ref()
            .map_or_else(|| div().into_any_element(), |preview| preview(path, cx)),
        Editor::Number { .. } => number_widget(path, field, label, cx),
        Editor::Choice { .. } => labeled_row(label, choice_widget(path, field, cx)),
        Editor::Text { .. } => {
            let value = field.display(section);
            let shown = if value.is_empty() {
                "Not set".to_string()
            } else {
                value
            };
            labeled_row(label, muted(shown))
        }
        Editor::Custom { .. } => labeled_row(label, muted("Open".into())),
        Editor::Nested { .. } | Editor::Bundle(_) => div().into_any_element(),
    }
}

/// A number as a stepper over a slider: the label and the stepper share the
/// first line, the unit trailing the label.
fn number_widget<S: Editable>(
    path: &ConfigPath<S>,
    field: &Field<S>,
    label: Label,
    cx: &App,
) -> AnyElement {
    let Editor::Number {
        lens,
        min,
        max,
        step,
        integer,
        unit,
    } = &field.editor
    else {
        return div().into_any_element();
    };
    let id: SharedString = path.key_string(field.key).into();
    let lens = *lens;
    let value = (lens.get)(path.read(cx));
    let is_int = *integer;
    let (min_f, max_f, step_f) = (*min, *max, *step);

    let slider = {
        let path = path.clone();
        let field = field.clone();
        Slider::new(id.clone(), value as f32)
            .min(min_f as f32)
            .max(max_f as f32)
            .step(step_f as f32)
            .show_value(false)
            .on_change(move |v, _, cx| {
                let v = if is_int { (v as f64).round() } else { v as f64 };
                path.write(&field, |s| (lens.set)(s, v), cx);
            })
    };

    let stepper = {
        let path = path.clone();
        let field = field.clone();
        Stepper::new(SharedString::from(format!("{id}-stepper")), value)
            .min(min_f)
            .max(max_f)
            .step(step_f)
            .on_change(move |v, _, cx| {
                let v = if is_int { v.round() } else { v };
                path.write(&field, |s| (lens.set)(s, v), cx);
            })
    };

    let leading = h_flex()
        .items_baseline()
        .gap(Spacing::Small.pixels())
        .child(label)
        .children(unit.map(|unit| {
            Label::new(unit)
                .size(TextSize::XSmall)
                .color(Color::Muted)
        }));

    v_flex()
        .w_full()
        .gap(Spacing::Small.pixels())
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(Spacing::Medium.pixels())
                .child(leading)
                .child(stepper),
        )
        .child(div().w_full().child(slider))
        .into_any_element()
}

/// Mouse control for a choice: pills for a few options, a select for many.
fn choice_widget<S: Editable>(path: &ConfigPath<S>, field: &Field<S>, cx: &App) -> AnyElement {
    let Editor::Choice {
        current, options, ..
    } = &field.editor
    else {
        return div().into_any_element();
    };
    let id: SharedString = path.key_string(field.key).into();
    let selected = current(path.read(cx));
    if options.len() > 5 {
        let labels: Vec<SharedString> = options.iter().map(|o| o.label.into()).collect();
        let path = path.clone();
        let field = field.clone();
        return super::select::select(
            id,
            labels,
            (selected < options.len()).then_some(selected),
            move |ix, cx| field.choose(&path, ix, cx),
            cx,
        );
    }

    let labels: Vec<SharedString> = options.iter().map(|o| o.label.into()).collect();
    let (path, field) = (path.clone(), field.clone());
    pills(
        id,
        labels,
        selected,
        move |ix, cx| field.choose(&path, ix, cx),
        cx,
    )
}

/// A few options as pills, `selected` lit. `on_pick` gets the index clicked.
pub(super) fn pills(
    id: SharedString,
    options: Vec<SharedString>,
    selected: usize,
    on_pick: impl Fn(usize, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors();
    let on_pick = std::rc::Rc::new(on_pick);
    let mut pills = h_flex()
        .gap(px(4.))
        .p(px(3.))
        .rounded(Radius::Medium.pixels())
        .bg(colors.element_background)
        .items_center();
    for (ix, label) in options.into_iter().enumerate() {
        let is_sel = ix == selected;
        let on_pick = on_pick.clone();
        pills = pills.child(
            div()
                .id(ElementId::NamedInteger(
                    format!("{id}-opt").into(),
                    ix as u64,
                ))
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
                    this.text_color(colors.text_muted)
                        .hover(|s| s.bg(colors.element_hover).text_color(colors.text))
                })
                .on_click(move |_, _, cx| on_pick(ix, cx))
                .child(label),
        );
    }
    pills.into_any_element()
}
