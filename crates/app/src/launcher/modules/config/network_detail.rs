use gpui::{App, FontWeight, div, prelude::*, px};
use services::ActiveConnectionInfo;
use ui::patterns::PanelSurface;
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, TextSize,
    h_flex, v_flex,
};

use crate::launcher::content::Detail;
use crate::state::AppState;

pub fn network_detail(_cx: &App) -> Detail {
    Detail::new().widget(move |cx| {
        let theme = cx.theme();
        let colors = theme.colors();
        let network = AppState::network(cx).get();
        let active = network.active_connections.first();

        let header = v_flex()
            .w_full()
            .gap(Spacing::Small.pixels())
            .child(
                h_flex()
                    .w_full()
                    .gap(Spacing::Small.pixels())
                    .items_center()
                    .child(Icon::new(IconName::Wifi).size(IconSize::Medium))
                    .child(
                        Label::new("Network & Wi-Fi")
                            .size(TextSize::Medium)
                            .weight(FontWeight::BOLD),
                    )
                    .child(
                        div()
                            .px(Spacing::Small.pixels())
                            .py(px(2.))
                            .rounded(Radius::Small.pixels())
                            .bg(colors.element_background)
                            .child(
                                Label::new(if network.wifi_enabled {
                                    "Wi-Fi On"
                                } else {
                                    "Wi-Fi Off"
                                })
                                .size(TextSize::XSmall)
                                .color(if network.wifi_enabled {
                                    Color::Custom(colors.status.success)
                                } else {
                                    Color::Muted
                                }),
                            ),
                    ),
            )
            .child(
                Label::new("Manage wireless and wired network connections, IP and DNS")
                    .size(TextSize::Small)
                    .color(Color::Muted),
            );

        let active_card = match active {
            Some(ActiveConnectionInfo::WiFi {
                name,
                strength,
                interface,
                hw_address,
                ipv4_address,
                ipv4_gateway,
                dns_servers,
                frequency,
                channel,
                bitrate,
                security,
                ..
            }) => {
                let freq_str = frequency
                    .map(|f| {
                        let band = if f >= 5925 {
                            "6 GHz"
                        } else if f >= 4900 {
                            "5 GHz"
                        } else {
                            "2.4 GHz"
                        };
                        if let Some(ch) = channel {
                            format!("{f} MHz ({band}, Ch {ch})")
                        } else {
                            format!("{f} MHz ({band})")
                        }
                    })
                    .unwrap_or_else(|| "Unknown".to_string());

                let speed_str = bitrate
                    .map(|b| format!("{:.1} Mbps", b as f64 / 1000.0))
                    .unwrap_or_else(|| "Unknown".to_string());

                v_flex()
                    .w_full()
                    .p(Spacing::Large.pixels())
                    .gap(Spacing::Small.pixels())
                    .panel_card(cx)
                    .child(
                        h_flex()
                            .items_center()
                            .gap(Spacing::Small.pixels())
                            .child(
                                Icon::new(IconName::Wifi)
                                    .size(IconSize::Small)
                                    .color(Color::Custom(colors.status.success)),
                            )
                            .child(
                                Label::new(format!("Connected: {name}"))
                                    .size(TextSize::Medium)
                                    .weight(FontWeight::BOLD),
                            )
                            .child(
                                div()
                                    .px(Spacing::Small.pixels())
                                    .py(px(2.))
                                    .rounded(Radius::Small.pixels())
                                    .bg(colors.element_background)
                                    .child(
                                        Label::new(format!("{strength}% signal"))
                                            .size(TextSize::XSmall)
                                            .color(Color::Custom(colors.status.success)),
                                    ),
                            ),
                    )
                    .child(div().h(px(1.)).bg(colors.border_variant).w_full())
                    .child(prop_row("Security", security.as_deref().unwrap_or("WPA2"), cx))
                    .child(prop_row("Frequency", &freq_str, cx))
                    .child(prop_row("Link Speed", &speed_str, cx))
                    .child(prop_row("Interface", interface, cx))
                    .child(prop_row(
                        "IP Address",
                        ipv4_address.as_deref().unwrap_or("None"),
                        cx,
                    ))
                    .child(prop_row(
                        "Gateway",
                        ipv4_gateway.as_deref().unwrap_or("None"),
                        cx,
                    ))
                    .child(prop_row(
                        "DNS",
                        &if dns_servers.is_empty() {
                            "None".to_string()
                        } else {
                            dns_servers.join(", ")
                        },
                        cx,
                    ))
                    .child(prop_row(
                        "MAC Address",
                        hw_address.as_deref().unwrap_or("Unknown"),
                        cx,
                    ))
            }
            Some(ActiveConnectionInfo::Wired {
                name,
                speed,
                interface,
                hw_address,
                ipv4_address,
                ipv4_gateway,
                dns_servers,
                ..
            }) => v_flex()
                .w_full()
                .p(Spacing::Large.pixels())
                .gap(Spacing::Small.pixels())
                .panel_card(cx)
                .child(
                    h_flex()
                        .items_center()
                        .gap(Spacing::Small.pixels())
                        .child(
                            Icon::new(IconName::Ethernet)
                                .size(IconSize::Small)
                                .color(Color::Custom(colors.status.success)),
                        )
                        .child(
                            Label::new(format!("Connected: {name}"))
                                .size(TextSize::Medium)
                                .weight(FontWeight::BOLD),
                        ),
                )
                .child(div().h(px(1.)).bg(colors.border_variant).w_full())
                .child(prop_row("Type", "Ethernet (Wired)", cx))
                .child(prop_row("Speed", &format!("{speed} Mbps"), cx))
                .child(prop_row("Interface", interface, cx))
                .child(prop_row(
                    "IP Address",
                    ipv4_address.as_deref().unwrap_or("None"),
                    cx,
                ))
                .child(prop_row(
                    "Gateway",
                    ipv4_gateway.as_deref().unwrap_or("None"),
                    cx,
                ))
                .child(prop_row(
                    "DNS",
                    &if dns_servers.is_empty() {
                        "None".to_string()
                    } else {
                        dns_servers.join(", ")
                    },
                    cx,
                ))
                .child(prop_row(
                    "MAC Address",
                    hw_address.as_deref().unwrap_or("Unknown"),
                    cx,
                )),
            Some(ActiveConnectionInfo::Vpn { name, .. }) => v_flex()
                .w_full()
                .p(Spacing::Large.pixels())
                .gap(Spacing::Small.pixels())
                .panel_card(cx)
                .child(
                    Label::new(format!("Active VPN: {name}"))
                        .size(TextSize::Medium)
                        .weight(FontWeight::BOLD),
                ),
            None => v_flex()
                .w_full()
                .p(Spacing::Large.pixels())
                .gap(Spacing::Small.pixels())
                .panel_card(cx)
                .child(
                    h_flex()
                        .gap(Spacing::Small.pixels())
                        .items_center()
                        .child(
                            Icon::new(IconName::WifiOff)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                        .child(
                            Label::new("No Active Network Connection")
                                .size(TextSize::Medium)
                                .color(Color::Muted),
                        ),
                )
                .child(
                    Label::new("Connect to a Wi-Fi network or plug in an Ethernet cable.")
                        .size(TextSize::Small)
                        .color(Color::Muted),
                ),
        };

        let ap_count = network.wireless_access_points.len();
        let summary_card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Small.pixels())
            .panel_card(cx)
            .child(
                h_flex()
                    .justify_between()
                    .child(Label::new("Available Wi-Fi Networks").size(TextSize::Small))
                    .child(
                        Label::new(format!("{ap_count} nearby"))
                            .size(TextSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(
                Label::new("Press Enter to inspect details, connect or switch networks.")
                    .size(TextSize::XSmall)
                    .color(Color::Muted),
            );

        v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Large.pixels())
            .child(header)
            .child(active_card)
            .child(summary_card)
            .into_any_element()
    })
}

fn prop_row(label: &str, value: &str, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    h_flex()
        .w_full()
        .justify_between()
        .items_center()
        .child(
            Label::new(label.to_string())
                .size(TextSize::Small)
                .color(Color::Muted),
        )
        .child(
            Label::new(value.to_string())
                .size(TextSize::Small)
                .weight(FontWeight::MEDIUM)
                .color(Color::Custom(colors.text)),
        )
}
