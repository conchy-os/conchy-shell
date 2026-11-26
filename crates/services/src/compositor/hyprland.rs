//! Hyprland compositor backend.
//!
//! This module provides integration with the Hyprland compositor via its IPC socket.
//! Uses incremental updates with direct Mutable mutation for efficiency.
//!
//! Built only with the `hyprland` cargo feature: Conchy OS ships Conchy WM, and
//! this backend stays for development on a Hyprland desktop.

use std::process::Command;

use crate::lifecycle::RunToken;
use anyhow::{Context, Result, anyhow, bail};
use futures_signals::signal::Mutable;
use hyprland::{
    data::{Client, Clients, Devices, Monitors, Workspace as HWorkspace, Workspaces},
    dispatch::{
        Dispatch, DispatchType, MonitorIdentifier, WindowIdentifier, WorkspaceIdentifierWithSpecial,
    },
    event_listener::EventListener,
    prelude::*,
};
use itertools::Itertools;
use tracing::{debug, info};

use serde_json::Value;

use super::types::{
    ActiveWindow, CompositorCommand, CompositorState, Monitor, OutputConfig, OutputMode, Workspace,
};

/// Check if Hyprland is available (running).
pub fn is_available() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}

/// Map a Hyprland `Client` into our generic `Window`, resolving its
/// monitor id to a name via the already-fetched monitor list.
fn map_client(c: Client, monitors: &[super::types::Monitor]) -> super::types::Window {
    let monitor = c
        .monitor
        .and_then(|id| monitors.iter().find(|m| m.id == id))
        .map(|m| m.name.clone())
        .unwrap_or_default();

    super::types::Window {
        id: c.address.to_string(),
        app_id: c.class,
        title: c.title,
        monitor,
        workspace_id: c.workspace.id,
        is_focused: false,
        is_minimized: false,
        is_urgent: false,
        pid: u32::try_from(c.pid).ok(),
        geometry: Some(super::types::WindowGeometry {
            x: c.at.0 as i32,
            y: c.at.1 as i32,
            width: c.size.0 as u32,
            height: c.size.1 as u32,
        }),
    }
}

/// Refetch the full client list and replace `state.windows` with it.
/// Called from the window opened/closed/moved event handlers, which don't
/// carry enough information in their event payload to patch the list
/// incrementally.
fn refresh_windows(state: &mut super::types::CompositorState) {
    if let Ok(clients) = Clients::get() {
        let monitors = state.monitors.clone();
        let focused_address = state.active_window.as_ref().map(|w| w.address.clone());
        // Clients carry no urgency, so keep what the `urgent` events set.
        let urgent: Vec<String> = state
            .windows
            .iter()
            .filter(|w| w.is_urgent)
            .map(|w| w.id.clone())
            .collect();
        state.windows = clients
            .into_iter()
            .map(|c| {
                let mut w = map_client(c, &monitors);
                w.is_focused = Some(&w.id) == focused_address.as_ref();
                w.is_urgent = !w.is_focused && urgent.contains(&w.id);
                w
            })
            .collect();
        sync_urgent_workspaces(state);
    }
}

fn sync_urgent_workspaces(state: &mut super::types::CompositorState) {
    for ws in state.workspaces.iter_mut() {
        ws.has_urgent = state
            .windows
            .iter()
            .any(|w| w.is_urgent && w.workspace_id == ws.id);
    }
}

