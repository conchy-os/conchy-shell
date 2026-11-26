//! What the microphone is set to, the presets, and the file it is kept in
//! (`~/.config/conchy/mic.kdl`).
//!
//! Everything a voice can be tuned by is a [`Param`]: one table says what
//! each is called, how far it goes and what it starts at, and the chain, the
//! file, the presets and the UI all go by that table.

use std::path::PathBuf;

use anyhow::{Context, anyhow};
use kdl::KdlValue;

use crate::{fsutil, kdl_edit};

/// One setting of the voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Param {
    /// 0 off, 1 RNNoise, 2 DeepFilterNet.
    Denoiser,
    /// RNNoise: how sure it must be that someone talks (%).
    RnnoiseVad,
    /// DeepFilterNet: the most noise it may take away (dB).
    DeepFilterLimit,
    HighpassOn,
    HighpassHz,
    GateOn,
    GateThreshold,
    GateReduction,
    EqOn,
    EqLow,
    EqMud,
    EqPresence,
    EqAir,
    CompOn,
    CompThreshold,
    CompRatio,
    CompMakeup,
    LimiterOn,
    LimiterCeiling,
    OutputGain,
}

/// What a [`Param`] is: its name in the file and its range.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub key: &'static str,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default: f32,
}

const fn spec(key: &'static str, min: f32, max: f32, step: f32, default: f32) -> Spec {
    Spec {
        key,
        min,
        max,
        step,
        default,
    }
}

impl Param {
    pub const ALL: [Param; 20] = [
        Param::Denoiser,
        Param::RnnoiseVad,
        Param::DeepFilterLimit,
        Param::HighpassOn,
        Param::HighpassHz,
        Param::GateOn,
        Param::GateThreshold,
        Param::GateReduction,
        Param::EqOn,
        Param::EqLow,
        Param::EqMud,
        Param::EqPresence,
        Param::EqAir,
        Param::CompOn,
        Param::CompThreshold,
        Param::CompRatio,
        Param::CompMakeup,
        Param::LimiterOn,
        Param::LimiterCeiling,
        Param::OutputGain,
    ];

    pub fn spec(self) -> Spec {
        match self {
            Param::Denoiser => spec("denoiser", 0.0, 2.0, 1.0, 0.0),
            Param::RnnoiseVad => spec("rnnoise-vad", 0.0, 95.0, 5.0, 40.0),
            Param::DeepFilterLimit => spec("deepfilter-limit", 0.0, 100.0, 5.0, 70.0),
            Param::HighpassOn => spec("highpass", 0.0, 1.0, 1.0, 1.0),
            Param::HighpassHz => spec("highpass-hz", 40.0, 400.0, 5.0, 80.0),
            Param::GateOn => spec("gate", 0.0, 1.0, 1.0, 0.0),
            Param::GateThreshold => spec("gate-threshold", -80.0, -20.0, 1.0, -50.0),
            Param::GateReduction => spec("gate-reduction", -60.0, -6.0, 1.0, -25.0),
            Param::EqOn => spec("eq", 0.0, 1.0, 1.0, 0.0),
            Param::EqLow => spec("eq-low", -8.0, 8.0, 0.5, 0.0),
            Param::EqMud => spec("eq-mud", -8.0, 4.0, 0.5, 0.0),
            Param::EqPresence => spec("eq-presence", -8.0, 8.0, 0.5, 0.0),
            Param::EqAir => spec("eq-air", -8.0, 8.0, 0.5, 0.0),
            Param::CompOn => spec("compressor", 0.0, 1.0, 1.0, 0.0),
            Param::CompThreshold => spec("compressor-threshold", -50.0, 0.0, 1.0, -22.0),
            Param::CompRatio => spec("compressor-ratio", 1.0, 10.0, 0.5, 3.0),
            Param::CompMakeup => spec("compressor-makeup", 0.0, 24.0, 0.5, 4.0),
            Param::LimiterOn => spec("limiter", 0.0, 1.0, 1.0, 0.0),
            Param::LimiterCeiling => spec("limiter-ceiling", -12.0, 0.0, 0.5, -1.0),
            Param::OutputGain => spec("output-gain", -20.0, 20.0, 0.5, 0.0),
        }
    }

