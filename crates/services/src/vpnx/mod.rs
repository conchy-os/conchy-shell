//! conchy-vpnx (mihomo) client.
//!
//! Follows the `org.conchy.Vpnx` daemon on the session bus. The daemon is a
//! separate program that may be missing, or come and go at any time:
//! `available` tracks whether it is there, and the UI hides itself when not.

use std::collections::HashMap;

use anyhow::{Context as _, Result, bail};
use futures_signals::signal::Mutable;
use futures_util::StreamExt;
use futures_util::stream;
use serde::Deserialize;
use tracing::{debug, info};
use vpnx_client::{BUS_NAME, OBJECT_PATH, VpnxProxy};
use zbus::names::BusName;
use zbus::proxy::CacheProperties;
use zbus::{Connection, fdo};

use crate::ServiceCore;
use crate::jobs::JobQueue;
use crate::lifecycle::{Lifecycle, ManagedService, RunToken};

/// Latency probes running at once when testing a whole group.
const DELAY_CONCURRENCY: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VpnxState {
    #[default]
    Down,
    Starting,
    Up,
    Restarting,
    Error,
}

impl VpnxState {
    fn parse(s: &str) -> Self {
        match s {
            "starting" => Self::Starting,
            "up" => Self::Up,
            "restarting" => Self::Restarting,
            "error" => Self::Error,
            _ => Self::Down,
        }
    }

    /// On, or on its way there.
    pub fn is_on(self) -> bool {
        matches!(self, Self::Starting | Self::Up | Self::Restarting)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VpnxMode {
    /// TUN when mihomo may create one, system proxy otherwise.
    #[default]
    Auto,
    Tun,
    Proxy,
}

impl VpnxMode {
    pub const ALL: [VpnxMode; 3] = [VpnxMode::Auto, VpnxMode::Tun, VpnxMode::Proxy];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Tun => "tun",
            Self::Proxy => "proxy",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Tun => "TUN",
            Self::Proxy => "Proxy",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == s)
    }
}

/// A mihomo proxy group as the daemon reports it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProxyGroup {
    pub name: String,
    /// mihomo's type: `Selector`, `URLTest`, `Fallback`, `LoadBalance`, ...
    #[serde(rename = "type")]
    pub kind: String,
    /// Currently used member.
    #[serde(default)]
    pub now: String,
    #[serde(default)]
    pub all: Vec<String>,
}

