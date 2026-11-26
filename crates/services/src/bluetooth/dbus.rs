//! D-Bus proxy definitions for BlueZ Bluetooth service.

use super::types::{BluetoothDevice, BluetoothDeviceType, BluetoothState};
use anyhow::Result;
use std::collections::HashMap;
use zbus::{proxy, zvariant::OwnedObjectPath, zvariant::OwnedValue};

type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

/// Helper struct for interacting with BlueZ over D-Bus.
pub struct BluetoothDbus<'a> {
    pub bluez: BluezObjectManagerProxy<'a>,
    pub adapter: Option<AdapterProxy<'a>>,
}

fn get_str(props: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    props
        .get(key)
        .and_then(|v| String::try_from(v.clone()).ok())
}

fn get_bool(props: &HashMap<String, OwnedValue>, key: &str) -> bool {
    props
        .get(key)
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(false)
}

fn get_u8(props: &HashMap<String, OwnedValue>, key: &str) -> Option<u8> {
    props.get(key).and_then(|v| u8::try_from(v.clone()).ok())
}

fn get_u32(props: &HashMap<String, OwnedValue>, key: &str) -> Option<u32> {
    props.get(key).and_then(|v| u32::try_from(v.clone()).ok())
}

fn get_i16(props: &HashMap<String, OwnedValue>, key: &str) -> Option<i16> {
    props.get(key).and_then(|v| i16::try_from(v.clone()).ok())
}

impl BluetoothDbus<'_> {
    /// Create a new BluetoothDbus instance.
    pub async fn new(conn: &zbus::Connection) -> Result<BluetoothDbus<'_>> {
        let bluez = BluezObjectManagerProxy::new(conn).await?;

        // Find the first Bluetooth adapter
        let adapter_path = bluez
            .get_managed_objects()
            .await?
            .into_iter()
            .filter_map(|(key, item)| {
                if item.contains_key("org.bluez.Adapter1") {
                    Some(key)
                } else {
                    None
                }
            })
            .next();

        let adapter = if let Some(adapter_path) = adapter_path {
            Some(
                AdapterProxy::builder(conn)
                    .path(adapter_path)?
                    .build()
                    .await?,
            )
        } else {
            None
        };

        Ok(BluetoothDbus { bluez, adapter })
    }

    /// Set the Bluetooth adapter power state.
    pub async fn set_powered(&self, value: bool) -> zbus::Result<()> {
        if let Some(adapter) = &self.adapter {
            adapter.set_powered(value).await?;
        }
        Ok(())
    }

    /// Get adapter name, address, and discoverable status.
    pub async fn adapter_details(&self) -> (String, String, bool) {
        if let Some(adapter) = &self.adapter {
            let name = match adapter.alias().await {
                Ok(alias) if !alias.is_empty() => alias,
                _ => adapter
                    .name()
                    .await
                    .unwrap_or_else(|_| "Bluetooth Adapter".to_string()),
            };
            let address = adapter.address().await.unwrap_or_default();
            let discoverable = adapter.discoverable().await.unwrap_or(false);
            (name, address, discoverable)
        } else {
            (String::new(), String::new(), false)
        }
    }

    /// Set adapter discoverable state.
    pub async fn set_discoverable(&self, value: bool) -> zbus::Result<()> {
        if let Some(adapter) = &self.adapter {
            adapter.set_discoverable(value).await?;
        }
        Ok(())
    }

    /// Set device trusted state.
    pub async fn set_trusted(
        &self,
        device_path: &OwnedObjectPath,
        value: bool,
    ) -> zbus::Result<()> {
        let device = DeviceProxy::builder(self.bluez.inner().connection())
            .path(device_path)?
            .build()
            .await?;
        device.set_trusted(value).await
    }

    /// Get the current Bluetooth state.
    pub async fn state(&self) -> zbus::Result<BluetoothState> {
        match &self.adapter {
            Some(adapter) => {
                if adapter.powered().await? {
                    Ok(BluetoothState::Active)
                } else {
                    Ok(BluetoothState::Inactive)
                }
            }
            None => Ok(BluetoothState::Unavailable),
        }
    }

    /// Start device discovery.
    pub async fn start_discovery(&self) -> zbus::Result<()> {
        if let Some(adapter) = &self.adapter {
            adapter.start_discovery().await?;
        }
        Ok(())
    }

    /// Stop device discovery.
    pub async fn stop_discovery(&self) -> zbus::Result<()> {
        if let Some(adapter) = &self.adapter {
            adapter.stop_discovery().await?;
        }
        Ok(())
    }

    /// Check if device discovery is currently active.
    pub async fn discovering(&self) -> zbus::Result<bool> {
        match &self.adapter {
            Some(adapter) => adapter.discovering().await,
            None => Ok(false),
        }
    }

    /// Get all known Bluetooth devices.
    pub async fn devices(&self) -> Result<Vec<BluetoothDevice>> {
        let objects = self.bluez.get_managed_objects().await?;
        let mut devices = Vec::new();

        for (device_path, interfaces) in objects {
            let Some(device_props) = interfaces.get("org.bluez.Device1") else {
                continue;
            };

            let name = get_str(device_props, "Alias")
                .or_else(|| get_str(device_props, "Name"))
                .unwrap_or_else(|| "Unknown device".to_string());
            let address = get_str(device_props, "Address").unwrap_or_default();
            let icon = get_str(device_props, "Icon");
            let class = get_u32(device_props, "Class");
            let connected = get_bool(device_props, "Connected");
            let paired = get_bool(device_props, "Paired");
            let trusted = get_bool(device_props, "Trusted");
            let rssi = get_i16(device_props, "RSSI");
            let device_type = BluetoothDeviceType::from_icon_and_class(icon.as_deref(), class);

            let battery = interfaces
                .get("org.bluez.Battery1")
                .and_then(|battery_props| get_u8(battery_props, "Percentage"));

            devices.push(BluetoothDevice {
                name,
                address,
                battery,
                path: device_path,
                connected,
                paired,
                trusted,
                rssi,
                icon,
                device_type,
            });
        }

        Ok(devices)
    }

    /// Pair with a device.
    pub async fn pair_device(&self, device_path: &OwnedObjectPath) -> zbus::Result<()> {
        let device = DeviceProxy::builder(self.bluez.inner().connection())
            .path(device_path)?
            .build()
            .await?;
        device.pair().await
    }

    /// Connect to a device.
    pub async fn connect_device(&self, device_path: &OwnedObjectPath) -> zbus::Result<()> {
        let device = DeviceProxy::builder(self.bluez.inner().connection())
            .path(device_path)?
            .build()
            .await?;
        device.connect().await
    }

    /// Disconnect from a device.
    pub async fn disconnect_device(&self, device_path: &OwnedObjectPath) -> zbus::Result<()> {
        let device = DeviceProxy::builder(self.bluez.inner().connection())
            .path(device_path)?
            .build()
            .await?;
        device.disconnect().await
    }

    /// Remove/unpair a device.
    pub async fn remove_device(&self, device_path: &OwnedObjectPath) -> zbus::Result<()> {
        if let Some(adapter) = &self.adapter {
            adapter.remove_device(device_path.as_ref()).await?;
        }
        Ok(())
    }
}

