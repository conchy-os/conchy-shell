//! What the equalizer is set to, the presets, and the file it is kept in
//! (`~/.config/conchy/equalizer.kdl`).

use std::path::PathBuf;

use anyhow::{Context, anyhow};
use kdl::KdlValue;

use super::dsp::{BAND_COUNT, MAX_GAIN_DB, MAX_PREAMP_DB, headroom_db};
use crate::{fsutil, kdl_edit};

/// Gains smaller than this are the same setting as no gain.
const GAIN_STEP: f32 = 0.05;

/// A band layout and the preamp that goes with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    /// Gain of each band in dB.
    pub gains: [f32; BAND_COUNT],
    /// Gain ahead of the bands, in dB.
    pub preamp_db: f32,
}

impl Default for Curve {
    fn default() -> Self {
        Self {
            gains: [0.0; BAND_COUNT],
            preamp_db: 0.0,
        }
    }
}

impl Curve {
    /// Equal to `other` within what the sliders can tell apart.
    pub fn same_as(&self, other: &Curve) -> bool {
        (self.preamp_db - other.preamp_db).abs() < GAIN_STEP
            && self
                .gains
                .iter()
                .zip(&other.gains)
                .all(|(a, b)| (a - b).abs() < GAIN_STEP)
    }

    pub fn is_flat(&self) -> bool {
        self.same_as(&Curve::default())
    }

    fn clamped(mut self) -> Self {
        for gain in &mut self.gains {
            *gain = round_step(gain.clamp(-MAX_GAIN_DB, MAX_GAIN_DB));
        }
        self.preamp_db = round_step(self.preamp_db.clamp(-MAX_PREAMP_DB, MAX_PREAMP_DB));
        self
    }
}

fn round_step(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub name: String,
    pub curve: Curve,
    /// Ships with the shell; cannot be deleted or overwritten.
    pub builtin: bool,
}

