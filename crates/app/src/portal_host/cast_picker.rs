//! ScreenCast source picker (`ShellHost.PickCastSources`): which screens or windows to share.
//!
//! Same overlay machinery as the permission dialog (they share one queue, because only one
//! overlay may own the keyboard). The first version has no live previews: a checkbox row per
//! monitor or window with its title. Monitors come from the shell's compositor service; windows
//! come from `org.gnome.Shell.Introspect.GetWindows` in conchy-wm itself, so the ids we return
//! are exactly the ones `RecordWindow` expects (see PLAN.md, section 1).
//!
//! Keyboard: Up/Down move, Space toggles, Tab switches between screens and windows,
//! Enter shares, Escape cancels.

use std::collections::HashMap;

use conchy_portal_proto::cast::{
    CastPickRequest, CastPickResult, CastSource, SOURCE_MONITOR, SOURCE_WINDOW,
};
use conchy_portal_proto::response;
use futures::channel::oneshot;
use gpui::{
    AnyWindowHandle, App, Context, FocusHandle, Focusable, KeyDownEvent, Render, Window, div,
    prelude::*, px,
};
use ui::{
    ActiveTheme, Button, ButtonCommon, ButtonStyle, Checkbox, Clickable, Color, Disableable, Label,
    LabelCommon, Modal, Spacing, TextSize, TintColor, ToggleState, h_flex, modal_overlay, v_flex,
};
use zbus::zvariant::OwnedValue;

use super::access::{Pending, dialog_closed, enqueue, overlay_options};
use crate::state::AppState;

type Reply = oneshot::Sender<(u32, CastPickResult)>;

/// conchy-wm lists its own "dynamic cast target" placeholder window; it is not shareable.
const DYNAMIC_TARGET_TITLE: &str = "niri Dynamic Cast Target";

#[derive(Clone)]
struct MonitorItem {
    connector: String,
    label: String,
}

#[derive(Clone)]
struct WindowItem {
    id: u64,
    title: String,
    app_id: String,
    app_name: String,
}

pub(super) struct CastPending {
    pub id: String,
    req: CastPickRequest,
    reply: Reply,
    monitors: Vec<MonitorItem>,
    windows: Vec<WindowItem>,
}

impl CastPending {
    pub fn refuse(self) {
        let _ = self.reply.send((response::CANCELLED, CastPickResult::default()));
    }
}

/// Gather monitors and windows, then queue the picker.
pub(super) fn request(id: String, req: CastPickRequest, reply: Reply, cx: &mut App) {
    let monitors = if req.types & SOURCE_MONITOR != 0 { monitors(cx) } else { Vec::new() };
    let want_windows = req.types & SOURCE_WINDOW != 0;
    let (tx, rx) = oneshot::channel();
    tokio::spawn(async move {
        let _ = tx.send(if want_windows { fetch_windows().await } else { Vec::new() });
    });
    cx.spawn(async move |cx| {
        let raw = rx.await.unwrap_or_default();
        cx.update(|cx| {
            let windows = raw
                .into_iter()
                .map(|(id, title, app_id)| WindowItem {
                    id,
                    app_name: app_display_name(&app_id, cx),
                    title,
                    app_id,
                })
                .collect();
            enqueue(Pending::Cast(CastPending { id, req, reply, monitors, windows }), cx);
        });
    })
    .detach();
}

fn monitors(cx: &App) -> Vec<MonitorItem> {
    if cx.has_global::<AppState>() {
        return AppState::compositor(cx)
            .get()
            .monitors
            .iter()
            .map(|m| MonitorItem {
                connector: m.name.clone(),
                label: format!("{}  ({}×{})", m.name, m.width, m.height),
            })
            .collect();
    }
    // Dev mode without the compositor service: `NAME:WxH,NAME:WxH`.
    std::env::var("CONCHY_PORTAL_FAKE_MONITORS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|m| m.split_once(':'))
        .map(|(n, size)| MonitorItem { connector: n.to_string(), label: format!("{n}  ({size})") })
        .collect()
}

/// `(window id, title, app id)` from `Shell.Introspect.GetWindows`, titles sorted.
async fn fetch_windows() -> Vec<(u64, String, String)> {
    let Ok(conn) = services::bus::session().await else { return Vec::new() };
    let Ok(proxy) = zbus::Proxy::new(
        &conn,
        "org.gnome.Shell.Introspect",
        "/org/gnome/Shell/Introspect",
        "org.gnome.Shell.Introspect",
    )
    .await
    else {
        return Vec::new();
    };
    let windows: HashMap<u64, HashMap<String, OwnedValue>> = match proxy.call("GetWindows", &()).await {
        Ok(w) => w,
        Err(err) => {
            tracing::warn!("Shell.Introspect.GetWindows failed: {err}");
            return Vec::new();
        }
    };
    let text = |m: &HashMap<String, OwnedValue>, key: &str| {
        m.get(key).and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default()
    };
    let mut out: Vec<_> = windows
        .iter()
        .map(|(id, m)| (*id, text(m, "title"), text(m, "app-id")))
        .filter(|(_, title, _)| title != DYNAMIC_TARGET_TITLE)
        .collect();
    out.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()).then(a.0.cmp(&b.0)));
    out
}

