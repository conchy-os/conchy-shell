//! Network & Wi-Fi: the form beside the "Network & Wi-Fi" row. The Wi-Fi
//! switch, the connection in use and the networks around, each one a row the
//! keyboard can act on.

use gpui::{App, SharedString};
use services::{ActiveConnectionInfo, NetworkCommand};
use ui::{Color, IconName};

use super::settings_form::{self as rows, Entry};
use crate::icons;
use crate::launcher::action::{Action, Outcome};
use crate::launcher::content::{Detail, Form, Item};
use crate::state::AppState;

pub fn item() -> Item {
    Item::new("network", "Network & Wi-Fi")
        .subtitle("Wi-Fi, Ethernet, and IP configuration")
        .icon(IconName::Wifi)
        .keywords("wifi network internet ethernet ip dns address wlan ssid")
        .actions(super::system::edit())
        .detail(Detail::new().form(network_form))
}

fn network_form(form: &mut Form, cx: &App) {
    let service = AppState::network(cx);
    let data = service.get();
    let wifi_on = data.wifi_enabled;

    rows::title(
        form,
        "Network & Wi-Fi",
        "Wireless and wired connections, their addresses and the networks around",
    );

    rows::heading(form, "Wi-Fi");
    rows::toggle(
        form,
        "net-wifi",
        "Wi-Fi",
        "Turn the wireless adapter on or off",
        wifi_on,
        {
            let service = service.clone();
            move |_, _| service.dispatch(NetworkCommand::ToggleWifi)
        },
        cx,
    );
    if wifi_on {
        rows::action(
            form,
            if data.scanning {
                "Looking for networks…"
            } else {
                "Look for networks"
            },
            "Ask the adapter to scan again",
            {
                let service = service.clone();
                move |_, _| {
                    service.dispatch(NetworkCommand::RequestScan);
                    Outcome::Stay
                }
            },
            cx,
        );
    }

    rows::heading(form, "Connection");
    if data.active_connections.is_empty() {
        rows::note(
            form,
            "Not connected. Join a Wi-Fi network or plug in a cable.",
            Color::Muted,
        );
    }
    for conn in &data.active_connections {
        match conn {
            ActiveConnectionInfo::WiFi {
                name,
                strength,
                interface,
                ipv4_address,
                ..
            } => {
                let (disconnect, forget) = (service.clone(), service.clone());
                let (disconnect_name, forget_name) = (name.clone(), name.clone());
                rows::entry(
                    form,
                    Entry {
                        icon: icons::wifi_signal_icon(*strength),
                        title: name.clone().into(),
                        subtitle: format!(
                            "{interface} • {}",
                            ipv4_address.as_deref().unwrap_or("getting an address…")
                        )
                        .into(),
                        tag: Some((format!("{strength}%").into(), Color::Success)),
                    },
                    vec![
                        Action::new("Disconnect", move |_, _| {
                            disconnect
                                .dispatch(NetworkCommand::Disconnect(disconnect_name.clone()));
                            Outcome::Stay
                        })
                        .icon(IconName::WifiOff),
                        Action::new("Forget", move |_, _| {
                            forget.dispatch(NetworkCommand::Forget(forget_name.clone()));
                            Outcome::Stay
                        })
                        .icon(IconName::Trash)
                        .destructive(),
                    ],
                    cx,
                );
            }
            ActiveConnectionInfo::Wired {
                name,
                speed,
                interface,
                ipv4_address,
                ..
            } => {
                rows::info(form, "Ethernet", name.clone());
                rows::info(form, "Link", format!("{interface} • {speed} Mbps"));
                rows::info(
                    form,
                    "IP address",
                    ipv4_address.clone().unwrap_or_else(|| "None".into()),
                );
            }
            ActiveConnectionInfo::Vpn { name, .. } => rows::info(form, "VPN", name.clone()),
        }
    }
    if let Some(details) = data.active_connections.first() {
        details_rows(form, details);
    }

    if wifi_on {
        let around: Vec<_> = data
            .wireless_access_points
            .iter()
            .filter(|ap| {
                !data.active_connections.iter().any(
                    |c| matches!(c, ActiveConnectionInfo::WiFi { name, .. } if name == &ap.ssid),
                )
            })
            .collect();
        rows::heading(
            form,
            if data.scanning {
                "Networks around (scanning…)"
            } else {
                "Networks around"
            },
        );
        if around.is_empty() {
            rows::note(
                form,
                if data.scanning {
                    "Searching for nearby access points…"
                } else {
                    "No networks found. Use “Look for networks” to scan again."
                },
                Color::Muted,
            );
        }
        for ap in around {
            let mut actions = vec![{
                let (service, ssid) = (service.clone(), ap.ssid.clone());
                Action::new("Connect", move |_, _| {
                    service.dispatch(NetworkCommand::Connect {
                        ssid: ssid.clone(),
                        password: None,
                    });
                    Outcome::Stay
                })
                .icon(IconName::Wifi)
            }];
            if ap.known {
                let (service, ssid) = (service.clone(), ap.ssid.clone());
                actions.push(
                    Action::new("Forget", move |_, _| {
                        service.dispatch(NetworkCommand::Forget(ssid.clone()));
                        Outcome::Stay
                    })
                    .icon(IconName::Trash)
                    .destructive(),
                );
            }
            let tag: SharedString = if ap.known {
                format!("saved • {}%", ap.strength).into()
            } else {
                format!("{}%", ap.strength).into()
            };
            rows::entry(
                form,
                Entry {
                    icon: icons::wifi_signal_icon(ap.strength),
                    title: ap.ssid.clone().into(),
                    subtitle: format!(
                        "{} • {} • channel {}",
                        ap.band(),
                        if ap.public {
                            "open"
                        } else {
                            ap.security.as_str()
                        },
                        ap.channel()
                    )
                    .into(),
                    tag: Some((tag, Color::Muted)),
                },
                actions,
                cx,
            );
        }
    }
}

