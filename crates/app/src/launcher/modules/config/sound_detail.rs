use gpui::{App, FontWeight, div, prelude::*, px};
use ui::patterns::PanelSurface;
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, TextSize,
    h_flex, v_flex,
};

use crate::launcher::content::Detail;
use crate::state::AppState;

pub fn sound_detail(_cx: &App) -> Detail {
    Detail::new().widget(move |cx| {
        let theme = cx.theme();
        let colors = theme.colors();
        let audio = AppState::audio(cx).get();

        let header = v_flex()
            .w_full()
            .gap(Spacing::Small.pixels())
            .child(
                h_flex()
                    .w_full()
                    .gap(Spacing::Small.pixels())
                    .items_center()
                    .child(Icon::new(IconName::Volume).size(IconSize::Medium))
                    .child(
                        Label::new("Sound & Audio")
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
                                Label::new(if audio.sink_muted {
                                    "Muted"
                                } else {
                                    "Active"
                                })
                                .size(TextSize::XSmall)
                                .color(if audio.sink_muted {
                                    Color::Muted
                                } else {
                                    Color::Custom(colors.status.success)
                                }),
                            ),
                    ),
            )
            .child(
                Label::new("Playback output, microphone input and volume controls")
                    .size(TextSize::Small)
                    .color(Color::Muted),
            );

        let sink_card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Small.pixels())
            .panel_card(cx)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        h_flex()
                            .gap(Spacing::Small.pixels())
                            .items_center()
                            .child(
                                Icon::new(if audio.sink_muted {
                                    IconName::VolumeOff
                                } else {
                                    IconName::Volume
                                })
                                .size(IconSize::Small),
                            )
                            .child(
                                Label::new("Output Volume (Speakers / Headphones)")
                                    .size(TextSize::Small)
                                    .weight(FontWeight::BOLD),
                            ),
                    )
                    .child(
                        Label::new(format!("{}%", audio.sink_volume))
                            .size(TextSize::Small)
                            .weight(FontWeight::BOLD)
                            .color(if audio.sink_muted {
                                Color::Muted
                            } else {
                                Color::Custom(colors.text)
                            }),
                    ),
            )
            .child(div().h(px(1.)).bg(colors.border_variant).w_full())
            .child(prop_row(
                "Mute State",
                if audio.sink_muted { "Muted" } else { "Unmuted" },
                cx,
            ))
            .child(prop_row(
                "Audio Server",
                "PipeWire / PulseAudio via libpulse",
                cx,
            ));

        let source_card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Small.pixels())
            .panel_card(cx)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        h_flex()
                            .gap(Spacing::Small.pixels())
                            .items_center()
                            .child(
                                Icon::new(if audio.source_muted {
                                    IconName::MicOff
                                } else {
                                    IconName::Mic
                                })
                                .size(IconSize::Small),
                            )
                            .child(
                                Label::new("Input Volume (Microphone)")
                                    .size(TextSize::Small)
                                    .weight(FontWeight::BOLD),
                            ),
                    )
                    .child(
                        Label::new(format!("{}%", audio.source_volume))
                            .size(TextSize::Small)
                            .weight(FontWeight::BOLD)
                            .color(if audio.source_muted {
                                Color::Muted
                            } else {
                                Color::Custom(colors.text)
                            }),
                    ),
            )
            .child(div().h(px(1.)).bg(colors.border_variant).w_full())
            .child(prop_row(
                "Microphone Status",
                if audio.source_muted {
                    "Muted (Privacy Off)"
                } else {
                    "Live / Ready"
                },
                cx,
            ));

        v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Large.pixels())
            .child(header)
            .child(sink_card)
            .child(source_card)
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