/// Introspect reports app ids as `<id>.desktop`.
fn app_display_name(app_id: &str, cx: &App) -> String {
    let bare = app_id.strip_suffix(".desktop").unwrap_or(app_id);
    if bare.is_empty() || !cx.has_global::<AppState>() {
        return bare.to_string();
    }
    services::applications::match_app_id(AppState::applications(cx).all(), bare)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| bare.to_string())
}

pub(super) fn open(
    c: CastPending,
    started: std::time::Instant,
    cx: &mut App,
) -> Option<AnyWindowHandle> {
    let options = overlay_options("portal-cast-picker", "conchy-shell-portal-cast", cx);
    let handle = cx
        .open_window(options, move |window, cx| {
            window.set_rem_size(cx.theme().font_size);
            let handle = window.window_handle();
            cx.new(|cx| CastPicker::new(c, handle, started, window, cx))
        })
        .ok()?;
    Some(handle.into())
}

#[derive(Clone, PartialEq, Eq)]
enum Key {
    Monitor(String),
    Window(u64),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Screens,
    Windows,
}

struct CastPicker {
    id: String,
    req: CastPickRequest,
    reply: Option<Reply>,
    window: AnyWindowHandle,
    focus: FocusHandle,
    tab: Tab,
    monitors: Vec<MonitorItem>,
    windows: Vec<WindowItem>,
    selected: Vec<Key>,
    cursor: usize,
    started: std::time::Instant,
    first_frame_logged: bool,
}

impl CastPicker {
    fn new(
        c: CastPending,
        window_handle: AnyWindowHandle,
        started: std::time::Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        // However the window goes away, the daemon gets an answer and the next dialog opens.
        cx.on_release(|this, cx| {
            this.answer(response::CANCELLED, Vec::new());
            dialog_closed(cx);
        })
        .detach();
        let tab = if c.req.types & SOURCE_MONITOR != 0 { Tab::Screens } else { Tab::Windows };
        Self {
            id: c.id,
            req: c.req,
            reply: Some(c.reply),
            window: window_handle,
            focus,
            tab,
            monitors: c.monitors,
            windows: c.windows,
            selected: Vec::new(),
            cursor: 0,
            started,
            first_frame_logged: false,
        }
    }

    fn has_tab(&self, tab: Tab) -> bool {
        match tab {
            Tab::Screens => self.req.types & SOURCE_MONITOR != 0,
            Tab::Windows => self.req.types & SOURCE_WINDOW != 0,
        }
    }

    /// Rows of the current tab: key, title, secondary text.
    fn rows(&self) -> Vec<(Key, String, String)> {
        match self.tab {
            Tab::Screens => self
                .monitors
                .iter()
                .map(|m| (Key::Monitor(m.connector.clone()), m.label.clone(), String::new()))
                .collect(),
            Tab::Windows => self
                .windows
                .iter()
                .map(|w| (Key::Window(w.id), w.title.clone(), w.app_name.clone()))
                .collect(),
        }
    }

    fn toggle(&mut self, key: Key) {
        if let Some(pos) = self.selected.iter().position(|k| *k == key) {
            self.selected.remove(pos);
        } else if self.req.multiple {
            self.selected.push(key);
        } else {
            self.selected = vec![key];
        }
    }

    fn sources(&self) -> Vec<CastSource> {
        self.selected
            .iter()
            .filter_map(|key| match key {
                Key::Monitor(c) => Some(CastSource {
                    kind: SOURCE_MONITOR,
                    connector: c.clone(),
                    ..Default::default()
                }),
                Key::Window(id) => self.windows.iter().find(|w| w.id == *id).map(|w| CastSource {
                    kind: SOURCE_WINDOW,
                    window_id: w.id,
                    title: w.title.clone(),
                    app_id: w.app_id.clone(),
                    ..Default::default()
                }),
            })
            .collect()
    }

