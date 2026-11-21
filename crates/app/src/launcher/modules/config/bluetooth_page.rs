use gpui::{App, Context, prelude::*};
use services::{BluetoothCommand, BluetoothState};
use ui::{
    ActiveTheme, Color, IconName,
};

use super::bluetooth_detail::device_icon;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, EmptyView, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};
use crate::state::{AppState, watch};

pub struct BluetoothPage;

impl BluetoothPage {
    pub fn open(cx: &mut App) -> Page {
        Page::new(cx.new(|cx| Self::new(cx)))
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        let bluetooth = AppState::bluetooth(cx);
        watch(cx, bluetooth.subscribe(), |_, _, cx| {
            cx.notify();
        });
        Self
    }
}

impl LauncherPage for BluetoothPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let service = AppState::bluetooth(cx);
        let data = service.get();

        let is_active = data.state == BluetoothState::Active;
        let discovering = data.discovering;

        let mut list = List::new()
            .show_detail(true)
            .click_selects(true)
            .title("Bluetooth")
            .placeholder("Search Bluetooth devices…");

        // Global actions
        let mut actions = ActionPanel::default()
            .action(
                Action::new(
                    if is_active {
                        "Turn Bluetooth Off"
                    } else {
                        "Turn Bluetooth On"
                    },
                    {
                        let service = service.clone();
                        move |_, _| {
                            service.dispatch(BluetoothCommand::Toggle);
                            Outcome::Stay
                        }
                    },
                )
                .icon(if is_active {
                    IconName::BluetoothOff
                } else {
                    IconName::Bluetooth
                }),
            );

        if is_active {
            let s_scan = service.clone();
            actions = actions.action(
                Action::new(
                    if discovering {
                        "Stop Scanning"
                    } else {
                        "Scan for Devices"
                    },
                    move |_, _| {
                        s_scan.dispatch(if discovering {
                            BluetoothCommand::StopDiscovery
                        } else {
                            BluetoothCommand::StartDiscovery
                        });
                        Outcome::Stay
                    },
                )
                .icon(IconName::Refresh),
            );

            let s_disc = service.clone();
            let current_disc = data.discoverable;
            actions = actions.action(Action::new(
                if current_disc {
                    "Disable Discoverable"
                } else {
                    "Enable Discoverable"
                },
                move |_, _| {
                    s_disc.dispatch(BluetoothCommand::SetDiscoverable(!current_disc));
                    Outcome::Stay
                },
            ));
        }

        list = list.actions(actions);

        if !is_active {
            list = list.empty(
                EmptyView::new("Bluetooth is turned off")
                    .description("Press Enter or trigger power action to turn Bluetooth on.")
                    .icon(IconName::BluetoothOff),
            );
            return list.into();
        }

        // 1. Paired Devices
        let paired: Vec<_> = data.devices.iter().filter(|d| d.paired).cloned().collect();
        let mut paired_items = Vec::new();

        for (idx, dev) in paired.into_iter().enumerate() {
            let is_connected = dev.connected;
            let icon = device_icon(&dev);

            let subtitle = if is_connected {
                format!("{} • Connected", dev.address)
            } else {
                format!("{} • Disconnected", dev.address)
            };

            let mut item = Item::new(format!("paired-{idx}"), dev.name.clone())
                .subtitle(subtitle)
                .icon(icon);

            if is_connected {
                item = item.accessory(Accessory::tag("Connected").color(Color::Custom(
                    cx.theme().colors().status.success,
                )));
            }

            if let Some(battery) = dev.battery {
                item = item.accessory(Accessory::text(format!("🔋 {battery}%")));
            }

            if dev.trusted {
                item = item.accessory(
                    Accessory::icon(IconName::Check)
                        .color(Color::Custom(cx.theme().colors().status.success)),
                );
            }

            let path = dev.path.clone();
            let s = service.clone();
            let is_trusted = dev.trusted;

            let mut item_actions = ActionPanel::default().action(Action::new(
                if is_connected {
                    "Disconnect"
                } else {
                    "Connect"
                },
                {
                    let s = s.clone();
                    let p = path.clone();
                    move |_, _| {
                        s.dispatch(if is_connected {
                            BluetoothCommand::DisconnectDevice(p.clone())
                        } else {
                            BluetoothCommand::ConnectDevice(p.clone())
                        });
                        Outcome::Stay
                    }
                },
            ));

            let p_trust = path.clone();
            let s_trust = s.clone();
            item_actions = item_actions.action(Action::new(
                if is_trusted {
                    "Untrust Device"
                } else {
                    "Trust Device"
                },
                move |_, _| {
                    s_trust.dispatch(BluetoothCommand::SetTrusted(p_trust.clone(), !is_trusted));
                    Outcome::Stay
                },
            ));

            let p_remove = path.clone();
            let s_remove = s.clone();
            item_actions = item_actions.action(Action::new("Remove / Unpair", move |_, _| {
                s_remove.dispatch(BluetoothCommand::RemoveDevice(p_remove.clone()));
                Outcome::Stay
            }));

            paired_items.push(item.actions(item_actions));
        }

        if !paired_items.is_empty() {
            list = list.section(Section::new("Paired Devices").items(paired_items));
        }

        // 2. Discovered (Unpaired) Devices
        let available: Vec<_> = data.devices.iter().filter(|d| !d.paired).cloned().collect();
        let mut available_items = Vec::new();

        for (idx, dev) in available.into_iter().enumerate() {
            let icon = device_icon(&dev);
            let mut item = Item::new(format!("avail-{idx}"), dev.name.clone())
                .subtitle(dev.address.clone())
                .icon(icon);

            if let Some(rssi) = dev.rssi {
                item = item.accessory(Accessory::text(format!("{rssi} dBm")));
            }

            let p = dev.path.clone();
            let s = service.clone();
            let item_actions = ActionPanel::default().action(Action::new("Pair & Connect", {
                let p = p.clone();
                let s = s.clone();
                move |_, _| {
                    s.dispatch(BluetoothCommand::PairDevice(p.clone()));
                    Outcome::Stay
                }
            }));

            available_items.push(item.actions(item_actions));
        }

        if !available_items.is_empty() {
            list = list.section(
                Section::new(if discovering {
                    "Nearby Devices (Scanning…)"
                } else {
                    "Nearby Devices"
                })
                .items(available_items),
            );
        } else if discovering {
            list = list.section(
                Section::new("Nearby Devices")
                    .item(Item::new("scanning-hint", "Scanning for nearby devices…")
                        .subtitle("Make sure your accessories are in pairing mode")
                        .icon(IconName::Refresh)),
            );
        }

        list.into()
    }
}
