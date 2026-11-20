//! Bluetooth service types.

use zbus::zvariant::OwnedObjectPath;

/// Bluetooth adapter state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BluetoothState {
    /// No Bluetooth adapter available.
    #[default]
    Unavailable,
    /// Bluetooth is powered on and active.
    Active,
    /// Bluetooth adapter exists but is powered off.
    Inactive,
}

/// Category / icon for a Bluetooth device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BluetoothDeviceType {
    #[default]
    Unknown,
    AudioHeadset,
    AudioHeadphones,
    AudioSpeaker,
    Keyboard,
    Mouse,
    Gamepad,
    Phone,
    Computer,
}

impl BluetoothDeviceType {
    pub fn from_icon_and_class(icon: Option<&str>, class: Option<u32>) -> Self {
        if let Some(icon) = icon {
            match icon {
                "audio-headset" => return Self::AudioHeadset,
                "audio-headphones" => return Self::AudioHeadphones,
                "audio-card" | "audio-speakers" => return Self::AudioSpeaker,
                "input-keyboard" => return Self::Keyboard,
                "input-mouse" => return Self::Mouse,
                "input-gaming" => return Self::Gamepad,
                "phone" => return Self::Phone,
                "computer" => return Self::Computer,
                _ => {}
            }
        }
        if let Some(class) = class {
            let major = (class >> 8) & 0x1f;
            let minor = (class >> 2) & 0x3f;
            match major {
                1 => Self::Computer,
                2 => Self::Phone,
                4 => match minor {
                    1 | 2 => Self::AudioHeadset,
                    5 | 6 => Self::AudioHeadphones,
                    _ => Self::AudioSpeaker,
                },
                5 => match minor {
                    1 => Self::Keyboard,
                    2 => Self::Mouse,
                    _ => Self::Gamepad,
                },
                _ => Self::Unknown,
            }
        } else {
            Self::Unknown
        }
    }
}

/// A Bluetooth device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothDevice {
    /// Device name (alias).
    pub name: String,
    /// Hardware MAC address (e.g. "00:25:D1:45:76:E7").
    pub address: String,
    /// Battery percentage if available.
    pub battery: Option<u8>,
    /// D-Bus object path for this device.
    pub path: OwnedObjectPath,
    /// Whether the device is currently connected.
    pub connected: bool,
    /// Whether the device is paired.
    pub paired: bool,
    /// Whether the device is trusted (auto-reconnects).
    pub trusted: bool,
    /// Signal strength in dBm, if available.
    pub rssi: Option<i16>,
    /// Icon string from BlueZ.
    pub icon: Option<String>,
    /// Device type derived from icon and class.
    pub device_type: BluetoothDeviceType,
}

/// Bluetooth service data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BluetoothData {
    /// Current Bluetooth adapter state.
    pub state: BluetoothState,
    /// Adapter friendly name or alias.
    pub adapter_name: String,
    /// Adapter MAC address.
    pub adapter_address: String,
    /// Whether the adapter is discoverable to other devices.
    pub discoverable: bool,
    /// List of known Bluetooth devices.
    pub devices: Vec<BluetoothDevice>,
    /// Whether device discovery is currently active.
    pub discovering: bool,
}

impl BluetoothData {
    /// Check if any device is currently connected.
    pub fn has_connected_device(&self) -> bool {
        self.devices.iter().any(|d| d.connected)
    }

    /// Get connected devices.
    pub fn connected_devices(&self) -> impl Iterator<Item = &BluetoothDevice> {
        self.devices.iter().filter(|d| d.connected)
    }

    /// Get paired devices.
    pub fn paired_devices(&self) -> impl Iterator<Item = &BluetoothDevice> {
        self.devices.iter().filter(|d| d.paired)
    }
}

/// Commands for the Bluetooth service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothCommand {
    /// Set adapter power on or off.
    SetPowered(bool),
    /// Toggle Bluetooth power on/off.
    Toggle,
    /// Start scanning for nearby devices.
    StartDiscovery,
    /// Stop scanning for devices.
    StopDiscovery,
    /// Pair with a device.
    PairDevice(OwnedObjectPath),
    /// Connect to a paired device.
    ConnectDevice(OwnedObjectPath),
    /// Disconnect from a device.
    DisconnectDevice(OwnedObjectPath),
    /// Remove/unpair a device.
    RemoveDevice(OwnedObjectPath),
    /// Trust or untrust a device.
    SetTrusted(OwnedObjectPath, bool),
    /// Set discoverable mode on adapter.
    SetDiscoverable(bool),
}
