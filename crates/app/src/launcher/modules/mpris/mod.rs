//! Media module - every MPRIS player with its track, cover and controls.
//!
//! `;m` shows a now-playing card for the active player above the list of
//! players; Enter toggles playback, the action panel and its shortcuts skip
//! tracks and nudge the volume.

pub mod config;

use gpui::{AnyElement, Context, FontWeight, ObjectFit, Window, div, img, prelude::*, px};
use services::{
    MprisCommand, MprisData, MprisPlayerData, MprisSubscriber, PlaybackStatus, PlayerCommand,
};
use ui::{
    ActiveTheme, ButtonCommon, ButtonSize, ButtonStyle, Clickable, Color, Icon, IconButton,
    IconName, IconSize, Label, LabelCommon, Radius, Spacing, TextSize, format_track_time, h_flex,
    image_source, track_progress, v_flex,
};

use self::config::MprisConfig;
use crate::bar::modules::mpris::tick_while_playing;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{
    Accessory, Content, Detail, EmptyView, Item, List, Metadata, Section,
};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};
use crate::state::{AppState, watch};

const MPRIS_ICON: IconName = IconName::Music;
/// Volume change per keypress, in percent.
const VOLUME_STEP: f64 = 5.0;
/// Side of the cover in the now-playing card.
const CARD_COVER_SIZE: f32 = 72.0;

pub fn descriptor(config: &MprisConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "mpris",
            prefix: config.prefix.clone().into(),
            name: "Media",
            icon: MPRIS_ICON,
            description: "Play, pause and skip in any media player",
            default: false,
            in_help: true,
        },
        MprisModule::new,
    )
}

pub struct MprisModule {
    subscriber: MprisSubscriber,
    data: MprisData,
}