/// Execute a compositor command synchronously.
pub fn execute_command(cmd: CompositorCommand) -> Result<()> {
    match cmd {
        CompositorCommand::FocusWorkspace(id) => {
            Dispatch::call(DispatchType::Workspace(WorkspaceIdentifierWithSpecial::Id(
                id,
            )))?;
        }
        CompositorCommand::FocusSpecialWorkspace(name) => {
            Dispatch::call(DispatchType::Workspace(
                WorkspaceIdentifierWithSpecial::Special(Some(name.as_str())),
            ))?;
        }
        CompositorCommand::ToggleSpecialWorkspace(name) => {
            Dispatch::call(DispatchType::ToggleSpecialWorkspace(Some(name)))?;
        }
        CompositorCommand::FocusMonitor(id) => {
            Dispatch::call(DispatchType::FocusMonitor(MonitorIdentifier::Id(id)))?;
        }
        CompositorCommand::ScrollWorkspace(dir) => {
            let d = if dir > 0 { "+1" } else { "-1" };
            Dispatch::call(DispatchType::Workspace(
                WorkspaceIdentifierWithSpecial::Relative(d.to_string().parse()?),
            ))?;
        }
        CompositorCommand::NextKeyboardLayout => {
            hyprland::ctl::switch_xkb_layout::call(
                "all",
                hyprland::ctl::switch_xkb_layout::SwitchXKBLayoutCmdTypes::Next,
            )?;
        }
        CompositorCommand::FocusWindow(address) => {
            Dispatch::call(DispatchType::FocusWindow(WindowIdentifier::Address(
                hyprland::shared::Address::new(address),
            )))?;
        }
        CompositorCommand::CloseWindow(address) => {
            Dispatch::call(DispatchType::CloseWindow(WindowIdentifier::Address(
                hyprland::shared::Address::new(address),
            )))?;
        }
        CompositorCommand::Custom(dispatcher, args) => {
            Dispatch::call(DispatchType::Custom(&dispatcher, &args))?;
        }
        CompositorCommand::SetOutputMode {
            output,
            width,
            height,
            refresh_rate,
        } => {
            let mut config = current_output(&output)?;
            config.mode = Some(super::types::OutputMode {
                width: u32::from(width),
                height: u32::from(height),
                refresh_rate,
            });
            apply_output(&config)?;
        }
        CompositorCommand::SetOutputScale { output, scale } => {
            let mut config = current_output(&output)?;
            config.scale = scale;
            apply_output(&config)?;
        }
        CompositorCommand::SetOutputTransform { output, transform } => {
            let mut config = current_output(&output)?;
            config.transform = transform;
            apply_output(&config)?;
        }
        CompositorCommand::SetOutputPosition { output, x, y } => {
            let mut config = current_output(&output)?;
            config.x = x;
            config.y = y;
            apply_output(&config)?;
        }
        CompositorCommand::SetOutputVrr { output, enabled } => {
            let mut config = current_output(&output)?;
            config.vrr = enabled;
            apply_output(&config)?;
        }
        CompositorCommand::SetOutputPower { output, on } => {
            let mut config = current_output(&output)?;
            config.enabled = on;
            apply_output(&config)?;
        }
        CompositorCommand::ApplyOutput(config) => {
            apply_output(&config)?;
        }
        CompositorCommand::ToggleMinimizeWindow(_) | CompositorCommand::ToggleOverview => {
            anyhow::bail!("ToggleMinimizeWindow / ToggleOverview not supported on Hyprland");
        }
    }
    Ok(())
}

/// Fetch the complete compositor state from Hyprland.
/// Used for initial state and occasional full refresh.
pub fn fetch_full_state() -> Result<CompositorState> {
    let workspaces = Workspaces::get()?
        .into_iter()
        .sorted_by_key(|w| w.id)
        .map(|w| Workspace {
            id: w.id,
            index: w.id,
            name: w.name,
            monitor: w.monitor,
            monitor_id: w.monitor_id,
            windows: w.windows,
            is_special: w.id < 0,
            has_urgent: false,
        })
        .collect();

    let monitors: Vec<Monitor> = Monitors::get()?
        .into_iter()
        .map(|m| Monitor {
            id: m.id,
            name: m.name,
            active_workspace_id: m.active_workspace.id,
            special_workspace_id: m.special_workspace.id,
            width: m.width as u32,
            height: m.height as u32,
            x: m.x,
            y: m.y,
            scale: m.scale,
            refresh_rate: m.refresh_rate as f64,
            transform: m.transform as u16,
            vrr_enabled: m.vrr,
            is_focused: m.focused,
            is_disabled: m.disabled,
            ..Default::default()
        })
        .collect();

    let windows: Vec<super::types::Window> = Clients::get()?
        .into_iter()
        .map(|c| map_client(c, &monitors))
        .collect();

    let active_workspace_id = HWorkspace::get_active().ok().map(|w| w.id);

    let active_window = Client::get_active().ok().flatten().map(|w| ActiveWindow {
        title: w.title,
        class: w.class,
        address: w.address.to_string(),
    });

    let keyboard_layout = Devices::get()
        .ok()
        .and_then(|d| {
            d.keyboards
                .into_iter()
                .find(|k| k.main)
                .map(|k| k.active_keymap)
        })
        .unwrap_or_else(|| "Unknown".to_string());

    Ok(CompositorState {
        workspaces,
        monitors,
        active_workspace_id,
        active_window,
        windows,
        keyboard_layout,
        submap: None,
    })
}

