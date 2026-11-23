//! Equalizer module - ten bands and a preamp on everything the machine plays.
//!
//! `;equ` shows the response curve with a handle per band (drag them, or
//! pick a band with Alt+←/→ and nudge it with Alt+↑/↓), then the presets and
//! the outputs. Enter on the first row turns the equalizer on or off; Enter
//! on a preset applies it. The sound itself is handled by the equalizer
//! service (`services::equalizer`).

pub mod config;
mod graph;

use gpui::{AnyElement, Context, FontWeight, Window, div, prelude::*, px};
use services::equalizer::dsp::{self, BAND_COUNT, MAX_PREAMP_DB};
use services::{
    EqualizerCommand, EqualizerData, EqualizerEngineState, EqualizerSubscriber,
};
use ui::{
    ActiveTheme, Color, IconName, Label, LabelCommon, Radius, Spacing, Switch, TextSize, h_flex,
    v_flex,
};

use self::config::EqualizerConfig;
use self::graph::{EqGraph, gain_label};
use super::config::prompt_page::PromptPage;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{
    Accessory, Content, Detail, EmptyView, Item, List, Metadata, Section,
};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};
use crate::state::{AppState, watch};

const EQUALIZER_ICON: IconName = IconName::Sliders;
/// What Alt+↑/↓ changes a band by, in dB.
const BAND_STEP: f32 = 1.0;
/// What Alt+Shift+↑/↓ changes the preamp by, in dB.
const PREAMP_STEP: f32 = 1.0;

pub fn descriptor(config: &EqualizerConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "equalizer",
            prefix: config.prefix.clone().into(),
            name: "Equalizer",
            icon: EQUALIZER_ICON,
            description: "Shape the sound of everything that plays",
            default: false,
            in_help: true,
        },
        EqualizerModule::new,
    )
}

pub struct EqualizerModule {
    subscriber: EqualizerSubscriber,
    data: EqualizerData,
    /// The band Alt+↑/↓ moves.
    selected: usize,
}

