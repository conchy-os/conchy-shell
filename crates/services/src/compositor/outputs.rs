//! Outputs in full: every monitor with the modes it offers, and setting one
//! up in a single step.
//!
//! The compositor service's own state tracks monitors for the bar and the
//! docks and does not carry modes. Settings need them, so they ask the
//! compositor directly, on demand.

use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use kdl::KdlDocument;
use serde_json::Value;

use super::types::{CompositorBackend, Monitor, OutputConfig, OutputMode};
use crate::{kdl_edit, wm_config};

/// Every output the compositor knows, switched on or off, with its modes.
/// Blocking: call it off the UI thread.
pub fn query_monitors(backend: CompositorBackend) -> Result<Vec<Monitor>> {
    match backend {
        CompositorBackend::Hyprland => {
            let out = Command::new("hyprctl")
                .args(["-j", "monitors", "all"])
                .output()
                .context("failed to run hyprctl")?;
            if !out.status.success() {
                bail!(
                    "hyprctl monitors: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            monitors_from_hyprland_json(&String::from_utf8_lossy(&out.stdout))
        }
        CompositorBackend::Niri => {
            let outputs = super::niri::fetch_outputs()?;
            Ok(monitors_from_conchy(outputs))
        }
        CompositorBackend::Mango => bail!("display settings are not available on Mango"),
    }
}

// -- Hyprland -----------------------------------------------------------------

fn monitors_from_hyprland_json(text: &str) -> Result<Vec<Monitor>> {
    let list: Vec<Value> = serde_json::from_str(text).context("unreadable `hyprctl monitors`")?;
    let str_of =
        |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    Ok(list
        .iter()
        .map(|m| {
            let serial = str_of(m, "serial");
            Monitor {
                id: m.get("id").and_then(Value::as_i64).map_or(-1, i128::from),
                name: str_of(m, "name"),
                make: str_of(m, "make"),
                model: str_of(m, "model"),
                serial: (!serial.is_empty()).then_some(serial),
                active_workspace_id: m
                    .pointer("/activeWorkspace/id")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                special_workspace_id: 0,
                width: m.get("width").and_then(Value::as_u64).unwrap_or(0) as u32,
                height: m.get("height").and_then(Value::as_u64).unwrap_or(0) as u32,
                refresh_rate: m.get("refreshRate").and_then(Value::as_f64).unwrap_or(0.0),
                x: m.get("x").and_then(Value::as_i64).unwrap_or(0) as i32,
                y: m.get("y").and_then(Value::as_i64).unwrap_or(0) as i32,
                scale: m.get("scale").and_then(Value::as_f64).unwrap_or(1.0) as f32,
                transform: m.get("transform").and_then(Value::as_u64).unwrap_or(0) as u16,
                available_modes: m
                    .get("availableModes")
                    .and_then(Value::as_array)
                    .map(|modes| {
                        modes
                            .iter()
                            .filter_map(Value::as_str)
                            .filter_map(OutputMode::parse)
                            .collect()
                    })
                    .unwrap_or_default(),
                vrr_supported: m
                    .pointer("/hardwareDetails/vrrCapable")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                vrr_enabled: m.get("vrr").and_then(Value::as_bool).unwrap_or(false),
                is_focused: m.get("focused").and_then(Value::as_bool).unwrap_or(false),
                is_disabled: m.get("disabled").and_then(Value::as_bool).unwrap_or(false),
            }
        })
        .collect())
}

/// The `monitor` rule in Hyprland's Lua API.
fn hyprland_lua(cfg: &OutputConfig) -> String {
    if !cfg.enabled {
        return format!(
            "hl.monitor({{ output = {:?}, disabled = true }})",
            cfg.output
        );
    }
    let mode = cfg.mode.as_ref().map_or_else(
        || "preferred".to_string(),
        |m| format!("{}x{}@{:.3}", m.width, m.height, m.refresh_rate),
    );
    format!(
        "hl.monitor({{ output = {:?}, mode = {:?}, position = \"{}x{}\", scale = {}, transform = {}, vrr = {} }})",
        cfg.output,
        mode,
        cfg.x,
        cfg.y,
        cfg.scale,
        cfg.transform,
        i32::from(cfg.vrr)
    )
}

/// The same rule for a config in the legacy `monitor=` syntax.
fn hyprland_legacy(cfg: &OutputConfig) -> String {
    if !cfg.enabled {
        return format!("{},disable", cfg.output);
    }
    let mode = cfg.mode.as_ref().map_or_else(
        || "preferred".to_string(),
        |m| format!("{}x{}@{:.3}", m.width, m.height, m.refresh_rate),
    );
    format!(
        "{},{},{}x{},{},transform,{},vrr,{}",
        cfg.output,
        mode,
        cfg.x,
        cfg.y,
        cfg.scale,
        cfg.transform,
        i32::from(cfg.vrr)
    )
}

/// Set an output up on Hyprland: the Lua API when this Hyprland has one, the
/// `keyword` request when it does not.
pub(super) fn apply_hyprland(cfg: &OutputConfig) -> Result<()> {
    let lua = Command::new("hyprctl")
        .args(["eval", &hyprland_lua(cfg)])
        .output()
        .context("failed to run hyprctl")?;
    let answer = String::from_utf8_lossy(&lua.stdout);
    let answer = answer.trim();
    if answer == "ok" {
        return Ok(());
    }
    if !answer.contains("unknown request") {
        return Err(anyhow!("hyprctl eval: {answer}"));
    }
    let legacy = Command::new("hyprctl")
        .args(["keyword", "monitor", &hyprland_legacy(cfg)])
        .output()
        .context("failed to run hyprctl")?;
    let answer = String::from_utf8_lossy(&legacy.stdout);
    if answer.trim() == "ok" {
        Ok(())
    } else {
        Err(anyhow!("hyprctl keyword monitor: {}", answer.trim()))
    }
}

/// The output's setup now, to change one field of and apply again.
pub(super) fn current_hyprland(name: &str) -> Result<OutputConfig> {
    query_monitors(CompositorBackend::Hyprland)?
        .iter()
        .find(|m| m.name == name)
        .map(OutputConfig::from_monitor)
        .ok_or_else(|| anyhow!("no output named {name}"))
}

// -- Conchy WM ----------------------------------------------------------------

fn conchy_transform_number(t: conchy_ipc::Transform) -> u16 {
    use conchy_ipc::Transform as T;
    match t {
        T::Normal => 0,
        T::_90 => 1,
        T::_180 => 2,
        T::_270 => 3,
        T::Flipped => 4,
        T::Flipped90 => 5,
        T::Flipped180 => 6,
        T::Flipped270 => 7,
    }
}

fn conchy_transform(n: u16) -> conchy_ipc::Transform {
    use conchy_ipc::Transform as T;
    match n {
        1 => T::_90,
        2 => T::_180,
        3 => T::_270,
        4 => T::Flipped,
        5 => T::Flipped90,
        6 => T::Flipped180,
        7 => T::Flipped270,
        _ => T::Normal,
    }
}

fn monitors_from_conchy(
    outputs: std::collections::HashMap<String, conchy_ipc::Output>,
) -> Vec<Monitor> {
    let mut names: Vec<&String> = outputs.keys().collect();
    names.sort();
    names
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            let o = &outputs[name];
            let modes: Vec<OutputMode> = o
                .modes
                .iter()
                .map(|m| OutputMode {
                    width: u32::from(m.width),
                    height: u32::from(m.height),
                    refresh_rate: f64::from(m.refresh_rate) / 1000.0,
                })
                .collect();
            let current = o.current_mode.and_then(|i| o.modes.get(i));
            let logical = o.logical.as_ref();
            Monitor {
                id: index as i128,
                name: name.clone(),
                make: o.make.clone(),
                model: o.model.clone(),
                serial: o.serial.clone().filter(|s| !s.is_empty()),
                width: current.map_or(0, |m| u32::from(m.width)),
                height: current.map_or(0, |m| u32::from(m.height)),
                refresh_rate: current.map_or(0.0, |m| f64::from(m.refresh_rate) / 1000.0),
                x: logical.map_or(0, |l| l.x),
                y: logical.map_or(0, |l| l.y),
                scale: logical.map_or(1.0, |l| l.scale as f32),
                transform: logical.map_or(0, |l| conchy_transform_number(l.transform)),
                available_modes: modes,
                vrr_supported: o.vrr_supported,
                vrr_enabled: o.vrr_enabled,
                is_disabled: o.current_mode.is_none(),
                ..Default::default()
            }
        })
        .collect()
}

