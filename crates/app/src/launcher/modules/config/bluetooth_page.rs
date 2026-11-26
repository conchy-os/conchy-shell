//! Bluetooth: the form beside the "Bluetooth" row. The adapter's switches,
//! the paired devices and the ones in range, each a row the keyboard can act on.

use gpui::{App, SharedString};
use services::{BluetoothCommand, BluetoothState};
use ui::{Color, IconName};

use super::bluetooth_detail::device_icon;
use super::settings_form::{self as rows, Entry};
use crate::launcher::action::{Action, Outcome};
use crate::launcher::content::{Detail, Form, Item};
use crate::state::AppState;

pub fn item() -> Item {
    Item::new("bluetooth", "Bluetooth")
        .subtitle("Devices, pairing, and adapter settings")
        .icon(IconName::Bluetooth)
        .keywords("bluetooth bt pairing devices headphones audio wireless")
        .actions(super::system::edit())
        .detail(Detail::new().form(bluetooth_form))
}

fn bluetooth_form(form: &mut Form, cx: &App) {
    let service = AppState::bluetooth(cx);
    let data = service.get();
    let active = data.state == BluetoothState::Active;

    rows::title(
        form,
        "Bluetooth",
        "The adapter, paired accessories and nearby discovery",
    );

    rows::heading(form, "Adapter");
    rows::toggle(
        form,
        "bt-power",
        "Bluetooth",
        "Turn the adapter on or off",
        active,
        {
            let service = service.clone();
            move |_, _| service.dispatch(BluetoothCommand::Toggle)
        },
        cx,
    );
    if !active {
        rows::note(form, "Bluetooth is turned off.", Color::Muted);
        return;
    }
    let discovering = data.discovering;
    rows::toggle(
        form,
        "bt-scan",
        "Look for devices",
        "Scan for accessories in pairing mode",
        discovering,
        {
            let service = service.clone();
            move |_, _| {
                service.dispatch(if discovering {
                    BluetoothCommand::StopDiscovery
                } else {
                    BluetoothCommand::StartDiscovery
                })
            }
        },
        cx,
    );
    let discoverable = data.discoverable;
    rows::toggle(
        form,
        "bt-discoverable",
        "Visible to other devices",
        "Let other devices find this computer",
        discoverable,
        {
            let service = service.clone();
            move |_, _| service.dispatch(BluetoothCommand::SetDiscoverable(!discoverable))
        },
        cx,
    );

    let paired: Vec<_> = data.devices.iter().filter(|d| d.paired).collect();
    if !paired.is_empty() {
        rows::heading(form, "Paired devices");
    }
    for dev in paired {
        let (connected, trusted) = (dev.connected, dev.trusted);
        let connect = {
            let (service, path) = (service.clone(), dev.path.clone());
            Action::new(
                if connected { "Disconnect" } else { "Connect" },
                move |_, _| {
                    service.dispatch(if connected {
                        BluetoothCommand::DisconnectDevice(path.clone())
                    } else {
                        BluetoothCommand::ConnectDevice(path.clone())
                    });
                    Outcome::Stay
                },
            )
            .icon(IconName::Bluetooth)
        };
        let trust = {
            let (service, path) = (service.clone(), dev.path.clone());
            Action::new(if trusted { "Untrust" } else { "Trust" }, move |_, _| {
                service.dispatch(BluetoothCommand::SetTrusted(path.clone(), !trusted));
                Outcome::Stay
            })
            .icon(IconName::Check)
        };
        let remove = {
            let (service, path) = (service.clone(), dev.path.clone());
            Action::new("Unpair", move |_, _| {
                service.dispatch(BluetoothCommand::RemoveDevice(path.clone()));
                Outcome::Stay
            })
            .icon(IconName::Trash)
            .destructive()
            .shortcut("ctrl-d")
        };
        let mut subtitle = dev.address.clone();
        if let Some(battery) = dev.battery {
            subtitle.push_str(&format!(" • battery {battery}%"));
        }
        if trusted {
            subtitle.push_str(" • trusted");
        }
        rows::entry(
            form,
            Entry {
                icon: device_icon(dev),
                title: dev.name.clone().into(),
                subtitle: subtitle.into(),
                tag: Some(if connected {
                    ("Connected".into(), Color::Success)
                } else {
                    ("Not connected".into(), Color::Muted)
                }),
            },
            vec![connect, trust, remove],
            cx,
        );
    }

    let nearby: Vec<_> = data.devices.iter().filter(|d| !d.paired).collect();
    if !nearby.is_empty() || discovering {
        rows::heading(
            form,
            if discovering {
                "Nearby devices (scanning…)"
            } else {
                "Nearby devices"
            },
        );
    }
    if nearby.is_empty() && discovering {
        rows::note(
            form,
            "Make sure the accessory is in pairing mode.",
            Color::Muted,
        );
    }
    for dev in nearby {
        let pair = {
            let (service, path) = (service.clone(), dev.path.clone());
            Action::new("Pair & connect", move |_, _| {
                service.dispatch(BluetoothCommand::PairDevice(path.clone()));
                Outcome::Stay
            })
            .icon(IconName::Plus)
        };
        let tag: Option<(SharedString, Color)> = dev
            .rssi
            .map(|rssi| (format!("{rssi} dBm").into(), Color::Muted));
        rows::entry(
            form,
            Entry {
                icon: device_icon(dev),
                title: dev.name.clone().into(),
                subtitle: dev.address.clone().into(),
                tag,
            },
            vec![pair],
            cx,
        );
    }
}