impl ProxyGroup {
    /// Only selector groups take a manual choice; the others pick by themselves.
    pub fn is_selectable(&self) -> bool {
        self.kind.eq_ignore_ascii_case("Selector")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VpnxData {
    /// The daemon is running on the session bus.
    pub available: bool,
    pub state: VpnxState,
    /// Configured mode.
    pub mode: VpnxMode,
    /// What actually runs (`Tun` or `Proxy`), `None` while down.
    pub effective_mode: Option<VpnxMode>,
    /// Why the daemon is in the error state.
    pub last_error: String,
    /// Problems found while merging the subscription and the user overlay.
    pub warnings: Vec<String>,
    /// Groups in config order, without mihomo's internal `GLOBAL`.
    pub groups: Vec<ProxyGroup>,
    /// Latency per proxy in ms; `None` when the last probe failed.
    pub delays: HashMap<String, Option<u32>>,
    /// The last command that failed, e.g. a subscription download.
    pub command_error: Option<String>,
    /// mihomo may create a TUN device.
    pub can_tun: bool,
    /// The mihomo binary the daemon uses; empty when it is not installed.
    pub mihomo_path: String,
}

impl VpnxData {
    /// The server traffic goes through: the choice of the first selector
    /// group, followed through nested groups (Proxy -> Auto -> node).
    pub fn current_server(&self) -> Option<&str> {
        let mut name = self.groups.iter().find(|g| g.is_selectable())?.now.as_str();
        // Bounded: a config could make groups point at each other.
        for _ in 0..self.groups.len() {
            match self.groups.iter().find(|g| g.name == name) {
                Some(group) if !group.now.is_empty() => name = &group.now,
                _ => break,
            }
        }
        (!name.is_empty()).then_some(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VpnxCommand {
    Up,
    Down,
    SetMode(VpnxMode),
    Select {
        group: String,
        proxy: String,
    },
    /// Measure every member of a group.
    TestDelay(String),
    UpdateSubscription,
    RefreshGroups,
    /// Give mihomo CAP_NET_ADMIN via polkit, then re-apply the mode.
    GrantTun,
}

#[derive(Debug, Clone)]
pub struct VpnxSubscriber {
    core: ServiceCore<VpnxData, VpnxCommand>,
}

impl Default for VpnxSubscriber {
    fn default() -> Self {
        Self {
            core: ServiceCore::with_jobs(JobQueue::new("Vpnx")),
        }
    }
}

impl std::ops::Deref for VpnxSubscriber {
    type Target = ServiceCore<VpnxData, VpnxCommand>;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl VpnxSubscriber {
    pub fn new() -> Self {
        Self::default()
    }

    async fn execute(&self, command: VpnxCommand, token: &RunToken) -> Result<()> {
        let result = self.execute_inner(command, token).await;
        let error = result.as_ref().err().map(daemon_message);
        self.data.lock_mut().command_error = error;
        result
    }

    async fn execute_inner(&self, command: VpnxCommand, token: &RunToken) -> Result<()> {
        let conn = crate::bus::session().await?;
        let vpnx = proxy(&conn).await?;
        match command {
            VpnxCommand::Up => vpnx.up().await?,
            VpnxCommand::Down => vpnx.down().await?,
            VpnxCommand::SetMode(mode) => vpnx.set_mode(mode.as_str()).await?,
            VpnxCommand::UpdateSubscription => vpnx.update_subscription().await?,
            VpnxCommand::Select { group, proxy } => {
                vpnx.select_proxy(&group, &proxy).await?;
                refresh_groups(&vpnx, &self.data).await?;
            }
            VpnxCommand::RefreshGroups => refresh_groups(&vpnx, &self.data).await?,
            VpnxCommand::GrantTun => {
                let mihomo = vpnx.mihomo_path().await?;
                if mihomo.is_empty() {
                    bail!("mihomo is not installed");
                }
                // Through conchy-vpnx-tun, so package hooks keep the
                // capability across mihomo upgrades.
                let cmd = vpnx_client::tun_access_command(true, &mihomo);
                let status = tokio::process::Command::from(cmd)
                    .status()
                    .await
                    .context("running pkexec")?;
                if !status.success() {
                    bail!("granting TUN access failed or was cancelled");
                }
                // Lets `auto` switch from proxy to TUN. While down it only
                // re-checks the capability, so a missing subscription is fine.
                if let Err(e) = vpnx.reload().await
                    && vpnx.state().await? != "down"
                {
                    return Err(e.into());
                }
            }
            VpnxCommand::TestDelay(group) => {
                // Probes take seconds; don't hold up the queue for them.
                let members = self
                    .data
                    .lock_ref()
                    .groups
                    .iter()
                    .find(|g| g.name == group)
                    .map(|g| g.all.clone())
                    .unwrap_or_default();
                let data = self.data.clone();
                let token = token.clone();
                tokio::spawn(async move {
                    token
                        .run_until_cancelled(test_delays(vpnx, members, data))
                        .await;
                });
            }
        }
        Ok(())
    }
}

impl ManagedService for VpnxSubscriber {
    fn name(&self) -> &'static str {
        "Vpnx"
    }

    fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    fn memory_bytes(&self) -> usize {
        let data = self.data.lock_ref();
        size_of::<VpnxData>()
            + data.last_error.len()
            + data.warnings.iter().map(String::len).sum::<usize>()
            + data
                .groups
                .iter()
                .map(|g| {
                    size_of::<ProxyGroup>()
                        + g.name.len()
                        + g.kind.len()
                        + g.now.len()
                        + g.all.iter().map(String::len).sum::<usize>()
                })
                .sum::<usize>()
            + data
                .delays
                .keys()
                .map(|k| k.len() + size_of::<Option<u32>>())
                .sum::<usize>()
    }

    fn start(&self) {
        let token = self.lifecycle.begin();

        let subscriber = self.clone();
        let jobs_token = token.clone();
        self.jobs.run(token.clone(), move |command| {
            let subscriber = subscriber.clone();
            let token = jobs_token.clone();
            async move { subscriber.execute(command, &token).await }
        });

        let data = self.data.clone();
        token.supervise(self.name(), move |token| run(data.clone(), token));
    }
}

async fn proxy(conn: &Connection) -> Result<VpnxProxy<'static>> {
    Ok(VpnxProxy::builder(conn)
        .cache_properties(CacheProperties::No)
        .build()
        .await?)
}

/// The daemon's own message, without zbus's `org.freedesktop.DBus.Error.Failed:` prefix.
fn daemon_message(err: &anyhow::Error) -> String {
    match err.downcast_ref::<zbus::Error>() {
        Some(zbus::Error::MethodError(_, Some(msg), _)) => msg.clone(),
        _ => format!("{err:#}"),
    }
}

/// One run: wait for the daemon, follow it while it is there, repeat.
async fn run(data: Mutable<VpnxData>, token: RunToken) -> Result<()> {
    let conn = crate::bus::session()
        .await
        .context("connecting to the session bus")?;
    let dbus = fdo::DBusProxy::new(&conn).await?;
    // Subscribe before checking, so an owner change in between is not lost.
    let mut owners = dbus
        .receive_name_owner_changed_with_args(&[(0, BUS_NAME)])
        .await?;
    token.active();

    // Installed but not running: start it through D-Bus activation, once per
    // run, so the toggle is there without any setup.
    if !dbus.name_has_owner(BusName::try_from(BUS_NAME)?).await?
        && dbus
            .list_activatable_names()
            .await?
            .iter()
            .any(|n| n.as_str() == BUS_NAME)
    {
        info!("Starting conchy-vpnxd through D-Bus activation");
        if let Err(e) = dbus
            .start_service_by_name(zbus::names::WellKnownName::try_from(BUS_NAME)?, 0)
            .await
        {
            debug!("Activating conchy-vpnxd failed: {e}");
        }
    }

    loop {
        if !dbus.name_has_owner(BusName::try_from(BUS_NAME)?).await? {
            data.set_neq(VpnxData::default());
            debug!("conchy-vpnxd is not running; waiting for it");
            loop {
                let Some(signal) = owners.next().await else {
                    bail!("NameOwnerChanged stream ended");
                };
                if signal.args()?.new_owner().is_some() {
                    break;
                }
            }
        }
        info!("Following conchy-vpnxd");
        follow(&conn, &data, &mut owners).await?;
        info!("conchy-vpnxd went away");
    }
}

/// Mirror the daemon's properties until it leaves the bus.
async fn follow(
    conn: &Connection,
    data: &Mutable<VpnxData>,
    owners: &mut fdo::NameOwnerChangedStream,
) -> Result<()> {
    let vpnx = proxy(conn).await?;
    let props = fdo::PropertiesProxy::builder(conn)
        .destination(BUS_NAME)?
        .path(OBJECT_PATH)?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let mut changes = props.receive_properties_changed().await?;
    refresh(&vpnx, data).await?;

    loop {
        tokio::select! {
            Some(_) = changes.next() => refresh(&vpnx, data).await?,
            owner = owners.next() => {
                let Some(signal) = owner else {
                    bail!("NameOwnerChanged stream ended");
                };
                if signal.args()?.new_owner().is_none() {
                    data.set_neq(VpnxData::default());
                    return Ok(());
                }
                // Replaced by a new instance.
                refresh(&vpnx, data).await?;
            }
        }
    }
}

/// Re-read all properties. They change rarely, so this beats tracking each one.
async fn refresh(vpnx: &VpnxProxy<'_>, data: &Mutable<VpnxData>) -> Result<()> {
    let state = VpnxState::parse(&vpnx.state().await?);
    let mode = VpnxMode::parse(&vpnx.mode().await?).unwrap_or_default();
    let effective_mode = VpnxMode::parse(&vpnx.effective_mode().await?);
    let last_error = vpnx.last_error().await?;
    let warnings = vpnx.warnings().await?;
    let can_tun = vpnx.can_tun().await?;
    let mihomo_path = vpnx.mihomo_path().await?;
    let groups = if state == VpnxState::Up {
        fetch_groups(vpnx).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    debug!(
        "conchy-vpnxd: {state:?}, mode {mode:?} ({effective_mode:?}), {} groups, can_tun {can_tun}",
        groups.len()
    );
    let mut d = data.lock_mut();
    d.available = true;
    d.state = state;
    d.mode = mode;
    d.effective_mode = effective_mode;
    d.last_error = last_error;
    d.warnings = warnings;
    d.can_tun = can_tun;
    d.mihomo_path = mihomo_path;
    if d.groups != groups {
        d.groups = groups;
    }
    if !state.is_on() {
        d.delays.clear();
    }
    Ok(())
}

async fn fetch_groups(vpnx: &VpnxProxy<'_>) -> Result<Vec<ProxyGroup>> {
    let mut groups: Vec<ProxyGroup> = serde_json::from_str(&vpnx.groups().await?)?;
    groups.retain(|g| g.name != "GLOBAL");
    Ok(groups)
}

async fn refresh_groups(vpnx: &VpnxProxy<'_>, data: &Mutable<VpnxData>) -> Result<()> {
    let groups = fetch_groups(vpnx).await?;
    data.lock_mut().groups = groups;
    Ok(())
}

async fn test_delays(vpnx: VpnxProxy<'static>, members: Vec<String>, data: Mutable<VpnxData>) {
    stream::iter(members)
        .map(|name| {
            let vpnx = vpnx.clone();
            async move {
                let delay = vpnx.delay(&name).await.ok();
                (name, delay)
            }
        })
        .buffer_unordered(DELAY_CONCURRENCY)
        // Fill rows in as results arrive.
        .for_each(|(name, delay)| {
            data.lock_mut().delays.insert(name, delay);
            async {}
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_daemon_groups() {
        let json = r#"[{"name":"Proxy","type":"Selector","now":"a","all":["a","b"]},
                       {"name":"Auto","type":"URLTest","now":"b","all":["a","b"]}]"#;
        let groups: Vec<ProxyGroup> = serde_json::from_str(json).unwrap();
        assert!(groups[0].is_selectable());
        assert!(!groups[1].is_selectable());
        assert_eq!(groups[1].now, "b");
    }

    #[test]
    fn current_server_follows_nested_groups() {
        let group = |name: &str, kind: &str, now: &str| ProxyGroup {
            name: name.into(),
            kind: kind.into(),
            now: now.into(),
            all: vec![],
        };
        let mut data = VpnxData {
            groups: vec![
                group("Blocked", "Selector", "Any"),
                group("Any", "Fallback", "FI"),
            ],
            ..Default::default()
        };
        assert_eq!(data.current_server(), Some("FI"));

        data.groups = vec![group("A", "Selector", "B"), group("B", "Selector", "A")];
        assert!(data.current_server().is_some());
        data.groups.clear();
        assert_eq!(data.current_server(), None);
    }

    #[test]
    fn parses_states_and_modes() {
        assert_eq!(VpnxState::parse("up"), VpnxState::Up);
        assert_eq!(VpnxState::parse("???"), VpnxState::Down);
        assert!(VpnxState::Restarting.is_on());
        assert_eq!(VpnxMode::parse("tun"), Some(VpnxMode::Tun));
        assert_eq!(VpnxMode::parse(""), None);
    }
}
