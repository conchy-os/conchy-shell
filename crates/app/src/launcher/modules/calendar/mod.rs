//! Calendar launcher module for quick date lookup and opening the calendar panel.

pub mod config;

use gpui::{Context, IntoElement, Render, Window, div, prelude::*};
use ui::{IconName, Spacing};

use self::config::CalendarConfig;
use crate::calendar::CalendarPanel;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Content, Custom};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};
use crate::windows::ShellWindows;

pub fn descriptor(config: &CalendarConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "calendar",
            prefix: config.prefix.clone().into(),
            name: "Calendar",
            icon: IconName::Calendar,
            description: "View calendar and date",
            default: false,
            in_help: true,
        },
        CalendarModule::new,
    )
}

pub struct CalendarModule {
    calendar: gpui::Entity<CalendarPanel>,
}

impl CalendarModule {
    fn new(cx: &mut Context<Self>) -> Self {
        let calendar = cx.new(CalendarPanel::new);
        Self { calendar }
    }
}

impl LauncherPage for CalendarModule {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let actions = ActionPanel::new().action(
            Action::new("Open in Panel", move |window, cx| {
                let display_id = ShellWindows::display_for_window(cx, window);
                crate::calendar::toggle_on_display(display_id, cx);
                Outcome::Close
            })
            .icon(IconName::Calendar),
        );
        Custom::new(cx.entity()).actions(actions).into()
    }
}

impl Render for CalendarModule {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(Spacing::Large.pixels())
            .flex()
            .justify_center()
            .child(self.calendar.clone())
    }
}
