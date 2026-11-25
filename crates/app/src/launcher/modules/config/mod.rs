//! Config module - edit `shell.kdl` from the launcher.
//!
//! `;cfg` lists the config sections with the selected one's form beside
//! them. Enter or → moves the keyboard into the form, where each setting is
//! toggled, stepped or edited in the search line. Edits apply at once and are
//! written to the file a moment later.

mod about_detail;
pub mod about_page;
mod appearance_model;
pub mod appearance_page;
mod autostart_page;
mod bar_preview;
mod bluetooth_detail;
pub mod bluetooth_page;
mod change_queue;
mod config_page;
// Every launcher module keeps its settings in `config.rs`, this one included.
#[allow(clippy::module_inception)]
pub mod config;
pub mod default_apps_page;
mod displays_model;
pub mod displays_page;
mod dock_preview;
mod form;
mod frame_preview;
mod input_model;
mod keyboard_page;
mod launcher_preview;
mod layout_page;
mod look_preset;
mod mouse_page;
pub mod network_page;
mod notification_preview;
mod osd_preview;
mod preview;
pub mod prompt_page;
pub mod schema;
mod sections;
pub mod select;
pub(in crate::launcher) mod settings_form;
mod shortcuts_detail;
mod shortcuts_page;
pub mod sound_page;
pub(in crate::launcher) mod system;
mod themes_detail;
mod value_page;

use ui::IconName;

use self::config::ConfigEditorConfig;
use self::config_page::ConfigPage;
use crate::launcher::page::{ModuleDescriptor, ModuleMeta};

pub fn descriptor(config: &ConfigEditorConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "config",
            prefix: config.prefix.clone().into(),
            name: "Config",
            icon: IconName::Sliders,
            description: "Edit shell configuration (shell.kdl)",
            default: false,
            in_help: true,
        },
        ConfigPage::new,
    )
}
