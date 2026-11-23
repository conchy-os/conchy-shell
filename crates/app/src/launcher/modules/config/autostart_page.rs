//! Startup applications: what the session starts at login, with a switch for
//! each, and a way to add programs of one's own.

use gpui::{App, AppContext, Context};
use services::autostart::{Autostart, Entry, Off, Origin};
use ui::{Color, IconName};

use super::prompt_page::PromptPage;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Detail, EmptyView, Item, List, Metadata, Section};
use crate::launcher::page::{LauncherPage, Page};

pub struct AutostartPage {
    autostart: Option<Autostart>,
    entries: Vec<Entry>,
    /// What went wrong, or what was done, last.
    notice: Option<(String, bool)>,
    error: Option<String>,
}

impl AutostartPage {
    pub fn open(cx: &mut App) -> Page {
        Page::new(cx.new(Self::new))
    }

    pub fn new(_cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            autostart: None,
            entries: Vec::new(),
            notice: None,
            error: None,
        };
        match Autostart::standard() {
            Ok(autostart) => page.autostart = Some(autostart),
            Err(err) => page.error = Some(format!("{err:#}")),
        }
        page.reload();
        page
    }

    fn reload(&mut self) {
        if let Some(autostart) = &self.autostart {
            self.entries = autostart.list();
        }
    }

    fn toggle(&mut self, id: &str, enabled: bool, cx: &mut Context<Self>) {
        let Some(autostart) = &self.autostart else { return };
        self.notice = match autostart.set_enabled(id, enabled) {
            Ok(()) => None,
            Err(err) => Some((format!("{err:#}"), true)),
        };
        self.reload();
        cx.notify();
    }

    fn remove(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(autostart) = &self.autostart else { return };
        self.notice = match autostart.remove(id) {
            Ok(()) => Some(("Removed from startup.".into(), false)),
            Err(err) => Some((format!("{err:#}"), true)),
        };
        self.reload();
        cx.notify();
    }

    fn add(&mut self, command: &str, cx: &mut Context<Self>) {
        let Some(autostart) = &self.autostart else { return };
        let name = program_name(command);
        self.notice = match autostart.add(&name, command) {
            Ok(_) => Some((format!("{name} will start at login."), false)),
            Err(err) => Some((format!("{err:#}"), true)),
        };
        self.reload();
        cx.notify();
    }
}

