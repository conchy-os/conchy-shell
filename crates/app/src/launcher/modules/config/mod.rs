//! Config module - edit `config.toml` from the launcher.
//!
//! `;cfg` lists the config sections; each field is a row whose Enter action
//! toggles, picks from a list or edits the value in the search line. Edits
//! apply at once and are written when the user saves (Ctrl+S).

mod bar_preview;
mod choice_page;
// Every launcher module keeps its settings in `config.rs`, this one included.
#[allow(clippy::module_inception)]
pub mod config;
mod dock_preview;
mod frame_preview;
mod launcher_preview;
mod layout_page;
mod look_preset;
mod notification_preview;
mod osd_preview;
mod preview;
pub mod schema;
pub mod section_page;
mod sections;
pub mod select;
mod value_page;

use ui::IconName;

use self::config::ConfigEditorConfig;
use self::schema::ConfigPath;
use self::section_page::SectionPage;
use crate::config::Config;
use crate::launcher::page::{ModuleDescriptor, ModuleMeta};

pub fn descriptor(config: &ConfigEditorConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "config",
            prefix: config.prefix.clone().into(),
            name: "Config",
            icon: IconName::Sliders,
            description: "Edit config.toml",
            default: false,
            in_help: true,
        },
        |cx| SectionPage::<Config>::new(ConfigPath::root(), cx),
    )
}