/// BlueZ ObjectManager proxy for discovering adapters and devices.
#[proxy(
    default_service = "org.bluez",
    default_path = "/",
    interface = "org.freedesktop.DBus.ObjectManager"
)]
pub trait BluezObjectManager {
    /// Get all managed objects (adapters, devices, etc.)
    fn get_managed_objects(&self) -> zbus::Result<ManagedObjects>;

    /// Signal emitted when interfaces are added.
    #[zbus(signal)]
    fn interfaces_added(
        &self,
        object_path: OwnedObjectPath,
        interfaces: HashMap<String, HashMap<String, OwnedValue>>,
    ) -> zbus::Result<()>;

    /// Signal emitted when interfaces are removed.
    #[zbus(signal)]
    fn interfaces_removed(
        &self,
        object_path: OwnedObjectPath,
        interfaces: Vec<String>,
    ) -> zbus::Result<()>;
}

/// BlueZ Adapter proxy.
#[proxy(
    default_service = "org.bluez",
    default_path = "/org/bluez/hci0",
    interface = "org.bluez.Adapter1"
)]
pub trait Adapter {
    /// Whether the adapter is powered on.
    #[zbus(property)]
    fn powered(&self) -> zbus::Result<bool>;

    /// Set the adapter power state.
    #[zbus(property)]
    fn set_powered(&self, value: bool) -> zbus::Result<()>;

    /// Adapter MAC address.
    #[zbus(property)]
    fn address(&self) -> zbus::Result<String>;

    /// Adapter alias / friendly name.
    #[zbus(property)]
    fn alias(&self) -> zbus::Result<String>;

    /// Adapter name.
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;

    /// Whether the adapter is discoverable by other devices.
    #[zbus(property)]
    fn discoverable(&self) -> zbus::Result<bool>;

    /// Set whether the adapter is discoverable.
    #[zbus(property)]
    fn set_discoverable(&self, value: bool) -> zbus::Result<()>;

    /// Start device discovery.
    fn start_discovery(&self) -> zbus::Result<()>;

    /// Stop device discovery.
    fn stop_discovery(&self) -> zbus::Result<()>;

    /// Whether device discovery is active.
    #[zbus(property)]
    fn discovering(&self) -> zbus::Result<bool>;

    /// Remove a device from the adapter.
    fn remove_device(&self, device: zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;
}

/// BlueZ Device proxy.
#[proxy(default_service = "org.bluez", interface = "org.bluez.Device1")]
pub trait Device {
    /// Device alias (friendly name).
    #[zbus(property)]
    fn alias(&self) -> zbus::Result<String>;

    /// Set device alias.
    #[zbus(property)]
    fn set_alias(&self, value: String) -> zbus::Result<()>;

    /// Device MAC address.
    #[zbus(property)]
    fn address(&self) -> zbus::Result<String>;

    /// Device icon name.
    #[zbus(property)]
    fn icon(&self) -> zbus::Result<String>;

    /// Device class.
    #[zbus(property)]
    fn class(&self) -> zbus::Result<u32>;

    /// Whether the device is connected.
    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;

    /// Whether the device is paired.
    #[zbus(property)]
    fn paired(&self) -> zbus::Result<bool>;

    /// Whether the device is trusted.
    #[zbus(property)]
    fn trusted(&self) -> zbus::Result<bool>;

    /// Set whether the device is trusted.
    #[zbus(property)]
    fn set_trusted(&self, value: bool) -> zbus::Result<()>;

    /// Signal strength in dBm.
    #[zbus(property)]
    fn rssi(&self) -> zbus::Result<i16>;

    /// Pair with the device.
    fn pair(&self) -> zbus::Result<()>;

    /// Connect to the device.
    fn connect(&self) -> zbus::Result<()>;

    /// Disconnect from the device.
    fn disconnect(&self) -> zbus::Result<()>;
}

/// BlueZ Battery proxy (for devices that report battery level).
#[proxy(default_service = "org.bluez", interface = "org.bluez.Battery1")]
pub trait Battery {
    /// Battery percentage (0-100).
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<u8>;
}