/// The presets that ship with the shell.
pub fn builtin_presets() -> Vec<Preset> {
    const TABLE: [(&str, [f32; BAND_COUNT]); 11] = [
        ("Flat", [0.0; BAND_COUNT]),
        (
            "Bass Boost",
            [6.0, 5.0, 4.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        ),
        (
            "Bass Reducer",
            [-6.0, -5.0, -4.0, -2.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        ),
        (
            "Treble Boost",
            [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 4.0, 5.0, 6.0],
        ),
        (
            "Vocal",
            [-2.0, -2.0, -1.0, 1.0, 3.0, 4.0, 4.0, 3.0, 1.0, 0.0],
        ),
        ("Rock", [5.0, 4.0, 3.0, 1.0, -1.0, -1.0, 1.0, 3.0, 4.0, 5.0]),
        (
            "Pop",
            [-1.0, 1.0, 3.0, 4.0, 3.0, 0.0, -1.0, -1.0, -1.0, -1.0],
        ),
        ("Jazz", [4.0, 3.0, 1.0, 2.0, -2.0, -2.0, 0.0, 1.0, 3.0, 4.0]),
        (
            "Classical",
            [4.0, 3.0, 3.0, 2.0, -1.0, -1.0, 0.0, 2.0, 3.0, 4.0],
        ),
        (
            "Electronic",
            [5.0, 4.0, 1.0, 0.0, -2.0, 2.0, 1.0, 1.0, 4.0, 5.0],
        ),
        (
            "Loudness",
            [6.0, 4.0, 0.0, 0.0, -2.0, 0.0, 0.0, -1.0, 3.0, 5.0],
        ),
    ];
    TABLE
        .into_iter()
        .map(|(name, gains)| Preset {
            name: name.to_string(),
            curve: Curve {
                gains,
                preamp_db: 0.0,
            },
            builtin: true,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The equalizer is in the audio path.
    pub enabled: bool,
    pub curve: Curve,
    /// Lower the preamp by what the bands boost, so the sum does not clip.
    pub auto_preamp: bool,
    /// The sink the equalized sound goes to (its PipeWire node name); `None`
    /// follows whatever was the default when the equalizer was turned on.
    pub output: Option<String>,
    pub user_presets: Vec<Preset>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            curve: Curve::default(),
            auto_preamp: true,
            output: None,
            user_presets: Vec::new(),
        }
    }
}

impl Settings {
    /// The preamp the filters actually get, in dB.
    pub fn effective_preamp_db(&self) -> f32 {
        if self.auto_preamp {
            self.curve.preamp_db - headroom_db(&self.curve.gains)
        } else {
            self.curve.preamp_db
        }
    }

    /// Built-in presets, then the user's.
    pub fn presets(&self) -> impl Iterator<Item = Preset> + '_ {
        builtin_presets()
            .into_iter()
            .chain(self.user_presets.iter().cloned())
    }

    /// The preset the curve is set to, if it is one.
    pub fn active_preset(&self) -> Option<String> {
        self.presets()
            .find(|preset| preset.curve.same_as(&self.curve))
            .map(|preset| preset.name)
    }

    pub fn set_band(&mut self, band: usize, gain: f32) {
        if let Some(slot) = self.curve.gains.get_mut(band) {
            *slot = round_step(gain.clamp(-MAX_GAIN_DB, MAX_GAIN_DB));
        }
    }

    pub fn set_preamp(&mut self, db: f32) {
        self.curve.preamp_db = round_step(db.clamp(-MAX_PREAMP_DB, MAX_PREAMP_DB));
    }

    /// Switch to the preset called `name`. False when there is none.
    pub fn apply_preset(&mut self, name: &str) -> bool {
        let Some(preset) = self.presets().find(|p| p.name.eq_ignore_ascii_case(name)) else {
            return false;
        };
        self.curve = preset.curve;
        true
    }

    /// Keep the current curve as a preset called `name`. A built-in name is
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
            curve: self.curve.clone(),
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
    Ok(fsutil::config_home()?.join("conchy/equalizer.kdl"))
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

fn numbers(values: &[f32]) -> String {
    values
        .iter()
        .map(|v| format!("{v:.1}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quoted(text: &str) -> String {
    KdlValue::String(text.to_string()).to_string()
}

pub fn to_kdl(settings: &Settings) -> String {
    let mut out = String::from(
        "// Written by the shell's equalizer (;equ). Gains are in dB, lowest band first.\n",
    );
    out.push_str(&format!("enabled {}\n", settings.enabled));
    out.push_str(&format!("auto-preamp {}\n", settings.auto_preamp));
    out.push_str(&format!("preamp {:.1}\n", settings.curve.preamp_db));
    out.push_str(&format!("bands {}\n", numbers(&settings.curve.gains)));
    if let Some(output) = &settings.output {
        out.push_str(&format!("output {}\n", quoted(output)));
    }
    for preset in &settings.user_presets {
        out.push_str(&format!(
            "preset {} preamp={:.1} {{\n    bands {}\n}}\n",
            quoted(&preset.name),
            preset.curve.preamp_db,
            numbers(&preset.curve.gains),
        ));
    }
    out
}

fn read_bands(node: &kdl::KdlNode) -> Option<[f32; BAND_COUNT]> {
    let values: Vec<f32> = node
        .entries()
        .iter()
        .filter(|entry| entry.name().is_none())
        .filter_map(|entry| {
            let value = entry.value();
            value.as_f64().or_else(|| value.as_i64().map(|i| i as f64))
        })
        .map(|v| v as f32)
        .collect();
    values.try_into().ok()
}

pub fn from_kdl(text: &str) -> anyhow::Result<Settings> {
    let doc = kdl_edit::parse(text)?;
    let mut settings = Settings::default();

    if let Some(enabled) = kdl_edit::arg_bool(&doc, &["enabled"]) {
        settings.enabled = enabled;
    }
    if let Some(auto) = kdl_edit::arg_bool(&doc, &["auto-preamp"]) {
        settings.auto_preamp = auto;
    }
    if let Some(preamp) = kdl_edit::arg_f64(&doc, &["preamp"]) {
        settings.curve.preamp_db = preamp as f32;
    }
    if let Some(gains) = kdl_edit::node(&doc, &["bands"]).and_then(read_bands) {
        settings.curve.gains = gains;
    }
    settings.output = kdl_edit::arg_str(&doc, &["output"]).filter(|name| !name.is_empty());
    settings.curve = std::mem::take(&mut settings.curve).clamped();

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
        let preamp = node
            .entries()
            .iter()
            .find(|entry| entry.name().is_some_and(|n| n.value() == "preamp"))
            .and_then(|entry| entry.value().as_f64())
            .unwrap_or(0.0) as f32;
        let Some(gains) = node
            .children()
            .and_then(|block| block.nodes().iter().find(|n| n.name().value() == "bands"))
            .and_then(read_bands)
        else {
            continue;
        };
        settings.user_presets.push(Preset {
            name,
            curve: Curve {
                gains,
                preamp_db: preamp,
            }
            .clamped(),
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
            assert!(
                preset.curve.gains.iter().all(|g| g.abs() <= MAX_GAIN_DB),
                "{}",
                preset.name
            );
        }
        let mut names: Vec<_> = presets.iter().map(|p| p.name.to_lowercase()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), presets.len());
        assert!(presets[0].curve.is_flat());
    }

    #[test]
    fn auto_preamp_takes_the_boost_off() {
        let mut settings = Settings::default();
        assert_eq!(settings.effective_preamp_db(), 0.0);
        settings.set_band(5, 6.0);
        let boosted = settings.effective_preamp_db();
        assert!(boosted < -5.0, "{boosted}");
        settings.auto_preamp = false;
        assert_eq!(settings.effective_preamp_db(), 0.0);
        settings.set_preamp(-3.0);
        assert_eq!(settings.effective_preamp_db(), -3.0);
    }

    #[test]
    fn values_are_clamped_and_rounded() {
        let mut settings = Settings::default();
        settings.set_band(2, 40.0);
        settings.set_band(3, -40.0);
        settings.set_band(4, 1.234);
        settings.set_band(99, 5.0);
        settings.set_preamp(-99.0);
        assert_eq!(settings.curve.gains[2], MAX_GAIN_DB);
        assert_eq!(settings.curve.gains[3], -MAX_GAIN_DB);
        assert_eq!(settings.curve.gains[4], 1.2);
        assert_eq!(settings.curve.preamp_db, -MAX_PREAMP_DB);
    }

    #[test]
    fn the_active_preset_follows_the_curve() {
        let mut settings = Settings::default();
        assert_eq!(settings.active_preset().as_deref(), Some("Flat"));
        assert!(settings.apply_preset("rock"));
        assert_eq!(settings.active_preset().as_deref(), Some("Rock"));
        settings.set_band(0, 0.0);
        assert_eq!(settings.active_preset(), None);
        assert!(!settings.apply_preset("nonexistent"));
    }

    #[test]
    fn user_presets_save_overwrite_and_delete() {
        let mut settings = Settings::default();
        settings.set_band(1, 3.0);
        settings.save_preset("  Mine ").unwrap();
        assert_eq!(settings.user_presets.len(), 1);
        assert_eq!(settings.active_preset().as_deref(), Some("Mine"));

        settings.set_band(1, 5.0);
        settings.save_preset("mine").unwrap();
        assert_eq!(settings.user_presets.len(), 1);
        assert_eq!(settings.user_presets[0].curve.gains[1], 5.0);

        assert!(settings.save_preset("Rock").is_err());
        assert!(settings.save_preset("   ").is_err());
        assert!(settings.delete_preset("Mine"));
        assert!(!settings.delete_preset("Mine"));
    }

    #[test]
    fn settings_round_trip_through_kdl() {
        let mut settings = Settings {
            enabled: true,
            auto_preamp: false,
            output: Some("alsa_output.pci-0000_00_1b.0.analog-stereo".into()),
            ..Settings::default()
        };
        settings.set_band(0, 4.5);
        settings.set_band(9, -2.0);
        settings.set_preamp(-1.5);
        settings.set_band(5, 1.0);
        settings.save_preset("My \"mix\"").unwrap();
        settings.set_band(5, 0.0);

        let text = to_kdl(&settings);
        let back = from_kdl(&text).unwrap();
        assert_eq!(back, settings, "{text}");
    }

    #[test]
    fn a_hand_edited_file_is_read_leniently() {
        let text =
            "enabled true\nbands 99 0 0 0 0 0 0 0 0 -99\npreset \"Broken\" {\n    bands 1 2 3\n}\n";
        let settings = from_kdl(text).unwrap();
        assert!(settings.enabled);
        assert_eq!(settings.curve.gains[0], MAX_GAIN_DB);
        assert_eq!(settings.curve.gains[9], -MAX_GAIN_DB);
        assert!(settings.user_presets.is_empty());

        assert_eq!(from_kdl("").unwrap(), Settings::default());
        // Too few bands: keep the flat default instead of guessing.
        assert!(from_kdl("bands 1 2 3\n").unwrap().curve.is_flat());
    }
}
