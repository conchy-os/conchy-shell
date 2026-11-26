//! Microphone module - effects for the voice, in the manner of EasyEffects.
//!
//! `;mic` lists the stages the voice goes through (noise reduction with a
//! neural network, rumble filter, gate, equalizer, compressor, limiter, gain),
//! each with its settings in the form beside it (Enter moves the keyboard
//! there), then the presets and the microphones. The first row, or the
//! switch above, turns the chain on or off. The sound itself is handled by
//! the microphone service (`services::mic`).

pub mod config;

use gpui::{AnyElement, App, Context, FontWeight, SharedString, Window, div, prelude::*, px};
use services::mic::chain::{self, Stage};
use services::mic::{Denoiser, Param};
use services::{MicCommand, MicData, MicEngineState, MicSubscriber};
use ui::{
    ActiveTheme, Color, IconName, Label, LabelCommon, Radius, Spacing, Switch, TextSize, h_flex,
    v_flex,
};

use self::config::MicConfig;
use super::config::prompt_page::PromptPage;
use super::config::settings_form::{self as rows, Number};
use super::config::system;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{
    Accessory, Content, Detail, EmptyView, Form, Item, List, Metadata, Section,
};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};
use crate::state::{AppState, watch};

const MIC_ICON: IconName = IconName::Mic;

pub fn descriptor(config: &MicConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "mic",
            prefix: config.prefix.clone().into(),
            name: "Microphone",
            icon: MIC_ICON,
            description: "Clean up and shape the sound of your voice",
            default: false,
            in_help: true,
        },
        MicModule::new,
    )
}

/// One row of the list of effects: what it is, and which settings are its.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Noise,
    Rumble,
    Gate,
    Equalizer,
    Compressor,
    Limiter,
    Gain,
}

impl Group {
    const ALL: [Group; 7] = [
        Group::Noise,
        Group::Rumble,
        Group::Gate,
        Group::Equalizer,
        Group::Compressor,
        Group::Limiter,
        Group::Gain,
    ];

    fn title(self) -> &'static str {
        match self {
            Group::Noise => "Noise Reduction",
            Group::Rumble => "Rumble Filter",
            Group::Gate => "Noise Gate",
            Group::Equalizer => "Equalizer",
            Group::Compressor => "Compressor",
            Group::Limiter => "Limiter",
            Group::Gain => "Output Gain",
        }
    }

    fn short(self) -> &'static str {
        match self {
            Group::Noise => "Denoise",
            Group::Rumble => "Rumble",
            Group::Gate => "Gate",
            Group::Equalizer => "EQ",
            Group::Compressor => "Comp",
            Group::Limiter => "Limit",
            Group::Gain => "Gain",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Group::Noise => "A neural network takes the room away: fans, keys, hiss",
            Group::Rumble => "Cuts the low rumble of desks, traffic and breathing",
            Group::Gate => "Closes between words so the background stays quiet",
            Group::Equalizer => "Warmth, clarity and air for the voice",
            Group::Compressor => "Evens out loud and quiet speech",
            Group::Limiter => "Stops sudden peaks from clipping",
            Group::Gain => "How loud the voice leaves the chain",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Group::Noise => IconName::Sparkles,
            Group::Rumble => IconName::Filter,
            Group::Gate => IconName::Shield,
            Group::Equalizer => IconName::Sliders,
            Group::Compressor => IconName::Gauge,
            Group::Limiter => IconName::Zap,
            Group::Gain => IconName::Volume,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Group::Noise => "denoise",
            Group::Rumble => "rumble",
            Group::Gate => "gate",
            Group::Equalizer => "eq",
            Group::Compressor => "compressor",
            Group::Limiter => "limiter",
            Group::Gain => "gain",
        }
    }

    /// The switch of the group, when it has one (gain is always there).
    fn switch(self) -> Option<Param> {
        match self {
            Group::Noise => Some(Param::Denoiser),
            Group::Rumble => Some(Param::HighpassOn),
            Group::Gate => Some(Param::GateOn),
            Group::Equalizer => Some(Param::EqOn),
            Group::Compressor => Some(Param::CompOn),
            Group::Limiter => Some(Param::LimiterOn),
            Group::Gain => None,
        }
    }

    fn on(self, data: &MicData) -> bool {
        self.switch()
            .is_none_or(|param| data.settings.voice.get(param) >= 0.5)
    }

    /// The stage of the graph this group is, when the plugin for it is
    /// installed.
    fn running(self, data: &MicData) -> bool {
        let stages = chain::stages(&data.settings.voice, &data.plugins);
        match self {
            Group::Noise => stages.contains(&Stage::RnNoise) || stages.contains(&Stage::DeepFilter),
            Group::Rumble => stages.contains(&Stage::Highpass),
            Group::Gate => stages.contains(&Stage::Gate),
            Group::Equalizer => stages.contains(&Stage::Equalizer),
            Group::Compressor => stages.contains(&Stage::Compressor),
            Group::Limiter => stages.contains(&Stage::Limiter),
            Group::Gain => true,
        }
    }
}

