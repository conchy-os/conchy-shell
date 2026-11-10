//! Applications module for searching and launching desktop applications.

pub mod config;

use gpui::Context;
use services::Application;
use ui::IconName;

use self::config::AppsConfig;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Content, Detail, Item, ItemIcon, List, Metadata};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};
use crate::state::AppState;

const ICON: IconName = IconName::Layers;

pub fn descriptor(config: &AppsConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "apps",
            prefix: config.prefix.clone().into(),
            name: "Applications",
            icon: ICON,
            description: "Search and launch applications",
            default: true,
            in_help: true,
        },
        |_| AppsModule,
    )
}

pub struct AppsModule;

impl LauncherPage for AppsModule {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let apps = AppState::applications(cx).all();
        List::new()
            .items(apps.iter().map(|app| {
                let icon = match &app.icon_path {
                    Some(path) => ItemIcon::Path(path.clone()),
                    None => ICON.into(),
                };
                let item = Item::new(
                    app.desktop_file.to_string_lossy().to_string(),
                    app.name.clone(),
                )
                .icon(icon)
                .actions(actions(app));
                match &app.description {
                    Some(description) => item.subtitle(description.clone()),
                    None => item,
                }
            }))
            .into()
    }
}

fn actions(app: &Application) -> ActionPanel {
    let launch = app.clone();
    let shown = app.clone();
    ActionPanel::new()
        .action(
            Action::new("Open", move |_, _| {
                launch.launch();
                Outcome::Close
            })
            .icon(IconName::ExternalLink),
        )
        .action(Action::push_detail("Show Details", move |_| detail(&shown)).shortcut("ctrl-i"))
        .section(
            "Copy",
            [
                Action::copy(app.exec.clone())
                    .title("Copy Command")
                    .shortcut("ctrl-shift-c"),
                Action::copy(app.desktop_file.to_string_lossy().to_string())
                    .title("Copy Desktop File Path"),
            ],
        )
}

fn detail(app: &Application) -> Detail {
    let mut markdown = String::new();
    if let Some(path) = &app.icon_path {
        markdown.push_str(&format!("![icon]({})\n\n", path.display()));
    }
    markdown.push_str(&format!("# {}\n", app.name));
    if let Some(description) = &app.description {
        markdown.push_str(&format!("\n{description}\n"));
    }
    let launch = app.clone();
    let mut detail = Detail::new()
        .title(app.name.clone())
        .markdown(markdown)
        .metadata(Metadata::label("Command", app.exec.clone()))
        .metadata(Metadata::label(
            "Desktop file",
            app.desktop_file.to_string_lossy().to_string(),
        ))
        .actions(
            ActionPanel::new()
                .action(Action::new("Open", move |_, _| {
                    launch.launch();
                    Outcome::Close
                }))
                .action(Action::copy(app.exec.clone()).title("Copy Command")),
        );
    if let Some(class) = &app.startup_wm_class {
        detail = detail.metadata(Metadata::label("Window class", class.clone()));
    }
    detail
}
