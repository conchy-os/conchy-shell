use gpui::{App, Context, prelude::*};
use services::{ActiveConnectionInfo, NetworkCommand};
use ui::{
    ActiveTheme, Color, IconName,
};

use crate::icons;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, EmptyView, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};
use crate::state::{AppState, watch};

pub struct NetworkPage;

impl NetworkPage {
    pub fn open(cx: &mut App) -> Page {
        Page::new(cx.new(|cx| Self::new(cx)))
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        let network = AppState::network(cx);
        watch(cx, network.subscribe(), |_, _, cx| {
            cx.notify();
        });
        Self
    }
}

impl LauncherPage for NetworkPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let service = AppState::network(cx);
        let data = service.get();

        let wifi_on = data.wifi_enabled;
        let scanning = data.scanning;

        let mut list = List::new()
            .show_detail(true)
            .click_selects(true)
            .title("Network & Wi-Fi")
            .placeholder("Search network connections…");

        // Global actions
        let actions = ActionPanel::default()
            .action(
                Action::new(
                    if wifi_on {
                        "Turn Wi-Fi Off"
                    } else {
                        "Turn Wi-Fi On"
                    },
                    {
                        let service = service.clone();
                        move |_, _| {
                            service.dispatch(NetworkCommand::ToggleWifi);
                            Outcome::Stay
                        }
                    },
                )
                .icon(if wifi_on {
                    IconName::WifiOff
                } else {
                    IconName::Wifi
                }),
            )
            .action(Action::new("Rescan Networks", {
                let service = service.clone();
                move |_, _| {
                    service.dispatch(NetworkCommand::RequestScan);
                    Outcome::Stay
                }
            }));

        list = list.actions(actions);

        // 1. Active Connections section
        let mut active_items = Vec::new();
        for (idx, conn) in data.active_connections.iter().enumerate() {
            match conn {
                ActiveConnectionInfo::WiFi {
                    id: _,
                    name,
                    strength,
                    interface,
                    hw_address: _,
                    ipv4_address,
                    ipv4_gateway: _,
                    dns_servers: _,
                    frequency: _,
                    channel: _,
                    bitrate: _,
                    security: _,
                    ..
                } => {
                    let mut item = Item::new(
                        format!("active-wifi-{idx}"),
                        format!("{name} (Connected Wi-Fi)"),
                    )
                    .subtitle(format!(
                        "Interface: {} • IP: {}",
                        interface,
                        ipv4_address.as_deref().unwrap_or("Assigning...")
                    ))
                    .icon(icons::wifi_signal_icon(*strength))
                    .accessory(Accessory::tag("Active").color(Color::Custom(
                        cx.theme().colors().status.success,
                    )));

                    if let Some(ip) = ipv4_address {
                        item = item.accessory(Accessory::text(ip.clone()));
                    }

                    let disconnect_name = name.clone();
                    let s = service.clone();
                    let item_actions = ActionPanel::default()
                        .action(Action::new("Disconnect", move |_, _| {
                            s.dispatch(NetworkCommand::Disconnect(disconnect_name.clone()));
                            Outcome::Stay
                        }))
                        .action(Action::new("Forget Network", {
                            let s = service.clone();
                            let forget_ssid = name.clone();
                            move |_, _| {
                                s.dispatch(NetworkCommand::Forget(forget_ssid.clone()));
                                Outcome::Stay
                            }
                        }));

                    active_items.push(item.actions(item_actions));
                }
                ActiveConnectionInfo::Wired {
                    name,
                    speed,
                    interface,
                    hw_address: _,
                    ipv4_address,
                    ipv4_gateway: _,
                    dns_servers: _,
                    ..
                } => {
                    let mut item = Item::new(
                        format!("active-wired-{idx}"),
                        format!("{name} (Ethernet)"),
                    )
                    .subtitle(format!(
                        "{} • {} Mbps • IP: {}",
                        interface,
                        speed,
                        ipv4_address.as_deref().unwrap_or("Connected")
                    ))
                    .icon(IconName::Ethernet)
                    .accessory(Accessory::tag("Active").color(Color::Custom(
                        cx.theme().colors().status.success,
                    )));

                    if let Some(ip) = ipv4_address {
                        item = item.accessory(Accessory::text(ip.clone()));
                    }

                    active_items.push(item);
                }
                ActiveConnectionInfo::Vpn { name, .. } => {
                    active_items.push(
                        Item::new(format!("active-vpn-{idx}"), format!("VPN: {name}"))
                            .subtitle("Secure encrypted tunnel")
                            .icon(IconName::Shield)
                            .accessory(Accessory::tag("VPN").color(Color::Custom(
                                cx.theme().colors().status.success,
                            ))),
                    );
                }
            }
        }

        if !active_items.is_empty() {
            list = list.section(Section::new("Active Connection").items(active_items));
        }

        // 2. Wi-Fi Access Points section
        if wifi_on {
            let mut wifi_items = Vec::new();
            for (idx, ap) in data.wireless_access_points.iter().enumerate() {
                // If it's already active, skip duplicate in scanned list
                let is_active = data
                    .active_connections
                    .iter()
                    .any(|c| matches!(c, ActiveConnectionInfo::WiFi { name, .. } if name == &ap.ssid));

                if is_active {
                    continue;
                }

                let sub = format!(
                    "{} • {} • Ch {}",
                    ap.band(),
                    if ap.public { "Open" } else { &ap.security },
                    ap.channel()
                );

                let mut item = Item::new(format!("ap-{idx}"), ap.ssid.clone())
                    .subtitle(sub)
                    .icon(icons::wifi_signal_icon(ap.strength))
                    .accessory(Accessory::text(format!("{}%", ap.strength)));

                if ap.known {
                    item = item.accessory(
                        Accessory::icon(IconName::Check)
                            .color(Color::Custom(cx.theme().colors().status.success)),
                    );
                }

                if !ap.public {
                    item = item.accessory(Accessory::icon(IconName::Lock).color(Color::Muted));
                }

                let ap_ssid = ap.ssid.clone();
                let is_known = ap.known;
                let is_public = ap.public;
                let s = service.clone();

                let mut item_actions = ActionPanel::default().action(Action::new(
                    if is_known || is_public {
                        "Connect"
                    } else {
                        "Connect with Password"
                    },
                    move |_, _cx| {
                        if is_known || is_public {
                            s.dispatch(NetworkCommand::Connect {
                                ssid: ap_ssid.clone(),
                                password: None,
                            });
                            Outcome::Stay
                        } else {
                            // Trigger connect password attempt
                            s.dispatch(NetworkCommand::Connect {
                                ssid: ap_ssid.clone(),
                                password: None,
                            });
                            Outcome::Stay
                        }
                    },
                ));

                if ap.known {
                    let forget_ssid = ap.ssid.clone();
                    let s_forget = service.clone();
                    item_actions =
                        item_actions.action(Action::new("Forget Network", move |_, _| {
                            s_forget.dispatch(NetworkCommand::Forget(forget_ssid.clone()));
                            Outcome::Stay
                        }));
                }

                wifi_items.push(item.actions(item_actions));
            }

            if !wifi_items.is_empty() {
                list = list.section(
                    Section::new(if scanning {
                        "Available Wi-Fi Networks (Scanning…)"
                    } else {
                        "Available Wi-Fi Networks"
                    })
                    .items(wifi_items),
                );
            } else if scanning {
                list = list.empty(
                    EmptyView::new("Scanning for Wi-Fi networks…")
                        .description("Searching for nearby wireless access points.")
                        .icon(IconName::Wifi),
                );
            } else {
                list = list.empty(
                    EmptyView::new("No Wi-Fi networks found")
                        .description("Press Mod+R to rescan for nearby wireless networks.")
                        .icon(IconName::WifiOff),
                );
            }
        } else {
            list = list.empty(
                EmptyView::new("Wi-Fi is turned off")
                    .description("Press Enter or toggle action to turn Wi-Fi on.")
                    .icon(IconName::WifiOff),
            );
        }

        list.into()
    }
}
