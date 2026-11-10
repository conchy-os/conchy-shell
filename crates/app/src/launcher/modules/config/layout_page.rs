//! Arrange the bar: the three slots as sections, each widget a row that can
//! move, change slot or go; a trailing row per slot adds one.

use gpui::{AnyElement, App, AppContext, Context, SharedString, Window, div, prelude::*, px};
use ui::{
    ActiveTheme, ButtonCommon, ButtonStyle, Clickable, Color, Disableable, Icon, IconButton,
    IconName, IconSize, Label, LabelCommon, Spacing, TextSize, Tooltip, h_flex, v_flex,
};

use super::schema::{ConfigPath, to_value};
use super::section_page::{
    file_actions, form_heading, header_card, unsaved_banner, watch_revision,
};
use crate::bar::config::{BarConfig, BarItem};
use crate::bar::modules::{self as widgets, BarWidgetMeta};
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Detail, EmptyView, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Start,
    Center,
    End,
}

impl Slot {
    const ALL: [Slot; 3] = [Slot::Start, Slot::Center, Slot::End];

    fn key(self) -> &'static str {
        match self {
            Slot::Start => "start",
            Slot::Center => "center",
            Slot::End => "end",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Slot::Start => "Start",
            Slot::Center => "Center",
            Slot::End => "End",
        }
    }

    fn items(self, bar: &BarConfig) -> &Vec<BarItem> {
        match self {
            Slot::Start => &bar.start,
            Slot::Center => &bar.center,
            Slot::End => &bar.end,
        }
    }

    fn items_mut(self, bar: &mut BarConfig) -> &mut Vec<BarItem> {
        match self {
            Slot::Start => &mut bar.start,
            Slot::Center => &mut bar.center,
            Slot::End => &mut bar.end,
        }
    }
}

pub struct LayoutPage {
    path: ConfigPath<BarConfig>,
}

impl LayoutPage {
    pub fn open(path: ConfigPath<BarConfig>, cx: &mut App) -> Page {
        Page::new(cx.new(|cx| {
            watch_revision(cx);
            Self { path }
        }))
    }
}

/// Change one or more slots and record each as its own key.
fn edit_slots(
    path: &ConfigPath<BarConfig>,
    slots: &[Slot],
    change: impl FnOnce(&mut BarConfig),
    cx: &mut App,
) {
    let mut bar = path.read(cx).clone();
    change(&mut bar);
    for slot in slots {
        let value = to_value(slot.items(&bar));
        path.set_raw(path.keys_for(slot.key()), value, bar.clone(), cx);
    }
}

