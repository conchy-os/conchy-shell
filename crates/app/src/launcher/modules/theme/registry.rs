//! The registries page: every scheme provider, what is cached, how fresh it
//! is and the action that clones or pulls it. The state lives on the theme
//! module; this page observes it.

use gpui::{Context, Entity, SharedString};
use ui::{Color, IconName};

use super::{Registry, Source, ThemeModule, format_age};
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{
    Accessory, Content, Detail, EmptyView, Item, List, Metadata, Section,
};
use crate::launcher::page::LauncherPage;

pub struct RegistryPage {
    module: Entity<ThemeModule>,
}

/// A provider's state, worded for the row, the tag and the detail.
struct Status {
    label: &'static str,
    icon: IconName,
    color: Color,
    detail: SharedString,
}

impl RegistryPage {
    pub fn new(module: Entity<ThemeModule>, cx: &mut Context<Self>) -> Self {
        cx.observe(&module, |_, _, cx| cx.notify()).detach();
        Self { module }
    }

    fn item(
        &self,
        registry: &Registry,
        fetching: bool,
        error: Option<SharedString>,
        cx: &Context<Self>,
    ) -> Item {
        let provider = registry.provider;
        let status = status(registry, fetching, error.as_ref());
        let (verb, icon) = if registry.cached {
            ("Update", IconName::Refresh)
        } else {
            ("Download", IconName::Download)
        };

        let mut actions = ActionPanel::new()
            .action(
                Action::listener(verb, cx, move |this, _, cx| {
                    this.module
                        .update(cx, |module, cx| module.fetch(provider, cx));
                    Outcome::Stay
                })
                .icon(icon),
            )
            .action(Action::open(github_url(provider.repo)).title("Open on GitHub"));
        if let Some(error) = &error {
            actions = actions.action(Action::copy(error.clone()).title("Copy Error"));
        }

        Item::new(provider.id, provider.name)
            .subtitle(status.detail.clone())
            .icon(Source::Registry.icon())
            .keywords(provider.repo)
            .accessory(
                Accessory::tag(status.label)
                    .with_icon(status.icon)
                    .color(status.color),
            )
            .detail(detail(registry, &status, error.as_ref()))
            .actions(actions)
    }
}

impl LauncherPage for RegistryPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let module = self.module.read(cx);
        let items: Vec<Item> = module
            .registries
            .iter()
            .map(|registry| {
                let id = registry.provider.id;
                let fetching = module.fetching.contains(id);
                let error = module.fetch_errors.get(id).cloned();
                self.item(registry, fetching, error, cx)
            })
            .collect();
        let schemes: usize = module.registries.iter().map(|r| r.schemes).sum();

        List::new()
            .title("Registries")
            .placeholder("Search registries...")
            .show_detail(true)
            .loading(!module.fetching.is_empty())
            .empty(EmptyView::new("No registries").icon(Source::Registry.icon()))
            .section(
                Section::new("Scheme registries")
                    .subtitle(format!("{schemes} schemes"))
                    .items(items),
            )
            .into()
    }
}

fn status(registry: &Registry, fetching: bool, error: Option<&SharedString>) -> Status {
    let repo = registry.provider.repo;
    match (fetching, error, registry.cached) {
        (true, _, true) => Status {
            label: "Updating…",
            icon: IconName::Refresh,
            color: Color::Muted,
            detail: "Pulling the latest schemes".into(),
        },
        (true, _, false) => Status {
            label: "Cloning…",
            icon: IconName::Download,
            color: Color::Muted,
            detail: format!("Cloning {repo}").into(),
        },
        (false, Some(error), _) => Status {
            label: "Failed",
            icon: IconName::CircleAlert,
            color: Color::Error,
            detail: error.clone(),
        },
        (false, None, true) => {
            let mut detail = format!("{} schemes", registry.schemes);
            if let Some(updated) = registry.updated {
                detail.push_str(&format!(" · updated {}", format_age(updated)));
            }
            Status {
                label: "Downloaded",
                icon: IconName::Check,
                color: Color::Success,
                detail: detail.into(),
            }
        }
        (false, None, false) => Status {
            label: "Not downloaded",
            icon: IconName::Download,
            color: Color::Muted,
            detail: format!("Not downloaded · {repo}").into(),
        },
    }
}

fn detail(registry: &Registry, status: &Status, error: Option<&SharedString>) -> Detail {
    let provider = registry.provider;
    let mut markdown = format!(
        "## {}\n\nBase16 schemes from GitHub, cloned into the shell's cache and read from \
         its {} folder.\n",
        provider.name, provider.subdirectory
    );
    if let Some(error) = error {
        markdown.push_str(&format!("\n> {error}\n"));
    }
    Detail::new()
        .markdown(markdown)
        .metadata(
            Metadata::label("Status", status.label)
                .icon(status.icon)
                .color(status.color),
        )
        .metadata(Metadata::label("Schemes", registry.schemes.to_string()))
        .metadata(Metadata::label(
            "Updated",
            registry.updated.map_or("Never".to_string(), format_age),
        ))
        .metadata(Metadata::Separator)
        .metadata(Metadata::link(
            "Repository",
            provider.repo,
            github_url(provider.repo),
        ))
        .metadata(Metadata::label("Branch", provider.branch))
}

fn github_url(repo: &str) -> String {
    format!("https://github.com/{repo}")
}
