//! Rows for the settings pages that are not backed by `shell.kdl`: a switch,
//! a choice, a number, each with the keyboard's way into it, so the pages for
//! the mouse, keyboard, displays and appearance look and behave like the
//! forms of the shell's own sections.

use gpui::{App, FontWeight, SharedString, div, prelude::*, px};
use ui::{
    ActiveTheme, Color, IconName, Label, LabelCommon, Slider, Spacing, Stepper, Switch, TextSize,
    h_flex, v_flex,
};

use super::form::{control_frame, form_heading, labeled_row, pills};
use super::select::select;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Control, Form};

/// A group heading, with room above it unless it opens the form.
pub fn heading(form: &mut Form, title: &str) {
    let above = if form.is_empty() {
        Spacing::XSmall
    } else {
        Spacing::XLarge
    };
    form.row(
        div()
            .w_full()
            .pt(above.pixels())
            .pb(Spacing::XSmall.pixels())
            .child(form_heading(title)),
    );
}

/// A title: bold, with what it is for under it. Opens the form, or a part of
/// a form that stacks several (with room above it then).
pub fn title(form: &mut Form, title: &str, about: &str) {
    let above = if form.is_empty() {
        Spacing::None
    } else {
        Spacing::XLarge
    };
    form.row(
        v_flex()
            .w_full()
            .pt(above.pixels())
            .pb(Spacing::Medium.pixels())
            .gap(Spacing::Small.pixels())
            .child(
                Label::new(SharedString::from(title.to_string()))
                    .size(TextSize::Medium)
                    .weight(FontWeight::BOLD),
            )
            .when(!about.is_empty(), |el| {
                el.child(
                    Label::new(SharedString::from(about.to_string()))
                        .size(TextSize::Small)
                        .color(Color::Muted),
                )
            }),
    );
}

/// A line of explanation, or of what went wrong.
pub fn note(form: &mut Form, text: impl Into<SharedString>, color: Color) {
    form.row(
        div()
            .w_full()
            .px(Spacing::Small.pixels())
            .py(px(3.))
            .child(Label::new(text.into()).size(TextSize::XSmall).color(color)),
    );
}

/// A switch.
pub fn toggle(
    form: &mut Form,
    id: impl Into<SharedString>,
    label: &'static str,
    description: &'static str,
    value: bool,
    on_change: impl Fn(bool, &mut App) + 'static,
    cx: &App,
) {
    let id: SharedString = id.into();
    let on_change = std::rc::Rc::new(on_change);

    let flip = {
        let on_change = on_change.clone();
        Action::new("Toggle", move |_, cx| {
            on_change(!value, cx);
            Outcome::Stay
        })
        .icon(IconName::Check)
    };
    let row = h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(Spacing::Medium.pixels())
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .child(Label::new(label).size(TextSize::Small)),
        )
        .child(Switch::new(id, value).on_click(move |_, _, cx| on_change(!value, cx)))
        .into_any_element();
    form.control(Control::new(ActionPanel::new().action(flip)), |focused| {
        control_frame(row, description, focused, 0, cx)
    });
}

/// A choice out of a few (pills) or many (a select). Enter and → step to the
/// next option, ← to the previous.
// One row, one function: the arguments are the row's own parts, and every
// page calls it the same way.
#[allow(clippy::too_many_arguments)]
pub fn choice(
    form: &mut Form,
    id: impl Into<SharedString>,
    label: &'static str,
    description: &'static str,
    options: Vec<SharedString>,
    selected: Option<usize>,
    on_pick: impl Fn(usize, &mut App) + 'static,
    cx: &App,
) {
    choice_resettable(
        form,
        id,
        label,
        description,
        options,
        selected,
        on_pick,
        None,
        cx,
    );
}