impl MprisModule {
    fn new(cx: &mut Context<Self>) -> Self {
        let subscriber = AppState::mpris(cx).clone();
        let data = subscriber.get();
        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });
        tick_while_playing(cx, |this| &this.data);
        Self { subscriber, data }
    }

    /// The player the card follows: whichever is playing, else the first.
    fn primary(&self) -> Option<&MprisPlayerData> {
        self.data
            .players
            .iter()
            .find(|player| player.state == PlaybackStatus::Playing)
            .or_else(|| self.data.players.first())
    }

    fn dispatch(&self, player: &MprisPlayerData, command: PlayerCommand) -> impl Fn() + 'static {
        let subscriber = self.subscriber.clone();
        let service_name = player.service.clone();
        move || {
            subscriber.dispatch(MprisCommand {
                service_name: service_name.clone(),
                command,
            })
        }
    }

    /// An action that sends `command` to `player` and keeps the launcher
    /// open; the listener redraws once the player reports back.
    fn control(
        &self,
        player: &MprisPlayerData,
        command: PlayerCommand,
        title: &'static str,
        icon: IconName,
    ) -> Action {
        let send = self.dispatch(player, command);
        Action::new(title, move |_, _| {
            send();
            Outcome::Stay
        })
        .icon(icon)
    }

    fn actions(&self, player: &MprisPlayerData) -> ActionPanel {
        let mut actions = ActionPanel::new();
        if player.can_control {
            let (title, icon) = play_pause(player.state);
            actions = actions
                .action(self.control(player, PlayerCommand::PlayPause, title, icon))
                .section(
                    "Playback",
                    [
                        self.control(
                            player,
                            PlayerCommand::Next,
                            "Next Track",
                            IconName::SkipForward,
                        )
                        .shortcut("alt-right"),
                        self.control(
                            player,
                            PlayerCommand::Prev,
                            "Previous Track",
                            IconName::SkipBack,
                        )
                        .shortcut("alt-left"),
                    ],
                );
            if let Some(volume) = player.volume {
                let up = (volume + VOLUME_STEP).min(100.0);
                let down = (volume - VOLUME_STEP).max(0.0);
                actions = actions.section(
                    "Volume",
                    [
                        self.control(
                            player,
                            PlayerCommand::Volume(up),
                            "Volume Up",
                            IconName::Volume,
                        )
                        .shortcut("alt-up"),
                        self.control(
                            player,
                            PlayerCommand::Volume(down),
                            "Volume Down",
                            IconName::VolumeLow,
                        )
                        .shortcut("alt-down"),
                    ],
                );
            }
        }
        if let Some(track) = track_label(player) {
            actions = actions.action(
                Action::copy(track)
                    .title("Copy Track")
                    .shortcut("ctrl-shift-c"),
            );
        }
        actions
    }

    fn item(&self, player: &MprisPlayerData) -> Item {
        let name = player.name();
        let (title, artists) = title_and_artists(player);
        let subtitle = match &artists {
            Some(artists) => format!("{artists} · {name}"),
            None if title.is_some() => name.clone(),
            None => format!("{name} · No track"),
        };
        let (status, icon, color) = status_display(player.state);

        Item::new(
            player.service.clone(),
            title.unwrap_or_else(|| name.clone()),
        )
        .subtitle(subtitle)
        .icon(player_icon(&name))
        .keywords(format!("{name} {status} {}", artists.unwrap_or_default()))
        .accessory(Accessory::tag(status).with_icon(icon).color(color))
        .detail(detail(player, &name))
        .actions(self.actions(player))
    }

    fn transport_button(
        &self,
        id: &'static str,
        icon: IconName,
        player: &MprisPlayerData,
        command: PlayerCommand,
    ) -> IconButton {
        let send = self.dispatch(player, command);
        IconButton::new(id, icon)
            .style(ButtonStyle::Subtle)
            .size(ButtonSize::Large)
            .icon_size(IconSize::Small)
            .on_click(move |_, _, _| send())
    }

    fn card(&self, player: &MprisPlayerData, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let name = player.name();
        let (status, _, color) = status_display(player.state);
        let (title, artists) = title_and_artists(player);
        let (_, play_icon) = play_pause(player.state);
        let progress = match (player.position_us(), player.duration_us) {
            (Some(position), Some(duration)) if duration > 0 => {
                Some(track_progress(position, duration, cx))
            }
            _ => None,
        };

        let cover = div()
            .flex_none()
            .size(px(CARD_COVER_SIZE))
            .rounded(Radius::Small.pixels())
            .overflow_hidden()
            .bg(theme.colors.elevated_surface_background)
            .flex()
            .items_center()
            .justify_center()
            .map(|slot| match &player.art_url {
                Some(art) => slot.child(
                    img(image_source(art))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                ),
                None => slot.child(
                    Icon::new(player_icon(&name))
                        .size(IconSize::Large)
                        .color(Color::Muted),
                ),
            });

        let controls =
            h_flex()
                .flex_none()
                .gap(Spacing::XSmall.pixels())
                .when(player.can_control, |el| {
                    el.child(self.transport_button(
                        "mpris-card-prev",
                        IconName::SkipBack,
                        player,
                        PlayerCommand::Prev,
                    ))
                    .child(self.transport_button(
                        "mpris-card-play",
                        play_icon,
                        player,
                        PlayerCommand::PlayPause,
                    ))
                    .child(self.transport_button(
                        "mpris-card-next",
                        IconName::SkipForward,
                        player,
                        PlayerCommand::Next,
                    ))
                });

        let text = v_flex()
            .flex_1()
            .min_w(px(0.))
            .gap(px(2.))
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_start()
                    .gap(Spacing::Medium.pixels())
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.))
                            .child(
                                Label::new(title.unwrap_or_else(|| name.clone()))
                                    .size(TextSize::Default)
                                    .weight(FontWeight::MEDIUM)
                                    .truncate(),
                            )
                            .children(artists.map(|artists| {
                                Label::new(artists)
                                    .size(TextSize::Small)
                                    .color(Color::Muted)
                                    .truncate()
                            })),
                    )
                    .child(controls),
            )
            .children(progress)
            .child(
                Label::new(format!("{name} · {status}"))
                    .size(TextSize::XSmall)
                    .color(color),
            );

        div()
            .p(Spacing::Medium.pixels())
            .child(
                h_flex()
                    .w_full()
                    .p(Spacing::Medium.pixels())
                    .gap(Spacing::Medium.pixels())
                    .bg(theme.colors.surface_background)
                    .rounded(Radius::Large.pixels())
                    .child(cover)
                    .child(text),
            )
            .into_any_element()
    }
}

