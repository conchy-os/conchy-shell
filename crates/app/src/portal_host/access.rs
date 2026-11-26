//! Permission dialog (`org.freedesktop.impl.portal.Access`).
//!
//! A full-output layer-shell overlay with exclusive keyboard focus, a dimmed backdrop and a
//! centered card. Several requests queue up and are shown one at a time.
//!
//! Keyboard: Escape denies. Enter presses the focused button, and focus starts on **Deny**:
//! granting takes an explicit click or Tab then Enter.

use std::collections::VecDeque;

use conchy_portal_proto::access::{AccessRequest, AccessResult};
use conchy_portal_proto::files::{Choice, ChoiceSelection};
use conchy_portal_proto::response;
use futures::channel::oneshot;
use gpui::{
    AnyWindowHandle, App, Bounds, Context, FocusHandle, Focusable, Global, KeyDownEvent, Render,
    Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div,
    layer_shell::*, point, prelude::*, px,
};
use ui::{
    ActiveTheme, Button, ButtonCommon, ButtonStyle, Checkbox, Clickable, Color, Label, LabelCommon,
    Modal, Spacing, TextSize, TintColor, ToggleState, h_flex, modal_overlay, v_flex,
};

use super::dbus::HostEvent;
use crate::state::{AppState, display_extent};

type Reply = oneshot::Sender<(u32, AccessResult)>;

/// A dialog waiting for its turn: only one overlay owns the keyboard at a time.
pub(super) enum Pending {
    Access {
        id: String,
        req: AccessRequest,
        reply: Reply,
    },
    Cast(super::cast_picker::CastPending),
}

impl Pending {
    fn id(&self) -> &str {
        match self {
            Self::Access { id, .. } => id,
            Self::Cast(c) => &c.id,
        }
    }

    /// Answer "cancelled" without ever showing the dialog.
    fn refuse(self) {
        match self {
            Self::Access { reply, .. } => {
                let _ = reply.send((response::CANCELLED, AccessResult::default()));
            }
            Self::Cast(c) => c.refuse(),
        }
    }
}

struct Current {
    id: String,
    handle: AnyWindowHandle,
}

/// Dialogs waiting for their turn and the one on screen.
#[derive(Default)]
struct AccessQueue {
    pending: VecDeque<Pending>,
    current: Option<Current>,
}

impl Global for AccessQueue {}

pub fn handle(event: HostEvent, cx: &mut App) {
    match event {
        HostEvent::Access { id, req, reply } => {
            enqueue(
                Pending::Access {
                    id,
                    req: *req,
                    reply,
                },
                cx,
            );
        }
        HostEvent::PickCast { id, req, reply } => super::cast_picker::request(id, *req, reply, cx),
        HostEvent::Cancel { id } => cancel(&id, cx),
    }
}

pub(super) fn enqueue(dialog: Pending, cx: &mut App) {
    cx.default_global::<AccessQueue>().pending.push_back(dialog);
    pump(cx);
}

/// A dialog window went away: open the next queued one.
pub(super) fn dialog_closed(cx: &mut App) {
    cx.default_global::<AccessQueue>().current = None;
    cx.defer(pump);
}

/// Show the next queued dialog if none is open.
fn pump(cx: &mut App) {
    let queue = cx.default_global::<AccessQueue>();
    if queue.current.is_some() {
        return;
    }
    let Some(next) = queue.pending.pop_front() else {
        return;
    };
    let id = next.id().to_string();
    match open_dialog(next, cx) {
        Some(handle) => cx.default_global::<AccessQueue>().current = Some(Current { id, handle }),
        // The window could not be created: refuse rather than hang the app waiting.
        None => pump(cx),
    }
}

fn cancel(id: &str, cx: &mut App) {
    let queue = cx.default_global::<AccessQueue>();
    if let Some(pos) = queue.pending.iter().position(|p| p.id() == id) {
        if let Some(p) = queue.pending.remove(pos) {
            p.refuse();
        }
        return;
    }
    if let Some(current) = queue.current.as_ref().filter(|c| c.id == id) {
        let handle = current.handle;
        // The dialog answers `CANCELLED` itself when its window goes away.
        cx.defer(move |cx| {
            let _ = cx.update_window(handle, |_, window, _| window.remove_window());
        });
    }
}

