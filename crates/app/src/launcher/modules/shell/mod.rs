//! Shell module: run the typed command in a terminal or in the background.

pub mod config;

use std::process::Command;

use gpui::Context;
use ui::IconName;

use self::config::ShellConfig;
use crate::launcher::action::{Action, Outcome};
use crate::launcher::content::{Content, EmptyView, Item, List};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};

const ICON: IconName = IconName::SquareTerminal;

/// Tried in order when no terminal is configured, or the configured one is
/// missing.
const TERMINALS: &[(&str, &[&str])] = &[
    ("ghostty", &["-e", "sh", "-c"]),
    ("kitty", &["--", "sh", "-c"]),
    ("alacritty", &["-e", "sh", "-c"]),
    ("wezterm", &["start", "--", "sh", "-c"]),
    ("foot", &["sh", "-c"]),
    ("gnome-terminal", &["--", "sh", "-c"]),
    ("konsole", &["-e", "sh", "-c"]),
    ("xfce4-terminal", &["-e", "sh", "-c"]),
    ("xterm", &["-e", "sh", "-c"]),
    ("x-terminal-emulator", &["-e", "sh", "-c"]),
];

pub fn descriptor(config: &ShellConfig) -> ModuleDescriptor {
    let terminal = config.terminal.clone();
    ModuleDescriptor::new(
        ModuleMeta {
            id: "shell",
            prefix: config.prefix.clone().into(),
            name: "Shell",
            icon: ICON,
            description: "Run shell commands in terminal",
            default: false,
            in_help: true,
        },
        move |_| ShellModule {
            terminal: terminal.clone(),
        },
    )
}

pub struct ShellModule {
    terminal: String,
}

impl LauncherPage for ShellModule {
    fn content(&mut self, query: &str, _cx: &mut Context<Self>) -> Content {
        let list = List::new().filtering(false).empty(
            EmptyView::new("Type a command to run")
                .description("e.g. htop or cargo build")
                .icon(ICON),
        );
        let command = query.trim().to_string();
        if command.is_empty() {
            return list.into();
        }
        let terminal = if self.terminal.is_empty() {
            "your terminal"
        } else {
            &self.terminal
        };

        let in_terminal = (command.clone(), self.terminal.clone());
        let in_background = command.clone();
        list.item(
            Item::new("run", command.clone())
                .subtitle(format!("Run in {terminal}"))
                .icon(ICON)
                .action(
                    Action::new("Run", move |_, _| {
                        run_in_terminal(in_terminal.0.clone(), in_terminal.1.clone());
                        Outcome::Close
                    })
                    .icon(IconName::Terminal),
                )
                .action(Action::new("Run in Background", move |_, _| {
                    let mut sh = Command::new("sh");
                    sh.args(["-c", &in_background]);
                    if let Err(err) = services::spawn_reaped(sh, in_background.clone()) {
                        tracing::warn!("Failed to run `{in_background}`: {err}");
                    }
                    Outcome::Close
                }))
                .action(Action::copy(command).title("Copy Command")),
        )
        .into()
    }
}

/// Keeps the terminal open after the command exits so its output stays.
fn run_in_terminal(command: String, preferred: String) {
    let script = format!("{command}; echo ''; echo 'Press Enter to close...'; read");
    let preferred =
        (!preferred.is_empty()).then(|| (preferred.as_str(), ["-e", "sh", "-c"].as_slice()));

    for (terminal, args) in preferred.into_iter().chain(TERMINALS.iter().copied()) {
        let mut cmd = Command::new(terminal);
        cmd.args(args).arg(&script);
        if services::spawn_reaped(cmd, terminal).is_ok() {
            return;
        }
        tracing::debug!("Terminal '{terminal}' not available, trying the next");
    }
    tracing::warn!("No terminal found to run `{command}`");
}