    fn index(self) -> usize {
        Param::ALL
            .iter()
            .position(|p| *p == self)
            .expect("every param is in ALL")
    }

    pub fn from_key(key: &str) -> Option<Param> {
        Param::ALL.into_iter().find(|p| p.spec().key == key)
    }
}

/// Which noise remover the voice goes through first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denoiser {
    Off,
    RnNoise,
    DeepFilter,
}

/// The value of every [`Param`].
#[derive(Debug, Clone, PartialEq)]
pub struct Voice {
    values: [f32; Param::ALL.len()],
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            values: Param::ALL.map(|p| p.spec().default),
        }
    }
}

fn round_to(value: f32, step: f32) -> f32 {
    let rounded = (value / step).round() * step;
    // Drop the float noise a division leaves (`0.30000001`).
    (rounded * 100.0).round() / 100.0
}

impl Voice {
    pub fn get(&self, param: Param) -> f32 {
        self.values[param.index()]
    }

    pub fn on(&self, param: Param) -> bool {
        self.get(param) >= 0.5
    }

    pub fn set(&mut self, param: Param, value: f32) {
        let spec = param.spec();
        let value = if value.is_finite() {
            value
        } else {
            spec.default
        };
        self.values[param.index()] = round_to(value.clamp(spec.min, spec.max), spec.step);
    }

    pub fn denoiser(&self) -> Denoiser {
        match self.get(Param::Denoiser).round() as i32 {
            1 => Denoiser::RnNoise,
            2 => Denoiser::DeepFilter,
            _ => Denoiser::Off,
        }
    }

    /// Equal to `other` within what the sliders can tell apart.
    pub fn same_as(&self, other: &Voice) -> bool {
        self.values
            .iter()
            .zip(&other.values)
            .all(|(a, b)| (a - b).abs() < 0.01)
    }

