use gpui::{App, Context, prelude::*};
use services::AudioCommand;
use ui::IconName;

use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};
use crate::state::{AppState, watch};

pub struct SoundPage;

impl SoundPage {
    pub fn open(cx: &mut App) -> Page {
        Page::new(cx.new(|cx| Self::new(cx)))
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        let audio = AppState::audio(cx);
        watch(cx, audio.subscribe(), |_, _, cx| {
            cx.notify();
        });
        Self
    }
}

impl LauncherPage for SoundPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let service = AppState::audio(cx);
        let audio = service.get();

        let mut list = List::new()
            .show_detail(true)
            .click_selects(true)
            .title("Sound & Audio")
            .placeholder("Search sound settings…");

        // Global actions
        let s_mute = service.clone();
        let s_mic = service.clone();
        let actions = ActionPanel::default()
            .action(Action::new(
                if audio.sink_muted {
                    "Unmute Speakers"
                } else {
                    "Mute Speakers"
                },
                move |_, _| {
                    s_mute.dispatch(AudioCommand::ToggleSinkMute);
                    Outcome::Stay
                },
            ))
            .action(Action::new(
                if audio.source_muted {
                    "Unmute Microphone"
                } else {
                    "Mute Microphone"
                },
                move |_, _| {
                    s_mic.dispatch(AudioCommand::ToggleSourceMute);
                    Outcome::Stay
                },
            ));

        list = list.actions(actions);

        // 1. Output (Sinks) section
        let s = service.clone();
        let out_icon = if audio.sink_muted {
            IconName::VolumeOff
        } else if audio.sink_volume > 66 {
            IconName::Volume
        } else if audio.sink_volume > 33 {
            IconName::VolumeMedium
        } else {
            IconName::VolumeLow
        };

        let output_item = Item::new("output-volume", "Output Volume (Master)")
            .subtitle(format!(
                "{} • {}",
                if audio.sink_muted { "Muted" } else { "Active" },
                "PipeWire / PulseAudio"
            ))
            .icon(out_icon)
            .accessory(Accessory::tag(format!("{}%", audio.sink_volume)))
            .actions(
                ActionPanel::default()
                    .action(Action::new(
                        if audio.sink_muted { "Unmute" } else { "Mute" },
                        {
                            let s = s.clone();
                            move |_, _| {
                                s.dispatch(AudioCommand::ToggleSinkMute);
                                Outcome::Stay
                            }
                        },
                    ))
                    .action(Action::new("Volume Up (+5%)", {
                        let s = s.clone();
                        move |_, _| {
                            s.dispatch(AudioCommand::AdjustSinkVolume(5));
                            Outcome::Stay
                        }
                    }))
                    .action(Action::new("Volume Down (-5%)", {
                        let s = s.clone();
                        move |_, _| {
                            s.dispatch(AudioCommand::AdjustSinkVolume(-5));
                            Outcome::Stay
                        }
                    }))
                    .action(Action::new("Set Volume 100%", {
                        let s = s.clone();
                        move |_, _| {
                            s.dispatch(AudioCommand::SetSinkVolume(100));
                            Outcome::Stay
                        }
                    }))
                    .action(Action::new("Set Volume 75%", {
                        let s = s.clone();
                        move |_, _| {
                            s.dispatch(AudioCommand::SetSinkVolume(75));
                            Outcome::Stay
                        }
                    }))
                    .action(Action::new("Set Volume 50%", {
                        let s = s.clone();
                        move |_, _| {
                            s.dispatch(AudioCommand::SetSinkVolume(50));
                            Outcome::Stay
                        }
                    }))
                    .action(Action::new("Set Volume 25%", {
                        let s = s.clone();
                        move |_, _| {
                            s.dispatch(AudioCommand::SetSinkVolume(25));
                            Outcome::Stay
                        }
                    })),
            );

        // 2. Input (Sources) section
        let s_in = service.clone();
        let mic_icon = if audio.source_muted {
            IconName::MicOff
        } else {
            IconName::Mic
        };

        let input_item = Item::new("input-volume", "Microphone Input (Master)")
            .subtitle(if audio.source_muted {
                "Microphone is muted".to_string()
            } else {
                "Microphone is live".to_string()
            })
            .icon(mic_icon)
            .accessory(Accessory::tag(format!("{}%", audio.source_volume)))
            .actions(
                ActionPanel::default()
                    .action(Action::new(
                        if audio.source_muted {
                            "Unmute Microphone"
                        } else {
                            "Mute Microphone"
                        },
                        {
                            let s = s_in.clone();
                            move |_, _| {
                                s.dispatch(AudioCommand::ToggleSourceMute);
                                Outcome::Stay
                            }
                        },
                    ))
                    .action(Action::new("Mic Volume Up (+5%)", {
                        let s = s_in.clone();
                        move |_, _| {
                            s.dispatch(AudioCommand::AdjustSourceVolume(5));
                            Outcome::Stay
                        }
                    }))
                    .action(Action::new("Mic Volume Down (-5%)", {
                        let s = s_in.clone();
                        move |_, _| {
                            s.dispatch(AudioCommand::AdjustSourceVolume(-5));
                            Outcome::Stay
                        }
                    })),
            );

        list = list
            .section(Section::new("Audio Output").item(output_item))
            .section(Section::new("Audio Input").item(input_item));

        list.into()
    }
}
