use gpui::{App, FontWeight, div, prelude::*, px};
use services::{BluetoothDevice, BluetoothDeviceType, BluetoothState};
use ui::patterns::PanelSurface;
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, TextSize,
    h_flex, v_flex,
};

use crate::launcher::content::Detail;
use crate::state::AppState;

pub fn bluetooth_detail(_cx: &App) -> Detail {
    Detail::new().widget(move |cx| {
        let theme = cx.theme();
        let colors = theme.colors();
        let bluetooth = AppState::bluetooth(cx).get();

        let is_active = bluetooth.state == BluetoothState::Active;

        let header = v_flex()
            .w_full()
            .gap(Spacing::Small.pixels())
            .child(
                h_flex()
                    .w_full()
                    .gap(Spacing::Small.pixels())
                    .items_center()
                    .child(Icon::new(IconName::Bluetooth).size(IconSize::Medium))
                    .child(
                        Label::new("Bluetooth")
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
                                Label::new(if is_active { "Active" } else { "Off" })
                                    .size(TextSize::XSmall)
                                    .color(if is_active {
                                        Color::Custom(colors.status.success)
                                    } else {
                                        Color::Muted
                                    }),
                            ),
                    ),
            )
            .child(
                Label::new("Bluetooth adapter, paired accessories and nearby discovery")
                    .size(TextSize::Small)
                    .color(Color::Muted),
            );

        let adapter_card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Small.pixels())
            .panel_card(cx)
            .child(
                h_flex()
                    .items_center()
                    .gap(Spacing::Small.pixels())
                    .child(
                        Icon::new(if is_active {
                            IconName::Bluetooth
                        } else {
                            IconName::BluetoothOff
                        })
                        .size(IconSize::Small)
                        .color(if is_active {
                            Color::Custom(colors.status.success)
                        } else {
                            Color::Muted
                        }),
                    )
                    .child(
                        Label::new(if bluetooth.adapter_name.is_empty() {
                            "Bluetooth Adapter".to_string()
                        } else {
                            bluetooth.adapter_name.clone()
                        })
                        .size(TextSize::Medium)
                        .weight(FontWeight::BOLD),
                    ),
            )
            .child(div().h(px(1.)).bg(colors.border_variant).w_full())
            .child(prop_row(
                "Power State",
                if is_active {
                    "Powered On"
                } else {
                    "Powered Off / Blocked"
                },
                cx,
            ))
            .child(prop_row(
                "Controller Address",
                if bluetooth.adapter_address.is_empty() {
                    "Unknown"
                } else {
                    &bluetooth.adapter_address
                },
                cx,
            ))
            .child(prop_row(
                "Discoverable",
                if bluetooth.discoverable { "Yes" } else { "No" },
                cx,
            ))
            .child(prop_row(
                "Scan Status",
                if bluetooth.discovering {
                    "Scanning for nearby devices…"
                } else {
                    "Idle"
                },
                cx,
            ));

        let paired_devices: Vec<_> = bluetooth.devices.iter().filter(|d| d.paired).collect();
        let connected_count = paired_devices.iter().filter(|d| d.connected).count();

        let devices_card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Small.pixels())
            .panel_card(cx)
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        Label::new(format!(
                            "Paired Devices ({} connected)",
                            connected_count
                        ))
                        .size(TextSize::Small)
                        .weight(FontWeight::BOLD),
                    )
                    .child(
                        Label::new(format!("{} total", paired_devices.len()))
                            .size(TextSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
            .child(div().h(px(1.)).bg(colors.border_variant).w_full())
            .when(paired_devices.is_empty(), |el| {
                el.child(
                    Label::new("No paired devices found. Press Enter to scan and pair.")
                        .size(TextSize::Small)
                        .color(Color::Muted),
                )
            })
            .when(!paired_devices.is_empty(), |el| {
                el.children(paired_devices.into_iter().map(|dev| {
                    let icon = device_icon(dev);
                    let battery_str = dev.battery.map(|b| format!(" • 🔋 {b}%")).unwrap_or_default();
                    let status_str = if dev.connected {
                        "Connected"
                    } else {
                        "Disconnected"
                    };

                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .py(px(2.))
                        .child(
                            h_flex()
                                .gap(Spacing::Small.pixels())
                                .items_center()
                                .child(Icon::new(icon).size(IconSize::Small).color(
                                    if dev.connected {
                                        Color::Custom(colors.status.success)
                                    } else {
                                        Color::Muted
                                    },
                                ))
                                .child(
                                    Label::new(dev.name.clone())
                                        .size(TextSize::Small)
                                        .weight(if dev.connected {
                                            FontWeight::BOLD
                                        } else {
                                            FontWeight::NORMAL
                                        }),
                                )
                                .child(
                                    Label::new(battery_str)
                                        .size(TextSize::XSmall)
                                        .color(Color::Muted),
                                ),
                        )
                        .child(
                            Label::new(status_str.to_string())
                                .size(TextSize::XSmall)
                                .color(if dev.connected {
                                    Color::Custom(colors.status.success)
                                } else {
                                    Color::Muted
                                }),
                        )
                }))
            });

        v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Large.pixels())
            .child(header)
            .child(adapter_card)
            .child(devices_card)
            .into_any_element()
    })
}

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