pub struct MicModule {
    subscriber: MicSubscriber,
    data: MicData,
}

impl MicModule {
    fn new(cx: &mut Context<Self>) -> Self {
        let subscriber = AppState::mic(cx).clone();
        let data = subscriber.get();
        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });
        Self { subscriber, data }
    }

    /// An action that changes the chain and keeps the launcher open; the
    /// service answers through the subscription.
    fn command(&self, title: &'static str, icon: IconName, command: MicCommand) -> Action {
        let subscriber = self.subscriber.clone();
        Action::new(title, move |_, _| {
            subscriber.dispatch(command.clone());
            Outcome::Stay
        })
        .icon(icon)
    }

    fn save_preset(cx: &Context<Self>) -> Action {
        Action::listener("Save Voice as Preset", cx, |this, _, cx| {
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
                    subscriber.dispatch(MicCommand::SavePreset(name));
                    Outcome::Pop
                },
            )
        })
        .icon(IconName::Save)
        .shortcut("ctrl-s")
    }

    /// The actions every row carries: the voice as a whole.
    fn common(&self, panel: ActionPanel, cx: &Context<Self>) -> ActionPanel {
        let enabled = self.data.settings.enabled;
        panel.section(
            "Microphone",
            [
                self.command(
                    if enabled {
                        "Turn Off Microphone Effects"
                    } else {
                        "Turn On Microphone Effects"
                    },
                    IconName::Power,
                    MicCommand::SetEnabled(!enabled),
                )
                .shortcut("ctrl-p"),
                Self::save_preset(cx),
                self.command("Reset to Defaults", IconName::RotateCcw, MicCommand::Reset)
                    .shortcut("ctrl-r"),
            ],
        )
    }

    // -- rows -----------------------------------------------------------------

    fn status(&self) -> (String, Color) {
        match &self.data.engine {
            MicEngineState::Off => ("Off".into(), Color::Muted),
            MicEngineState::Starting => ("Starting...".into(), Color::Warning),
            MicEngineState::Running => ("On".into(), Color::Success),
            MicEngineState::Failed(_) => ("Failed".into(), Color::Error),
        }
    }

    fn power_item(&self, cx: &Context<Self>) -> Item {
        let enabled = self.data.settings.enabled;
        let (status, color) = self.status();
        let subtitle = match (&self.data.engine, self.data.input_description()) {
            (MicEngineState::Failed(reason), _) => reason.clone(),
            (_, Some(input)) => format!("Listening to {input}"),
            (_, None) => "No microphone".to_string(),
        };
        let panel = ActionPanel::new().action(self.command(
            if enabled {
                "Turn Off Microphone Effects"
            } else {
                "Turn On Microphone Effects"
            },
            IconName::Power,
            MicCommand::SetEnabled(!enabled),
        ));
        Item::new("power", "Microphone Effects")
            .subtitle(subtitle)
            .icon(MIC_ICON)
            .keywords("microphone mic effects voice on off toggle enable disable noise")
            .accessory(Accessory::tag(status).color(color))
            .detail(self.status_detail())
            .actions(self.common(panel, cx))
    }

    fn status_detail(&self) -> Detail {
        let (status, color) = self.status();
        let mut detail = Detail::new()
            .markdown("## Microphone Effects\n\nYour microphone goes through the effects below and comes out as a virtual microphone, **Conchy Microphone**. It becomes the default, so every app records the cleaned-up voice.")
            .metadata(Metadata::label("Status", status).color(color))
            .metadata(Metadata::label(
                "Microphone",
                self.data.input_description().unwrap_or("None").to_string(),
            ))
            .metadata(Metadata::label(
                "Preset",
                self.data
                    .settings
                    .active_preset()
                    .unwrap_or_else(|| "Custom".to_string()),
            ))
            .metadata(Metadata::label("Chain", self.chain_summary()));
        let missing = self.data.missing_packages();
        if !missing.is_empty() {
            detail = detail
                .metadata(Metadata::label("Missing", missing.join(", ")).color(Color::Warning));
        }
        if let MicEngineState::Failed(reason) = &self.data.engine {
            detail = detail.metadata(Metadata::label("Error", reason.clone()).color(Color::Error));
        }
        detail
    }

    /// `Rumble › Denoise › Comp › Gain`: what the voice goes through.
    fn chain_summary(&self) -> String {
        Group::ALL
            .iter()
            .filter(|group| group.on(&self.data) && group.running(&self.data))
            .map(|group| group.short())
            .collect::<Vec<_>>()
            .join(" › ")
    }

    fn group_item(&self, group: Group, cx: &Context<Self>) -> Item {
        let data = &self.data;
        let on = group.on(data);
        let running = group.running(data);

        let tag = match (on, running) {
            (false, _) => Accessory::tag("Off").color(Color::Muted),
            (true, false) => Accessory::tag("Needs plugin")
                .with_icon(IconName::Warning)
                .color(Color::Warning),
            (true, true) => Accessory::tag(self.group_value(group)).color(Color::Success),
        };

        let subscriber = self.subscriber.clone();
        Item::new(format!("group:{}", group.id()), group.title())
            .subtitle(group.about())
            .icon(group.icon())
            .keywords(format!(
                "{} microphone effect",
                group.title().to_lowercase()
            ))
            .accessory(tag)
            .detail(Detail::new().form(move |form, cx| group_form(form, group, &subscriber, cx)))
            .actions(self.common(system::edit(), cx))
    }

    /// What the tag of a running group says: the one number that matters.
    fn group_value(&self, group: Group) -> String {
        let voice = &self.data.settings.voice;
        match group {
            Group::Noise => match voice.denoiser() {
                Denoiser::RnNoise => "RNNoise".into(),
                Denoiser::DeepFilter => "DeepFilterNet".into(),
                Denoiser::Off => "Off".into(),
            },
            Group::Rumble => format!("{:.0} Hz", voice.get(Param::HighpassHz)),
            Group::Gate => format!("{:.0} dB", voice.get(Param::GateThreshold)),
            Group::Equalizer => "On".into(),
            Group::Compressor => format!("{:.1}:1", voice.get(Param::CompRatio)),
            Group::Limiter => format!("{:.1} dB", voice.get(Param::LimiterCeiling)),
            Group::Gain => format!("{:+.1} dB", voice.get(Param::OutputGain)),
        }
    }

    fn preset_item(
        &self,
        preset: &services::mic::Preset,
        active: bool,
        cx: &Context<Self>,
    ) -> Item {
        let name = preset.name.clone();
        let mut panel = ActionPanel::new().action(self.command(
            "Apply Preset",
            IconName::Check,
            MicCommand::ApplyPreset(name.clone()),
        ));
        if !preset.builtin {
            panel = panel.action(
                self.command(
                    "Delete Preset",
                    IconName::Trash,
                    MicCommand::DeletePreset(name.clone()),
                )
                .destructive()
                .shortcut("ctrl-d"),
            );
        }

        let voice = &preset.voice;
        let summary = {
            let mut parts = Vec::new();
            match voice.denoiser() {
                Denoiser::RnNoise => parts.push("RNNoise"),
                Denoiser::DeepFilter => parts.push("DeepFilterNet"),
                Denoiser::Off => {}
            }
            for (param, label) in [
                (Param::GateOn, "gate"),
                (Param::EqOn, "equalizer"),
                (Param::CompOn, "compressor"),
                (Param::LimiterOn, "limiter"),
            ] {
                if voice.on(param) {
                    parts.push(label);
                }
            }
            if parts.is_empty() {
                "No effects".to_string()
            } else {
                parts.join(", ")
            }
        };

        let detail = Detail::new()
            .markdown(format!("## {name}"))
            .metadata(Metadata::label("Effects", summary.clone()))
            .metadata(Metadata::label(
                "Kind",
                if preset.builtin { "Built-in" } else { "Yours" },
            ));

        let mut item = Item::new(format!("preset:{name}"), name.clone())
            .subtitle(summary)
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

    fn input_item(&self, device: &services::InputDevice, cx: &Context<Self>) -> Item {
        let current = self.data.input.as_deref() == Some(device.name.as_str());
        let panel = ActionPanel::new().action(self.command(
            "Use This Microphone",
            MIC_ICON,
            MicCommand::SetInput(device.name.clone()),
        ));
        let mut item = Item::new(format!("input:{}", device.name), device.description.clone())
            .subtitle(device.name.clone())
            .icon(MIC_ICON)
            .keywords("input device microphone source")
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
        let data = &self.data;
        let (status, status_color) = self.status();
        let enabled = data.settings.enabled;

        let power = {
            let subscriber = self.subscriber.clone();
            Switch::new("mic-power", enabled).on_click(move |_, _, _| {
                subscriber.dispatch(MicCommand::SetEnabled(!enabled));
            })
        };

        let heading = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .child(
                h_flex()
                    .items_baseline()
                    .gap(Spacing::Small.pixels())
                    .child(
                        Label::new("Microphone")
                            .size(TextSize::Default)
                            .weight(FontWeight::MEDIUM),
                    )
                    .child(
                        Label::new(status)
                            .size(TextSize::XSmall)
                            .color(status_color),
                    ),
            )
            .child(power);

        // The chain, left to right: a stage that is on is lit, one waiting
        // for a plugin is a warning.
        let strip = h_flex()
            .w_full()
            .flex_wrap()
            .gap(Spacing::Small.pixels())
            .children(Group::ALL.iter().map(|group| {
                let on = group.on(data);
                let color = match (on, group.running(data)) {
                    (false, _) => Color::Disabled,
                    (true, false) => Color::Warning,
                    (true, true) => Color::Accent,
                };
                div()
                    .px(Spacing::Small.pixels())
                    .py(px(2.))
                    .rounded(Radius::Medium.pixels())
                    .bg(theme.colors.element_background)
                    .child(
                        Label::new(group.short())
                            .size(TextSize::XSmall)
                            .color(color),
                    )
            }));

        let footer = h_flex()
            .w_full()
            .justify_between()
            .child(
                Label::new(
                    data.settings
                        .active_preset()
                        .unwrap_or_else(|| "Custom voice".to_string()),
                )
                .size(TextSize::XSmall)
                .color(Color::Accent),
            )
            .child(
                Label::new(
                    data.input_description()
                        .unwrap_or("No microphone")
                        .to_string(),
                )
                .size(TextSize::XSmall)
                .color(Color::Muted),
            );

        div()
            .p(Spacing::Medium.pixels())
            .child(
                v_flex()
                    .w_full()
                    .p(Spacing::Medium.pixels())
                    .gap(px(8.))
                    .bg(theme.colors.surface_background)
                    .rounded(Radius::Large.pixels())
                    .child(heading)
                    .child(strip)
                    .child(footer),
            )
            .into_any_element()
    }
}

