//! Bluetooth service for device management via BlueZ.
//!
//! This module provides a reactive subscriber for monitoring and controlling
//! Bluetooth adapters and devices using BlueZ D-Bus interface.

mod dbus;
mod types;

pub use types::*;

use anyhow::Context as _;
use dbus::BluetoothDbus;
use futures_signals::signal::Mutable;
use futures_util::StreamExt;
use futures_util::stream::{BoxStream, select_all};
use inotify::{Inotify, WatchMask};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::task::AbortHandle;
use tracing::{debug, error, warn};

use crate::lifecycle::{Lifecycle, ManagedService, RunToken};

/// Event-driven Bluetooth subscriber.
///
/// This subscriber monitors Bluetooth adapter and device state via BlueZ D-Bus
/// and provides reactive state updates through `futures_signals`.
#[derive(Debug, Clone, Default)]
pub struct BluetoothSubscriber {
    core: crate::ServiceCore<BluetoothData, BluetoothCommand>,
    /// Stops a discovery after 15s. Replaced by every discovery command, so
    /// a timer from an earlier scan cannot cut a newer one short.
    discovery_timer: Arc<Mutex<Option<AbortHandle>>>,
}

impl std::ops::Deref for BluetoothSubscriber {
    type Target = crate::ServiceCore<BluetoothData, BluetoothCommand>;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl BluetoothSubscriber {
    /// Create a new Bluetooth subscriber. Monitoring starts with
    /// [`ManagedService::start`].
    pub fn new() -> Self {
        Self::default()
    }

    async fn execute(&self, command: BluetoothCommand, token: &RunToken) -> anyhow::Result<()> {
        let conn = crate::bus::system().await?;
        let bluetooth = BluetoothDbus::new(&conn).await?;

        if matches!(
            command,
            BluetoothCommand::StartDiscovery | BluetoothCommand::StopDiscovery
        ) && let Some(timer) = self
            .discovery_timer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            timer.abort();
        }

        match command {
            BluetoothCommand::SetPowered(powered) => {
                debug!("Setting Bluetooth powered: {powered}");
                set_bluetooth_power(powered).await?;
            }
            BluetoothCommand::Toggle => {
                let current_active = {
                    let data = self.data.lock_ref();
                    data.state == BluetoothState::Active
                };
                let target = !current_active;
                debug!("Toggling Bluetooth: {} -> {}", current_active, target);
                set_bluetooth_power(target).await?;
            }
            BluetoothCommand::SetDiscoverable(discoverable) => {
                debug!("Setting Bluetooth discoverable: {discoverable}");
                bluetooth.set_discoverable(discoverable).await?;
            }
            BluetoothCommand::SetTrusted(device_path, trusted) => {
                debug!("Setting Bluetooth device {device_path:?} trusted: {trusted}");
                bluetooth.set_trusted(&device_path, trusted).await?;
            }
            BluetoothCommand::StartDiscovery => {
                debug!("Starting Bluetooth discovery");
                bluetooth.start_discovery().await?;

                // Auto-stop discovery after 15 seconds, unless the run ends first.
                let token = token.clone();
                let timer = tokio::spawn(async move {
                    token
                        .run_until_cancelled(async {
                            tokio::time::sleep(Duration::from_secs(15)).await;
                            if let Ok(bt) = BluetoothDbus::new(&conn).await {
                                let _ = bt.stop_discovery().await;
                                debug!("Auto-stopped Bluetooth discovery after 15 seconds");
                            }
                        })
                        .await;
                });
                *self
                    .discovery_timer
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(timer.abort_handle());
            }
            BluetoothCommand::StopDiscovery => {
                debug!("Stopping Bluetooth discovery");
                bluetooth.stop_discovery().await?;
            }
            BluetoothCommand::PairDevice(device_path) => {
                debug!("Pairing Bluetooth device: {:?}", device_path);
                bluetooth.pair_device(&device_path).await?;
            }
            BluetoothCommand::ConnectDevice(device_path) => {
                debug!("Connecting to Bluetooth device: {:?}", device_path);
                bluetooth.connect_device(&device_path).await?;
            }
            BluetoothCommand::DisconnectDevice(device_path) => {
                debug!("Disconnecting Bluetooth device: {:?}", device_path);
                bluetooth.disconnect_device(&device_path).await?;
            }
            BluetoothCommand::RemoveDevice(device_path) => {
                debug!("Removing Bluetooth device: {:?}", device_path);
                bluetooth.remove_device(&device_path).await?;
            }
        }

        Ok(())
    }
}

