//! Mouse and touchpad: speed, acceleration, scrolling and tapping.

use gpui::{App, AppContext, Context, Entity, SharedString};
use services::input::{AccelProfile, Change, ClickMethod, InputBackend};
use ui::{Color, IconName};

use super::input_model::{InputModel, State};
use super::settings_form::{self as rows, Number};
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Detail, Form, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};

pub struct MousePage {
    model: Entity<InputModel>,
}

impl MousePage {
    pub fn open(cx: &mut App) -> Page {
        Page::new(cx.new(Self::new))
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        let model = cx.new(InputModel::new);
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        Self { model }
    }
}

impl LauncherPage for MousePage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let model = self.model.clone();
        let (has_touchpad, loaded) = match model.read(cx).loaded() {
            Some(l) => (l.has_touchpad, true),
            None => (true, false),
        };

        let edit = || {
            ActionPanel::new()
                .action(Action::new("Edit", |_, _| Outcome::FocusDetail).icon(IconName::ChevronRight))
        };

        let mut mouse = Item::new("mouse", "Mouse")
            .icon(IconName::Mouse)
            .subtitle("Speed, acceleration and scrolling of external mice")
            .keywords("mouse pointer speed sensitivity acceleration accel flat adaptive scroll natural left handed")
            .actions(edit());
        let mut touchpad = Item::new("touchpad", "Touchpad")
            .icon(IconName::Mouse)
            .subtitle(if !loaded || has_touchpad {
                "Tap to click, natural scrolling, speed"
            } else {
                "No touchpad detected on this computer"
            })
            .keywords("touchpad trackpad tap click natural scroll gestures disable while typing palm")
            .actions(edit());

        mouse = mouse.detail(Detail::new().form({
            let model = model.clone();
            move |form, cx| pointer_form(form, &model, cx)
        }));
        touchpad = touchpad.detail(Detail::new().form({
            let model = model.clone();
            move |form, cx| touchpad_form(form, &model, cx)
        }));
        if loaded && !has_touchpad {
            touchpad = touchpad.accessory(Accessory::tag("not found").color(Color::Muted));
        }

        List::new()
            .show_detail(true)
            .click_selects(true)
            .title("Mouse & Touchpad")
            .placeholder("Search pointer settings…")
            .section(Section::new("Pointing devices").items([mouse, touchpad]))
            .into()
    }
}

/// What every form says before and after its rows.
fn status(form: &mut Form, model: &InputModel) -> bool {
    match &model.state {
        State::Loading => {
            rows::note(form, "Reading the current settings…", Color::Muted);
            false
        }
        State::Failed(err) => {
            rows::note(form, format!("Could not read the settings: {err}"), Color::Error);
            false
        }
        State::Ready(_) => true,
    }
}

fn footer(form: &mut Form, model: &InputModel) {
    if let Some(err) = &model.error {
        rows::note(form, format!("Could not apply: {err}"), Color::Error);
    }
    rows::note(form, model.backend.persistence_note(), Color::Muted);
}

fn accel_options() -> Vec<SharedString> {
    vec!["Adaptive".into(), "Flat".into()]
}

fn accel_index(profile: AccelProfile) -> Option<usize> {
    Some(match profile {
        AccelProfile::Adaptive => 0,
        AccelProfile::Flat => 1,
    })
}

fn accel_from(index: usize) -> AccelProfile {
    if index == 1 {
        AccelProfile::Flat
    } else {
        AccelProfile::Adaptive
    }
}

fn pointer_form(form: &mut Form, model: &Entity<InputModel>, cx: &App) {
    let m = model.read(cx);
    rows::title(
        form,
        "Mouse",
        "How the pointer follows an external mouse and how it scrolls",
    );
    if m.backend == InputBackend::Unsupported {
        rows::note(form, m.backend.persistence_note(), Color::Warning);
        return;
    }
    if !status(form, m) {
        return;
    }
    let Some(loaded) = m.loaded() else { return };
    let p = &loaded.settings.pointer;

    rows::heading(form, "Movement");
    rows::number(
        form,
        Number {
            id: "mouse-speed".into(),
            label: "Pointer speed",
            description: "Slower on the left, faster on the right",
            value: p.speed,
            min: -1.0,
            max: 1.0,
            step: 0.05,
            unit: None,
            default: 0.0,
        },
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::PointerSpeed(v), cx))
        },
        cx,
    );
    rows::choice(
        form,
        "mouse-accel",
        "Acceleration",
        "Adaptive speeds the pointer up with the hand; flat keeps it 1:1",
        accel_options(),
        accel_index(p.accel_profile),
        {
            let model = model.clone();
            move |ix, cx| {
                model.update(cx, |m, cx| m.change(Change::PointerAccel(accel_from(ix)), cx))
            }
        },
        cx,
    );
    rows::toggle(
        form,
        "mouse-left-handed",
        "Left-handed",
        "Swap the left and right buttons",
        p.left_handed,
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::PointerLeftHanded(v), cx))
        },
        cx,
    );

    rows::heading(form, "Scrolling");
    rows::toggle(
        form,
        "mouse-natural",
        "Natural scrolling",
        "Content follows the wheel, like on a phone",
        p.natural_scroll,
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::PointerNaturalScroll(v), cx))
        },
        cx,
    );
    rows::number(
        form,
        Number {
            id: "mouse-scroll".into(),
            label: "Scroll speed",
            description: "How far one wheel step scrolls",
            value: p.scroll_speed,
            min: 0.1,
            max: 5.0,
            step: 0.1,
            unit: Some("×"),
            default: 1.0,
        },
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::PointerScrollSpeed(v), cx))
        },
        cx,
    );
    footer(form, m);
}