    fn answer(&mut self, code: u32, sources: Vec<CastSource>) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.send((code, CastPickResult { sources }));
        }
    }

    fn finish(&mut self, share: bool, cx: &mut Context<Self>) {
        if share {
            let sources = self.sources();
            if sources.is_empty() {
                return;
            }
            self.answer(response::OK, sources);
        } else {
            self.answer(response::CANCELLED, Vec::new());
        }
        let handle = self.window;
        cx.defer(move |cx| {
            let _ = cx.update_window(handle, |_, window, _| window.remove_window());
        });
    }

    fn switch_tab(&mut self) {
        let other = if self.tab == Tab::Screens { Tab::Windows } else { Tab::Screens };
        if self.has_tab(other) {
            self.tab = other;
            self.cursor = 0;
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let rows = self.rows();
        match ev.keystroke.key.as_str() {
            "escape" => return self.finish(false, cx),
            "enter" => {
                if self.selected.is_empty() {
                    if let Some((key, ..)) = rows.get(self.cursor) {
                        self.toggle(key.clone());
                    }
                }
                return self.finish(true, cx);
            }
            "down" if !rows.is_empty() => self.cursor = (self.cursor + 1).min(rows.len() - 1),
            "up" => self.cursor = self.cursor.saturating_sub(1),
            "space" => {
                if let Some((key, ..)) = rows.get(self.cursor) {
                    self.toggle(key.clone());
                }
            }
            "tab" => self.switch_tab(),
            _ => {}
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Focusable for CastPicker {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for CastPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(cx.theme().font_size);
        if !self.first_frame_logged {
            self.first_frame_logged = true;
            tracing::info!(
                id = %self.id,
                ms = self.started.elapsed().as_secs_f64() * 1000.0,
                "Cast picker first frame"
            );
        }

        let mut tabs = h_flex().gap(Spacing::Small.pixels());
        for (tab, label) in [(Tab::Screens, "Screens"), (Tab::Windows, "Windows")] {
            if !self.has_tab(tab) {
                continue;
            }
            tabs = tabs.child(
                Button::new(label, label)
                    .style(if self.tab == tab {
                        ButtonStyle::Tinted(TintColor::Accent)
                    } else {
                        ButtonStyle::Outlined
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = tab;
                        this.cursor = 0;
                        cx.notify();
                    })),
            );
        }

        let rows = self.rows();
        let mut list = v_flex().id("cast-list").max_h(px(320.)).overflow_y_scroll().gap(px(2.));
        for (i, (key, title, sub)) in rows.iter().enumerate() {
            let on = self.selected.contains(key);
            let label = if sub.is_empty() { title.clone() } else { format!("{title} — {sub}") };
            let key = key.clone();
            let cursor = i == self.cursor;
            list = list.child(
                div()
                    .rounded(px(4.))
                    .when(cursor, |d| d.bg(cx.theme().colors().element_hover))
                    .child(
                        Checkbox::new(("cast-row", i), ToggleState::from(on))
                            .label(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.cursor = i;
                                this.toggle(key.clone());
                                cx.notify();
                            })),
                    ),
            );
        }
        if rows.is_empty() {
            list = list.child(Label::new("Nothing to share").color(Color::Muted));
        }

        let who = if self.req.app_id.is_empty() {
            "An application wants to capture your screen".to_string()
        } else {
            format!("{} wants to capture your screen", self.req.app_id)
        };
        let footer = h_flex()
            .justify_end()
            .gap(Spacing::Small.pixels())
            .child(
                Button::new("cast-cancel", "Cancel")
                    .style(ButtonStyle::Outlined)
                    .on_click(cx.listener(|this, _, _, cx| this.finish(false, cx))),
            )
            .child(
                Button::new("cast-share", "Share")
                    .style(ButtonStyle::Tinted(TintColor::Accent))
                    .disabled(self.selected.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.finish(true, cx))),
            );
        let modal = Modal::new()
            .title("Share your screen")
            .width(px(520.))
            .child(Label::new(who).size(TextSize::Small).color(Color::Muted))
            .child(tabs)
            .child(list)
            .footer(footer);

        let entity = cx.entity();
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .child(modal_overlay(self.focus.clone(), modal, move |_, cx| {
                entity.update(cx, |this, cx| this.finish(false, cx));
            }))
    }
}