// -- the forms ------------------------------------------------------------------

/// A slider for `param`, in the unit it is read in.
fn number(
    form: &mut Form,
    subscriber: &MicSubscriber,
    param: Param,
    label: &'static str,
    description: &'static str,
    unit: &'static str,
    cx: &App,
) {
    let spec = param.spec();
    let value = subscriber.get().settings.voice.get(param);
    let subscriber = subscriber.clone();
    rows::number(
        form,
        Number {
            id: SharedString::from(format!("mic-{}", spec.key)),
            label,
            description,
            value: f64::from(value),
            min: f64::from(spec.min),
            max: f64::from(spec.max),
            step: f64::from(spec.step),
            unit: Some(unit),
            default: f64::from(spec.default),
        },
        move |v, _| subscriber.dispatch(MicCommand::Set(param, v as f32)),
        cx,
    );
}

fn switch(
    form: &mut Form,
    subscriber: &MicSubscriber,
    param: Param,
    label: &'static str,
    description: &'static str,
    cx: &App,
) {
    let on = subscriber.get().settings.voice.on(param);
    let subscriber = subscriber.clone();
    rows::toggle(
        form,
        SharedString::from(format!("mic-{}", param.spec().key)),
        label,
        description,
        on,
        move |on, _| subscriber.dispatch(MicCommand::Set(param, if on { 1.0 } else { 0.0 })),
        cx,
    );
}