fn touchpad_form(form: &mut Form, model: &Entity<InputModel>, cx: &App) {
    let m = model.read(cx);
    rows::title(
        form,
        "Touchpad",
        "Taps, scrolling and speed of the built-in touchpad",
    );
    if m.backend == InputBackend::Unsupported {
        rows::note(form, m.backend.persistence_note(), Color::Warning);
        return;
    }
    if !status(form, m) {
        return;
    }
    let Some(loaded) = m.loaded() else { return };
    let t = &loaded.settings.touchpad;
    if !loaded.has_touchpad {
        rows::note(
            form,
            "No touchpad was found; these settings apply when one is connected.",
            Color::Warning,
        );
    }

    rows::heading(form, "Clicking");
    rows::toggle(
        form,
        "tp-tap",
        "Tap to click",
        "A light tap counts as a click; two fingers as a right click",
        t.tap_to_click,
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::TouchpadTap(v), cx))
        },
        cx,
    );
    rows::toggle(
        form,
        "tp-drag",
        "Tap and drag",
        "Tap, then touch again and move to drag",
        t.tap_and_drag,
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::TouchpadTapAndDrag(v), cx))
        },
        cx,
    );
    rows::choice(
        form,
        "tp-click-method",
        "Press to click",
        "Corners of the pad for right click, or the number of fingers",
        vec!["Pad areas".into(), "Finger count".into()],
        Some(match t.click_method {
            ClickMethod::ButtonAreas => 0,
            ClickMethod::Clickfinger => 1,
        }),
        {
            let model = model.clone();
            move |ix, cx| {
                let method = if ix == 1 {
                    ClickMethod::Clickfinger
                } else {
                    ClickMethod::ButtonAreas
                };
                model.update(cx, |m, cx| m.change(Change::TouchpadClickMethod(method), cx))
            }
        },
        cx,
    );
    rows::toggle(
        form,
        "tp-dwt",
        "Disable while typing",
        "Ignore the touchpad while keys are pressed, so a palm does not move the pointer",
        t.disable_while_typing,
        {
            let model = model.clone();
            move |v, cx| {
                model.update(cx, |m, cx| m.change(Change::TouchpadDisableWhileTyping(v), cx))
            }
        },
        cx,
    );

    rows::heading(form, "Movement");
    rows::number(
        form,
        Number {
            id: "tp-speed".into(),
            label: "Pointer speed",
            description: "Slower on the left, faster on the right",
            value: t.speed,
            min: -1.0,
            max: 1.0,
            step: 0.05,
            unit: None,
            default: 0.0,
        },
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::TouchpadSpeed(v), cx))
        },
        cx,
    );
    rows::choice(
        form,
        "tp-accel",
        "Acceleration",
        "Adaptive speeds the pointer up with the finger; flat keeps it 1:1",
        accel_options(),
        accel_index(t.accel_profile),
        {
            let model = model.clone();
            move |ix, cx| {
                model.update(cx, |m, cx| m.change(Change::TouchpadAccel(accel_from(ix)), cx))
            }
        },
        cx,
    );

    rows::heading(form, "Scrolling");
    rows::toggle(
        form,
        "tp-natural",
        "Natural scrolling",
        "Content follows the fingers, like on a phone",
        t.natural_scroll,
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::TouchpadNaturalScroll(v), cx))
        },
        cx,
    );
    rows::number(
        form,
        Number {
            id: "tp-scroll".into(),
            label: "Scroll speed",
            description: "How far a two-finger swipe scrolls",
            value: t.scroll_speed,
            min: 0.1,
            max: 5.0,
            step: 0.1,
            unit: Some("×"),
            default: 1.0,
        },
        {
            let model = model.clone();
            move |v, cx| model.update(cx, |m, cx| m.change(Change::TouchpadScrollSpeed(v), cx))
        },
        cx,
    );
    footer(form, m);
}
