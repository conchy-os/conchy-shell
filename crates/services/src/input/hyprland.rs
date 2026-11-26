//! The Hyprland side of input settings, built only with the `hyprland`
//! cargo feature.
//!
//! Hyprland takes the settings at run time (`hyprctl eval` with its Lua API, or
//! `keyword` on a legacy config). Nothing there writes the user's own config,
//! so each change is also kept in `~/.config/conchy/input.kdl` and applied
//! again by [`restore`] when the shell starts.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, anyhow, bail};

use super::{
    AccelProfile, ClickMethod, Form, InputSettings, Setting, Val, kdl_value, touchpad_labels,
};
use crate::xkb::XkbSet;
use crate::{fsutil, kdl_edit};

/// The Hyprland option, as `getoption` and the Lua table spell it.
/// `None` for a setting that is not one global option (see [`hypr_device`]).
fn hypr(setting: Setting) -> Option<&'static [&'static str]> {
    Some(match setting {
        Setting::PointerSpeed => &["input", "sensitivity"],
        Setting::PointerNatural => &["input", "natural_scroll"],
        Setting::PointerAccel => &["input", "accel_profile"],
        Setting::PointerLeftHanded => &["input", "left_handed"],
        Setting::PointerScroll => &["input", "scroll_factor"],
        Setting::TouchpadNatural => &["input", "touchpad", "natural_scroll"],
        Setting::TouchpadTap => &["input", "touchpad", "tap_to_click"],
        Setting::TouchpadTapDrag => &["input", "touchpad", "tap_and_drag"],
        Setting::TouchpadDwt => &["input", "touchpad", "disable_while_typing"],
        Setting::TouchpadClick => &["input", "touchpad", "clickfinger_behavior"],
        Setting::TouchpadScroll => &["input", "touchpad", "scroll_factor"],
        Setting::RepeatDelay => &["input", "repeat_delay"],
        Setting::RepeatRate => &["input", "repeat_rate"],
        Setting::XkbLayout => &["input", "kb_layout"],
        Setting::XkbVariant => &["input", "kb_variant"],
        Setting::XkbOptions => &["input", "kb_options"],
        Setting::TouchpadSpeed | Setting::TouchpadAccel => return None,
    })
}

/// Hyprland has one pointer speed and profile for every pointing device;
/// a touchpad's own go on the device.
fn hypr_device(setting: Setting) -> Option<&'static str> {
    match setting {
        Setting::TouchpadSpeed => Some("sensitivity"),
        Setting::TouchpadAccel => Some("accel_profile"),
        _ => None,
    }
}

/// Make the change take effect now, and keep it for the next start: a
/// failure to keep it must not undo the change the user can already see.
pub(super) fn apply(parts: &[(Setting, Val)]) -> anyhow::Result<()> {
    for (setting, value) in parts {
        apply_one(*setting, value)?;
    }
    if let Err(err) = remember(parts) {
        tracing::warn!("Failed to keep the input setting: {err:#}");
    }
    Ok(())
}

/// Put back what the user set earlier: Hyprland forgets it on restart.
pub(super) fn restore() -> anyhow::Result<usize> {
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
            match apply_one(setting, &value) {
                Ok(()) => applied += 1,
                Err(err) => tracing::warn!("Failed to restore {setting:?}: {err:#}"),
            }
        }
    }
    Ok(applied)
}

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

fn apply_one(setting: Setting, value: &Val) -> anyhow::Result<()> {
    // `clickfinger_behavior` is on for the clickfinger method, so it takes
    // the method's name and a boolean.
    let value = match (setting, value) {
        (Setting::TouchpadClick, Val::Text(t)) => Val::Bool(t == "clickfinger"),
        _ => value.clone(),
    };

    if let Some(property) = hypr_device(setting) {
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

    let path = hypr(setting).ok_or_else(|| anyhow!("{setting:?} has no Hyprland option"))?;
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

/// The settings as Hyprland has them now.
pub(super) fn load() -> anyhow::Result<InputSettings> {
    let mut s = InputSettings::default();
    let num = |setting: Setting| hypr(setting).and_then(hypr_option).and_then(|v| v.as_f64());
    let flag = |setting: Setting| {
        hypr(setting)
            .and_then(hypr_option)
            .and_then(|v| v.as_bool())
    };
    let text = |setting: Setting| {
        hypr(setting)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Change;

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
                    hypr(setting).is_some() || hypr_device(setting).is_some(),
                    "{setting:?} has no Hyprland spelling"
                );
            }
        }
    }
}
