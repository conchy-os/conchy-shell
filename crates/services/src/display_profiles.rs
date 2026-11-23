//! Display profiles: the setup of every connected monitor, saved under a name
//! and put back later, or put back by itself when the same monitors are
//! connected again.
//!
//! A monitor is recognised by what it is (make, model, serial), not by the
//! port it sits in, so a profile made at the desk fits whichever dock port the
//! cable ends up in.
//!
//! Profiles live in `~/.config/conchy/displays.kdl`:
//!
//! ```kdl
//! profile "Docked" auto=true {
//!     output "HDMI-A-1" identity="Dell U2720Q S1" enabled=true mode="3840x2160@60.000" scale=1.5 transform=0 x=0 y=0 vrr=false
//! }
//! ```

use std::path::PathBuf;

use anyhow::{Context, anyhow};

use crate::compositor::{Monitor, OutputConfig, OutputMode};
use crate::{fsutil, kdl_edit};

#[derive(Debug, Clone, PartialEq)]
pub struct SavedOutput {
    /// [`Monitor::identity`] of the monitor this was saved from.
    pub identity: String,
    pub config: OutputConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub name: String,
    /// Put back by itself when exactly these monitors are connected.
    pub auto: bool,
    pub outputs: Vec<SavedOutput>,
}

impl Profile {
    /// The monitors as they are now, under `name`.
    pub fn snapshot(name: impl Into<String>, auto: bool, monitors: &[Monitor]) -> Self {
        Self {
            name: name.into(),
            auto,
            outputs: monitors
                .iter()
                .map(|m| SavedOutput {
                    identity: m.identity(),
                    config: OutputConfig::from_monitor(m),
                })
                .collect(),
        }
    }

    /// Whether the profile is for the monitors connected: the same ones, no
    /// more and no fewer.
    pub fn fits(&self, monitors: &[Monitor]) -> bool {
        let mut saved: Vec<&str> = self.outputs.iter().map(|o| o.identity.as_str()).collect();
        let mut now: Vec<String> = monitors.iter().map(Monitor::identity).collect();
        saved.sort_unstable();
        now.sort();
        saved.len() == now.len() && saved.iter().zip(&now).all(|(a, b)| a == b)
    }

    /// What to apply, output by output, for the monitors connected now. Each
    /// saved output goes to the monitor of its identity, to the one on the
    /// same port when several look alike. `None` when the profile does not fit.
    pub fn plan(&self, monitors: &[Monitor]) -> Option<Vec<OutputConfig>> {
        if !self.fits(monitors) {
            return None;
        }
        let mut free: Vec<&Monitor> = monitors.iter().collect();
        let mut plan = Vec::new();
        // Same-port matches first, so lookalikes keep their own ports.
        let mut pending: Vec<&SavedOutput> = Vec::new();
        for saved in &self.outputs {
            let same_port = free
                .iter()
                .position(|m| m.identity() == saved.identity && m.name == saved.config.output);
            match same_port {
                Some(at) => {
                    let monitor = free.remove(at);
                    plan.push(OutputConfig {
                        output: monitor.name.clone(),
                        ..saved.config.clone()
                    });
                }
                None => pending.push(saved),
            }
        }
        for saved in pending {
            let at = free.iter().position(|m| m.identity() == saved.identity)?;
            let monitor = free.remove(at);
            plan.push(OutputConfig {
                output: monitor.name.clone(),
                ..saved.config.clone()
            });
        }
        Some(plan)
    }

    /// The monitors are already set up this way.
    pub fn is_applied(&self, monitors: &[Monitor]) -> bool {
        let Some(plan) = self.plan(monitors) else {
            return false;
        };
        plan.iter().all(|want| {
            monitors
                .iter()
                .find(|m| m.name == want.output)
                .is_some_and(|m| same_setup(want, m))
        })
    }
}

/// Whether `monitor` is set up as `want` says.
fn same_setup(want: &OutputConfig, monitor: &Monitor) -> bool {
    let now = OutputConfig::from_monitor(monitor);
    if want.enabled != now.enabled {
        return false;
    }
    if !want.enabled {
        return true;
    }
    let mode_same = match (&want.mode, &now.mode) {
        (Some(a), Some(b)) => a.same_as(b),
        (None, _) => true,
        (Some(_), None) => false,
    };
    mode_same
        && (want.scale - now.scale).abs() < 0.01
        && want.transform == now.transform
        && want.x == now.x
        && want.y == now.y
        // VRR cannot be set on a monitor that lacks it.
        && (want.vrr == now.vrr || !monitor.vrr_supported)
}