/// Addresses and radio details of the connection in use.
fn details_rows(form: &mut Form, conn: &ActiveConnectionInfo) {
    let list = |servers: &[String]| {
        if servers.is_empty() {
            "None".to_string()
        } else {
            servers.join(", ")
        }
    };
    match conn {
        ActiveConnectionInfo::WiFi {
            hw_address,
            ipv4_address,
            ipv4_gateway,
            dns_servers,
            frequency,
            channel,
            bitrate,
            security,
            ..
        } => {
            rows::heading(form, "Details");
            rows::info(form, "Security", security.as_deref().unwrap_or("WPA2"));
            if let Some(f) = frequency {
                let band = if *f >= 5925 {
                    "6 GHz"
                } else if *f >= 4900 {
                    "5 GHz"
                } else {
                    "2.4 GHz"
                };
                rows::info(
                    form,
                    "Frequency",
                    match channel {
                        Some(ch) => format!("{f} MHz ({band}, channel {ch})"),
                        None => format!("{f} MHz ({band})"),
                    },
                );
            }
            if let Some(b) = bitrate {
                rows::info(
                    form,
                    "Link speed",
                    format!("{:.1} Mbps", f64::from(*b) / 1000.0),
                );
            }
            rows::info(
                form,
                "IP address",
                ipv4_address.clone().unwrap_or_else(|| "None".into()),
            );
            rows::info(
                form,
                "Gateway",
                ipv4_gateway.clone().unwrap_or_else(|| "None".into()),
            );
            rows::info(form, "DNS", list(dns_servers));
            rows::info(
                form,
                "MAC address",
                hw_address.clone().unwrap_or_else(|| "Unknown".into()),
            );
        }
        ActiveConnectionInfo::Wired {
            hw_address,
            ipv4_gateway,
            dns_servers,
            ..
        } => {
            rows::heading(form, "Details");
            rows::info(
                form,
                "Gateway",
                ipv4_gateway.clone().unwrap_or_else(|| "None".into()),
            );
            rows::info(form, "DNS", list(dns_servers));
            rows::info(
                form,
                "MAC address",
                hw_address.clone().unwrap_or_else(|| "Unknown".into()),
            );
        }
        ActiveConnectionInfo::Vpn { .. } => {}
    }
}
