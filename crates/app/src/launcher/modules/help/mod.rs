//! Help module listing every other module by prefix.

pub mod config;

use gpui::Context;
use ui::IconName;

use self::config::HelpConfig;
use crate::launcher::action::{Action, Outcome};
use crate::launcher::content::{Accessory, Content, Item, List, Section};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};

pub fn descriptor(config: &HelpConfig, modules: Vec<ModuleMeta>) -> ModuleDescriptor {
    let entries: Vec<ModuleMeta> = modules.into_iter().filter(|meta| meta.in_help).collect();
    ModuleDescriptor::new(
        ModuleMeta {
            id: "help",
            prefix: config.prefix.clone().into(),
            name: "Help",
            icon: IconName::CircleHelp,
            description: "Show available commands",
            default: false,
            in_help: false,
        },
        move |_| HelpModule {
            entries: entries.clone(),
        },
    )
}

pub struct HelpModule {
    entries: Vec<ModuleMeta>,
}

impl LauncherPage for HelpModule {
    fn content(&mut self, _query: &str, _cx: &mut Context<Self>) -> Content {
        List::new()
            .section(
                Section::new("Commands").items(self.entries.iter().map(|meta| {
                    let query = format!("{} ", meta.prefix);
                    Item::new(meta.prefix.clone(), meta.name)
                        .subtitle(meta.description)
                        .keywords(meta.prefix.clone())
                        .icon(meta.icon)
                        .accessory(Accessory::tag(meta.prefix.clone()))
                        .action(Action::new("Open", move |_, _| {
                            Outcome::SetQuery(query.clone())
                        }))
                })),
            )
            .into()
    }
}