/// Set an output up on Conchy WM: live through its IPC, and in `wm.kdl` so
/// that it stays that way after a restart.
pub(super) fn apply_conchy(cfg: &OutputConfig) -> Result<()> {
    use conchy_ipc::OutputAction as A;
    let name = cfg.output.clone();
    if !cfg.enabled {
        super::niri::send_output_action(name, A::Off)?;
    } else {
        super::niri::send_output_action(name.clone(), A::On)?;
        if let Some(mode) = &cfg.mode {
            super::niri::send_output_action(
                name.clone(),
                A::Mode {
                    mode: conchy_ipc::ModeToSet::Specific(conchy_ipc::ConfiguredMode {
                        width: mode.width as u16,
                        height: mode.height as u16,
                        refresh: Some(mode.refresh_rate),
                    }),
                },
            )?;
        }
        super::niri::send_output_action(
            name.clone(),
            A::Scale {
                scale: conchy_ipc::ScaleToSet::Specific(cfg.scale),
            },
        )?;
        super::niri::send_output_action(
            name.clone(),
            A::Transform {
                transform: conchy_transform(cfg.transform),
            },
        )?;
        super::niri::send_output_action(
            name.clone(),
            A::Position {
                position: conchy_ipc::PositionToSet::Specific(conchy_ipc::ConfiguredPosition {
                    x: cfg.x,
                    y: cfg.y,
                }),
            },
        )?;
        super::niri::send_output_action(
            name,
            A::Vrr {
                vrr: conchy_ipc::VrrToSet {
                    vrr: cfg.vrr,
                    on_demand: false,
                },
            },
        )?;
    }
    // The change is live; a config that cannot be written must not undo it.
    if let Err(err) = wm_config::update(|doc| persist_conchy(doc, cfg)) {
        tracing::warn!("Failed to save {} to wm.kdl: {err:#}", cfg.output);
    }
    Ok(())
}