/// The first automatic profile that fits the monitors and is not applied yet.
pub fn pending_auto<'a>(profiles: &'a [Profile], monitors: &[Monitor]) -> Option<&'a Profile> {
    profiles
        .iter()
        .find(|p| p.auto && p.fits(monitors))
        .filter(|p| !p.is_applied(monitors))
}

// -- file ---------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ProfileStore {
    path: PathBuf,
}

impl ProfileStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn standard() -> anyhow::Result<Self> {
        Ok(Self::new(fsutil::config_home()?.join("conchy/displays.kdl")))
    }

    pub fn load(&self) -> anyhow::Result<Vec<Profile>> {
        match fsutil::read_optional(&self.path)? {
            Some(text) => parse(&text).with_context(|| format!("{} does not parse", self.path.display())),
            None => Ok(Vec::new()),
        }
    }

    pub fn save(&self, profiles: &[Profile]) -> anyhow::Result<()> {
        fsutil::write_through(&self.path, &render(profiles))
    }

    /// Add `profile`, replacing the one of the same name.
    pub fn put(&self, profile: Profile) -> anyhow::Result<()> {
        let mut all = self.load()?;
        match all.iter_mut().find(|p| p.name == profile.name) {
            Some(slot) => *slot = profile,
            None => all.push(profile),
        }
        self.save(&all)
    }

    pub fn delete(&self, name: &str) -> anyhow::Result<()> {
        let mut all = self.load()?;
        all.retain(|p| p.name != name);
        self.save(&all)
    }

    /// Switch a profile's automatic application on or off.
    pub fn set_auto(&self, name: &str, auto: bool) -> anyhow::Result<()> {
        let mut all = self.load()?;
        let profile = all
            .iter_mut()
            .find(|p| p.name == name)
            .ok_or_else(|| anyhow!("no profile named {name}"))?;
        profile.auto = auto;
        self.save(&all)
    }
}

fn render(profiles: &[Profile]) -> String {
    let mut out = String::from("// Display profiles, saved from the shell's display settings.\n");
    for p in profiles {
        out.push_str(&format!("\nprofile {:?} auto={} {{\n", p.name, p.auto));
        for o in &p.outputs {
            let c = &o.config;
            let mode = c.mode.as_ref().map_or(String::new(), |m| {
                format!(" mode=\"{}x{}@{:.3}\"", m.width, m.height, m.refresh_rate)
            });
            out.push_str(&format!(
                "    output {:?} identity={:?} enabled={}{} scale={:?} transform={} x={} y={} vrr={}\n",
                c.output, o.identity, c.enabled, mode, c.scale, c.transform, c.x, c.y, c.vrr
            ));
        }
        out.push_str("}\n");
    }
    out
}

fn parse(text: &str) -> anyhow::Result<Vec<Profile>> {
    let doc = kdl_edit::parse(text)?;
    let mut profiles = Vec::new();
    for node in doc.nodes().iter().filter(|n| n.name().value() == "profile") {
        let name = first_arg(node)
            .ok_or_else(|| anyhow!("a profile has no name"))?
            .to_string();
        let auto = prop(node, "auto").and_then(|v| v.as_bool()).unwrap_or(false);
        let mut outputs = Vec::new();
        for out in node
            .children()
            .into_iter()
            .flat_map(|c| c.nodes())
            .filter(|n| n.name().value() == "output")
        {
            let output = first_arg(out)
                .ok_or_else(|| anyhow!("an output of profile {name} has no name"))?
                .to_string();
            let s = |key: &str| prop(out, key).and_then(|v| v.as_string()).map(str::to_string);
            let f = |key: &str| {
                prop(out, key).and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)))
            };
            let i = |key: &str| prop(out, key).and_then(|v| v.as_i64()).unwrap_or(0);
            let b = |key: &str, default: bool| {
                prop(out, key).and_then(|v| v.as_bool()).unwrap_or(default)
            };
            outputs.push(SavedOutput {
                identity: s("identity").unwrap_or_else(|| output.clone()),
                config: OutputConfig {
                    output,
                    enabled: b("enabled", true),
                    mode: s("mode").and_then(|m| OutputMode::parse(&m)),
                    scale: f("scale").unwrap_or(1.0),
                    transform: i("transform").clamp(0, 7) as u16,
                    x: i("x") as i32,
                    y: i("y") as i32,
                    vrr: b("vrr", false),
                },
            });
        }
        profiles.push(Profile { name, auto, outputs });
    }
    Ok(profiles)
}

