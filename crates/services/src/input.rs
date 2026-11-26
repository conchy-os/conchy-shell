//! Mouse, touchpad and keyboard settings, read from and written to the
//! running compositor.
//!
//! * **Conchy WM** keeps them in the `input` block of `wm.kdl`; a change is a
//!   one-node edit of that file, which the compositor reloads on its own.
//! * **Hyprland** takes them at run time (`hyprctl eval` with its Lua API, or
//!   `keyword` on a legacy config). Nothing there writes the user's own
//!   config, so each change is also kept in `~/.config/conchy/input.kdl` and
//!   applied again by [`restore`] when the shell starts.
//!
//! A [`Change`] names one thing the user changed; both backends turn it into
//! the same list of [`Setting`]s, so the two stay in step.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, anyhow, bail};
use kdl::{KdlDocument, KdlValue};

use crate::compositor::CompositorBackend;
use crate::xkb::XkbSet;
use crate::{fsutil, kdl_edit, wm_config};

// -- model --------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccelProfile {
    /// What the driver does without being told.
    #[default]
    Adaptive,
    /// No acceleration: the pointer moves as far as the hand does.
    Flat,
}

impl AccelProfile {
    pub fn name(self) -> &'static str {
        match self {
            Self::Adaptive => "adaptive",
            Self::Flat => "flat",
        }
    }

    fn parse(text: &str) -> Self {
        if text.trim().eq_ignore_ascii_case("flat") {
            Self::Flat
        } else {
            Self::Adaptive
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClickMethod {
    /// Pressing the bottom corners clicks right and middle.
    #[default]
    ButtonAreas,
    /// One, two and three fingers click left, right and middle.
    Clickfinger,
}

impl ClickMethod {
    fn kdl_name(self) -> &'static str {
        match self {
            Self::ButtonAreas => "button-areas",
            Self::Clickfinger => "clickfinger",
        }
    }

    fn parse(text: &str) -> Self {
        if text.trim().eq_ignore_ascii_case("clickfinger") {
            Self::Clickfinger
        } else {
            Self::ButtonAreas
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PointerSettings {
    /// -1 (slowest) to 1 (fastest), 0 is the device default.
    pub speed: f64,
    pub natural_scroll: bool,
    pub accel_profile: AccelProfile,
    pub left_handed: bool,
    /// Multiplier on the scroll distance.
    pub scroll_speed: f64,
}

impl Default for PointerSettings {
    fn default() -> Self {
        Self {
            speed: 0.0,
            natural_scroll: false,
            accel_profile: AccelProfile::Adaptive,
            left_handed: false,
            scroll_speed: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TouchpadSettings {
    pub speed: f64,
    pub natural_scroll: bool,
    pub accel_profile: AccelProfile,
    pub tap_to_click: bool,
    pub tap_and_drag: bool,
    pub disable_while_typing: bool,
    pub click_method: ClickMethod,
    pub scroll_speed: f64,
}

impl Default for TouchpadSettings {
    fn default() -> Self {
        Self {
            speed: 0.0,
            natural_scroll: false,
            accel_profile: AccelProfile::Adaptive,
            tap_to_click: false,
            tap_and_drag: true,
            disable_while_typing: true,
            click_method: ClickMethod::ButtonAreas,
            scroll_speed: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyboardSettings {
    pub xkb: XkbSet,
    /// Milliseconds a key is held before it repeats.
    pub repeat_delay: u32,
    /// Repeats per second.
    pub repeat_rate: u32,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            xkb: XkbSet::from_strings("us", "", ""),
            repeat_delay: 600,
            repeat_rate: 25,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InputSettings {
    pub pointer: PointerSettings,
    pub touchpad: TouchpadSettings,
    pub keyboard: KeyboardSettings,
}

/// One thing the user changed.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    PointerSpeed(f64),
    PointerNaturalScroll(bool),
    PointerAccel(AccelProfile),
    PointerLeftHanded(bool),
    PointerScrollSpeed(f64),
    TouchpadSpeed(f64),
    TouchpadNaturalScroll(bool),
    TouchpadAccel(AccelProfile),
    TouchpadTap(bool),
    TouchpadTapAndDrag(bool),
    TouchpadDisableWhileTyping(bool),
    TouchpadClickMethod(ClickMethod),
    TouchpadScrollSpeed(f64),
    KeyboardRepeatDelay(u32),
    KeyboardRepeatRate(u32),
    /// The layouts, their variants and the options (the switching keys).
    KeyboardLayouts(XkbSet),
}

impl InputSettings {
    /// What the settings say after `change`.
    pub fn apply(&mut self, change: &Change) {
        match change {
            Change::PointerSpeed(v) => self.pointer.speed = *v,
            Change::PointerNaturalScroll(v) => self.pointer.natural_scroll = *v,
            Change::PointerAccel(v) => self.pointer.accel_profile = *v,
            Change::PointerLeftHanded(v) => self.pointer.left_handed = *v,
            Change::PointerScrollSpeed(v) => self.pointer.scroll_speed = *v,
            Change::TouchpadSpeed(v) => self.touchpad.speed = *v,
            Change::TouchpadNaturalScroll(v) => self.touchpad.natural_scroll = *v,
            Change::TouchpadAccel(v) => self.touchpad.accel_profile = *v,
            Change::TouchpadTap(v) => self.touchpad.tap_to_click = *v,
            Change::TouchpadTapAndDrag(v) => self.touchpad.tap_and_drag = *v,
            Change::TouchpadDisableWhileTyping(v) => self.touchpad.disable_while_typing = *v,
            Change::TouchpadClickMethod(v) => self.touchpad.click_method = *v,
            Change::TouchpadScrollSpeed(v) => self.touchpad.scroll_speed = *v,
            Change::KeyboardRepeatDelay(v) => self.keyboard.repeat_delay = *v,
            Change::KeyboardRepeatRate(v) => self.keyboard.repeat_rate = *v,
            Change::KeyboardLayouts(v) => self.keyboard.xkb = v.clone(),
        }
    }
}

// -- settings: the vocabulary both backends share -----------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Setting {
    PointerSpeed,
    PointerNatural,
    PointerAccel,
    PointerLeftHanded,
    PointerScroll,
    TouchpadSpeed,
    TouchpadNatural,
    TouchpadAccel,
    TouchpadTap,
    TouchpadTapDrag,
    TouchpadDwt,
    TouchpadClick,
    TouchpadScroll,
    RepeatDelay,
    RepeatRate,
    XkbLayout,
    XkbVariant,
    XkbOptions,
}

#[derive(Debug, Clone, PartialEq)]
enum Val {
    Bool(bool),
    Num(f64),
    Int(i64),
    Text(String),
}

/// How a setting is written in KDL.
#[derive(Clone, Copy)]
enum Form {
    /// A node that is there or not: `tap`.
    Flag,
    /// A node with a boolean argument: `drag true`.
    BoolArg,
    Num,
    Int,
    Text,
}

impl Setting {
    const ALL: [Setting; 18] = [
        Setting::PointerSpeed,
        Setting::PointerNatural,
        Setting::PointerAccel,
        Setting::PointerLeftHanded,
        Setting::PointerScroll,
        Setting::TouchpadSpeed,
        Setting::TouchpadNatural,
        Setting::TouchpadAccel,
        Setting::TouchpadTap,
        Setting::TouchpadTapDrag,
        Setting::TouchpadDwt,
        Setting::TouchpadClick,
        Setting::TouchpadScroll,
        Setting::RepeatDelay,
        Setting::RepeatRate,
        Setting::XkbLayout,
        Setting::XkbVariant,
        Setting::XkbOptions,
    ];

    /// Where `wm.kdl` (and the Hyprland overrides file) keeps it.
    fn kdl(self) -> (&'static [&'static str], Form) {
        use Form::*;
        match self {
            Self::PointerSpeed => (&["input", "mouse", "accel-speed"], Num),
            Self::PointerNatural => (&["input", "mouse", "natural-scroll"], Flag),
            Self::PointerAccel => (&["input", "mouse", "accel-profile"], Text),
            Self::PointerLeftHanded => (&["input", "mouse", "left-handed"], Flag),
            Self::PointerScroll => (&["input", "mouse", "scroll-factor"], Num),
            Self::TouchpadSpeed => (&["input", "touchpad", "accel-speed"], Num),
            Self::TouchpadNatural => (&["input", "touchpad", "natural-scroll"], Flag),
            Self::TouchpadAccel => (&["input", "touchpad", "accel-profile"], Text),
            Self::TouchpadTap => (&["input", "touchpad", "tap"], Flag),
            Self::TouchpadTapDrag => (&["input", "touchpad", "drag"], BoolArg),
            Self::TouchpadDwt => (&["input", "touchpad", "dwt"], Flag),
            Self::TouchpadClick => (&["input", "touchpad", "click-method"], Text),
            Self::TouchpadScroll => (&["input", "touchpad", "scroll-factor"], Num),
            Self::RepeatDelay => (&["input", "keyboard", "repeat-delay"], Int),
            Self::RepeatRate => (&["input", "keyboard", "repeat-rate"], Int),
            Self::XkbLayout => (&["input", "keyboard", "xkb", "layout"], Text),
            Self::XkbVariant => (&["input", "keyboard", "xkb", "variant"], Text),
            Self::XkbOptions => (&["input", "keyboard", "xkb", "options"], Text),
        }
    }

    /// The Hyprland option, as `getoption` and the Lua table spell it.
    /// `None` for a setting that is not one global option (see [`hypr_device`]).
    fn hypr(self) -> Option<&'static [&'static str]> {
        Some(match self {
            Self::PointerSpeed => &["input", "sensitivity"],
            Self::PointerNatural => &["input", "natural_scroll"],
            Self::PointerAccel => &["input", "accel_profile"],
            Self::PointerLeftHanded => &["input", "left_handed"],
            Self::PointerScroll => &["input", "scroll_factor"],
            Self::TouchpadNatural => &["input", "touchpad", "natural_scroll"],
            Self::TouchpadTap => &["input", "touchpad", "tap_to_click"],
            Self::TouchpadTapDrag => &["input", "touchpad", "tap_and_drag"],
            Self::TouchpadDwt => &["input", "touchpad", "disable_while_typing"],
            Self::TouchpadClick => &["input", "touchpad", "clickfinger_behavior"],
            Self::TouchpadScroll => &["input", "touchpad", "scroll_factor"],
            Self::RepeatDelay => &["input", "repeat_delay"],
            Self::RepeatRate => &["input", "repeat_rate"],
            Self::XkbLayout => &["input", "kb_layout"],
            Self::XkbVariant => &["input", "kb_variant"],
            Self::XkbOptions => &["input", "kb_options"],
            Self::TouchpadSpeed | Self::TouchpadAccel => return None,
        })
    }

    /// Hyprland has one pointer speed and profile for every pointing device;
    /// a touchpad's own go on the device.
    fn hypr_device(self) -> Option<&'static str> {
        match self {
            Self::TouchpadSpeed => Some("sensitivity"),
            Self::TouchpadAccel => Some("accel_profile"),
            _ => None,
        }
    }
}

impl Change {
    fn parts(&self) -> Vec<(Setting, Val)> {
        use Setting as S;
        let accel = |a: &AccelProfile| Val::Text(a.name().to_string());
        match self {
            Self::PointerSpeed(v) => vec![(S::PointerSpeed, Val::Num(*v))],
            Self::PointerNaturalScroll(v) => vec![(S::PointerNatural, Val::Bool(*v))],
            Self::PointerAccel(v) => vec![(S::PointerAccel, accel(v))],
            Self::PointerLeftHanded(v) => vec![(S::PointerLeftHanded, Val::Bool(*v))],
            Self::PointerScrollSpeed(v) => vec![(S::PointerScroll, Val::Num(*v))],
            Self::TouchpadSpeed(v) => vec![(S::TouchpadSpeed, Val::Num(*v))],
            Self::TouchpadNaturalScroll(v) => vec![(S::TouchpadNatural, Val::Bool(*v))],
            Self::TouchpadAccel(v) => vec![(S::TouchpadAccel, accel(v))],
            Self::TouchpadTap(v) => vec![(S::TouchpadTap, Val::Bool(*v))],
            Self::TouchpadTapAndDrag(v) => vec![(S::TouchpadTapDrag, Val::Bool(*v))],
            Self::TouchpadDisableWhileTyping(v) => vec![(S::TouchpadDwt, Val::Bool(*v))],
            Self::TouchpadClickMethod(v) => {
                vec![(S::TouchpadClick, Val::Text(v.kdl_name().to_string()))]
            }
            Self::TouchpadScrollSpeed(v) => vec![(S::TouchpadScroll, Val::Num(*v))],
            Self::KeyboardRepeatDelay(v) => vec![(S::RepeatDelay, Val::Int(*v as i64))],
            Self::KeyboardRepeatRate(v) => vec![(S::RepeatRate, Val::Int(*v as i64))],
            Self::KeyboardLayouts(set) => vec![
                (S::XkbLayout, Val::Text(set.layout_string())),
                (S::XkbVariant, Val::Text(set.variant_string())),
                (S::XkbOptions, Val::Text(set.options_string())),
            ],
        }
    }
}

// -- backends -----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputBackend {
    Hyprland,
    ConchyWm,
    /// A compositor with no way to change input settings from outside.
    Unsupported,
}

impl InputBackend {
    pub fn from_compositor(backend: CompositorBackend) -> Self {
        match backend {
            CompositorBackend::Hyprland => Self::Hyprland,
            CompositorBackend::Niri => Self::ConchyWm,
            CompositorBackend::Mango => Self::Unsupported,
        }
    }

    /// Where a change lasts, for the page to say.
    pub fn persistence_note(self) -> &'static str {
        match self {
            Self::Hyprland => {
                "Applied now and again each time the shell starts (~/.config/conchy/input.kdl)"
            }
            Self::ConchyWm => "Saved to wm.kdl; the compositor reloads it at once",
            Self::Unsupported => "This compositor cannot be configured from here",
        }
    }

    /// The settings as the compositor has them now.
    pub fn load(self) -> anyhow::Result<InputSettings> {
        match self {
            Self::Hyprland => load_hyprland(),
            Self::ConchyWm => {
                let (_, doc) = wm_config::load()?;
                Ok(read_kdl(&doc))
            }
            Self::Unsupported => bail!("input settings are not available on this compositor"),
        }
    }

    /// Make `change` take effect, and last.
    pub fn apply(self, change: &Change) -> anyhow::Result<()> {
        let parts = change.parts();
        match self {
            Self::ConchyWm => wm_config::update(|doc| {
                for (setting, value) in &parts {
                    write_kdl(doc, *setting, value)?;
                }
                Ok(())
            }),
            Self::Hyprland => {
                for (setting, value) in &parts {
                    apply_hyprland(*setting, value)?;
                }
                // Kept for the next start; a failure here must not undo the
                // change the user can already see.
                if let Err(err) = remember(&parts) {
                    tracing::warn!("Failed to keep the input setting: {err:#}");
                }
                Ok(())
            }
            Self::Unsupported => bail!("input settings are not available on this compositor"),
        }
    }

    /// At shell start: put back what the user set earlier, on a compositor
    /// that forgets it. A no-op elsewhere.
    pub fn restore(self) -> anyhow::Result<usize> {
        if self != Self::Hyprland {
            return Ok(0);
        }
        let Some(text) = fsutil::read_optional(&overrides_path()?)? else {
            return Ok(0);
        };
        let doc = kdl_edit::parse(&text)?;
        let mut applied = 0;
        for setting in Setting::ALL {
            let (path, form) = setting.kdl();
            let value = match form {
                Form::Flag | Form::BoolArg => kdl_edit::arg_bool(&doc, path).map(Val::Bool),
                Form::Num => kdl_edit::arg_f64(&doc, path).map(Val::Num),
                Form::Int => kdl_edit::arg_f64(&doc, path).map(|v| Val::Int(v as i64)),
                Form::Text => kdl_edit::arg_str(&doc, path).map(Val::Text),
            };
            if let Some(value) = value {
                match apply_hyprland(setting, &value) {
                    Ok(()) => applied += 1,
                    Err(err) => tracing::warn!("Failed to restore {setting:?}: {err:#}"),
                }
            }
        }
        Ok(applied)
    }
}

// -- wm.kdl -------------------------------------------------------------------

fn read_kdl(doc: &KdlDocument) -> InputSettings {
    let mut s = InputSettings::default();
    let num = |setting: Setting| {
        let (path, _) = setting.kdl();
        kdl_edit::arg_f64(doc, path)
    };
    let flag = |setting: Setting| {
        let (path, _) = setting.kdl();
        kdl_edit::has(doc, path)
    };
    let text = |setting: Setting| {
        let (path, _) = setting.kdl();
        kdl_edit::arg_str(doc, path)
    };

    s.pointer.speed = num(Setting::PointerSpeed).unwrap_or(0.0);
    s.pointer.natural_scroll = flag(Setting::PointerNatural);
    s.pointer.accel_profile = text(Setting::PointerAccel)
        .map(|t| AccelProfile::parse(&t))
        .unwrap_or_default();
    s.pointer.left_handed = flag(Setting::PointerLeftHanded);
    s.pointer.scroll_speed = num(Setting::PointerScroll).unwrap_or(1.0);

    s.touchpad.speed = num(Setting::TouchpadSpeed).unwrap_or(0.0);
    s.touchpad.natural_scroll = flag(Setting::TouchpadNatural);
    s.touchpad.accel_profile = text(Setting::TouchpadAccel)
        .map(|t| AccelProfile::parse(&t))
        .unwrap_or_default();
    s.touchpad.tap_to_click = flag(Setting::TouchpadTap);
    s.touchpad.tap_and_drag = {
        let (path, _) = Setting::TouchpadTapDrag.kdl();
        kdl_edit::arg_bool(doc, path).unwrap_or(true)
    };
    s.touchpad.disable_while_typing = flag(Setting::TouchpadDwt);
    s.touchpad.click_method = text(Setting::TouchpadClick)
        .map(|t| ClickMethod::parse(&t))
        .unwrap_or_default();
    s.touchpad.scroll_speed = num(Setting::TouchpadScroll).unwrap_or(1.0);

    s.keyboard.repeat_delay = num(Setting::RepeatDelay).map_or(600, |v| v as u32);
    s.keyboard.repeat_rate = num(Setting::RepeatRate).map_or(25, |v| v as u32);
    // The compositor's default layout is `us` when the file says nothing.
    let layout = text(Setting::XkbLayout).unwrap_or_else(|| "us".to_string());
    let variant = text(Setting::XkbVariant).unwrap_or_default();
    let options = text(Setting::XkbOptions).unwrap_or_default();
    s.keyboard.xkb = XkbSet::from_strings(&layout, &variant, &options);
    s
}

fn kdl_value(value: &Val) -> Vec<KdlValue> {
    match value {
        Val::Bool(b) => vec![KdlValue::Bool(*b)],
        Val::Num(n) => vec![kdl_edit::float(*n)],
        Val::Int(i) => vec![kdl_edit::int(*i)],
        Val::Text(t) => vec![kdl_edit::string(t)],
    }
}

/// `wm.kdl` spelling: flags are present or absent, an empty variant or
/// options string is no node at all.
fn write_kdl(doc: &mut KdlDocument, setting: Setting, value: &Val) -> anyhow::Result<()> {
    let (path, form) = setting.kdl();
    match (form, value) {
        (Form::Flag, Val::Bool(on)) => kdl_edit::set_flag(doc, path, *on),
        (Form::Text, Val::Text(t)) if t.is_empty() => {
            kdl_edit::remove(doc, path);
            Ok(())
        }
        // Speed 0 and scroll 1 are the defaults: no reason to keep the node.
        (Form::Num, Val::Num(n))
            if (matches!(setting, Setting::PointerSpeed | Setting::TouchpadSpeed) && *n == 0.0)
                || (matches!(setting, Setting::PointerScroll | Setting::TouchpadScroll)
                    && *n == 1.0) =>
        {
            kdl_edit::remove(doc, path);
            Ok(())
        }
        _ => kdl_edit::set_args(doc, path, kdl_value(value)),
    }
}

// -- Hyprland -----------------------------------------------------------------

fn overrides_path() -> anyhow::Result<PathBuf> {
    Ok(fsutil::config_home()?.join("conchy/input.kdl"))
}

/// Keep the change in the overrides file: every value spelled out, since
/// "absent" cannot mean "off" on a compositor whose own config says otherwise.
fn remember(parts: &[(Setting, Val)]) -> anyhow::Result<()> {
    let path = overrides_path()?;
    let existing = fsutil::read_optional(&path)?;
    let is_new = existing.is_none();
    let mut doc = kdl_edit::parse(&existing.unwrap_or_default())?;
    if is_new {
        doc.set_leading(
            "// Input settings changed in the shell's settings, applied again when it starts.\n",
        );
    }
    for (setting, value) in parts {
        let (kdl_path, _) = setting.kdl();
        kdl_edit::set_args(&mut doc, kdl_path, kdl_value(value))?;
    }
    fsutil::write_through(&path, &doc.to_string())
}

fn lua_value(value: &Val) -> String {
    match value {
        Val::Bool(b) => b.to_string(),
        Val::Num(n) => format!("{n}"),
        Val::Int(i) => i.to_string(),
        Val::Text(t) => format!("{t:?}"),
    }
}

/// `{ input = { touchpad = { tap_to_click = true } } }`
fn lua_table(path: &[&str], value: &str) -> String {
    let mut out = value.to_string();
    for key in path.iter().rev() {
        out = format!("{{ {key} = {out} }}");
    }
    out
}

/// Run `hyprctl eval`; its answer is `ok` or `error: …`. `None` when this
/// Hyprland has no Lua (the request is unknown).
fn hypr_eval(code: &str) -> anyhow::Result<Option<()>> {
    let out = Command::new("hyprctl")
        .args(["eval", code])
        .output()
        .context("failed to run hyprctl")?;
    let answer = String::from_utf8_lossy(&out.stdout);
    let answer = answer.trim();
    if answer == "ok" {
        Ok(Some(()))
    } else if answer.contains("unknown request") {
        Ok(None)
    } else {
        Err(anyhow!("hyprctl eval: {answer}"))
    }
}

fn hypr_keyword(name: &str, value: &str) -> anyhow::Result<()> {
    let out = Command::new("hyprctl")
        .args(["keyword", name, value])
        .output()
        .context("failed to run hyprctl")?;
    let answer = String::from_utf8_lossy(&out.stdout);
    if answer.trim() == "ok" {
        Ok(())
    } else {
        Err(anyhow!("hyprctl keyword {name}: {}", answer.trim()))
    }
}

fn keyword_value(value: &Val) -> String {
    match value {
        Val::Bool(b) => i32::from(*b).to_string(),
        Val::Num(n) => format!("{n}"),
        Val::Int(i) => i.to_string(),
        Val::Text(t) => t.clone(),
    }
}

fn apply_hyprland(setting: Setting, value: &Val) -> anyhow::Result<()> {
    // `clickfinger_behavior` is on for the clickfinger method, so it takes
    // the method's name and a boolean.
    let value = match (setting, value) {
        (Setting::TouchpadClick, Val::Text(t)) => Val::Bool(t == "clickfinger"),
        _ => value.clone(),
    };

    if let Some(property) = setting.hypr_device() {
        let devices = touchpad_names();
        if devices.is_empty() {
            return Ok(());
        }
        for name in devices {
            let table = format!("{{ name = {name:?}, {property} = {} }}", lua_value(&value));
            if hypr_eval(&format!("hl.device({table})"))?.is_none() {
                // Legacy config: `device:<name>:<property>`.
                hypr_keyword(
                    &format!("device[{name}]:{property}"),
                    &keyword_value(&value),
                )?;
            }
        }
        return Ok(());
    }

    let path = setting
        .hypr()
        .ok_or_else(|| anyhow!("{setting:?} has no Hyprland option"))?;
    let table = lua_table(path, &lua_value(&value));
    if hypr_eval(&format!("hl.config({table})"))?.is_some() {
        return Ok(());
    }
    hypr_keyword(&path.join(":"), &keyword_value(&value))
}

/// `hyprctl -j getoption`: the number, string or boolean it holds.
fn hypr_option(path: &[&str]) -> Option<serde_json::Value> {
    let out = Command::new("hyprctl")
        .args(["-j", "getoption", &path.join(":")])
        .output()
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    ["bool", "float", "int", "str"]
        .iter()
        .find_map(|key| json.get(*key).cloned())
}

fn load_hyprland() -> anyhow::Result<InputSettings> {
    let mut s = InputSettings::default();
    let num = |setting: Setting| {
        setting
            .hypr()
            .and_then(hypr_option)
            .and_then(|v| v.as_f64())
    };
    let flag = |setting: Setting| {
        setting
            .hypr()
            .and_then(hypr_option)
            .and_then(|v| v.as_bool())
    };
    let text = |setting: Setting| {
        setting
            .hypr()
            .and_then(hypr_option)
            .and_then(|v| v.as_str().map(str::to_string))
            // Hyprland's own marker for "not set".
            .filter(|t| t != "[[EMPTY]]")
    };
    if hypr_option(&["input", "sensitivity"]).is_none() {
        bail!("could not read Hyprland's options (is hyprctl working?)");
    }

    s.pointer.speed = num(Setting::PointerSpeed).unwrap_or(0.0);
    s.pointer.natural_scroll = flag(Setting::PointerNatural).unwrap_or(false);
    s.pointer.accel_profile = text(Setting::PointerAccel)
        .map(|t| AccelProfile::parse(&t))
        .unwrap_or_default();
    s.pointer.left_handed = flag(Setting::PointerLeftHanded).unwrap_or(false);
    s.pointer.scroll_speed = num(Setting::PointerScroll).unwrap_or(1.0);

    // A touchpad's speed and profile are the device's; read them back from
    // what the shell last set, else the global default.
    s.touchpad.speed = overrides_value(Setting::TouchpadSpeed)
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    s.touchpad.accel_profile = overrides_value(Setting::TouchpadAccel)
        .and_then(|v| v.as_str().map(AccelProfile::parse))
        .unwrap_or_default();
    s.touchpad.natural_scroll = flag(Setting::TouchpadNatural).unwrap_or(false);
    s.touchpad.tap_to_click = flag(Setting::TouchpadTap).unwrap_or(true);
    s.touchpad.tap_and_drag = flag(Setting::TouchpadTapDrag).unwrap_or(true);
    s.touchpad.disable_while_typing = flag(Setting::TouchpadDwt).unwrap_or(true);
    s.touchpad.click_method = if flag(Setting::TouchpadClick).unwrap_or(false) {
        ClickMethod::Clickfinger
    } else {
        ClickMethod::ButtonAreas
    };
    s.touchpad.scroll_speed = num(Setting::TouchpadScroll).unwrap_or(1.0);

    s.keyboard.repeat_delay = num(Setting::RepeatDelay).map_or(600, |v| v as u32);
    s.keyboard.repeat_rate = num(Setting::RepeatRate).map_or(25, |v| v as u32);
    s.keyboard.xkb = XkbSet::from_strings(
        &text(Setting::XkbLayout).unwrap_or_else(|| "us".into()),
        &text(Setting::XkbVariant).unwrap_or_default(),
        &text(Setting::XkbOptions).unwrap_or_default(),
    );
    Ok(s)
}

/// A value in the overrides file, as a loose JSON value.
fn overrides_value(setting: Setting) -> Option<serde_json::Value> {
    let text = fsutil::read_optional(&overrides_path().ok()?).ok()??;
    let doc = kdl_edit::parse(&text).ok()?;
    let (path, form) = setting.kdl();
    match form {
        Form::Num | Form::Int => kdl_edit::arg_f64(&doc, path).map(serde_json::Value::from),
        Form::Text => kdl_edit::arg_str(&doc, path).map(serde_json::Value::from),
        Form::Flag | Form::BoolArg => kdl_edit::arg_bool(&doc, path).map(serde_json::Value::from),
    }
}

// -- devices ------------------------------------------------------------------

/// The touchpads, named the way Hyprland names devices: lower case, spaces
/// as dashes.
fn touchpad_names() -> Vec<String> {
    let mut names: Vec<String> = touchpad_labels()
        .into_iter()
        .map(|n| n.trim().replace(' ', "-").to_lowercase())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The kernel's names of the touchpads.
fn touchpad_labels() -> Vec<String> {
    let mut found = Vec::new();
    let Ok(mut scan) = udev::Enumerator::new() else {
        return found;
    };
    if scan.match_subsystem("input").is_err() {
        return found;
    }
    let Ok(devices) = scan.scan_devices() else {
        return found;
    };
    for device in devices {
        let is_touchpad = device
            .property_value("ID_INPUT_TOUCHPAD")
            .is_some_and(|v| v == "1");
        let is_event = device
            .sysname()
            .to_str()
            .is_some_and(|n| n.starts_with("event"));
        if !is_touchpad || !is_event {
            continue;
        }
        let name = device
            .parent()
            .and_then(|parent| {
                parent
                    .property_value("NAME")
                    .map(|n| n.to_string_lossy().trim_matches('"').to_string())
            })
            .filter(|n| !n.is_empty());
        if let Some(name) = name {
            found.push(name);
        }
    }
    found
}

/// Whether the machine has a touchpad at all.
pub fn has_touchpad() -> bool {
    !touchpad_labels().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WM: &str = "input {\n    keyboard {\n        xkb {\n            layout \"us,ru\"\n            options \"grp:win_space_toggle\"\n        }\n        repeat-delay 300\n    }\n    touchpad {\n        tap\n        natural-scroll\n        accel-speed 0.4\n        click-method \"clickfinger\"\n    }\n    mouse {\n        accel-profile \"flat\"\n    }\n}\n";

    #[test]
    fn wm_kdl_is_read_into_settings() {
        let s = read_kdl(&kdl_edit::parse(WM).unwrap());
        assert_eq!(s.keyboard.repeat_delay, 300);
        assert_eq!(s.keyboard.repeat_rate, 25);
        assert_eq!(s.keyboard.xkb.layout_string(), "us,ru");
        assert_eq!(s.keyboard.xkb.switch_option(), Some("grp:win_space_toggle"));
        assert!(s.touchpad.tap_to_click);
        assert!(s.touchpad.natural_scroll);
        assert_eq!(s.touchpad.speed, 0.4);
        assert_eq!(s.touchpad.click_method, ClickMethod::Clickfinger);
        assert_eq!(s.pointer.accel_profile, AccelProfile::Flat);
        assert!(!s.pointer.natural_scroll);
    }

    #[test]
    fn a_change_round_trips_through_wm_kdl() {
        let mut doc = kdl_edit::parse(WM).unwrap();
        let changes = [
            Change::TouchpadTap(false),
            Change::PointerNaturalScroll(true),
            Change::PointerSpeed(-0.3),
            Change::KeyboardRepeatRate(40),
            Change::TouchpadTapAndDrag(false),
            Change::KeyboardLayouts(XkbSet::from_strings(
                "us,de",
                ",nodeadkeys",
                "grp:alt_shift_toggle",
            )),
        ];
        let mut expect = read_kdl(&doc);
        for change in &changes {
            for (setting, value) in change.parts() {
                write_kdl(&mut doc, setting, &value).unwrap();
            }
            expect.apply(change);
        }
        let reparsed = kdl_edit::parse(&doc.to_string()).unwrap();
        assert_eq!(read_kdl(&reparsed), expect);
        // The comments and shape around the edits are the user's.
        assert!(doc.to_string().contains("repeat-delay 300"));
    }

    #[test]
    fn default_valued_nodes_are_dropped() {
        let mut doc = kdl_edit::parse(WM).unwrap();
        write_kdl(&mut doc, Setting::TouchpadSpeed, &Val::Num(0.0)).unwrap();
        assert!(!kdl_edit::has(&doc, &["input", "touchpad", "accel-speed"]));
        write_kdl(&mut doc, Setting::XkbOptions, &Val::Text(String::new())).unwrap();
        assert!(!kdl_edit::has(
            &doc,
            &["input", "keyboard", "xkb", "options"]
        ));
    }

    #[test]
    fn lua_tables_nest() {
        assert_eq!(
            lua_table(&["input", "touchpad", "tap_to_click"], "true"),
            "{ input = { touchpad = { tap_to_click = true } } }"
        );
        assert_eq!(lua_value(&Val::Text("flat".into())), "\"flat\"");
        assert_eq!(lua_value(&Val::Num(0.5)), "0.5");
    }

    #[test]
    fn every_change_is_spelled_for_both_backends() {
        let all = [
            Change::PointerSpeed(0.1),
            Change::PointerNaturalScroll(true),
            Change::PointerAccel(AccelProfile::Flat),
            Change::PointerLeftHanded(true),
            Change::PointerScrollSpeed(2.0),
            Change::TouchpadSpeed(0.1),
            Change::TouchpadNaturalScroll(true),
            Change::TouchpadAccel(AccelProfile::Flat),
            Change::TouchpadTap(true),
            Change::TouchpadTapAndDrag(true),
            Change::TouchpadDisableWhileTyping(true),
            Change::TouchpadClickMethod(ClickMethod::Clickfinger),
            Change::TouchpadScrollSpeed(2.0),
            Change::KeyboardRepeatDelay(250),
            Change::KeyboardRepeatRate(30),
            Change::KeyboardLayouts(XkbSet::from_strings("us", "", "")),
        ];
        for change in &all {
            for (setting, _) in change.parts() {
                assert!(
                    setting.hypr().is_some() || setting.hypr_device().is_some(),
                    "{setting:?} has no Hyprland spelling"
                );
            }
        }
    }
}
