use gpui::App;

use crate::args::Args;
use crate::launcher;

const LAUNCHER_PREFIX: &str = "ipc:launcher:";
const CC_TOGGLE: &str = "ipc:control-center:toggle";
const OVERVIEW_TOGGLE: &str = "ipc:overview:toggle";
const MINIMIZE_TOGGLE: &str = "ipc:minimize:toggle";
const CLOSE_WINDOW: &str = "ipc:window:close";
const AUDIO_VOL_UP: &str = "ipc:audio:volume-up";
const AUDIO_VOL_DOWN: &str = "ipc:audio:volume-down";
const AUDIO_MUTE: &str = "ipc:audio:volume-mute";
const BRIGHTNESS_UP: &str = "ipc:brightness:up";
const BRIGHTNESS_DOWN: &str = "ipc:brightness:down";

#[derive(Debug, Clone, PartialEq)]
pub struct IpcMessage {
    pub id: u64,
    pub command: IpcCommand,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IpcCommand {
    LauncherToggle { input: Option<String> },
    ControlCenterToggle,
    OverviewToggle,
    MinimizeToggle,
    CloseWindow,
    AudioVolumeUp,
    AudioVolumeDown,
    AudioVolumeMute,
    BrightnessUp,
    BrightnessDown,
}

pub fn command_for_secondary(args: &Args) -> IpcCommand {
    args.command.clone().unwrap_or(IpcCommand::LauncherToggle { input: None })
}

pub fn encode_command(command: &IpcCommand) -> String {
    match command {
        IpcCommand::LauncherToggle { input } => {
            format!("{}{}", LAUNCHER_PREFIX, input.as_deref().unwrap_or(""))
        }
        IpcCommand::ControlCenterToggle => CC_TOGGLE.to_string(),
        IpcCommand::OverviewToggle => OVERVIEW_TOGGLE.to_string(),
        IpcCommand::MinimizeToggle => MINIMIZE_TOGGLE.to_string(),
        IpcCommand::CloseWindow => CLOSE_WINDOW.to_string(),
        IpcCommand::AudioVolumeUp => AUDIO_VOL_UP.to_string(),
        IpcCommand::AudioVolumeDown => AUDIO_VOL_DOWN.to_string(),
        IpcCommand::AudioVolumeMute => AUDIO_MUTE.to_string(),
        IpcCommand::BrightnessUp => BRIGHTNESS_UP.to_string(),
        IpcCommand::BrightnessDown => BRIGHTNESS_DOWN.to_string(),
    }
}

pub fn decode_command(payload: &str) -> IpcCommand {
    let trimmed = payload.trim();
    if let Some(rest) = trimmed.strip_prefix(LAUNCHER_PREFIX) {
        return IpcCommand::LauncherToggle {
            input: if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            },
        };
    }

    match trimmed {
        CC_TOGGLE => IpcCommand::ControlCenterToggle,
        OVERVIEW_TOGGLE => IpcCommand::OverviewToggle,
        MINIMIZE_TOGGLE => IpcCommand::MinimizeToggle,
        CLOSE_WINDOW => IpcCommand::CloseWindow,
        AUDIO_VOL_UP => IpcCommand::AudioVolumeUp,
        AUDIO_VOL_DOWN => IpcCommand::AudioVolumeDown,
        AUDIO_MUTE => IpcCommand::AudioVolumeMute,
        BRIGHTNESS_UP => IpcCommand::BrightnessUp,
        BRIGHTNESS_DOWN => IpcCommand::BrightnessDown,
        _ => IpcCommand::LauncherToggle {
            input: if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            },
        },
    }
}

pub(super) fn handle_message(message: IpcMessage, cx: &mut App) {
    match message.command {
        IpcCommand::LauncherToggle { input } => {
            tracing::info!(
                "Processing launcher request: id={}, input={:?}",
                message.id,
                input
            );
            launcher::toggle(input, cx);
        }
        IpcCommand::ControlCenterToggle => {
            tracing::info!("Processing control center toggle: id={}", message.id);
            crate::control_center::toggle(cx);
        }
        IpcCommand::OverviewToggle => {
            tracing::info!("Processing overview toggle: id={}", message.id);
            crate::state::AppState::compositor(cx)
                .dispatch(services::CompositorCommand::ToggleOverview);
        }
        IpcCommand::MinimizeToggle => {
            tracing::info!("Processing minimize toggle: id={}", message.id);
            crate::state::AppState::compositor(cx)
                .dispatch(services::CompositorCommand::ToggleMinimizeWindow(None));
        }
        IpcCommand::CloseWindow => {
            tracing::info!("Processing close window: id={}", message.id);
            crate::state::AppState::compositor(cx)
                .dispatch(services::CompositorCommand::CloseWindow("".to_string()));
        }
        IpcCommand::AudioVolumeUp => {
            tracing::info!("Processing audio volume up: id={}", message.id);
            crate::state::AppState::audio(cx)
                .dispatch(services::AudioCommand::AdjustSinkVolume(5));
        }
        IpcCommand::AudioVolumeDown => {
            tracing::info!("Processing audio volume down: id={}", message.id);
            crate::state::AppState::audio(cx)
                .dispatch(services::AudioCommand::AdjustSinkVolume(-5));
        }
        IpcCommand::AudioVolumeMute => {
            tracing::info!("Processing audio mute: id={}", message.id);
            crate::state::AppState::audio(cx)
                .dispatch(services::AudioCommand::ToggleSinkMute);
        }
        IpcCommand::BrightnessUp => {
            tracing::info!("Processing brightness up: id={}", message.id);
            crate::state::AppState::brightness(cx)
                .dispatch(services::BrightnessCommand::Increase(5));
        }
        IpcCommand::BrightnessDown => {
            tracing::info!("Processing brightness down: id={}", message.id);
            crate::state::AppState::brightness(cx)
                .dispatch(services::BrightnessCommand::Decrease(5));
        }
    }
}
