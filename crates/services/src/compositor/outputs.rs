//! Outputs in full: every monitor with the modes it offers, and setting one
//! up in a single step.
//!
//! The compositor service's own state tracks monitors for the bar and the
//! docks and does not carry modes. Settings need them, so they ask the
//! compositor directly, on demand.

use anyhow::{Result, bail};
use kdl::KdlDocument;

use super::types::{CompositorBackend, Monitor, OutputConfig, OutputMode};
use crate::{kdl_edit, wm_config};

/// Every output the compositor knows, switched on or off, with its modes.
/// Blocking: call it off the UI thread.
pub fn query_monitors(backend: CompositorBackend) -> Result<Vec<Monitor>> {
    match backend {
        #[cfg(feature = "hyprland")]
        CompositorBackend::Hyprland => super::hyprland::query_monitors(),
        CompositorBackend::Niri => {
            let outputs = super::niri::fetch_outputs()?;
            Ok(monitors_from_conchy(outputs))
        }
        CompositorBackend::Mango => bail!("display settings are not available on Mango"),
    }
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
    // The output's own block sits one level down from the top.
    const DEPTH: usize = 1;
    kdl_edit::set_flag_at(block, DEPTH, &["off"], !cfg.enabled)?;
    if !cfg.enabled {
        return Ok(());
    }
    kdl_edit::set_args_at(block, DEPTH, &["scale"], vec![kdl_edit::float(cfg.scale)])?;
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
    kdl_edit::set_args_at(
        block,
        DEPTH,
        &["transform"],
        vec![kdl_edit::string(transform)],
    )?;
    kdl_edit::set_props_at(
        block,
        DEPTH,
        &["position"],
        vec![
            ("x", kdl_edit::int(i64::from(cfg.x))),
            ("y", kdl_edit::int(i64::from(cfg.y))),
        ],
    )?;
    match &cfg.mode {
        Some(m) => kdl_edit::set_args_at(
            block,
            DEPTH,
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
    kdl_edit::set_flag_at(block, DEPTH, &["variable-refresh-rate"], cfg.vrr)
}

#[cfg(test)]
mod tests {
    use super::*;

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
