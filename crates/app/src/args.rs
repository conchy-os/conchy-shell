//! Command-line argument parsing for Conchy Shell.

use crate::ipc::IpcCommand;

/// Command-line arguments.
#[derive(Debug, Clone)]
pub struct Args {
    /// Command to send if secondary instance, or default daemon behavior.
    pub command: Option<IpcCommand>,
    /// Force running daemon even if an action argument was specified.
    pub daemon: bool,
    /// Parse the config files, report, and exit without starting the shell.
    pub validate: bool,
}

impl Args {
    /// Parse command-line arguments from `std::env::args()`.
    pub fn parse() -> Self {
        let raw_args: Vec<String> = std::env::args().collect();
        let mut command = None;
        let mut daemon = false;
        let mut validate = false;

        let mut i = 1;
        while i < raw_args.len() {
            let arg = &raw_args[i];
            match arg.as_str() {
                "--validate" => validate = true,
                "daemon" | "--daemon" => daemon = true,
                "toggle-launcher" => {
                    let input = if i + 1 < raw_args.len() && !raw_args[i + 1].starts_with('-') {
                        i += 1;
                        Some(raw_args[i].clone())
                    } else {
                        None
                    };
                    command = Some(IpcCommand::LauncherToggle { input });
                }
                "-i" | "--input" => {
                    if i + 1 < raw_args.len() {
                        i += 1;
                        command = Some(IpcCommand::LauncherToggle {
                            input: Some(raw_args[i].clone()),
                        });
                    }
                }
                "toggle-control-center" | "toggle-cc" => {
                    command = Some(IpcCommand::ControlCenterToggle);
                }
                "toggle-overview" | "overview" => {
                    command = Some(IpcCommand::OverviewToggle);
                }
                "toggle-minimize" | "minimize" => {
                    command = Some(IpcCommand::MinimizeToggle);
                }
                "close-window" => {
                    command = Some(IpcCommand::CloseWindow);
                }
                "osd" | "action" => {
                    if i + 1 < raw_args.len() {
                        i += 1;
                        match raw_args[i].as_str() {
                            "volume-up" | "volume+5" => command = Some(IpcCommand::AudioVolumeUp),
                            "volume-down" | "volume-5" => command = Some(IpcCommand::AudioVolumeDown),
                            "volume-mute" | "mute" => command = Some(IpcCommand::AudioVolumeMute),
                            "brightness-up" | "brightness+5" => command = Some(IpcCommand::BrightnessUp),
                            "brightness-down" | "brightness-5" => command = Some(IpcCommand::BrightnessDown),
                            "toggle-launcher" => command = Some(IpcCommand::LauncherToggle { input: None }),
                            "toggle-control-center" | "toggle-cc" => command = Some(IpcCommand::ControlCenterToggle),
                            "toggle-overview" => command = Some(IpcCommand::OverviewToggle),
                            "toggle-minimize" => command = Some(IpcCommand::MinimizeToggle),
                            other => {
                                eprintln!("Unknown sub-action: {}", other);
                            }
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }

        if command.is_none() && !daemon {
            command = Some(IpcCommand::LauncherToggle { input: None });
        }

        Args {
            command,
            daemon,
            validate,
        }
    }
}
