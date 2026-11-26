//! Startup applications: what the session starts at login, with a switch for
//! each, and a way to add programs of one's own. The form beside the
//! "Startup applications" row.

use gpui::{App, Context, Entity, SharedString};
use services::autostart::{Autostart, Entry, Off, Origin};
use ui::{Color, IconName};

use super::prompt_page::PromptPage;
use super::settings_form::{self as rows, Entry as Row};
use crate::launcher::action::{Action, Outcome};
use crate::launcher::content::{Detail, Form, Item};

pub struct AutostartPage {
    autostart: Option<Autostart>,
    entries: Vec<Entry>,
    /// What went wrong, or what was done, last.
    notice: Option<(String, bool)>,
    error: Option<String>,
}

impl AutostartPage {
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
        let Some(autostart) = &self.autostart else {
            return;
        };
        self.notice = match autostart.set_enabled(id, enabled) {
            Ok(()) => None,
            Err(err) => Some((format!("{err:#}"), true)),
        };
        self.reload();
        cx.notify();
    }

    fn remove(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(autostart) = &self.autostart else {
            return;
        };
        self.notice = match autostart.remove(id) {
            Ok(()) => Some(("Removed from startup.".into(), false)),
            Err(err) => Some((format!("{err:#}"), true)),
        };
        self.reload();
        cx.notify();
    }

    fn add(&mut self, command: &str, cx: &mut Context<Self>) {
        let Some(autostart) = &self.autostart else {
            return;
        };
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

fn state_label(entry: &Entry) -> (SharedString, Color) {
    match entry.off {
        None => ("On".into(), Color::Success),
        Some(Off::Switched) => ("Off".into(), Color::Muted),
        Some(Off::OtherDesktop) => ("Other desktop".into(), Color::Muted),
        Some(Off::NotInstalled) => ("Not installed".into(), Color::Warning),
    }
}

pub fn item(page: &Entity<AutostartPage>) -> Item {
    let page = page.clone();
    Item::new("startup", "Startup applications")
        .subtitle("Programs the session starts at login")
        .icon(IconName::Play)
        .keywords("startup autostart login boot applications programs session run at start")
        .actions(super::system::edit())
        .detail(Detail::new().form(move |form, cx| startup_form(form, &page, cx)))
}

fn startup_form(form: &mut Form, page: &Entity<AutostartPage>, cx: &App) {
    let this = page.read(cx);
    rows::title(
        form,
        "Startup applications",
        "Programs the session starts when you log in",
    );
    if let Some(err) = &this.error {
        rows::note(
            form,
            format!("Cannot find the autostart folders: {err}"),
            Color::Error,
        );
        return;
    }

    let add_to = page.clone();
    rows::action(
        form,
        "Add program…",
        "Run a command at every login",
        move |_, cx| {
            let page = add_to.clone();
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
        },
        cx,
    );
    if let Some((text, error)) = &this.notice {
        rows::note(
            form,
            text.clone(),
            if *error { Color::Error } else { Color::Muted },
        );
    }

    let row_of = |entry: &Entry, form: &mut Form| {
        let enabled = entry.enabled();
        let mut actions = Vec::new();
        if entry.switchable() {
            let (page, id) = (page.clone(), entry.id.clone());
            actions.push(
                Action::new(
                    if enabled {
                        "Don't start at login"
                    } else {
                        "Start at login"
                    },
                    move |_, cx| {
                        page.update(cx, |page, cx| page.toggle(&id, !enabled, cx));
                        Outcome::Stay
                    },
                )
                .icon(if enabled {
                    IconName::Stop
                } else {
                    IconName::Play
                }),
            );
        }
        if entry.removable() {
            let (page, id) = (page.clone(), entry.id.clone());
            let remove = Action::new("Remove from startup", move |_, cx| {
                page.update(cx, |page, cx| page.remove(&id, cx));
                Outcome::Stay
            })
            .icon(IconName::Trash)
            .destructive();
            // Enter and Shift+Enter are taken by the first two; this one has a key of its own.
            actions.push(if actions.is_empty() {
                remove
            } else {
                remove.shortcut("ctrl-d")
            });
        }
        let subtitle = if entry.comment.is_empty() {
            entry.exec.clone()
        } else {
            entry.comment.clone()
        };
        rows::entry(
            form,
            Row {
                icon: if enabled {
                    IconName::Play
                } else {
                    IconName::Pause
                },
                title: entry.name.clone().into(),
                subtitle: subtitle.into(),
                tag: Some(state_label(entry)),
            },
            actions,
            cx,
        );
    };

    let mine: Vec<&Entry> = this
        .entries
        .iter()
        .filter(|e| e.origin == Origin::User)
        .collect();
    let system: Vec<&Entry> = this
        .entries
        .iter()
        .filter(|e| e.origin == Origin::System)
        .collect();
    if mine.is_empty() && system.is_empty() {
        rows::note(form, "Nothing starts at login yet.", Color::Muted);
    }
    if !mine.is_empty() {
        rows::heading(form, "Your programs");
        for entry in mine {
            row_of(entry, form);
        }
    }
    if !system.is_empty() {
        rows::heading(form, "System");
        for entry in system {
            row_of(entry, form);
        }
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
