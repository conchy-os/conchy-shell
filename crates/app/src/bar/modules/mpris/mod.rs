//! MPRIS widget showing active media state and opening a players panel.

mod config;
pub use config::MprisConfig;

use gpui::{AnyElement, App, ClickEvent, Context, Pixels, Point, Window, prelude::*, px};
use services::{MprisData, PlaybackStatus};
use ui::{ButtonCommon, ButtonLike, ButtonStyle, Clickable, IconName, LabelCommon};

use super::{BarCtx, BarWidget, BarWidgetDescriptor, BarWidgetMeta, boxed, style};
use crate::config::{ActiveConfig, Config};
use crate::state::AppState;
use crate::state::watch;
use crate::windows::{PanelConfig, PanelSize, ShellWindows, panel_placement_from_event};

mod panel;
pub use panel::MprisPanel;
pub(crate) use panel::tick_while_playing;

pub const fn descriptor() -> BarWidgetDescriptor {
    BarWidgetDescriptor::new(
        BarWidgetMeta {
            name: "Mpris",
            aliases: &["Media", "Player"],
            label: "Media",
            description: "Now-playing controls for media players.",
            icon: IconName::Music,
        },
        |cx| boxed(cx, Mpris::new),
    )
}

/// Bar widget for media status and controls.
pub struct Mpris {
    subscriber: services::MprisSubscriber,
    data: MprisData,
}

impl Mpris {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscriber = AppState::mpris(cx).clone();
        let data = subscriber.get();

        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });

        Self { subscriber, data }
    }

    fn toggle_panel(&self, at: Point<Pixels>, window: &Window, cx: &mut App) {
        let subscriber = self.subscriber.clone();
        let config = Config::global(cx);
        let size = PanelSize::FitHeight {
            width: 380.0,
            max_height: 600.0,
        };
        let (anchor, margin) =
            panel_placement_from_event(config.bar.position, at, window, cx, size.initial());
        let config = PanelConfig {
            size,
            anchor,
            margin,
            namespace: "mpris-panel".to_string(),
        };

        ShellWindows::toggle_panel(cx, window, "mpris", config, move |cx| {
            MprisPanel::new(subscriber, cx)
        });
    }

    fn primary_player(&self) -> Option<&services::MprisPlayerData> {
        self.data
            .players
            .iter()
            .find(|p| p.state == PlaybackStatus::Playing)
            .or_else(|| self.data.players.first())
    }

    fn icon(&self) -> IconName {
        match self.primary_player().map(|p| p.state) {
            Some(PlaybackStatus::Playing) => IconName::Play,
            Some(PlaybackStatus::Paused) => IconName::Pause,
            Some(PlaybackStatus::Stopped) => IconName::Stop,
            None => IconName::Stop,
        }
    }

    /// The track as `(title, artist)`: the title in full ink, the artist
    /// muted after it. A player with no metadata shows its name alone.
    fn label(&self) -> (String, Option<String>) {
        let Some(player) = self.primary_player() else {
            return ("No media".to_string(), None);
        };

        if let Some(metadata) = &player.metadata {
            let artist = metadata
                .artists
                .as_ref()
                .map(|artists| artists.join(", "))
                .filter(|artist| !artist.is_empty());
            match (&metadata.title, artist) {
                (Some(title), artist) if !title.is_empty() => return (title.clone(), artist),
                (_, Some(artist)) => return (artist, None),
                _ => {}
            }
        }

        (player.name(), None)
    }

    /// A vertical bar has no room for a title, so it shows the state alone.
    fn render(&self, ctx: &BarCtx, cx: &mut Context<Self>) -> AnyElement {
        let max_width = cx.config().bar.modules.mpris.max_width;
        // Roughly how many characters the configured width holds, so the cut
        // lands on a word; `truncate` still guards the pixels.
        let chars = (max_width / f32::from(ctx.rhythm.icon()) * 2.) as usize;
        let label = (!ctx.vertical).then(|| self.label());

        ButtonLike::new("mpris-widget")
            .style(ButtonStyle::Transparent)
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                this.toggle_panel(event.position(), window, cx);
            }))
            .child(
                style::stack(ctx)
                    .justify_center()
                    .gap(ctx.metrics.gap)
                    .max_w(px(max_width))
                    .overflow_hidden()
                    .child(style::icon(ctx, self.icon()))
                    .when_some(label, |el, (title, artist)| {
                        let title_chars = if artist.is_some() {
                            chars * 2 / 3
                        } else {
                            chars
                        };
                        el.child(
                            style::label(ctx, style::fit_words(&title, title_chars)).truncate(),
                        )
                        .when_some(artist, |el, artist| {
                            el.child(
                                style::label_in(
                                    ctx,
                                    style::fit_words(&artist, chars - title_chars),
                                    ctx.ink.muted,
                                )
                                .truncate(),
                            )
                        })
                    }),
            )
            .into_any_element()
    }
}

impl BarWidget for Mpris {
    fn is_interactive(&self) -> bool {
        true
    }

    fn render_vertical(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, cx)
    }

    fn render_horizontal(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, cx)
    }
}