    /// The defaults, with these changes.
    fn with(changes: &[(Param, f32)]) -> Voice {
        let mut voice = Voice::default();
        for (param, value) in changes {
            voice.set(*param, *value);
        }
        voice
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub name: String,
    pub voice: Voice,
    /// Ships with the shell; cannot be deleted or overwritten.
    pub builtin: bool,
}

/// The presets that ship with the shell.
pub fn builtin_presets() -> Vec<Preset> {
    use Param::*;
    let table: [(&str, Voice); 7] = [
        ("Clean", Voice::default()),
        (
            "Voice Call",
            Voice::with(&[
                (Denoiser, 1.0),
                (RnnoiseVad, 40.0),
                (HighpassHz, 100.0),
                (EqOn, 1.0),
                (EqMud, -1.5),
                (EqPresence, 2.0),
                (CompOn, 1.0),
                (CompThreshold, -22.0),
                (CompRatio, 3.0),
                (CompMakeup, 4.0),
                (LimiterOn, 1.0),
                (LimiterCeiling, -1.0),
            ]),
        ),
        (
            "Streaming",
            Voice::with(&[
                (Denoiser, 1.0),
                (RnnoiseVad, 50.0),
                (HighpassHz, 100.0),
                (GateOn, 1.0),
                (GateThreshold, -45.0),
                (GateReduction, -30.0),
                (EqOn, 1.0),
                (EqMud, -2.0),
                (EqPresence, 3.0),
                (EqAir, 2.0),
                (CompOn, 1.0),
                (CompThreshold, -20.0),
                (CompRatio, 4.0),
                (CompMakeup, 6.0),
                (LimiterOn, 1.0),
                (LimiterCeiling, -1.0),
            ]),
        ),
        (
            "Studio Voice",
            Voice::with(&[
                (Denoiser, 2.0),
                (DeepFilterLimit, 40.0),
                (GateOn, 1.0),
                (GateThreshold, -52.0),
                (GateReduction, -20.0),
                (EqOn, 1.0),
                (EqLow, 1.5),
                (EqPresence, 2.0),
                (EqAir, 1.5),
                (CompOn, 1.0),
                (CompThreshold, -20.0),
                (CompRatio, 2.5),
                (CompMakeup, 4.0),
                (LimiterOn, 1.0),
                (LimiterCeiling, -1.0),
            ]),
        ),
        (
            "Podcast",
            Voice::with(&[
                (Denoiser, 2.0),
                (DeepFilterLimit, 50.0),
                (HighpassHz, 90.0),
                (GateOn, 1.0),
                (GateThreshold, -48.0),
                (EqOn, 1.0),
                (EqLow, 2.0),
                (EqMud, -2.0),
                (EqPresence, 2.5),
                (EqAir, 1.0),
                (CompOn, 1.0),
                (CompThreshold, -18.0),
                (CompRatio, 3.5),
                (CompMakeup, 6.0),
                (LimiterOn, 1.0),
                (LimiterCeiling, -1.5),
            ]),
        ),
        (
            "Noisy Room",
            Voice::with(&[
                (Denoiser, 2.0),
                (DeepFilterLimit, 100.0),
                (HighpassHz, 150.0),
                (GateOn, 1.0),
                (GateThreshold, -40.0),
                (GateReduction, -35.0),
                (CompOn, 1.0),
                (CompThreshold, -24.0),
                (CompRatio, 4.0),
                (CompMakeup, 8.0),
                (LimiterOn, 1.0),
                (LimiterCeiling, -2.0),
            ]),
        ),
        (
            "Quiet Mic Boost",
            Voice::with(&[
                (CompOn, 1.0),
                (CompThreshold, -32.0),
                (CompRatio, 3.0),
                (CompMakeup, 14.0),
                (LimiterOn, 1.0),
                (LimiterCeiling, -1.0),
            ]),
        ),
    ];
    table
        .into_iter()
        .map(|(name, voice)| Preset {
            name: name.to_string(),
            voice,
            builtin: true,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
    /// The microphone goes through the chain.
    pub enabled: bool,
    pub voice: Voice,
    /// The microphone to take the voice from (its PipeWire node name);
    /// `None` follows the system's default.
    pub input: Option<String>,
    pub user_presets: Vec<Preset>,
}

impl Settings {
    /// Built-in presets, then the user's.
    pub fn presets(&self) -> impl Iterator<Item = Preset> + '_ {
        builtin_presets()
            .into_iter()
            .chain(self.user_presets.iter().cloned())
    }

    /// The preset the voice is set to, if it is one.
    pub fn active_preset(&self) -> Option<String> {
        self.presets()
            .find(|preset| preset.voice.same_as(&self.voice))
            .map(|preset| preset.name)
    }

    /// Switch to the preset called `name`. False when there is none.
    pub fn apply_preset(&mut self, name: &str) -> bool {
        let Some(preset) = self.presets().find(|p| p.name.eq_ignore_ascii_case(name)) else {
            return false;
        };
        self.voice = preset.voice;
        true
    }

    /// Keep the current voice as a preset called `name`. A built-in name is
    /// refused; a user name is overwritten.
    pub fn save_preset(&mut self, name: &str) -> anyhow::Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(anyhow!("a preset needs a name"));
        }
        if builtin_presets()
            .iter()
            .any(|preset| preset.name.eq_ignore_ascii_case(name))
        {
            return Err(anyhow!("“{name}” is a built-in preset"));
        }
        let preset = Preset {
            name: name.to_string(),
            voice: self.voice.clone(),
            builtin: false,
        };
        match self
            .user_presets
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(name))
        {
            Some(slot) => *slot = preset,
            None => self.user_presets.push(preset),
        }
        Ok(())
    }

    pub fn delete_preset(&mut self, name: &str) -> bool {
        let before = self.user_presets.len();
        self.user_presets
            .retain(|p| !p.name.eq_ignore_ascii_case(name));
        self.user_presets.len() != before
    }
}