fn item_label(item: &BarItem) -> String {
    item.names()
        .iter()
        .map(|name| {
            widgets::find(name)
                .map(|widget| widget.meta.label.to_string())
                .unwrap_or_else(|| name.clone())
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn item_icon(item: &BarItem) -> IconName {
    match item {
        BarItem::Widget(name) => widgets::find(name)
            .map(|widget| widget.meta.icon)
            .unwrap_or(IconName::Hash),
        BarItem::Group(_) => IconName::Layers,
    }
}

impl LayoutPage {
    fn widget_row(&self, slot: Slot, ix: usize, item: &BarItem, len: usize) -> Item {
        let path = self.path.clone();
        let unknown = item
            .names()
            .iter()
            .any(|name| widgets::find(name).is_none());
        let mut row = Item::new(
            format!("{}:{ix}:{}", slot.key(), item.names().join("+")),
            item_label(item),
        )
        .icon(item_icon(item));
        row = match item {
            BarItem::Group(_) => row.subtitle("bubble group"),
            BarItem::Widget(name) => row.subtitle(name.clone()),
        };
        if unknown {
            row = row.accessory(Accessory::tag("unknown").color(Color::Warning));
        }

        let mut panel = ActionPanel::new();
        if ix > 0 {
            let path = path.clone();
            panel = panel.action(
                Action::new("Move up", move |_, cx| {
                    edit_slots(
                        &path,
                        &[slot],
                        |bar| slot.items_mut(bar).swap(ix, ix - 1),
                        cx,
                    );
                    Outcome::Stay
                })
                .icon(IconName::ArrowUp)
                .shortcut("alt-up"),
            );
        }
        if ix + 1 < len {
            let path = path.clone();
            panel = panel.action(
                Action::new("Move down", move |_, cx| {
                    edit_slots(
                        &path,
                        &[slot],
                        |bar| slot.items_mut(bar).swap(ix, ix + 1),
                        cx,
                    );
                    Outcome::Stay
                })
                .icon(IconName::ArrowDown)
                .shortcut("alt-down"),
            );
        }
        let moves = Slot::ALL.into_iter().filter(|s| *s != slot).map(|target| {
            let path = path.clone();
            Action::new(
                format!("Move to {}", target.label().to_lowercase()),
                move |_, cx| {
                    edit_slots(
                        &path,
                        &[slot, target],
                        |bar| {
                            let item = slot.items_mut(bar).remove(ix);
                            target.items_mut(bar).push(item);
                        },
                        cx,
                    );
                    Outcome::Stay
                },
            )
        });
        panel = panel.section("Move to", moves);
        if let BarItem::Group(_) = item {
            let path = path.clone();
            panel = panel.action(
                Action::new("Ungroup", move |_, cx| {
                    edit_slots(
                        &path,
                        &[slot],
                        |bar| {
                            let items = slot.items_mut(bar);
                            if let BarItem::Group(names) = items.remove(ix) {
                                for (offset, name) in names.into_iter().enumerate() {
                                    items.insert(ix + offset, BarItem::Widget(name));
                                }
                            }
                        },
                        cx,
                    );
                    Outcome::Stay
                })
                .icon(IconName::Layers),
            );
        }
        panel = panel.action(
            Action::new("Remove", move |_, cx| {
                edit_slots(
                    &path,
                    &[slot],
                    |bar| {
                        slot.items_mut(bar).remove(ix);
                    },
                    cx,
                );
                Outcome::Stay
            })
            .icon(IconName::Trash)
            .shortcut("ctrl-x")
            .destructive(),
        );
        row.actions(file_actions(panel))
            .detail(layout_detail(&self.path))
    }

    fn add_row(&self, slot: Slot) -> Item {
        let path = self.path.clone();
        Item::new(format!("{}:add", slot.key()), "Add widget…")
            .icon(IconName::Plus)
            .subtitle(format!(
                "Append to the {} slot",
                slot.label().to_lowercase()
            ))
            .actions(file_actions(
                ActionPanel::new().action(
                    Action::new("Choose a widget", move |_, cx| {
                        Outcome::push(cx.new(|cx| AddWidgetPage::new(path.clone(), slot, cx)))
                    })
                    .icon(IconName::ChevronRight),
                ),
            ))
            .detail(layout_detail(&self.path))
    }
}

fn layout_detail(path: &ConfigPath<BarConfig>) -> Detail {
    let path = path.clone();
    Detail::new().widget(move |cx| layout_form(&path, cx))
}

/// The whole layout as a mouse form: every slot, every widget with its
/// move and remove buttons, and a `+` per widget kind to append one.
fn layout_form(path: &ConfigPath<BarConfig>, cx: &App) -> AnyElement {
    let bar = path.read(cx).clone();
    let colors = cx.theme().colors();
    let mut column = v_flex().w_full().gap(Spacing::Large.pixels());
    for (slot_ix, slot) in Slot::ALL.into_iter().enumerate() {
        if slot_ix > 0 {
            column = column.child(div().w_full().h(px(1.)).bg(colors.border_variant));
        }
        let items = slot.items(&bar);
        let mut block = v_flex()
            .w_full()
            .gap(Spacing::XSmall.pixels())
            .child(form_heading(slot.label()));
        for (ix, item) in items.iter().enumerate() {
            block = block.child(layout_row(path, slot, ix, item, items.len()));
        }
        let adds = widgets::widgets().map(|kind| {
            let path = path.clone();
            IconButton::new(
                SharedString::from(format!("add-{}-{}", slot.key(), kind.name)),
                kind.icon,
            )
            .icon_size(IconSize::Small)
            .style(ButtonStyle::Ghost)
            .tooltip(Tooltip::text(format!("Add {}", kind.label.to_lowercase())))
            .on_click(move |_, _, cx| {
                edit_slots(
                    &path,
                    &[slot],
                    |bar| slot.items_mut(bar).push(BarItem::Widget(kind.name.into())),
                    cx,
                );
            })
        });
        block = block.child(
            h_flex()
                .w_full()
                .flex_wrap()
                .items_center()
                .gap(Spacing::XXSmall.pixels())
                .pt(Spacing::XSmall.pixels())
                .child(
                    Icon::new(IconName::Plus)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .children(adds),
        );
        column = column.child(block);
    }
    column.into_any_element()
}

fn layout_row(
    path: &ConfigPath<BarConfig>,
    slot: Slot,
    ix: usize,
    item: &BarItem,
    len: usize,
) -> AnyElement {
    let id = format!("{}-{ix}", slot.key());
    let button = |suffix: &str, icon: IconName, tip: String| {
        IconButton::new(SharedString::from(format!("{id}-{suffix}")), icon)
            .icon_size(IconSize::Small)
            .style(ButtonStyle::Ghost)
            .tooltip(Tooltip::text(tip))
    };
    let swap = |path: ConfigPath<BarConfig>, other: usize| {
        move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut App| {
            edit_slots(
                &path,
                &[slot],
                |bar| slot.items_mut(bar).swap(ix, other),
                cx,
            );
        }
    };
    let relocate = |path: ConfigPath<BarConfig>, target: Slot| {
        move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut App| {
            edit_slots(
                &path,
                &[slot, target],
                |bar| {
                    let item = slot.items_mut(bar).remove(ix);
                    target.items_mut(bar).push(item);
                },
                cx,
            );
        }
    };
    let previous = Slot::ALL
        .iter()
        .position(|s| *s == slot)
        .and_then(|i| i.checked_sub(1).map(|i| Slot::ALL[i]));
    let next = Slot::ALL
        .iter()
        .position(|s| *s == slot)
        .and_then(|i| Slot::ALL.get(i + 1).copied());
    let remove = {
        let path = path.clone();
        move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut App| {
            edit_slots(
                &path,
                &[slot],
                |bar| {
                    slot.items_mut(bar).remove(ix);
                },
                cx,
            );
        }
    };

    h_flex()
        .w_full()
        .items_center()
        .gap(Spacing::Medium.pixels())
        .child(
            h_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(Spacing::Small.pixels())
                .child(
                    Icon::new(item_icon(item))
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(item_label(item))
                        .size(TextSize::Small)
                        .truncate(),
                ),
        )
        .child(
            h_flex()
                .flex_none()
                .gap(Spacing::XXSmall.pixels())
                .child(
                    button("up", IconName::ArrowUp, "Move up".into())
                        .disabled(ix == 0)
                        .on_click(swap(path.clone(), ix.saturating_sub(1))),
                )
                .child(
                    button("down", IconName::ArrowDown, "Move down".into())
                        .disabled(ix + 1 >= len)
                        .on_click(swap(path.clone(), (ix + 1).min(len - 1))),
                )
                .child(
                    button(
                        "prev",
                        IconName::ChevronLeft,
                        previous.map_or("First slot".into(), |s| {
                            format!("Move to {}", s.label().to_lowercase())
                        }),
                    )
                    .disabled(previous.is_none())
                    .on_click(relocate(path.clone(), previous.unwrap_or(slot))),
                )
                .child(
                    button(
                        "next",
                        IconName::ChevronRight,
                        next.map_or("Last slot".into(), |s| {
                            format!("Move to {}", s.label().to_lowercase())
                        }),
                    )
                    .disabled(next.is_none())
                    .on_click(relocate(path.clone(), next.unwrap_or(slot))),
                )
                // Muted like the control center's forget and remove buttons;
                // the trash glyph carries the meaning.
                .child(button("remove", IconName::Trash, "Remove".into()).on_click(remove)),
        )
        .into_any_element()
}

impl LauncherPage for LayoutPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let bar = self.path.read(cx).clone();
        let sections = Slot::ALL.into_iter().map(|slot| {
            let items = slot.items(&bar);
            let rows = items
                .iter()
                .enumerate()
                .map(|(ix, item)| self.widget_row(slot, ix, item, items.len()))
                .chain([self.add_row(slot)]);
            Section::new(slot.label()).items(rows)
        });
        let mut list = List::new()
            .filtering(false)
            .title("Bar layout")
            .placeholder("Alt+Up/Down moves, Ctrl+X removes")
            .actions(file_actions(ActionPanel::new()));
        for section in sections {
            list = list.section(section);
        }
        list.into()
    }

    fn render_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let bar = self.path.read(cx);
        let summary = format!(
            "Start {} · Center {} · End {} · {} bar",
            bar.start.len(),
            bar.center.len(),
            bar.end.len(),
            bar.style.label()
        );
        let colors = cx.theme().colors();
        let card = header_card(
            h_flex()
                .w_full()
                .items_center()
                .gap(Spacing::Medium.pixels())
                .bg(colors.surface_background)
                .border_color(colors.border_variant)
                .child(
                    Icon::new(IconName::Layout)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(summary)
                        .size(TextSize::Small)
                        .color(Color::Muted)
                        .truncate(),
                ),
        );
        let banner = unsaved_banner(cx);
        Some(v_flex().children(banner).child(card).into_any_element())
    }
}

/// Pick a widget to append to one slot.
pub struct AddWidgetPage {
    path: ConfigPath<BarConfig>,
    slot: Slot,
}

impl AddWidgetPage {
    fn new(path: ConfigPath<BarConfig>, slot: Slot, cx: &mut Context<Self>) -> Self {
        watch_revision(cx);
        Self { path, slot }
    }
}

impl LauncherPage for AddWidgetPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let bar = self.path.read(cx);
        let placed: Vec<&BarWidgetMeta> = Slot::ALL
            .iter()
            .flat_map(|slot| slot.items(bar).iter())
            .flat_map(|item| item.names().iter())
            .filter_map(|name| widgets::find(name).map(|widget| &widget.meta))
            .collect();
        let slot = self.slot;
        let items = widgets::widgets().map(|kind| {
            let path = self.path.clone();
            let mut item = Item::new(kind.name, kind.label)
                .subtitle(kind.description)
                .icon(kind.icon)
                .keywords(kind.name)
                .actions(file_actions(ActionPanel::new().action(Action::new(
                    "Add",
                    move |_, cx| {
                        edit_slots(
                            &path,
                            &[slot],
                            |bar| slot.items_mut(bar).push(BarItem::Widget(kind.name.into())),
                            cx,
                        );
                        Outcome::Pop
                    },
                ))));
            if placed.contains(&kind) {
                item = item.accessory(Accessory::tag("placed"));
            }
            item
        });
        List::new()
            .title(format!("Add to {}", slot.label().to_lowercase()))
            .placeholder("Search widgets…")
            .empty(EmptyView::new("No such widget").icon(IconName::Layout))
            .actions(file_actions(ActionPanel::new()))
            .section(Section::untitled().items(items))
            .into()
    }

    fn render_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        unsaved_banner(cx)
    }
}
