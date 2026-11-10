//! MPRIS panel with a list of players and transport controls.

use std::time::Duration;

use crate::windows::ShellPanel;
use gpui::{AnyElement, App, Context, FontWeight, ObjectFit, Window, div, img, prelude::*, px};
use services::{
    MprisCommand, MprisData, MprisPlayerData, MprisSubscriber, PlaybackStatus, PlayerCommand,
};
use ui::patterns::PanelSurface;
use ui::{
    ActiveTheme, ButtonCommon, ButtonSize, ButtonStyle, Clickable, Color, Icon, IconButton,
    IconName, IconSize, Label, LabelCommon, Spacing, TextSize, h_flex, image_source,
    track_progress, v_flex,
};

use crate::config::ActiveConfig;
use crate::state::watch;

/// Width of the album-art column; it stretches to the card's height.
const COVER_WIDTH: f32 = 88.0;

/// Panel content for controlling media players exposed via MPRIS.
pub struct MprisPanel {
    subscriber: MprisSubscriber,
    data: MprisData,
}

impl MprisPanel {
    pub fn new(subscriber: MprisSubscriber, cx: &mut Context<Self>) -> Self {
        let data = subscriber.get();

        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });
        tick_while_playing(cx, |this| &this.data);

        Self { subscriber, data }
    }

    fn status_text(state: PlaybackStatus) -> &'static str {
        match state {
            PlaybackStatus::Playing => "Playing",
            PlaybackStatus::Paused => "Paused",
            PlaybackStatus::Stopped => "Stopped",
        }
    }

    fn title_for(player: &MprisPlayerData) -> String {
        player
            .metadata
            .as_ref()
            .and_then(|m| m.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| player.name())
    }

    fn artists_for(player: &MprisPlayerData) -> Option<String> {
        player
            .metadata
            .as_ref()
            .and_then(|m| m.artists.clone())
            .map(|a| a.join(", "))
            .filter(|a| !a.is_empty())
    }

    fn sorted_players(&self) -> Vec<MprisPlayerData> {
        let mut players = self.data.players.clone();
        players.sort_by_key(|p| match p.state {
            PlaybackStatus::Playing => 0u8,
            PlaybackStatus::Paused => 1u8,
            PlaybackStatus::Stopped => 2u8,
        });
        players
    }

    fn control_button(
        &self,
        id: String,
        icon: IconName,
        service_name: &str,
        command: PlayerCommand,
    ) -> IconButton {
        let subscriber = self.subscriber.clone();
        let service_name = service_name.to_string();
        IconButton::new(id, icon)
            .style(ButtonStyle::Subtle)
            .size(ButtonSize::Large)
            .icon_size(IconSize::XSmall)
            .on_click(move |_, _, _| {
                subscriber.dispatch(MprisCommand {
                    service_name: service_name.clone(),
                    command,
                })
            })
    }

    /// The album art, or a note when there is none or covers are off.
    fn render_cover(player: &MprisPlayerData, show_cover: bool, cx: &App) -> AnyElement {
        let slot = div()
            .flex_none()
            .w(px(COVER_WIDTH))
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .bg(cx.theme().colors.elevated_surface_background);
        match player.art_url.as_deref().filter(|_| show_cover) {
            Some(art) => slot.child(
                img(image_source(art))
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            ),
            None => slot.child(
                Icon::new(IconName::Music)
                    .size(IconSize::Large)
                    .color(Color::Muted),
            ),
        }
        .into_any_element()
    }

    fn render_player_card(&self, player: MprisPlayerData, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let show_cover = cx.config().bar.modules.mpris.show_cover;
        let service = player.service.clone();
        let name = player.name();
        let play_icon = match player.state {
            PlaybackStatus::Playing => IconName::Pause,
            _ => IconName::Play,
        };
        let status_color = match player.state {
            PlaybackStatus::Playing => theme.colors.status.success,
            PlaybackStatus::Paused => theme.colors.status.warning,
            PlaybackStatus::Stopped => theme.colors.text_muted,
        };
        let volume_value = player.volume.unwrap_or(0.0);
        let footer = match player.volume {
            Some(volume) => format!("{name} · {:.0}%", volume.clamp(0.0, 100.0)),
            None => name,
        };
        // Every card keeps the same three rows, so a stopped player shows an
        // empty line rather than a shorter card.
        let (position, duration) = match (player.position_us(), player.duration_us) {
            (Some(position), Some(duration)) if duration > 0 => (position, duration),
            _ => (0, 0),
        };

        let heading = h_flex()
            .w_full()
            .justify_between()
            .items_start()
            .gap(Spacing::Medium.pixels())
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .child(
                        Label::new(Self::title_for(&player))
                            .size(TextSize::Small)
                            .weight(FontWeight::MEDIUM)
                            .truncate(),
                    )
                    .child(
                        Label::new(Self::artists_for(&player).unwrap_or_else(|| "No track".into()))
                            .size(TextSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                    ),
            )
            .child(
                div().flex_none().whitespace_nowrap().child(
                    Label::new(Self::status_text(player.state))
                        .size(TextSize::XSmall)
                        .color(Color::Custom(status_color)),
                ),
            );

        let controls = h_flex()
            .flex_none()
            .gap(Spacing::XSmall.pixels())
            .when(player.can_control, |el| {
                el.child(self.control_button(
                    format!("mpris-prev-{service}"),
                    IconName::SkipBack,
                    &service,
                    PlayerCommand::Prev,
                ))
                .child(self.control_button(
                    format!("mpris-play-{service}"),
                    play_icon,
                    &service,
                    PlayerCommand::PlayPause,
                ))
                .child(self.control_button(
                    format!("mpris-next-{service}"),
                    IconName::SkipForward,
                    &service,
                    PlayerCommand::Next,
                ))
            })
            .when(player.can_control && player.volume.is_some(), |el| {
                el.child(self.control_button(
                    format!("mpris-dec-{service}"),
                    IconName::Dash,
                    &service,
                    PlayerCommand::Volume(volume_value - 5.0),
                ))
                .child(self.control_button(
                    format!("mpris-inc-{service}"),
                    IconName::Plus,
                    &service,
                    PlayerCommand::Volume(volume_value + 5.0),
                ))
            });

        h_flex()
            .w_full()
            .panel_card(cx)
            .overflow_hidden()
            .child(Self::render_cover(&player, show_cover, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .px(Spacing::Medium.pixels())
                    .py(Spacing::Small.pixels())
                    .gap(Spacing::XSmall.pixels())
                    .child(heading)
                    .child(track_progress(position, duration, cx))
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .items_center()
                            .gap(Spacing::XSmall.pixels())
                            .child(
                                Label::new(footer)
                                    .size(TextSize::XSmall)
                                    .color(Color::Muted)
                                    .truncate(),
                            )
                            .child(controls),
                    ),
            )
            .into_any_element()
    }
}

