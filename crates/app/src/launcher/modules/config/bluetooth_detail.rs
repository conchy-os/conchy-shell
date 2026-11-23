//! The icon of a Bluetooth device, from what it says it is and, failing that, its name.

use services::{BluetoothDevice, BluetoothDeviceType};
use ui::IconName;

pub fn device_icon(dev: &BluetoothDevice) -> IconName {
    match dev.device_type {
        BluetoothDeviceType::AudioHeadset | BluetoothDeviceType::AudioHeadphones => {
            IconName::Headphones
        }
        BluetoothDeviceType::AudioSpeaker => IconName::Volume,
        BluetoothDeviceType::Keyboard => IconName::Keyboard,
        BluetoothDeviceType::Mouse => IconName::Mouse,
        BluetoothDeviceType::Gamepad => IconName::Gamepad,
        BluetoothDeviceType::Phone => IconName::Phone,
        BluetoothDeviceType::Computer => IconName::ScreenShare,
        BluetoothDeviceType::Unknown => {
            let lower = dev.name.to_lowercase();
            if lower.contains("major")
                || lower.contains("airpod")
                || lower.contains("buds")
                || lower.contains("headphone")
            {
                IconName::Headphones
            } else if lower.contains("mouse") {
                IconName::Mouse
            } else if lower.contains("keyboard") {
                IconName::Keyboard
            } else if lower.contains("speaker") {
                IconName::Volume
            } else if lower.contains("phone") {
                IconName::Phone
            } else if lower.contains("controller") || lower.contains("gamepad") {
                IconName::Gamepad
            } else {
                IconName::Bluetooth
            }
        }
    }
}