/// The form beside a group of effects.
fn group_form(form: &mut Form, group: Group, subscriber: &MicSubscriber, cx: &App) {
    let data = subscriber.get();
    let voice = &data.settings.voice;
    rows::title(form, group.title(), group.about());

    match group {
        Group::Noise => {
            let choice = match voice.denoiser() {
                Denoiser::Off => 0,
                Denoiser::RnNoise => 1,
                Denoiser::DeepFilter => 2,
            };
            let on_pick = {
                let subscriber = subscriber.clone();
                move |ix: usize, _: &mut App| {
                    subscriber.dispatch(MicCommand::Set(Param::Denoiser, ix as f32));
                }
            };
            rows::choice(
                form,
                "mic-denoiser",
                "Engine",
                "RNNoise is light and fast; DeepFilterNet is heavier and cleaner",
                vec!["Off".into(), "RNNoise".into(), "DeepFilterNet".into()],
                Some(choice),
                on_pick,
                cx,
            );
            match voice.denoiser() {
                Denoiser::RnNoise => number(
                    form,
                    subscriber,
                    Param::RnnoiseVad,
                    "Voice threshold",
                    "How sure the network must be that someone talks before the sound passes. Higher cuts more, and may clip quiet words",
                    "%",
                    cx,
                ),
                Denoiser::DeepFilter => number(
                    form,
                    subscriber,
                    Param::DeepFilterLimit,
                    "Maximum reduction",
                    "The most noise it may take away. Lower keeps the voice more natural",
                    "dB",
                    cx,
                ),
                Denoiser::Off => {}
            }
        }
        Group::Rumble => {
            switch(
                form,
                subscriber,
                Param::HighpassOn,
                "Rumble filter",
                "Cut everything below the frequency",
                cx,
            );
            number(
                form,
                subscriber,
                Param::HighpassHz,
                "Cutoff",
                "Voices rarely go below 80 Hz",
                "Hz",
                cx,
            );
        }
        Group::Gate => {
            switch(
                form,
                subscriber,
                Param::GateOn,
                "Noise gate",
                "Quiet the microphone when nobody speaks",
                cx,
            );
            number(
                form,
                subscriber,
                Param::GateThreshold,
                "Threshold",
                "The gate opens above this level",
                "dB",
                cx,
            );
            number(
                form,
                subscriber,
                Param::GateReduction,
                "Closed level",
                "How far the sound drops while the gate is closed",
                "dB",
                cx,
            );
        }
        Group::Equalizer => {
            switch(
                form,
                subscriber,
                Param::EqOn,
                "Equalizer",
                "Shape the tone of the voice",
                cx,
            );
            number(
                form,
                subscriber,
                Param::EqLow,
                "Warmth",
                "Body of the voice, around 120 Hz",
                "dB",
                cx,
            );
            number(
                form,
                subscriber,
                Param::EqMud,
                "Mud",
                "Boxy, muffled sound, around 300 Hz. Cut it to clear the voice",
                "dB",
                cx,
            );
            number(
                form,
                subscriber,
                Param::EqPresence,
                "Presence",
                "Clarity and intelligibility, around 3.5 kHz",
                "dB",
                cx,
            );
            number(
                form,
                subscriber,
                Param::EqAir,
                "Air",
                "Brightness above 10 kHz",
                "dB",
                cx,
            );
        }
        Group::Compressor => {
            switch(
                form,
                subscriber,
                Param::CompOn,
                "Compressor",
                "Bring loud and quiet speech closer together",
                cx,
            );
            number(
                form,
                subscriber,
                Param::CompThreshold,
                "Threshold",
                "Sound above this level is compressed",
                "dB",
                cx,
            );
            number(
                form,
                subscriber,
                Param::CompRatio,
                "Ratio",
                "How hard the loud part is pushed down",
                ":1",
                cx,
            );
            number(
                form,
                subscriber,
                Param::CompMakeup,
                "Makeup gain",
                "Brings the level back up afterwards",
                "dB",
                cx,
            );
        }
        Group::Limiter => {
            switch(
                form,
                subscriber,
                Param::LimiterOn,
                "Limiter",
                "Nothing gets louder than the ceiling",
                cx,
            );
            number(
                form,
                subscriber,
                Param::LimiterCeiling,
                "Ceiling",
                "The loudest the voice may get",
                "dB",
                cx,
            );
        }
        Group::Gain => {
            number(
                form,
                subscriber,
                Param::OutputGain,
                "Output gain",
                "Volume of the voice after all the effects",
                "dB",
                cx,
            );
        }
    }

    missing_note(form, group, &data, subscriber, cx);
}