/// Follow Hyprland's event socket on a dedicated thread. The socket closing
/// fails the attempt, and the retry refetches whatever it missed.
pub fn start_listener(data: Mutable<CompositorState>, token: RunToken) {
    let mut resync = false;
    token.supervise_blocking("Hyprland", move |token| {
        if std::mem::replace(&mut resync, true) {
            data.set_neq(fetch_full_state()?);
        }
        token.active();
        run_listener(data.clone())?;
        anyhow::bail!("Hyprland event socket closed")
    });
}

/// Run the sync event listener with incremental updates.
fn run_listener(data: Mutable<CompositorState>) -> Result<()> {
    info!("Starting Hyprland event listener (incremental updates)");

    let mut listener = EventListener::new();

    // Workspace changed (active workspace switched)
    {
        let data = data.clone();
        listener.add_workspace_changed_handler(move |evt| {
            debug!("Workspace changed: {:?}", evt.name);
            let mut state = data.lock_mut();
            state.active_workspace_id = Some(evt.id);
        });
    }

    // Workspace added
    {
        let data = data.clone();
        listener.add_workspace_added_handler(move |evt| {
            debug!("Workspace added: {:?}", evt.name);
            let mut state = data.lock_mut();
            // Check if workspace already exists
            if !state.workspaces.iter().any(|w| w.id == evt.id) {
                state.workspaces.push(Workspace {
                    id: evt.id,
                    index: evt.id,
                    name: evt.name.to_string(),
                    monitor: String::new(), // Will be updated by monitor events
                    monitor_id: None,
                    windows: 0,
                    is_special: evt.id < 0,
                    has_urgent: false,
                });
                state.workspaces.sort_by_key(|w| w.id);
            }
        });
    }

    // Workspace deleted
    {
        let data = data.clone();
        listener.add_workspace_deleted_handler(move |evt| {
            debug!("Workspace deleted: {:?}", evt.name);
            let mut state = data.lock_mut();
            state.workspaces.retain(|w| w.id != evt.id);
        });
    }

    // Workspace moved to different monitor
    {
        let data = data.clone();
        listener.add_workspace_moved_handler(move |evt| {
            debug!("Workspace {} moved to monitor {}", evt.id, evt.monitor);
            let mut state = data.lock_mut();
            if let Some(ws) = state.workspaces.iter_mut().find(|w| w.id == evt.id) {
                ws.monitor = evt.monitor.clone();
            }
        });
    }

    // Active window changed
    {
        let data = data.clone();
        listener.add_active_window_changed_handler(move |evt| {
            debug!("Active window changed: {:?}", evt);
            let mut state = data.lock_mut();
            let focused_address = evt.as_ref().map(|w| w.address.to_string());
            for w in state.windows.iter_mut() {
                w.is_focused = Some(&w.id) == focused_address.as_ref();
                // Hyprland clears urgency once the window is focused.
                w.is_urgent &= !w.is_focused;
            }
            sync_urgent_workspaces(&mut state);
            state.active_window = evt.map(|w| ActiveWindow {
                title: w.title,
                class: w.class,
                address: w.address.to_string(),
            });
        });
    }

    // Window asked for attention
    {
        let data = data.clone();
        listener.add_urgent_state_changed_handler(move |address| {
            debug!("Urgent window: {}", address);
            let address = address.to_string();
            let mut state = data.lock_mut();
            if let Some(w) = state
                .windows
                .iter_mut()
                .find(|w| w.id == address && !w.is_focused)
            {
                w.is_urgent = true;
            }
            sync_urgent_workspaces(&mut state);
        });
    }

    // Window opened - increment window count
    {
        let data = data.clone();
        listener.add_window_opened_handler(move |evt| {
            debug!(
                "Window opened: {} on workspace {:?}",
                evt.window_class, evt.workspace_name
            );
            let mut state = data.lock_mut();
            // Try to find workspace by name and increment window count
            if let Some(ws) = state
                .workspaces
                .iter_mut()
                .find(|w| w.name == evt.workspace_name)
            {
                ws.windows = ws.windows.saturating_add(1);
            }
            refresh_windows(&mut state);
        });
    }

    // Window closed - decrement window count
    {
        let data = data.clone();
        listener.add_window_closed_handler(move |_evt| {
            debug!("Window closed");
            // We don't know which workspace, so do a lightweight refresh of window counts
            // This is a compromise - we could track window addresses but that adds complexity
            if let Ok(workspaces) = Workspaces::get() {
                let mut state = data.lock_mut();
                for ws_data in workspaces {
                    if let Some(ws) = state.workspaces.iter_mut().find(|w| w.id == ws_data.id) {
                        ws.windows = ws_data.windows;
                    }
                }
                refresh_windows(&mut state);
            }
        });
    }

    // Window moved between workspaces
    {
        let data = data.clone();
        listener.add_window_moved_handler(move |evt| {
            debug!("Window moved to workspace {}", evt.workspace_name);
            // Refresh window counts for affected workspaces
            if let Ok(workspaces) = Workspaces::get() {
                let mut state = data.lock_mut();
                for ws_data in workspaces {
                    if let Some(ws) = state.workspaces.iter_mut().find(|w| w.id == ws_data.id) {
                        ws.windows = ws_data.windows;
                    }
                }
                refresh_windows(&mut state);
            }
        });
    }

    // Monitor added/changed
    {
        let data = data.clone();
        listener.add_active_monitor_changed_handler(move |evt| {
            debug!(
                "Active monitor changed: {} workspace {:?}",
                evt.monitor_name, evt.workspace_name
            );
            // Find workspace id first (before mutable borrow)
            let ws_id = evt.workspace_name.as_ref().and_then(|ws_type| {
                let ws_name = ws_type.to_string();
                let state = data.lock_ref();
                state
                    .workspaces
                    .iter()
                    .find(|w| w.name == ws_name)
                    .map(|w| w.id)
            });
            // Now update the monitor
            if let Some(ws_id) = ws_id {
                let mut state = data.lock_mut();
                if let Some(mon) = state
                    .monitors
                    .iter_mut()
                    .find(|m| m.name == evt.monitor_name)
                {
                    mon.active_workspace_id = ws_id;
                }
            }
        });
    }

    // Submap changed
    {
        let data = data.clone();
        listener.add_sub_map_changed_handler(move |submap| {
            debug!("Submap changed: {}", submap);
            let mut state = data.lock_mut();
            state.submap = if submap.is_empty() {
                None
            } else {
                Some(submap)
            };
        });
    }

    // Keyboard layout changed
    {
        let data = data.clone();
        listener.add_layout_changed_handler(move |evt| {
            debug!("Keyboard layout changed: {}", evt.layout_name);
            let mut state = data.lock_mut();
            state.keyboard_layout = evt.layout_name;
        });
    }

    // Special workspace events
    {
        let data = data.clone();
        listener.add_changed_special_handler(move |evt| {
            debug!("Special workspace changed: {:?}", evt.workspace_name);
            // Refresh special workspace state
            if let Ok(monitors) = Monitors::get() {
                let mut state = data.lock_mut();
                for mon_data in monitors {
                    if let Some(mon) = state.monitors.iter_mut().find(|m| m.name == mon_data.name) {
                        mon.special_workspace_id = mon_data.special_workspace.id;
                    }
                }
            }
        });
    }

    {
        let data = data.clone();
        listener.add_special_removed_handler(move |_evt| {
            debug!("Special workspace removed");
            // Refresh special workspace state
            if let Ok(monitors) = Monitors::get() {
                let mut state = data.lock_mut();
                for mon_data in monitors {
                    if let Some(mon) = state.monitors.iter_mut().find(|m| m.name == mon_data.name) {
                        mon.special_workspace_id = mon_data.special_workspace.id;
                    }
                }
            }
        });
    }

    // Start the blocking event listener
    listener.start_listener()?;

    Ok(())
}

// -- outputs ------------------------------------------------------------------

/// Every output with its modes, from `hyprctl monitors all`.
pub(super) fn query_monitors() -> Result<Vec<Monitor>> {
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
fn apply_output(cfg: &OutputConfig) -> Result<()> {
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
fn current_output(name: &str) -> Result<OutputConfig> {
    query_monitors()?
        .iter()
        .find(|m| m.name == name)
        .map(OutputConfig::from_monitor)
        .ok_or_else(|| anyhow!("no output named {name}"))
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
}