impl EqualizerModule {
    fn new(cx: &mut Context<Self>) -> Self {
        let subscriber = AppState::equalizer(cx).clone();
        let data = subscriber.get();
        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });
        Self {
            subscriber,
            data,
            selected: 0,
        }
    }

    fn send(&self, command: EqualizerCommand) {
        self.subscriber.dispatch(command);
    }

    /// An action that changes the equalizer and keeps the launcher open;
    /// the service answers through the subscription.
    fn command(
        &self,
        title: &'static str,
        icon: IconName,
        command: EqualizerCommand,
    ) -> Action {
        let subscriber = self.subscriber.clone();
        Action::new(title, move |_, _| {
            subscriber.dispatch(command.clone());
            Outcome::Stay
        })
        .icon(icon)
    }

    fn move_selection(cx: &Context<Self>, title: &'static str, forward: bool) -> Action {
        Action::listener(title, cx, move |this, _, cx| {
            this.selected = if forward {
                (this.selected + 1) % BAND_COUNT
            } else {
                (this.selected + BAND_COUNT - 1) % BAND_COUNT
            };
            cx.notify();
            Outcome::Stay
        })
        .icon(if forward {
            IconName::ChevronRight
        } else {
            IconName::ChevronLeft
        })
    }

    fn nudge_band(cx: &Context<Self>, title: &'static str, delta: f32) -> Action {
        Action::listener(title, cx, move |this, _, _| {
            let gain = this.data.settings.curve.gains[this.selected] + delta;
            this.send(EqualizerCommand::SetBand {
                band: this.selected,
                gain,
            });
            Outcome::Stay
        })
        .icon(if delta > 0.0 {
            IconName::ChevronUp
        } else {
            IconName::ChevronDown
        })
    }

    fn nudge_preamp(cx: &Context<Self>, title: &'static str, delta: f32) -> Action {
        Action::listener(title, cx, move |this, _, _| {
            let db = (this.data.settings.curve.preamp_db + delta).clamp(-MAX_PREAMP_DB, MAX_PREAMP_DB);
            this.send(EqualizerCommand::SetPreamp(db));
            Outcome::Stay
        })
        .icon(IconName::Volume)
    }

    fn save_preset(cx: &Context<Self>) -> Action {
        Action::listener("Save Curve as Preset", cx, |this, _, cx| {
            let subscriber = this.subscriber.clone();
            let taken: Vec<String> = this
                .data
                .settings
                .presets()
                .filter(|preset| preset.builtin)
                .map(|preset| preset.name.to_lowercase())
                .collect();
            PromptPage::ask(
                cx,
                "Save Preset",
                "Name the preset...",
                "Save",
                this.data.settings.active_preset().unwrap_or_default(),
                move |name| {
                    if taken.contains(&name.trim().to_lowercase()) {
                        Err("That name belongs to a built-in preset".into())
                    } else {
                        Ok(())
                    }
                },
                move |name, _| {
                    subscriber.dispatch(EqualizerCommand::SavePreset(name));
                    Outcome::Pop
                },
            )
        })
        .icon(IconName::Save)
        .shortcut("ctrl-s")
    }

    /// The actions every row carries: the band under the keyboard, the
    /// preamp, and the curve as a whole.
    fn common(&self, mut panel: ActionPanel, cx: &Context<Self>) -> ActionPanel {
        let selected = dsp::LABELS[self.selected];
        panel = panel
            .section(
                format!("Band {selected} Hz"),
                [
                    Self::move_selection(cx, "Previous Band", false).shortcut("alt-left"),
                    Self::move_selection(cx, "Next Band", true).shortcut("alt-right"),
                    Self::nudge_band(cx, "Raise Band", BAND_STEP).shortcut("alt-up"),
                    Self::nudge_band(cx, "Lower Band", -BAND_STEP).shortcut("alt-down"),
                ],
            )
            .section(
                "Preamp",
                [
                    Self::nudge_preamp(cx, "Raise Preamp", PREAMP_STEP).shortcut("alt-shift-up"),
                    Self::nudge_preamp(cx, "Lower Preamp", -PREAMP_STEP)
                        .shortcut("alt-shift-down"),
                    self.command(
                        if self.data.settings.auto_preamp {
                            "Turn Off Clipping Guard"
                        } else {
                            "Turn On Clipping Guard"
                        },
                        IconName::Shield,
                        EqualizerCommand::SetAutoPreamp(!self.data.settings.auto_preamp),
                    ),
                ],
            )
            .section(
                "Curve",
                [
                    Self::save_preset(cx),
                    self.command(
                        "Reset to Flat",
                        IconName::RotateCcw,
                        EqualizerCommand::Reset,
                    )
                    .shortcut("ctrl-r"),
                ],
            );
        panel
    }

    // -- rows -----------------------------------------------------------------

    fn status(&self) -> (String, Color) {
        match &self.data.engine {
            EqualizerEngineState::Off => ("Off".into(), Color::Muted),
            EqualizerEngineState::Starting => ("Starting...".into(), Color::Warning),
            EqualizerEngineState::Running => ("On".into(), Color::Success),
            EqualizerEngineState::Failed(_) => ("Failed".into(), Color::Error),
        }
    }

    fn power_item(&self, cx: &Context<Self>) -> Item {
        let enabled = self.data.settings.enabled;
        let (status, color) = self.status();
        let subtitle = match (&self.data.engine, self.data.output_description()) {
            (EqualizerEngineState::Failed(reason), _) => reason.clone(),
            (_, Some(output)) => format!("Playing to {output}"),
            (_, None) => "No output device".to_string(),
        };

        let panel = ActionPanel::new().action(
            self.command(
                if enabled {
                    "Turn Off Equalizer"
                } else {
                    "Turn On Equalizer"
                },
                IconName::Power,
                EqualizerCommand::SetEnabled(!enabled),
            ),
        );

        Item::new("power", "Equalizer")
            .subtitle(subtitle)
            .icon(EQUALIZER_ICON)
            .keywords("equalizer eq on off toggle enable disable")
            .accessory(Accessory::tag(status).color(color))
            .detail(self.status_detail())
            .actions(self.common(panel, cx))
    }

    fn status_detail(&self) -> Detail {
        let settings = &self.data.settings;
        let (status, color) = self.status();
        let mut detail = Detail::new()
            .markdown("## Equalizer\n\nTen bands and a preamp on all system sound. Every app is routed through a virtual sink, then on to the output below.")
            .metadata(Metadata::label("Status", status).color(color))
            .metadata(Metadata::label(
                "Output",
                self.data
                    .output_description()
                    .unwrap_or("None")
                    .to_string(),
            ))
            .metadata(Metadata::label(
                "Preset",
                settings
                    .active_preset()
                    .unwrap_or_else(|| "Custom".to_string()),
            ));
        let effective = settings.effective_preamp_db();
        detail = detail.metadata(Metadata::label(
            "Preamp",
            if settings.auto_preamp {
                format!("{effective:+.1} dB (clipping guard on)")
            } else {
                format!("{effective:+.1} dB")
            },
        ));
        if let EqualizerEngineState::Failed(reason) = &self.data.engine {
            detail = detail.metadata(Metadata::label("Error", reason.clone()).color(Color::Error));
        }
        detail
    }

    fn preset_item(
        &self,
        preset: &services::equalizer::Preset,
        active: bool,
        cx: &Context<Self>,
    ) -> Item {
        let name = preset.name.clone();
        let mut panel = ActionPanel::new().action(
            self.command(
                "Apply Preset",
                IconName::Check,
                EqualizerCommand::ApplyPreset(name.clone()),
            ),
        );
        if !preset.builtin {
            panel = panel.action(
                self.command(
                    "Delete Preset",
                    IconName::Trash,
                    EqualizerCommand::DeletePreset(name.clone()),
                )
                .destructive()
                .shortcut("ctrl-d"),
            );
        }

        let gains = preset
            .curve
            .gains
            .iter()
            .map(|gain| gain_label(*gain))
            .collect::<Vec<_>>()
            .join(" ");
        let detail = Detail::new()
            .markdown(format!("## {name}"))
            .metadata(Metadata::label("Bands", format!("{gains} dB")))
            .metadata(Metadata::label(
                "Preamp",
                format!("{:+.1} dB", preset.curve.preamp_db),
            ))
            .metadata(Metadata::label(
                "Kind",
                if preset.builtin { "Built-in" } else { "Yours" },
            ));

        let mut item = Item::new(format!("preset:{name}"), name.clone())
            .subtitle(gains)
            .icon(if preset.builtin {
                IconName::Sliders
            } else {
                IconName::Star
            })
            .detail(detail)
            .actions(self.common(panel, cx));
        if active {
            item = item.accessory(
                Accessory::tag("Active")
                    .with_icon(IconName::Check)
                    .color(Color::Success),
            );
        }
        item
    }

    fn output_item(&self, device: &services::OutputDevice, cx: &Context<Self>) -> Item {
        let current = self.data.output.as_deref() == Some(device.name.as_str());
        let panel = ActionPanel::new().action(self.command(
            "Play to This Output",
            IconName::Volume,
            EqualizerCommand::SetOutput(device.name.clone()),
        ));
        let mut item = Item::new(format!("output:{}", device.name), device.description.clone())
            .subtitle(device.name.clone())
            .icon(IconName::Volume)
            .keywords("output device speakers headphones sink")
            .actions(self.common(panel, cx));
        if current {
            item = item.accessory(
                Accessory::tag("Current")
                    .with_icon(IconName::Check)
                    .color(Color::Success),
            );
        }
        item
    }

    fn card(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let settings = &self.data.settings;
        let (status, status_color) = self.status();
        let running = matches!(self.data.engine, EqualizerEngineState::Running);

        let weak = cx.weak_entity();
        let subscriber = self.subscriber.clone();
        let graph = EqGraph::new("equalizer-graph", settings.curve.gains)
            .selected(self.selected)
            .active(settings.enabled)
            .on_change(move |band, gain, _, cx| {
                weak.update(cx, |this, cx| {
                    this.selected = band;
                    cx.notify();
                })
                .ok();
                subscriber.dispatch(EqualizerCommand::SetBand { band, gain });
            });

        let power = {
            let subscriber = self.subscriber.clone();
            let enabled = settings.enabled;
            Switch::new("equalizer-power", enabled).on_click(move |_, _, _| {
                subscriber.dispatch(EqualizerCommand::SetEnabled(!enabled));
            })
        };

        let band = self.selected;
        let band_text = format!(
            "{} Hz  {} dB",
            dsp::FREQUENCIES[band],
            gain_label(settings.curve.gains[band])
        );
        let preamp_text = format!(
            "Preamp {:+.1} dB{}",
            settings.effective_preamp_db(),
            if settings.auto_preamp { " (auto)" } else { "" }
        );

        let heading = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .child(
                h_flex()
                    .items_baseline()
                    .gap(Spacing::Small.pixels())
                    .child(
                        Label::new("Equalizer")
                            .size(TextSize::Default)
                            .weight(FontWeight::MEDIUM),
                    )
                    .child(
                        Label::new(status)
                            .size(TextSize::XSmall)
                            .color(if running || settings.enabled {
                                status_color
                            } else {
                                Color::Muted
                            }),
                    ),
            )
            .child(power);

        let footer = h_flex()
            .w_full()
            .justify_between()
            .child(
                Label::new(band_text)
                    .size(TextSize::XSmall)
                    .color(Color::Accent),
            )
            .child(
                Label::new(preamp_text)
                    .size(TextSize::XSmall)
                    .color(Color::Muted),
            );

        div()
            .p(Spacing::Medium.pixels())
            .child(
                v_flex()
                    .w_full()
                    .p(Spacing::Medium.pixels())
                    .gap(px(6.))
                    .bg(theme.colors.surface_background)
                    .rounded(Radius::Large.pixels())
                    .child(heading)
                    .child(graph)
                    .child(footer),
            )
            .into_any_element()
    }
}