// -- the file -----------------------------------------------------------------

pub fn path() -> anyhow::Result<PathBuf> {
    Ok(fsutil::config_home()?.join("conchy/mic.kdl"))
}

pub fn load() -> anyhow::Result<Settings> {
    let path = path()?;
    match fsutil::read_optional(&path)? {
        Some(text) => {
            from_kdl(&text).with_context(|| format!("Failed to parse {}", path.display()))
        }
        None => Ok(Settings::default()),
    }
}

pub fn save(settings: &Settings) -> anyhow::Result<()> {
    fsutil::write_through(&path()?, &to_kdl(settings))
}

fn quoted(text: &str) -> String {
    KdlValue::String(text.to_string()).to_string()
}

fn voice_lines(voice: &Voice, indent: &str) -> String {
    Param::ALL
        .into_iter()
        .map(|p| format!("{indent}{} {:.2}\n", p.spec().key, voice.get(p)))
        .collect()
}

pub fn to_kdl(settings: &Settings) -> String {
    let mut out = String::from(
        "// Written by the shell's microphone settings (;mic). Gains are in dB; a stage is 1 (on) or 0.\n",
    );
    out.push_str(&format!("enabled {}\n", settings.enabled));
    if let Some(input) = &settings.input {
        out.push_str(&format!("input {}\n", quoted(input)));
    }
    out.push_str(&voice_lines(&settings.voice, ""));
    for preset in &settings.user_presets {
        out.push_str(&format!(
            "preset {} {{\n{}}}\n",
            quoted(&preset.name),
            voice_lines(&preset.voice, "    ")
        ));
    }
    out
}

fn read_value(node: &kdl::KdlNode) -> Option<f32> {
    let value = node
        .entries()
        .iter()
        .find(|entry| entry.name().is_none())?
        .value();
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|i| i as f64))
        .map(|v| v as f32)
}

/// Every param the nodes name, on top of `voice`.
fn read_voice<'a>(voice: &mut Voice, nodes: impl IntoIterator<Item = &'a kdl::KdlNode>) {
    for node in nodes {
        if let (Some(param), Some(value)) = (Param::from_key(node.name().value()), read_value(node))
        {
            voice.set(param, value);
        }
    }
}