impl LauncherPage for MprisModule {
    fn content(&mut self, _query: &str, _cx: &mut Context<Self>) -> Content {
        List::new()
            .show_detail(true)
            .placeholder("Search players...")
            .empty(
                EmptyView::new("No media players")
                    .description("Nothing on the session bus speaks MPRIS")
                    .icon(MPRIS_ICON),
            )
            .section(
                Section::new("Players")
                    .subtitle(self.data.players.len().to_string())
                    .items(self.data.players.iter().map(|player| self.item(player))),
            )
            .into()
    }

    fn render_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let player = self.primary()?.clone();
        Some(self.card(&player, cx))
    }
}

fn detail(player: &MprisPlayerData, name: &str) -> Detail {
    let (status, icon, color) = status_display(player.state);
    let (title, artists) = title_and_artists(player);
    let mut markdown = String::new();
    if let Some(cover) = &player.art_url {
        markdown.push_str(&format!("![cover]({cover})\n\n"));
    }
    markdown.push_str(&format!("## {}\n", title.as_deref().unwrap_or(name)));
    if let Some(artists) = artists {
        markdown.push_str(&format!("\n{artists}\n"));
    }

    let mut detail = Detail::new()
        .markdown(markdown)
        .metadata(Metadata::label("Player", name.to_string()))
        .metadata(Metadata::label("Status", status).icon(icon).color(color));
    match (player.position_us(), player.duration_us) {
        (Some(position), Some(duration)) => {
            detail = detail.metadata(Metadata::label(
                "Position",
                format!(
                    "{} / {}",
                    format_track_time(position),
                    format_track_time(duration)
                ),
            ));
        }
        (_, Some(duration)) => {
            detail = detail.metadata(Metadata::label("Duration", format_track_time(duration)));
        }
        _ => {}
    }
    if let Some(volume) = player.volume {
        detail = detail.metadata(Metadata::label(
            "Volume",
            format!("{:.0}%", volume.clamp(0.0, 100.0)),
        ));
    }
    detail
}

fn title_and_artists(player: &MprisPlayerData) -> (Option<String>, Option<String>) {
    let Some(meta) = &player.metadata else {
        return (None, None);
    };
    let title = meta.title.clone().filter(|title| !title.is_empty());
    let artists = meta
        .artists
        .as_ref()
        .filter(|artists| !artists.is_empty())
        .map(|artists| artists.join(", "));
    (title, artists)
}

/// `Artist - Title` as the service formats it, or nothing when the player
/// reports no track.
fn track_label(player: &MprisPlayerData) -> Option<String> {
    player
        .metadata
        .as_ref()
        .map(ToString::to_string)
        .filter(|label| !label.is_empty())
}

fn play_pause(state: PlaybackStatus) -> (&'static str, IconName) {
    match state {
        PlaybackStatus::Playing => ("Pause", IconName::Pause),
        _ => ("Play", IconName::Play),
    }
}

fn status_display(state: PlaybackStatus) -> (&'static str, IconName, Color) {
    match state {
        PlaybackStatus::Playing => ("Playing", IconName::Play, Color::Success),
        PlaybackStatus::Paused => ("Paused", IconName::Pause, Color::Warning),
        PlaybackStatus::Stopped => ("Stopped", IconName::Stop, Color::Muted),
    }
}

fn player_icon(name: &str) -> IconName {
    match name.to_lowercase().as_str() {
        "spotify" => IconName::Spotify,
        "firefox" => IconName::Firefox,
        "brave" | "chrome" | "chromium" => IconName::Chrome,
        _ => MPRIS_ICON,
    }
}