/// Window options of a full-output overlay with exclusive keyboard focus.
pub(super) fn overlay_options(namespace: &str, app_id: &str, cx: &App) -> WindowOptions {
    // Without the shell state (dev mode `CONCHY_SHELL_PORTAL_ONLY`) fall back to gpui's display.
    let size = if cx.has_global::<AppState>() {
        display_extent(None, cx)
    } else {
        cx.primary_display()
            .map(|d| d.bounds().size)
            .unwrap_or_else(|| gpui::size(px(1920.), px(1080.)))
    };
    WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.), px(0.)),
            size,
        })),
        app_id: Some(app_id.to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: namespace.to_string(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            // Cover the whole output, bars included.
            exclusive_zone: Some(px(-1.)),
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        focus: true,
        ..Default::default()
    }
}

fn open_dialog(next: Pending, cx: &mut App) -> Option<AnyWindowHandle> {
    let started = std::time::Instant::now();
    match next {
        Pending::Access { id, req, reply } => {
            let options = overlay_options("portal-access", "conchy-shell-portal-access", cx);
            let handle = cx
                .open_window(options, move |window, cx| {
                    window.set_rem_size(cx.theme().font_size);
                    let handle = window.window_handle();
                    cx.new(|cx| AccessDialog::new(id, req, reply, handle, started, window, cx))
                })
                .ok()?;
            Some(handle.into())
        }
        Pending::Cast(c) => super::cast_picker::open(c, started, cx),
    }
}

/// The app's display name from its `.desktop` file, else its raw app id.
fn app_name(app_id: &str, cx: &App) -> String {
    if app_id.is_empty() {
        return String::new();
    }
    if !cx.has_global::<AppState>() {
        return app_id.to_string();
    }
    // Flatpak ids reach us without the `.desktop` suffix; the catalog matches on either form.
    let apps = AppState::applications(cx).all();
    services::applications::match_app_id(apps, app_id)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| app_id.to_string())
}

struct AccessDialog {
    id: String,
    req: AccessRequest,
    reply: Option<Reply>,
    window: AnyWindowHandle,
    focus: FocusHandle,
    /// Keyboard focus is on the grant button (starts on deny).
    grant_focused: bool,
    choices: Vec<(Choice, String)>,
    app_name: String,
    started: std::time::Instant,
    first_frame_logged: bool,
}

impl AccessDialog {
    fn new(
        id: String,
        req: AccessRequest,
        reply: Reply,
        window_handle: AnyWindowHandle,
        started: std::time::Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        // Whatever way the window goes away (button, Escape, wm, cancel), the daemon gets an
        // answer and the next queued dialog opens.
        cx.on_release(|this, cx| {
            this.answer(response::CANCELLED, false);
            dialog_closed(cx);
        })
        .detach();
        let choices = req
            .choices
            .iter()
            .map(|c| (c.clone(), choice_initial(c)))
            .collect();
        let app_name = app_name(&req.app_id, cx);
        Self {
            id,
            req,
            reply: Some(reply),
            window: window_handle,
            focus,
            grant_focused: false,
            choices,
            app_name,
            started,
            first_frame_logged: false,
        }
    }

    /// Send the answer once.
    fn answer(&mut self, code: u32, granted: bool) {
        let Some(reply) = self.reply.take() else {
            return;
        };
        let result = if granted {
            AccessResult {
                choices: self
                    .choices
                    .iter()
                    .map(|(c, v)| ChoiceSelection {
                        id: c.id.clone(),
                        value: v.clone(),
                    })
                    .collect(),
            }
        } else {
            AccessResult::default()
        };
        let _ = reply.send((code, result));
    }

    fn finish(&mut self, grant: bool, cx: &mut Context<Self>) {
        self.answer(
            if grant {
                response::OK
            } else {
                response::CANCELLED
            },
            grant,
        );
        let handle = self.window;
        cx.defer(move |cx| {
            let _ = cx.update_window(handle, |_, window, _| window.remove_window());
        });
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match ev.keystroke.key.as_str() {
            "escape" => self.finish(false, cx),
            "enter" => self.finish(self.grant_focused, cx),
            "tab" | "left" | "right" => {
                self.grant_focused = !self.grant_focused;
                cx.notify();
            }
            _ => {}
        }
        cx.stop_propagation();
    }
}