/// Redraw once a second while a player with a known position is playing,
/// so the progress line moves between service updates.
pub(crate) fn tick_while_playing<V: 'static>(
    cx: &mut Context<V>,
    data: impl Fn(&V) -> &MprisData + 'static,
) {
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let alive = this.update(cx, |this, cx| {
                let playing = data(this)
                    .players
                    .iter()
                    .any(|p| p.state == PlaybackStatus::Playing && p.position.is_some());
                if playing {
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
        }
    })
    .detach();
}

impl Render for MprisPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let players = self.sorted_players();
        let is_empty = players.is_empty();

        div()
            .id("mpris-panel")
            .w_full()
            .h_full()
            .p(Spacing::XLarge.pixels())
            .shell_panel(cx)
            .overflow_hidden()
            .child(
                v_flex()
                    .w_full()
                    .h_full()
                    .gap(Spacing::Large.pixels())
                    .child(
                        h_flex()
                            .gap(Spacing::Medium.pixels())
                            .child(
                                Icon::new(IconName::Volume)
                                    .size(IconSize::Large)
                                    .color(Color::Default),
                            )
                            .child(
                                Label::new("Media Players")
                                    .size(TextSize::Large)
                                    .weight(FontWeight::BOLD),
                            ),
                    )
                    .when(is_empty, |el| {
                        el.child(
                            h_flex()
                                .py(Spacing::XLarge.pixels())
                                .justify_center()
                                .child(
                                    Label::new("No MPRIS players detected")
                                        .size(TextSize::Small)
                                        .color(Color::Muted),
                                ),
                        )
                    })
                    .when(!is_empty, |el| {
                        el.child(
                            v_flex()
                                .flex_grow_1()
                                .overflow_hidden()
                                .gap(Spacing::Medium.pixels())
                                .children(
                                    players
                                        .into_iter()
                                        .map(|player| self.render_player_card(player, cx)),
                                ),
                        )
                    }),
            )
    }
}