/// Unblock RFKill for bluetooth devices directly via `/dev/rfkill` or `rfkill` CLI.
fn unblock_rfkill_bluetooth() {
    #[repr(C)]
    struct RfkillEvent {
        idx: u32,
        type_: u8,
        op: u8,
        soft: u8,
        hard: u8,
    }

    const RFKILL_TYPE_BLUETOOTH: u8 = 2;
    const RFKILL_OP_CHANGE_ALL: u8 = 3;

    let event = RfkillEvent {
        idx: 0,
        type_: RFKILL_TYPE_BLUETOOTH,
        op: RFKILL_OP_CHANGE_ALL,
        soft: 0,
        hard: 0,
    };

    let mut written = false;
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open("/dev/rfkill") {
        let slice = unsafe {
            std::slice::from_raw_parts(
                &event as *const _ as *const u8,
                std::mem::size_of::<RfkillEvent>(),
            )
        };
        use std::io::Write as _;
        if f.write_all(slice).is_ok() {
            written = true;
        }
    }

    if !written {
        let _ = std::process::Command::new("rfkill")
            .args(["unblock", "bluetooth"])
            .output();
    }
}

/// Reliably set Bluetooth power state with rfkill unblocking and hardware wake retry loop.
async fn set_bluetooth_power(target_powered: bool) -> anyhow::Result<()> {
    if target_powered {
        // 1. Unblock rfkill first
        unblock_rfkill_bluetooth();

        // 2. Hardware may need 100-300ms to exit low-power state. Retry setting powered:
        let conn = crate::bus::system().await?;
        let mut last_err = None;
        for attempt in 0..15 {
            if let Ok(bt) = BluetoothDbus::new(&conn).await {
                match bt.set_powered(true).await {
                    Ok(()) => {
                        debug!(
                            "Bluetooth powered on successfully on attempt {}",
                            attempt + 1
                        );
                        return Ok(());
                    }
                    Err(e) => {
                        last_err = Some(e);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if let Some(e) = last_err {
            warn!("Failed to power on Bluetooth adapter after retries: {e}");
        }
    } else {
        let conn = crate::bus::system().await?;
        if let Ok(bt) = BluetoothDbus::new(&conn).await {
            let _ = bt.set_powered(false).await;
        }
    }
    Ok(())
}

/// Whether any Bluetooth radio is soft-blocked, read from sysfs instead of
/// forking `rfkill` on every BlueZ signal.
fn check_rfkill_soft_block() -> bool {
    let Ok(entries) = std::fs::read_dir("/sys/class/rfkill") else {
        return false;
    };
    entries.flatten().any(|entry| {
        let read = |name| std::fs::read_to_string(entry.path().join(name)).unwrap_or_default();
        read("type").trim() == "bluetooth" && read("soft").trim() == "1"
    })
}

/// Fetch the current Bluetooth data state.
async fn fetch_bluetooth_data(conn: &zbus::Connection) -> anyhow::Result<BluetoothData> {
    let bluetooth = BluetoothDbus::new(conn).await?;

    let state = bluetooth.state().await?;
    let rfkill_soft_block = check_rfkill_soft_block();

    // Account for rfkill soft block
    let state = match state {
        BluetoothState::Unavailable => BluetoothState::Unavailable,
        BluetoothState::Active if rfkill_soft_block => BluetoothState::Inactive,
        state => state,
    };

    let (adapter_name, adapter_address, discoverable) = bluetooth.adapter_details().await;
    let devices = bluetooth.devices().await?;
    let discovering = bluetooth.discovering().await.unwrap_or(false);

    Ok(BluetoothData {
        state,
        adapter_name,
        adapter_address,
        discoverable,
        devices,
        discovering,
    })
}

impl ManagedService for BluetoothSubscriber {
    fn name(&self) -> &'static str {
        "Bluetooth"
    }

    fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    fn memory_bytes(&self) -> usize {
        let data = self.data.lock_ref();
        size_of::<BluetoothData>()
            + data.adapter_name.len()
            + data.adapter_address.len()
            + data
                .devices
                .iter()
                .map(|device| {
                    size_of::<BluetoothDevice>()
                        + device.name.len()
                        + device.address.len()
                        + device.path.as_str().len()
                        + device.icon.as_ref().map(|s| s.len()).unwrap_or(0)
                })
                .sum::<usize>()
    }

    fn start(&self) {
        let token = self.lifecycle.begin();
        let data = self.data.clone();

        let subscriber = self.clone();
        let jobs_token = token.clone();
        self.jobs.run(token.clone(), move |command| {
            let subscriber = subscriber.clone();
            let token = jobs_token.clone();
            async move { subscriber.execute(command, &token).await }
        });

        token.supervise(self.name(), move |token| run(data.clone(), token));
    }
}

/// One run: snapshot, then follow BlueZ until every stream ends.
async fn run(data: Mutable<BluetoothData>, token: RunToken) -> anyhow::Result<()> {
    let conn = crate::bus::system()
        .await
        .context("connecting to the system bus")?;
    let fetched = fetch_bluetooth_data(&conn)
        .await
        .context("fetching initial Bluetooth state")?;
    data.set_neq(fetched);
    token.active();
    run_listener(data, conn).await
}

/// Run the Bluetooth service listener.
///
/// Every event triggers a full refetch, so the streams are merged into one
/// stream of "something changed" ticks.
async fn run_listener(data: Mutable<BluetoothData>, conn: zbus::Connection) -> anyhow::Result<()> {
    let bluetooth = BluetoothDbus::new(&conn).await?;

    let mut changes: Vec<BoxStream<'_, &'static str>> = vec![
        bluetooth
            .bluez
            .receive_interfaces_added()
            .await?
            .map(|_| "interfaces added")
            .boxed(),
        bluetooth
            .bluez
            .receive_interfaces_removed()
            .await?
            .map(|_| "interfaces removed")
            .boxed(),
    ];

    if let Some(adapter) = &bluetooth.adapter {
        changes.push(
            adapter
                .receive_powered_changed()
                .await
                .map(|_| "powered state changed")
                .boxed(),
        );
        changes.push(
            adapter
                .receive_discovering_changed()
                .await
                .map(|_| "discovering state changed")
                .boxed(),
        );
    }

    if let Some(rfkill) = rfkill_changes() {
        changes.push(rfkill);
    }

    let mut changes = select_all(changes);
    while let Some(change) = changes.next().await {
        debug!("Bluetooth {change}");
        if let Ok(new_data) = fetch_bluetooth_data(&conn).await {
            data.set_neq(new_data);
        }
    }

    anyhow::bail!("Bluetooth event streams ended")
}

/// rfkill soft-block changes, watched with inotify on the tokio reactor so
/// the watch goes away with the run instead of parking a thread.
fn rfkill_changes() -> Option<BoxStream<'static, &'static str>> {
    let inotify = Inotify::init().ok()?;
    inotify
        .watches()
        .add("/dev/rfkill", WatchMask::MODIFY)
        .ok()?;

    let events = match inotify.into_event_stream([0; 512]) {
        Ok(events) => events,
        Err(e) => {
            error!("Error watching rfkill events: {}", e);
            return None;
        }
    };

    Some(
        events
            .take_while(|event| {
                if let Err(e) = event {
                    error!("Error reading rfkill events: {}", e);
                }
                std::future::ready(event.is_ok())
            })
            .map(|_| "rfkill state changed")
            .boxed(),
    )
}