/// [`choice`] with one more key: `reset`, which a page gives a shortcut of
/// its own (Ctrl+R), puts the system's own choice back.
#[allow(clippy::too_many_arguments)]
pub fn choice_resettable(
    form: &mut Form,
    id: impl Into<SharedString>,
    label: &'static str,
    description: &'static str,
    options: Vec<SharedString>,
    selected: Option<usize>,
    on_pick: impl Fn(usize, &mut App) + 'static,
    reset: Option<Action>,
    cx: &App,
) {
    let id: SharedString = id.into();
    let count = options.len();
    let on_pick = std::rc::Rc::new(on_pick);

    let widget = if count > 4 || options.iter().map(|o| o.len()).sum::<usize>() > 34 {
        let on_pick = on_pick.clone();
        select(id, options, selected, move |ix, cx| on_pick(ix, cx), cx)
    } else {
        let on_pick = on_pick.clone();
        pills(
            id,
            options,
            selected.unwrap_or(usize::MAX),
            move |ix, cx| on_pick(ix, cx),
            cx,
        )
    };

    let step = |forward: bool| {
        let on_pick = on_pick.clone();
        Action::new(
            if forward {
                "Next option"
            } else {
                "Previous option"
            },
            move |_, cx| {
                if count > 0 {
                    let current = selected.unwrap_or(if forward { count - 1 } else { 0 });
                    let next = if forward {
                        (current + 1) % count
                    } else {
                        (current + count - 1) % count
                    };
                    on_pick(next, cx);
                }
                Outcome::Stay
            },
        )
    };
    let mut panel = ActionPanel::new().action(step(true).icon(IconName::ListTree));
    if let Some(reset) = reset {
        panel = panel.action(reset);
    }
    let control = Control::new(panel).step(step(false), step(true));
    let row = labeled_row(Label::new(label).size(TextSize::Small), widget);
    form.control(control, |focused| {
        control_frame(row, description, focused, 0, cx)
    });
}

/// What a number row needs besides its label.
pub struct Number {
    pub id: SharedString,
    pub label: &'static str,
    pub description: &'static str,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub unit: Option<&'static str>,
    /// Ctrl+R and Enter put this back.
    pub default: f64,
}

/// A number: a stepper over a slider. ← → step it, Enter puts the default back.
pub fn number(
    form: &mut Form,
    spec: Number,
    on_change: impl Fn(f64, &mut App) + 'static,
    cx: &App,
) {
    let on_change = std::rc::Rc::new(on_change);
    let Number {
        id,
        label,
        description,
        value,
        min,
        max,
        step,
        unit,
        default,
    } = spec;

    let slider = {
        let on_change = on_change.clone();
        Slider::new(id.clone(), value as f32)
            .min(min as f32)
            .max(max as f32)
            .step(step as f32)
            .show_value(false)
            .on_change(move |v, _, cx| on_change(round_to(f64::from(v), step), cx))
    };
    let stepper = {
        let on_change = on_change.clone();
        Stepper::new(SharedString::from(format!("{id}-stepper")), value)
            .min(min)
            .max(max)
            .step(step)
            .on_change(move |v, _, cx| on_change(round_to(v, step), cx))
    };
    let leading = h_flex()
        .items_baseline()
        .gap(Spacing::Small.pixels())
        .child(Label::new(label).size(TextSize::Small))
        .children(unit.map(|unit| Label::new(unit).size(TextSize::XSmall).color(Color::Muted)));
    let row = v_flex()
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
        .into_any_element();

    let nudge = |label: &'static str, direction: f64| {
        let on_change = on_change.clone();
        Action::new(label, move |_, cx| {
            let next = (value + direction * step).clamp(min, max);
            on_change(round_to(next, step), cx);
            Outcome::Stay
        })
    };
    let reset = {
        let on_change = on_change.clone();
        Action::new("Reset to default", move |_, cx| {
            on_change(default, cx);
            Outcome::Stay
        })
        .icon(IconName::RotateCcw)
        .shortcut("ctrl-r")
    };
    let control = Control::new(ActionPanel::new().action(reset))
        .step(nudge("Decrease", -1.0), nudge("Increase", 1.0));
    form.control(control, |focused| {
        control_frame(row, description, focused, 0, cx)
    });
}