fn first_arg(node: &kdl::KdlNode) -> Option<&str> {
    node.entries()
        .iter()
        .find(|e| e.name().is_none())
        .and_then(|e| e.value().as_string())
}

fn prop<'a>(node: &'a kdl::KdlNode, key: &str) -> Option<&'a kdl::KdlValue> {
    node.entries()
        .iter()
        .find(|e| e.name().is_some_and(|n| n.value() == key))
        .map(kdl::KdlEntry::value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, serial: &str, x: i32, scale: f32) -> Monitor {
        Monitor {
            name: name.into(),
            make: "Dell".into(),
            model: "U2720Q".into(),
            serial: Some(serial.into()),
            width: 3840,
            height: 2160,
            refresh_rate: 60.0,
            x,
            scale,
            vrr_supported: true,
            ..Default::default()
        }
    }

    #[test]
    fn profiles_survive_the_file() {
        let monitors = vec![monitor("HDMI-A-1", "S1", 0, 1.5), monitor("DP-1", "S2", 3840, 1.0)];
        let profiles = vec![
            Profile::snapshot("Docked \"desk\"", true, &monitors),
            Profile::snapshot("Alone", false, &monitors[..1]),
        ];
        let again = parse(&render(&profiles)).unwrap();
        assert_eq!(again, profiles);
    }

    #[test]
    fn a_profile_fits_the_same_monitors_in_any_port() {
        let at_desk = vec![monitor("HDMI-A-1", "S1", 0, 1.5), monitor("DP-1", "S2", 3840, 1.0)];
        let profile = Profile::snapshot("Docked", true, &at_desk);
        // The cables were swapped.
        let swapped = vec![monitor("DP-1", "S1", 0, 1.0), monitor("HDMI-A-1", "S2", 0, 1.0)];
        assert!(profile.fits(&swapped));
        let plan = profile.plan(&swapped).unwrap();
        let for_s1 = plan.iter().find(|c| c.output == "DP-1").unwrap();
        assert_eq!(for_s1.scale, 1.5);
        assert_eq!(plan.iter().find(|c| c.output == "HDMI-A-1").unwrap().x, 3840);
        // One monitor is not the pair.
        assert!(!profile.fits(&swapped[..1]));
        assert!(profile.plan(&swapped[..1]).is_none());
    }

    #[test]
    fn lookalike_monitors_keep_their_own_ports() {
        let a = Monitor { serial: None, ..monitor("DP-1", "", 0, 1.0) };
        let b = Monitor { serial: None, ..monitor("DP-2", "", 1920, 2.0) };
        let profile = Profile::snapshot("Pair", true, &[a.clone(), b.clone()]);
        let plan = profile.plan(&[a, b]).unwrap();
        assert_eq!(plan.iter().find(|c| c.output == "DP-1").unwrap().scale, 1.0);
        assert_eq!(plan.iter().find(|c| c.output == "DP-2").unwrap().scale, 2.0);
    }

    #[test]
    fn an_applied_profile_is_not_pending() {
        let monitors = vec![monitor("HDMI-A-1", "S1", 0, 1.5)];
        let profile = Profile::snapshot("Docked", true, &monitors);
        assert!(profile.is_applied(&monitors));
        assert!(pending_auto(std::slice::from_ref(&profile), &monitors).is_none());

        let changed = vec![monitor("HDMI-A-1", "S1", 0, 1.0)];
        assert!(!profile.is_applied(&changed));
        assert_eq!(pending_auto(std::slice::from_ref(&profile), &changed).unwrap().name, "Docked");

        // A manual profile is never pending.
        let manual = Profile { auto: false, ..profile };
        assert!(pending_auto(&[manual], &changed).is_none());
    }

    #[test]
    fn the_store_adds_replaces_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::new(dir.path().join("displays.kdl"));
        assert!(store.load().unwrap().is_empty());
        let m = vec![monitor("DP-1", "S", 0, 1.0)];
        store.put(Profile::snapshot("A", false, &m)).unwrap();
        store.put(Profile::snapshot("B", false, &m)).unwrap();
        store.put(Profile::snapshot("A", true, &m)).unwrap();
        let all = store.load().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all[0].auto);
        store.set_auto("B", true).unwrap();
        assert!(store.load().unwrap()[1].auto);
        store.delete("A").unwrap();
        assert_eq!(store.load().unwrap().len(), 1);
        assert!(store.set_auto("nope", true).is_err());
    }
}