/// `~/bin/my-tool --flag` → `My tool`.
fn program_name(command: &str) -> String {
    let program = command.split_whitespace().next().unwrap_or(command);
    let base = program.rsplit('/').next().unwrap_or(program);
    let spaced = base.replace(['-', '_'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn state_tag(entry: &Entry) -> Accessory {
    match entry.off {
        None => Accessory::tag("On").color(Color::Success),
        Some(Off::Switched) => Accessory::tag("Off").color(Color::Muted),
        Some(Off::OtherDesktop) => Accessory::tag("Other desktop").color(Color::Muted),
        Some(Off::NotInstalled) => Accessory::tag("Not installed").color(Color::Warning),
    }
}

fn detail_of(entry: &Entry) -> Detail {
    let mut detail = Detail::new()
        .markdown(format!(
            "# {}\n\n{}",
            entry.name,
            if entry.comment.is_empty() {
                "Started when the session begins."
            } else {
                entry.comment.as_str()
            }
        ))
        .metadata(Metadata::label("Command", entry.exec.clone()))
        .metadata(Metadata::label(
            "Status",
            match entry.off {
                None => "Starts at login",
                Some(Off::Switched) => "Switched off",
                Some(Off::OtherDesktop) => "Not for this desktop (OnlyShowIn / NotShowIn)",
                Some(Off::NotInstalled) => "The program is not installed (TryExec)",
            },
        ))
        .metadata(Metadata::label(
            "From",
            match entry.origin {
                Origin::User if entry.shadows_system => "Your override of a system entry",
                Origin::User => "Your autostart folder",
                Origin::System => "The system (off here means a copy in your folder)",
            },
        ))
        .metadata(Metadata::label("File", entry.path.display().to_string()));
    if let Some(icon) = &entry.icon {
        detail = detail.metadata(Metadata::label("Icon", icon.clone()));
    }
    detail
}

impl LauncherPage for AutostartPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let add = Action::listener("Add program…", cx, |_, _, cx| {
            let page = cx.entity();
            PromptPage::ask(
                cx,
                "Add startup program",
                "Command to run at login…",
                "Start",
                String::new(),
                |command| {
                    if command.contains('\n') {
                        Err("A command is one line".into())
                    } else {
                        Ok(())
                    }
                },
                move |command, cx| {
                    page.update(cx, |page, cx| page.add(&command, cx));
                    Outcome::Pop
                },
            )
        })
        .icon(IconName::Plus)
        .shortcut("ctrl-n");

        let mut list = List::new()
            .show_detail(true)
            .click_selects(true)
            .title("Startup applications")
            .placeholder("Search startup applications…")
            .actions(ActionPanel::new().action(add.clone()));

        if let Some(err) = &self.error {
            return list
                .empty(
                    EmptyView::new("Cannot find the autostart folders")
                        .description(err.clone())
                        .icon(IconName::Warning),
                )
                .into();
        }

        if let Some((text, error)) = &self.notice {
            list = list.section(
                Section::untitled().item(
                    Item::new("notice", text.clone())
                        .icon(if *error { IconName::Warning } else { IconName::Info }),
                ),
            );
        }

        let item_of = |entry: &Entry, cx: &Context<Self>| {
            let id = entry.id.clone();
            let enabled = entry.enabled();
            let subtitle = if entry.comment.is_empty() {
                entry.exec.clone()
            } else {
                entry.comment.clone()
            };
            let mut actions = ActionPanel::new();
            if entry.switchable() {
                let id = id.clone();
                actions = actions.action(
                    Action::listener(
                        if enabled { "Don't start at login" } else { "Start at login" },
                        cx,
                        move |page, _, cx| {
                            page.toggle(&id, !enabled, cx);
                            Outcome::Stay
                        },
                    )
                    .icon(if enabled { IconName::Stop } else { IconName::Play }),
                );
            }
            if entry.removable() {
                let id = id.clone();
                actions = actions.action(
                    Action::listener("Remove from startup", cx, move |page, _, cx| {
                        page.remove(&id, cx);
                        Outcome::Stay
                    })
                    .icon(IconName::Trash)
                    .destructive(),
                );
            }
            actions = actions.action(add.clone());
            Item::new(format!("autostart-{id}"), entry.name.clone())
                .icon(if enabled { IconName::Play } else { IconName::Pause })
                .subtitle(subtitle)
                .keywords(format!("{} {} startup autostart login", entry.exec, entry.id))
                .accessory(state_tag(entry))
                .actions(actions)
                .detail(detail_of(entry))
        };

        let mine: Vec<Item> = self
            .entries
            .iter()
            .filter(|e| e.origin == Origin::User)
            .map(|e| item_of(e, cx))
            .collect();
        let system: Vec<Item> = self
            .entries
            .iter()
            .filter(|e| e.origin == Origin::System)
            .map(|e| item_of(e, cx))
            .collect();

        if mine.is_empty() && system.is_empty() {
            return list
                .empty(
                    EmptyView::new("Nothing starts at login")
                        .description("Press Ctrl+N to add a program.")
                        .icon(IconName::Play),
                )
                .into();
        }
        if !mine.is_empty() {
            list = list.section(Section::new("Your programs").items(mine));
        }
        if !system.is_empty() {
            list = list.section(Section::new("System").items(system));
        }
        list.into()
    }
}

#[cfg(test)]
mod tests {
    use super::program_name;

    #[test]
    fn a_command_becomes_a_name() {
        assert_eq!(program_name("blueman-applet"), "Blueman applet");
        assert_eq!(program_name("/home/me/bin/my_tool --flag"), "My tool");
        assert_eq!(program_name("sh -c 'x'"), "Sh");
    }
}