/// Initial value of a choice: the portal's, else the first option / "false" for a checkbox.
fn choice_initial(c: &Choice) -> String {
    if !c.initial.is_empty() {
        return c.initial.clone();
    }
    c.options
        .first()
        .map_or_else(|| "false".to_string(), |o| o.id.clone())
}

impl Focusable for AccessDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for AccessDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(cx.theme().font_size);
        if !self.first_frame_logged {
            self.first_frame_logged = true;
            tracing::info!(
                id = %self.id,
                ms = self.started.elapsed().as_secs_f64() * 1000.0,
                "Access dialog first frame"
            );
        }

        let deny_label = non_empty(&self.req.deny_label, "Deny");
        let grant_label = non_empty(&self.req.grant_label, "Allow");

        let mut modal = Modal::new().title(self.req.title.clone());
        if !self.app_name.is_empty() {
            modal = modal.child(
                Label::new(self.app_name.clone())
                    .size(TextSize::Small)
                    .color(Color::Accent),
            );
        }
        if !self.req.subtitle.is_empty() {
            modal = modal.child(Label::new(self.req.subtitle.clone()));
        }
        if !self.req.body.is_empty() {
            modal = modal.child(Label::new(self.req.body.clone()).color(Color::Muted));
        }
        for (i, (choice, value)) in self.choices.iter().enumerate() {
            modal = modal.child(render_choice(i, choice, value, cx));
        }

        let deny = Button::new("access-deny", deny_label)
            .style(if self.grant_focused {
                ButtonStyle::Outlined
            } else {
                ButtonStyle::Filled
            })
            .on_click(cx.listener(|this, _, _, cx| this.finish(false, cx)));
        let grant = Button::new("access-grant", grant_label)
            .style(if self.grant_focused {
                ButtonStyle::Tinted(TintColor::Accent)
            } else {
                ButtonStyle::Outlined
            })
            .on_click(cx.listener(|this, _, _, cx| this.finish(true, cx)));
        let modal = modal.footer(
            h_flex()
                .justify_end()
                .gap(Spacing::Small.pixels())
                .child(deny)
                .child(grant),
        );

        let entity = cx.entity();
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .child(modal_overlay(self.focus.clone(), modal, move |_, cx| {
                // Backdrop click denies.
                entity.update(cx, |this, cx| this.finish(false, cx));
            }))
    }
}

fn non_empty(s: &str, fallback: &str) -> String {
    if s.trim().is_empty() {
        fallback.to_string()
    } else {
        s.to_string()
    }
}

/// A checkbox (no options) or a row of option buttons.
fn render_choice(
    i: usize,
    choice: &Choice,
    value: &str,
    cx: &mut Context<AccessDialog>,
) -> impl IntoElement {
    if choice.options.is_empty() {
        let on = value == "true";
        return Checkbox::new(("access-choice", i), ToggleState::from(on))
            .label(choice.label.clone())
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some((_, v)) = this.choices.get_mut(i) {
                    *v = if *v == "true" {
                        "false".into()
                    } else {
                        "true".into()
                    };
                }
                cx.notify();
            }))
            .into_any_element();
    }
    let mut row = h_flex().gap(Spacing::Small.pixels()).items_center();
    row = row.child(Label::new(format!("{}:", choice.label)).color(Color::Muted));
    for (j, opt) in choice.options.iter().enumerate() {
        let id = opt.id.clone();
        row = row.child(
            Button::new(("access-opt", i * 100 + j), opt.label.clone())
                .style(if opt.id == value {
                    ButtonStyle::Tinted(TintColor::Accent)
                } else {
                    ButtonStyle::Outlined
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some((_, v)) = this.choices.get_mut(i) {
                        *v = id.clone();
                    }
                    cx.notify();
                })),
        );
    }
    v_flex().child(row).into_any_element()
}
