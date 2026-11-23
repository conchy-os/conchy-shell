//! The "System" half of the settings list: network, sound, mouse, displays and
//! the rest, which are not in `shell.kdl`. Every one is a row of the same list
//! with its form beside it, and Enter moves the keyboard into that form, like
//! for the shell's own sections: there are no pages to open.
//!
//! The state the forms read (what the hardware settings are now, what the
//! autostart folder holds) lives here, owned by the config page, and the page
//! redraws when any of it changes.

use std::rc::Rc;

use gpui::{App, AppContext, Context, Entity};
use ui::IconName;

use super::about_detail::{SystemInfo, get_system_info};
use super::appearance_model::AppearanceModel;
use super::autostart_page::AutostartPage;
use super::default_apps_page::DefaultAppsPage;
use super::displays_model::DisplayModel;
use super::input_model::InputModel;
use super::{
    about_page, appearance_page, autostart_page, bluetooth_page, default_apps_page, displays_page,
    keyboard_page, mouse_page, network_page, sound_page,
};
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::Section;
use crate::state::{AppState, watch};

/// What Enter does on a row of the list: the keyboard goes into its form.
pub fn edit() -> ActionPanel {
    ActionPanel::new().action(Action::new("Edit", |_, _| Outcome::FocusDetail).icon(IconName::ChevronRight))
}

pub struct System {
    input: Entity<InputModel>,
    appearance: Entity<AppearanceModel>,
    displays: Entity<DisplayModel>,
    autostart: Entity<AutostartPage>,
    default_apps: Entity<DefaultAppsPage>,
    about: Rc<SystemInfo>,
}

impl System {
    pub fn new<V: 'static>(cx: &mut Context<V>) -> Self {
        let input = cx.new(InputModel::new);
        cx.observe(&input, |_, _, cx| cx.notify()).detach();
        let appearance = cx.new(AppearanceModel::new);
        cx.observe(&appearance, |_, _, cx| cx.notify()).detach();
        let displays = cx.new(DisplayModel::new);
        cx.observe(&displays, |_, _, cx| cx.notify()).detach();
        let autostart = cx.new(AutostartPage::new);
        cx.observe(&autostart, |_, _, cx| cx.notify()).detach();
        let default_apps = cx.new(|cx| DefaultAppsPage::new(cx));
        cx.observe(&default_apps, |_, _, cx| cx.notify()).detach();

        let network = AppState::network(cx);
        watch(cx, network.subscribe(), |_, _, cx| cx.notify());
        let bluetooth = AppState::bluetooth(cx);
        watch(cx, bluetooth.subscribe(), |_, _, cx| cx.notify());
        let audio = AppState::audio(cx);
        watch(cx, audio.subscribe(), |_, _, cx| cx.notify());

        Self {
            input,
            appearance,
            displays,
            autostart,
            default_apps,
            about: Rc::new(get_system_info()),
        }
    }

    /// The sections, in the order they are listed.
    pub fn sections(&self, cx: &App) -> Vec<Section> {
        let mut sections = vec![
            Section::new("Connections").items([network_page::item(), bluetooth_page::item()]),
        ];

        let mut devices = vec![sound_page::item()];
        devices.extend(mouse_page::items(&self.input, cx));
        devices.extend(keyboard_page::items(&self.input, cx));
        sections.push(Section::new("Devices").items(devices));

        sections.extend(displays_page::sections(&self.displays, cx));
        sections.push(
            Section::new("Appearance").items(appearance_page::items(&self.appearance, cx)),
        );
        sections.push(Section::new("System").items([
            default_apps_page::item(&self.default_apps),
            autostart_page::item(&self.autostart),
            about_page::item(&self.about),
        ]));
        sections
    }
}