impl LauncherPage for EqualizerModule {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let settings = &self.data.settings;
        let active = settings.active_preset();

        let (builtin, user): (Vec<_>, Vec<_>) = settings.presets().partition(|preset| preset.builtin);

        let mut list = List::new()
            .show_detail(true)
            .placeholder("Search presets and outputs...")
            .empty(
                EmptyView::new("Nothing matches")
                    .description("Presets and output devices are searched")
                    .icon(EQUALIZER_ICON),
            )
            .section(Section::untitled().item(self.power_item(cx)))
            .section(
                Section::new("Presets").items(
                    builtin
                        .iter()
                        .map(|preset| self.preset_item(preset, active.as_deref() == Some(&preset.name), cx))
                        .collect::<Vec<_>>(),
                ),
            );
        if !user.is_empty() {
            list = list.section(
                Section::new("My Presets").items(
                    user.iter()
                        .map(|preset| {
                            self.preset_item(preset, active.as_deref() == Some(&preset.name), cx)
                        })
                        .collect::<Vec<_>>(),
                ),
            );
        }
        if !self.data.outputs.is_empty() {
            list = list.section(
                Section::new("Output").items(
                    self.data
                        .outputs
                        .iter()
                        .map(|device| self.output_item(device, cx))
                        .collect::<Vec<_>>(),
                ),
            );
        }
        list.into()
    }

    fn render_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        Some(self.card(cx))
    }
}