/// The `output "<name>" { … }` node of `wm.kdl` that says what `cfg` says.
fn persist_conchy(doc: &mut KdlDocument, cfg: &OutputConfig) -> Result<()> {
    let node = kdl_edit::keyed_node(doc, "output", &cfg.output);
    let block = kdl_edit::block_of(node);
    kdl_edit::set_flag(block, &["off"], !cfg.enabled)?;
    if !cfg.enabled {
        return Ok(());
    }
    kdl_edit::set_args(block, &["scale"], vec![kdl_edit::float(cfg.scale)])?;
    let transform = match cfg.transform {
        1 => "90",
        2 => "180",
        3 => "270",
        4 => "flipped",
        5 => "flipped-90",
        6 => "flipped-180",
        7 => "flipped-270",
        _ => "normal",
    };
    kdl_edit::set_args(block, &["transform"], vec![kdl_edit::string(transform)])?;
    kdl_edit::set_props(
        block,
        &["position"],
        vec![
            ("x", kdl_edit::int(i64::from(cfg.x))),
            ("y", kdl_edit::int(i64::from(cfg.y))),
        ],
    )?;
    match &cfg.mode {
        Some(m) => kdl_edit::set_args(
            block,
            &["mode"],
            vec![kdl_edit::string(&format!(
                "{}x{}@{:.3}",
                m.width, m.height, m.refresh_rate
            ))],
        )?,
        None => {
            kdl_edit::remove(block, &["mode"]);
        }
    }
    kdl_edit::set_flag(block, &["variable-refresh-rate"], cfg.vrr)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HYPRCTL: &str = r#"[{
        "id": 0, "name": "eDP-1", "make": "Apple Computer Inc", "model": "Color LCD", "serial": "",
        "width": 2880, "height": 1800, "refreshRate": 59.99000, "x": 0, "y": 0,
        "activeWorkspace": {"id": 2, "name": "2"}, "scale": 2, "transform": 0,
        "focused": true, "vrr": false, "disabled": false,
        "availableModes": ["2880x1800@59.99Hz", "1440x900@60.00Hz"],
        "hardwareDetails": {"vrrCapable": false}
    }, {
        "id": 1, "name": "eDP-2", "make": "", "model": "", "serial": "",
        "width": 0, "height": 0, "refreshRate": 0.0, "x": 0, "y": 0, "scale": 1,
        "transform": 0, "focused": false, "vrr": false, "disabled": true, "availableModes": []
    }]"#;

    #[test]
    fn hyprctl_monitors_are_read_with_their_modes() {
        let monitors = monitors_from_hyprland_json(HYPRCTL).unwrap();
        assert_eq!(monitors.len(), 2);
        let m = &monitors[0];
        assert_eq!(m.name, "eDP-1");
        assert_eq!(m.serial, None);
        assert_eq!(m.available_modes.len(), 2);
        assert_eq!(m.resolutions(), vec![(2880, 1800), (1440, 900)]);
        assert!(!m.vrr_supported);
        assert!(m.is_focused && !m.is_disabled);
        assert!(monitors[1].is_disabled);
    }

    #[test]
    fn hyprland_rules_are_spelled_in_both_syntaxes() {
        let cfg = OutputConfig {
            output: "eDP-1".into(),
            enabled: true,
            mode: Some(OutputMode {
                width: 2880,
                height: 1800,
                refresh_rate: 59.99,
            }),
            scale: 2.0,
            transform: 1,
            x: 0,
            y: 0,
            vrr: false,
        };
        assert_eq!(
            hyprland_lua(&cfg),
            r#"hl.monitor({ output = "eDP-1", mode = "2880x1800@59.990", position = "0x0", scale = 2, transform = 1, vrr = 0 })"#
        );
        assert_eq!(
            hyprland_legacy(&cfg),
            "eDP-1,2880x1800@59.990,0x0,2,transform,1,vrr,0"
        );
        let off = OutputConfig {
            enabled: false,
            ..cfg
        };
        assert_eq!(
            hyprland_lua(&off),
            r#"hl.monitor({ output = "eDP-1", disabled = true })"#
        );
        assert_eq!(hyprland_legacy(&off), "eDP-1,disable");
    }

    #[test]
    fn an_output_is_written_to_wm_kdl_and_read_back() {
        let mut doc = kdl_edit::parse(
            "// mine\noutput \"eDP-1\" {\n    scale 1.0\n}\n\nlayout {\n    gaps 16\n}\n",
        )
        .unwrap();
        let cfg = OutputConfig {
            output: "eDP-1".into(),
            enabled: true,
            mode: Some(OutputMode {
                width: 1920,
                height: 1080,
                refresh_rate: 144.0,
            }),
            scale: 1.5,
            transform: 3,
            x: 1920,
            y: 0,
            vrr: true,
        };
        persist_conchy(&mut doc, &cfg).unwrap();
        let text = doc.to_string();
        let again = kdl_edit::parse(&text).unwrap();
        assert!(
            text.contains("// mine") && text.contains("gaps 16"),
            "{text}"
        );
        let out = &["output"];
        assert!(kdl_edit::node(&again, out).is_some());
        assert!(text.contains("scale 1.5"), "{text}");
        assert!(text.contains("transform \"270\""), "{text}");
        assert!(text.contains("mode \"1920x1080@144.000\""), "{text}");
        assert!(text.contains("x=1920"), "{text}");
        assert!(text.contains("variable-refresh-rate"), "{text}");
        assert_eq!(text.matches("output \"eDP-1\"").count(), 1);

        // Off keeps the rest and adds the flag; turning VRR off drops its node.
        persist_conchy(
            &mut doc,
            &OutputConfig {
                enabled: false,
                ..cfg.clone()
            },
        )
        .unwrap();
        assert!(doc.to_string().contains("off"));
        persist_conchy(&mut doc, &OutputConfig { vrr: false, ..cfg }).unwrap();
        let text = doc.to_string();
        assert!(!text.contains("variable-refresh-rate"));
        assert!(!text.contains("\n    off"));
    }

    #[test]
    fn conchy_outputs_become_monitors() {
        let mut outputs = std::collections::HashMap::new();
        outputs.insert(
            "DP-1".to_string(),
            conchy_ipc::Output {
                name: "DP-1".into(),
                make: "Dell".into(),
                model: "U2720Q".into(),
                serial: Some("S1".into()),
                physical_size: None,
                modes: vec![
                    conchy_ipc::Mode {
                        width: 3840,
                        height: 2160,
                        refresh_rate: 60000,
                        is_preferred: true,
                    },
                    conchy_ipc::Mode {
                        width: 1920,
                        height: 1080,
                        refresh_rate: 59940,
                        is_preferred: false,
                    },
                ],
                current_mode: Some(0),
                is_custom_mode: false,
                vrr_supported: true,
                vrr_enabled: false,
                logical: Some(conchy_ipc::LogicalOutput {
                    x: 10,
                    y: 20,
                    width: 1920,
                    height: 1080,
                    scale: 2.0,
                    transform: conchy_ipc::Transform::_90,
                }),
                max_bpc: None,
            },
        );
        let monitors = monitors_from_conchy(outputs);
        let m = &monitors[0];
        assert_eq!((m.width, m.height), (3840, 2160));
        assert_eq!((m.x, m.y, m.scale, m.transform), (10, 20, 2.0, 1));
        assert_eq!(m.refresh_rates(1920, 1080), vec![59.94]);
        assert!(!m.is_disabled);
    }
}