/// `value` rounded to a multiple of `step`, so floating point drift from a
/// slider does not end up in a config file (`0.30000000000000004`).
pub fn round_to(value: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return value;
    }
    let rounded = (value / step).round() * step;
    // Keep the decimals `step` has.
    let decimals = (-step.log10().floor()).max(0.0) as i32 + 1;
    let factor = 10f64.powi(decimals);
    (rounded * factor).round() / factor
}

/// A row that does something when pressed: Enter on the keyboard.
pub fn action(
    form: &mut Form,
    label: &'static str,
    description: &'static str,
    run: impl Fn(&mut gpui::Window, &mut App) -> Outcome + 'static,
    cx: &App,
) {
    let colors = cx.theme().colors();
    let run = std::rc::Rc::new(run);
    let control = Control::new(ActionPanel::new().action(Action::new(label, {
        let run = run.clone();
        move |window, cx| run(window, cx)
    })));
    let row = h_flex()
        .w_full()
        .items_center()
        .gap(Spacing::Small.pixels())
        .child(
            ui::Icon::new(IconName::ChevronRight)
                .size(ui::IconSize::Small)
                .color(Color::Accent),
        )
        .child(Label::new(label).size(TextSize::Small).color(Color::Accent))
        .into_any_element();
    let _ = colors;
    form.control(control, |focused| {
        control_frame(row, description, focused, 0, cx)
    });
}

/// A key and its value, nothing to do on it: the rows of an information form.
pub fn info(form: &mut Form, label: &'static str, value: impl Into<SharedString>) {
    form.row(
        h_flex()
            .w_full()
            .items_start()
            .justify_between()
            .gap(Spacing::Medium.pixels())
            .px(Spacing::Small.pixels())
            .py(px(3.))
            .child(
                div()
                    .flex_none()
                    .child(Label::new(label).size(TextSize::Small).color(Color::Muted)),
            )
            .child(
                div().min_w(px(0.)).child(
                    Label::new(value.into())
                        .size(TextSize::Small)
                        .weight(FontWeight::MEDIUM),
                ),
            ),
    );
}

/// What a record row shows: a device, a network, a program.
pub struct Entry {
    pub icon: IconName,
    pub title: SharedString,
    pub subtitle: SharedString,
    pub tag: Option<(SharedString, Color)>,
}

/// A record the keyboard can act on: Enter runs the first action, Shift+Enter
/// the second, and any other one the shortcut it carries. The row tells which
/// is which while it has the focus.
pub fn entry(form: &mut Form, spec: Entry, actions: Vec<Action>, cx: &App) {
    let Entry {
        icon,
        title,
        subtitle,
        tag,
    } = spec;
    let hint = actions
        .iter()
        .enumerate()
        .filter_map(|(ix, action)| {
            let key = match ix {
                0 => "Enter".to_string(),
                1 => "Shift+Enter".to_string(),
                _ => action.shortcut_label()?.to_string(),
            };
            Some(format!("{key}: {}", action.title))
        })
        .collect::<Vec<_>>()
        .join("   ");

    let mut panel = ActionPanel::new();
    for action in actions {
        panel = panel.action(action);
    }
    let row = h_flex()
        .w_full()
        .items_center()
        .gap(Spacing::Medium.pixels())
        .child(
            ui::Icon::new(icon)
                .size(ui::IconSize::Small)
                .color(Color::Muted),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .child(Label::new(title).size(TextSize::Small))
                .child(
                    Label::new(subtitle)
                        .size(TextSize::XSmall)
                        .color(Color::Muted),
                ),
        )
        .children(tag.map(|(text, color)| Label::new(text).size(TextSize::XSmall).color(color)))
        .into_any_element();
    form.control(Control::new(panel), |focused| {
        control_frame(row, hint, focused, 0, cx)
    });
}

#[cfg(test)]
mod tests {
    use super::round_to;

    #[test]
    fn slider_noise_is_rounded_away() {
        assert_eq!(round_to(0.30000000000000004, 0.05), 0.3);
        assert_eq!(round_to(0.123, 0.1), 0.1);
        assert_eq!(round_to(12.0, 1.0), 12.0);
        assert_eq!(round_to(1.2000000000000002, 0.1), 1.2);
        assert_eq!(round_to(-0.35, 0.05), -0.35);
    }
}
