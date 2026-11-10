//! Right-click menu for a dock item: the app, a new window, its open
//! windows to jump to, pin or unpin, and close.

use gpui::{
    Context, FocusHandle, Focusable, MouseButton, Render, SharedString, Window, div, img,
    prelude::*, px,
};
use ui::{
    ActiveTheme, Color, Divider, Icon, IconName, IconSize, Label, LabelCommon, Spacing, TextSize,
    h_flex, nested_radius, v_flex,
};

use crate::keybinds::Cancel;
use crate::state::AppState;
use crate::windows::{ShellPanel, ShellWindows};

use super::item::DockItem;
use super::{DOCK_CONTEXT_MENU_VERTICAL_PADDING, dock_item_can_be_pinned, toggle_pin};

/// Rows the menu for `item` will have, for sizing its window before it
/// renders: the header, then one per action and one per listed window.
pub(super) fn row_count(item: &DockItem) -> usize {
    let windows = item.windows.len();
    1 + usize::from(item.exec.is_some())
        + if windows > 1 { windows } else { 0 }
        + usize::from(dock_item_can_be_pinned(item))
        + usize::from(windows > 0)
}

/// Window titles are long; the menu shows the start of one.
const TITLE_CHARS: usize = 40;

pub(super) struct DockContextMenu {
    item: DockItem,
    focus_handle: FocusHandle,
}

impl DockContextMenu {
    pub(super) fn new(item: DockItem, cx: &mut Context<Self>) -> Self {
        Self {
            item,
            focus_handle: cx.focus_handle(),
        }
    }

    fn entry(
        &self,
        id: &'static str,
        icon: IconName,
        label: impl Into<SharedString>,
        on_click: impl Fn(&DockItem, &mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors();
        h_flex()
            .id(id)
            .w_full()
            .gap(Spacing::Small.pixels())
            .px(Spacing::Medium.pixels())
            .py(Spacing::Small.pixels())
            .rounded(nested_radius(
                ui::patterns::surface_radius(),
                Spacing::Small.pixels(),
            ))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.element_hover))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    on_click(&this.item, cx);
                    ShellWindows::close_panel(cx);
                }),
            )
            .child(Icon::new(icon).size(IconSize::Small).color(Color::Muted))
            .child(Label::new(label).size(TextSize::Small).truncate())
    }

    /// Jump to one of the app's windows; only listed when there is a choice.
    fn window_entry(
        &self,
        window: &services::Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors();
        let id = window.id.clone();
        let title: String = window.title.chars().take(TITLE_CHARS).collect();
        let title = if title.len() < window.title.len() {
            format!("{title}…")
        } else {
            title
        };
        h_flex()
            .id(SharedString::from(format!("window-{}", window.id)))
            .w_full()
            .gap(Spacing::Small.pixels())
            .px(Spacing::Medium.pixels())
            .py(Spacing::Small.pixels())
            .rounded(nested_radius(
                ui::patterns::surface_radius(),
                Spacing::Small.pixels(),
            ))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.element_hover))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_this, _event, _window, cx| {
                    AppState::compositor(cx)
                        .dispatch(services::CompositorCommand::FocusWindow(id.clone()));
                    ShellWindows::close_panel(cx);
                }),
            )
            .child(
                Icon::new(if window.is_focused {
                    IconName::Check
                } else {
                    IconName::Square
                })
                .size(IconSize::Small)
                .color(if window.is_focused {
                    Color::Accent
                } else {
                    Color::Muted
                }),
            )
            .child(
                Label::new(title)
                    .size(TextSize::Small)
                    .color(if window.is_focused {
                        Color::Default
                    } else {
                        Color::Muted
                    })
                    .truncate(),
            )
    }

    /// The app's icon and name, with how many windows it has open.
    fn header(&self) -> impl IntoElement {
        let item = &self.item;
        let subtitle = match item.windows.len() {
            0 if item.is_pinned => "Pinned, not running".to_string(),
            0 => "Not running".to_string(),
            1 => "1 window".to_string(),
            n => format!("{n} windows"),
        };
        h_flex()
            .w_full()
            .gap(Spacing::Medium.pixels())
            .px(Spacing::Medium.pixels())
            .py(Spacing::Small.pixels())
            .child(
                div()
                    .flex_none()
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(item.icon_path.clone(), |cell, path| {
                        cell.child(img(path).size(px(24.0)))
                    })
                    .when(item.icon_path.is_none(), |cell| {
                        cell.child(
                            Label::new(item.name.chars().take(1).collect::<String>())
                                .size(TextSize::Medium),
                        )
                    }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .child(
                        Label::new(item.name.clone())
                            .size(TextSize::Small)
                            .weight(gpui::FontWeight::SEMIBOLD)
                            .truncate(),
                    )
                    .child(
                        Label::new(subtitle)
                            .size(TextSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
    }
}

/// Start the app again, from what the dock knows of it.
fn launch(item: &DockItem) {
    let Some(exec) = &item.exec else { return };
    services::Application {
        name: item.name.clone(),
        exec: exec.clone(),
        icon: None,
        icon_path: item.icon_path.clone(),
        description: None,
        desktop_file: std::path::PathBuf::from(&item.key),
        startup_wm_class: None,
    }
    .launch();
}

impl Focusable for DockContextMenu {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DockContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus_handle.is_focused(window) {
            self.focus_handle.focus(window, cx);
        }
        let item = self.item.clone();
        let windows = item.windows.len();

        let mut menu = v_flex()
            .id("dock-context-menu")
            .track_focus(&self.focus_handle)
            .key_context("DockContextMenu")
            .on_action(cx.listener(|_this, _: &Cancel, _window, cx| {
                ShellWindows::close_panel(cx);
            }))
            // Only as tall as its rows: the panel window fits itself to this.
            .w_full()
            .shell_panel(cx)
            .p(px(DOCK_CONTEXT_MENU_VERTICAL_PADDING))
            .text_color(cx.theme().colors().text)
            .child(self.header())
            .child(Divider::horizontal());

        if item.exec.is_some() {
            menu = menu.child(self.entry(
                "new-window",
                IconName::Plus,
                "New window",
                |item, _| launch(item),
                cx,
            ));
        }
        if windows > 1 {
            menu = menu.children(
                item.windows
                    .iter()
                    .map(|window| self.window_entry(window, cx)),
            );
        }
        if dock_item_can_be_pinned(&item) {
            let (icon, label) = if item.is_pinned {
                (IconName::PinOff, "Unpin from dock")
            } else {
                (IconName::Pin, "Pin to dock")
            };
            menu = menu.child(self.entry(
                "pin",
                icon,
                label,
                |item, cx| toggle_pin(&item.key, cx),
                cx,
            ));
        }
        if windows > 0 {
            let label = if windows > 1 {
                "Close all windows"
            } else {
                "Close"
            };
            menu = menu.child(self.entry(
                "close",
                IconName::Close,
                label,
                |item, cx| {
                    for window in &item.windows {
                        AppState::compositor(cx)
                            .dispatch(services::CompositorCommand::CloseWindow(window.id.clone()));
                    }
                },
                cx,
            ));
        }
        menu
    }
}
