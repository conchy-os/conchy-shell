//! The select every choice field uses: one outlined trigger as wide as the
//! form's control column, which opens a popover of the options under it.
//!
//! Forms are rebuilt every frame from plain functions, so the one open
//! select and the trigger bounds its popover anchors to live outside the
//! element tree: the open one in a global, the bounds in a thread-local map
//! the triggers write during layout.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, App, Bounds, Global, Pixels, SharedString, canvas, div, prelude::*, px,
};
use ui::{
    ActiveTheme, ButtonCommon, ButtonLike, ButtonStyle, Clickable, Color, Icon, IconName, IconSize,
    Label, LabelCommon, Popover, SelectableButton, Spacing, TextSize, Toggleable, anchored_popover,
    h_flex, v_flex,
};

/// Width of a select's trigger: the form's control column, so every
/// select lines up with the next.
pub const SELECT_WIDTH: f32 = 176.;

type Pick = Rc<dyn Fn(usize, &mut App)>;

#[derive(Default)]
struct OpenSelect(Option<SharedString>);

impl Global for OpenSelect {}

thread_local! {
    static TRIGGER_BOUNDS: RefCell<HashMap<SharedString, Bounds<Pixels>>> = RefCell::new(HashMap::new());
}

fn open_id(cx: &App) -> Option<SharedString> {
    cx.try_global::<OpenSelect>()
        .and_then(|open| open.0.clone())
}

fn set_open(id: Option<SharedString>, cx: &mut App) {
    cx.set_global(OpenSelect(id));
    cx.refresh_windows();
}

/// Close the open select, if any; whether there was one. The launcher
/// calls this on Escape and when it hides, so a select never outlives
/// the page it was opened on.
pub fn close(cx: &mut App) -> bool {
    if open_id(cx).is_none() {
        return false;
    }
    set_open(None, cx);
    true
}

/// A select showing `options[selected]`, or "Custom" when nothing matches;
/// `on_pick` gets the index chosen from the popover.
pub fn select(
    id: SharedString,
    options: Vec<SharedString>,
    selected: Option<usize>,
    on_pick: impl Fn(usize, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors();
    let open = open_id(cx).as_ref() == Some(&id);
    let label = selected
        .and_then(|ix| options.get(ix).cloned())
        .unwrap_or_else(|| "Custom".into());

    // Quiet while closed, the accent outline while open, like a focused field.
    let trigger = ButtonLike::new(id.clone())
        .style(ButtonStyle::OutlinedGhost)
        .selected_style(ButtonStyle::Outlined)
        .width(px(SELECT_WIDTH))
        .padding(Spacing::Small.pixels(), Spacing::XXSmall.pixels())
        .toggle_state(open)
        .on_click({
            let id = id.clone();
            move |_, _, cx| {
                let next = if open_id(cx).as_ref() == Some(&id) {
                    None
                } else {
                    Some(id.clone())
                };
                set_open(next, cx);
            }
        })
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap(Spacing::Small.pixels())
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .child(Label::new(label).size(TextSize::Small).truncate()),
                )
                .child(
                    Icon::new(if open {
                        IconName::ChevronUp
                    } else {
                        IconName::ChevronDown
                    })
                    .size(IconSize::Small)
                    .color(Color::Muted),
                ),
        );

    // The trigger's bounds, for the popover to hang from next frame.
    let bounds_probe = {
        let id = id.clone();
        canvas(
            move |bounds, _, _| {
                TRIGGER_BOUNDS.with(|map| map.borrow_mut().insert(id.clone(), bounds));
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
        .size_full()
    };
    let mut root = div()
        .relative()
        .w(px(SELECT_WIDTH))
        .flex_none()
        .child(trigger)
        .child(bounds_probe);

    let anchor = TRIGGER_BOUNDS.with(|map| map.borrow().get(&id).copied());
    if let (true, Some(anchor)) = (open, anchor) {
        let on_pick: Pick = Rc::new(on_pick);
        let rows = options.into_iter().enumerate().map(|(ix, option)| {
            let picked = selected == Some(ix);
            let on_pick = on_pick.clone();
            h_flex()
                .id(ix)
                .w_full()
                .items_center()
                .gap(Spacing::Small.pixels())
                .px(Spacing::Medium.pixels())
                .py(px(4.))
                .cursor_pointer()
                .hover(move |style| style.bg(colors.ghost_element_hover))
                .on_click(move |_, _, cx| {
                    set_open(None, cx);
                    on_pick(ix, cx);
                })
                .child(div().w(px(16.)).flex_none().when(picked, |slot| {
                    slot.child(
                        Icon::new(IconName::Check)
                            .size(IconSize::Small)
                            .color(Color::Accent),
                    )
                }))
                .child(
                    Label::new(option)
                        .size(TextSize::Small)
                        .color(if picked {
                            Color::Accent
                        } else {
                            Color::Default
                        })
                        .truncate(),
                )
        });
        let popover = Popover::new()
            .min_width(px(SELECT_WIDTH))
            .child(v_flex().w(px(SELECT_WIDTH)).children(rows));
        // The launcher keeps the keyboard focus, so Escape reaches this
        // through [`close`]; the overlay itself closes on a click outside.
        let focus = cx.focus_handle();
        root = root.child(anchored_popover(
            focus,
            Anchor::TopRight,
            anchor,
            popover,
            |_, cx| set_open(None, cx),
        ));
    }
    root.into_any_element()
}