pub fn from_kdl(text: &str) -> anyhow::Result<Settings> {
    let doc = kdl_edit::parse(text)?;
    let mut settings = Settings::default();

    if let Some(enabled) = kdl_edit::arg_bool(&doc, &["enabled"]) {
        settings.enabled = enabled;
    }
    settings.input = kdl_edit::arg_str(&doc, &["input"]).filter(|name| !name.is_empty());
    read_voice(&mut settings.voice, doc.nodes());

    for node in doc.nodes().iter().filter(|n| n.name().value() == "preset") {
        let Some(name) = node
            .entries()
            .iter()
            .find(|entry| entry.name().is_none())
            .and_then(|entry| entry.value().as_string())
            .map(str::to_string)
        else {
            continue;
        };
        let mut voice = Voice::default();
        read_voice(
            &mut voice,
            node.children().iter().flat_map(|block| block.nodes()),
        );
        settings.user_presets.push(Preset {
            name,
            voice,
            builtin: false,
        });
    }
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_preset_is_in_range_and_named_once() {
        let presets = builtin_presets();
        for preset in &presets {
            for param in Param::ALL {
                let spec = param.spec();
                let value = preset.voice.get(param);
                assert!(
                    (spec.min..=spec.max).contains(&value),
                    "{} / {}: {value}",
                    preset.name,
                    spec.key
                );
            }
        }
        let mut names: Vec<_> = presets.iter().map(|p| p.name.to_lowercase()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), presets.len());
        assert_eq!(presets[0].voice, Voice::default());
    }

    #[test]
    fn param_keys_are_unique_and_defaults_in_range() {
        let mut keys: Vec<_> = Param::ALL.iter().map(|p| p.spec().key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Param::ALL.len());
        for param in Param::ALL {
            let spec = param.spec();
            assert!(
                spec.min <= spec.default && spec.default <= spec.max,
                "{}",
                spec.key
            );
            assert_eq!(Param::from_key(spec.key), Some(param));
        }
    }

    #[test]
    fn values_are_clamped_and_rounded() {
        let mut voice = Voice::default();
        voice.set(Param::EqLow, 40.0);
        assert_eq!(voice.get(Param::EqLow), 8.0);
        voice.set(Param::EqPresence, 1.3);
        assert_eq!(voice.get(Param::EqPresence), 1.5);
        voice.set(Param::GateThreshold, f32::NAN);
        assert_eq!(voice.get(Param::GateThreshold), -50.0);
        voice.set(Param::Denoiser, 2.0);
        assert_eq!(voice.denoiser(), Denoiser::DeepFilter);
        assert!(voice.on(Param::HighpassOn));
    }

    #[test]
    fn the_active_preset_follows_the_voice() {
        let mut settings = Settings::default();
        assert_eq!(settings.active_preset().as_deref(), Some("Clean"));
        assert!(settings.apply_preset("podcast"));
        assert_eq!(settings.active_preset().as_deref(), Some("Podcast"));
        settings.voice.set(Param::CompRatio, 9.0);
        assert_eq!(settings.active_preset(), None);
        assert!(!settings.apply_preset("nonexistent"));
    }

    #[test]
    fn user_presets_save_overwrite_and_delete() {
        let mut settings = Settings::default();
        settings.voice.set(Param::OutputGain, 3.0);
        settings.save_preset("  Mine ").unwrap();
        assert_eq!(settings.user_presets.len(), 1);
        assert_eq!(settings.active_preset().as_deref(), Some("Mine"));

        settings.voice.set(Param::OutputGain, 5.0);
        settings.save_preset("mine").unwrap();
        assert_eq!(settings.user_presets.len(), 1);
        assert_eq!(settings.user_presets[0].voice.get(Param::OutputGain), 5.0);

        assert!(settings.save_preset("Podcast").is_err());
        assert!(settings.save_preset("   ").is_err());
        assert!(settings.delete_preset("Mine"));
        assert!(!settings.delete_preset("Mine"));
    }

    #[test]
    fn settings_round_trip_through_kdl() {
        let mut settings = Settings {
            enabled: true,
            input: Some("alsa_input.pci-0000_00_1b.0.analog-stereo".into()),
            ..Settings::default()
        };
        settings.apply_preset("Studio Voice");
        settings.voice.set(Param::OutputGain, -2.5);
        settings.save_preset("My \"mix\"").unwrap();
        settings.apply_preset("Clean");

        let text = to_kdl(&settings);
        let back = from_kdl(&text).unwrap();
        assert_eq!(back, settings, "{text}");
    }

    #[test]
    fn a_hand_edited_file_is_read_leniently() {
        let settings =
            from_kdl("enabled true\neq-low 99\ngate-threshold -999\nunknown 3\ndenoiser 1\n")
                .unwrap();
        assert!(settings.enabled);
        assert_eq!(settings.voice.get(Param::EqLow), 8.0);
        assert_eq!(settings.voice.get(Param::GateThreshold), -80.0);
        assert_eq!(settings.voice.denoiser(), Denoiser::RnNoise);
        // What the file does not say stays at its default.
        assert_eq!(settings.voice.get(Param::CompRatio), 3.0);

        assert_eq!(from_kdl("").unwrap(), Settings::default());
    }
}
