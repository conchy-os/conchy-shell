//! Workspaces module for switching between compositor workspaces.

pub mod config;

use gpui::Context;
use services::CompositorCommand;
use ui::IconName;

use self::config::WorkspacesConfig;
use crate::launcher::action::{Action, Outcome};
use crate::launcher::content::{Content, Item, List};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};
use crate::state::{AppState, watch};

const ICON: IconName = IconName::Layout;

pub fn descriptor(config: &WorkspacesConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "workspaces",
            prefix: config.prefix.clone().into(),
            name: "Workspaces",
            icon: ICON,
            description: "Switch between workspaces",
            default: false,
            in_help: true,
        },
        WorkspacesModule::new,
    )
}

pub struct WorkspacesModule;

impl WorkspacesModule {
    fn new(cx: &mut Context<Self>) -> Self {
        let compositor = AppState::compositor(cx).subscribe();
        watch(cx, compositor, |_, _, cx| cx.notify());
        Self
    }
}

impl LauncherPage for WorkspacesModule {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let compositor = AppState::compositor(cx).get();
        List::new()
            .items(
                compositor
                    .workspaces
                    .into_iter()
                    .filter(|ws| !ws.is_special)
                    .map(|ws| {
                        let title = if ws.name.is_empty() {
                            format!("Workspace {}", ws.id)
                        } else {
                            ws.name.clone()
                        };
                        let id = ws.id;
                        Item::new(ws.id.to_string(), title)
                            .subtitle(format!("{} windows on {}", ws.windows, ws.monitor))
                            .keywords(ws.monitor)
                            .icon(ICON)
                            .action(Action::new("Switch", move |_, cx| {
                                AppState::compositor(cx)
                                    .dispatch(CompositorCommand::FocusWorkspace(id));
                                Outcome::Close
                            }))
                    }),
            )
            .into()
    }
}