/// What to install when the group asks for a plugin that is not there.
fn missing_note(
    form: &mut Form,
    group: Group,
    data: &MicData,
    subscriber: &MicSubscriber,
    cx: &App,
) {
    if !group.on(data) || group.running(data) {
        return;
    }
    let package = match group {
        Group::Noise => match data.settings.voice.denoiser() {
            Denoiser::RnNoise => chain::PACKAGE_RNNOISE,
            _ => chain::PACKAGE_DEEPFILTER,
        },
        _ => chain::PACKAGE_LSP,
    };
    rows::heading(form, "Plugin missing");
    rows::note(
        form,
        format!("This effect is skipped until {package} is installed:  sudo pacman -S {package}"),
        Color::Warning,
    );
    let subscriber = subscriber.clone();
    rows::action(
        form,
        "I installed it, look again",
        "Looks for plugins and rebuilds the chain",
        move |_, _| {
            subscriber.dispatch(MicCommand::Rescan);
            Outcome::Stay
        },
        cx,
    );
}

impl LauncherPage for MicModule {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let settings = &self.data.settings;
        let active = settings.active_preset();
        let (builtin, user): (Vec<_>, Vec<_>) =
            settings.presets().partition(|preset| preset.builtin);

        let mut list = List::new()
            .show_detail(true)
            .placeholder("Search effects, presets and microphones...")
            .empty(
                EmptyView::new("Nothing matches")
                    .description("Effects, presets and microphones are searched")
                    .icon(MIC_ICON),
            )
            .section(Section::untitled().item(self.power_item(cx)))
            .section(
                Section::new("Effects").items(
                    Group::ALL
                        .iter()
                        .map(|group| self.group_item(*group, cx))
                        .collect::<Vec<_>>(),
                ),
            )
            .section(
                Section::new("Presets").items(
                    builtin
                        .iter()
                        .map(|preset| {
                            self.preset_item(preset, active.as_deref() == Some(&preset.name), cx)
                        })
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
        if !self.data.inputs.is_empty() {
            list = list.section(
                Section::new("Microphone").items(
                    self.data
                        .inputs
                        .iter()
                        .map(|device| self.input_item(device, cx))
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
