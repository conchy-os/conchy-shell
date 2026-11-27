//! The polkit authentication dialog: the shell is the session's polkit agent
//! (see [`services::polkit`]).
//!
//! A full-output layer-shell overlay with exclusive keyboard focus, like the
//! portal's permission dialog: what is being asked and by whom, who to
//! authenticate as, the password, and a retry after a wrong one. Requests
//! queue up and are shown one at a time.
//!
//! Keyboard: Enter authenticates, Escape (or a click on the backdrop) cancels.

use std::collections::VecDeque;

use futures::StreamExt;
use futures::channel::oneshot;
use gpui::{
    AnyWindowHandle, App, Context, FocusHandle, Focusable, Global, KeyDownEvent, Render, Window,
    div, prelude::*,
};
use services::polkit::{AgentEvent, AuthOutcome, AuthRequest, HelperMsg, HelperSession};
use ui::{
    ActiveTheme, Button, ButtonCommon, ButtonStyle, Clickable, Color, Disableable, InputBuffer,
    Label, LabelCommon, Modal, Radius, Spacing, TextSize, TintColor, h_flex, modal_overlay,
    render_input_line, render_masked_input_line, v_flex,
};

use crate::keybinds::{
    Backspace, Cancel, Confirm, CursorLeft, CursorRight, DeleteWordBack, SelectAll, SelectLeft,
    SelectRight, SelectWordLeft, SelectWordRight, WordLeft, WordRight,
};

/// Key context of the dialog, for its bindings in [`crate::keybinds`].
pub const KEY_CONTEXT: &str = "PolkitAuth";

/// Become the session's polkit agent and show the dialogs it is asked for.
pub fn init(cx: &mut App) {
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    tokio::spawn(async move {
        if let Err(err) = services::polkit::serve(tx).await {
            tracing::warn!("Polkit agent not available: {err:#}");
        }
    });
    cx.spawn(async move |cx| {
        while let Some(event) = rx.next().await {
            cx.update(|cx| handle(event, cx));
        }
    })
    .detach();
}

struct Pending {
    req: AuthRequest,
    reply: oneshot::Sender<AuthOutcome>,
}

struct Current {
    cookie: String,
    handle: AnyWindowHandle,
}

#[derive(Default)]
struct AuthQueue {
    pending: VecDeque<Pending>,
    current: Option<Current>,
}

impl Global for AuthQueue {}

fn handle(event: AgentEvent, cx: &mut App) {
    match event {
        AgentEvent::Begin { req, reply } => {
            cx.default_global::<AuthQueue>()
                .pending
                .push_back(Pending { req: *req, reply });
            pump(cx);
        }
        AgentEvent::Cancel { cookie } => cancel(&cookie, cx),
    }
}

/// Show the next queued request if no dialog is open.
fn pump(cx: &mut App) {
    let queue = cx.default_global::<AuthQueue>();
    if queue.current.is_some() {
        return;
    }
    let Some(next) = queue.pending.pop_front() else {
        return;
    };
    let cookie = next.req.cookie.clone();
    let options =
        crate::portal_host::overlay_options("polkit-agent", "conchy-shell-polkit-agent", cx);
    let opened = cx.open_window(options, move |window, cx| {
        window.set_rem_size(cx.theme().font_size);
        let handle = window.window_handle();
        cx.new(|cx| AuthDialog::new(next, handle, window, cx))
    });
    match opened {
        Ok(handle) => {
            cx.default_global::<AuthQueue>().current = Some(Current {
                cookie,
                handle: handle.into(),
            });
        }
        // No window: the request was answered "cancelled" when it was
        // dropped; try the next one.
        Err(err) => {
            tracing::warn!("Cannot open the polkit dialog: {err:#}");
            pump(cx);
        }
    }
}

fn dialog_closed(cx: &mut App) {
    cx.default_global::<AuthQueue>().current = None;
    cx.defer(pump);
}

/// polkitd withdrew a request: drop it from the queue or close its dialog.
fn cancel(cookie: &str, cx: &mut App) {
    let queue = cx.default_global::<AuthQueue>();
    if let Some(pos) = queue.pending.iter().position(|p| p.req.cookie == cookie) {
        if let Some(p) = queue.pending.remove(pos) {
            let _ = p.reply.send(AuthOutcome::Cancelled);
        }
        return;
    }
    if let Some(current) = queue.current.as_ref().filter(|c| c.cookie == cookie) {
        let handle = current.handle;
        cx.defer(move |cx| {
            let _ = cx.update_window(handle, |_, window, _| window.remove_window());
        });
    }
}

struct AuthDialog {
    req: AuthRequest,
    reply: Option<oneshot::Sender<AuthOutcome>>,
    window: AnyWindowHandle,
    focus: FocusHandle,
    /// Index into `req.identities`.
    selected: usize,
    input: InputBuffer,
    /// The helper's current question, and whether the answer may be shown.
    prompt: Option<(String, bool)>,
    error: Option<String>,
    info: Option<String>,
    session: Option<HelperSession>,
    /// Bumped per helper session, so lines from an old one are ignored.
    generation: u64,
    /// The helper asked something in this session (else a failure means it
    /// could not run, and retrying on its own would loop).
    prompted: bool,
    /// An answer was sent and the helper has not replied yet. PAM waits a
    /// few seconds after a wrong password, so this is shown.
    checking: bool,
}

impl AuthDialog {
    fn new(
        pending: Pending,
        window_handle: AnyWindowHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        // However the window goes away, polkitd gets an answer and the next
        // request opens; the helper is stopped with the session.
        cx.on_release(|this, cx| {
            this.answer(AuthOutcome::Cancelled);
            dialog_closed(cx);
        })
        .detach();
        let Pending { req, reply } = pending;
        let selected = req.preferred_identity();
        let mut this = Self {
            req,
            reply: Some(reply),
            window: window_handle,
            focus,
            selected,
            input: InputBuffer::new(""),
            prompt: None,
            error: None,
            info: None,
            session: None,
            generation: 0,
            prompted: false,
            checking: false,
        };
        this.start_session(cx);
        this
    }

    fn answer(&mut self, outcome: AuthOutcome) {
        self.session = None;
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(outcome);
        }
    }

    fn close(&mut self, outcome: AuthOutcome, cx: &mut Context<Self>) {
        self.answer(outcome);
        let handle = self.window;
        cx.defer(move |cx| {
            let _ = cx.update_window(handle, |_, window, _| window.remove_window());
        });
    }

    /// Ask the helper to authenticate the selected user.
    fn start_session(&mut self, cx: &mut Context<Self>) {
        let Some(identity) = self.req.identities.get(self.selected) else {
            return;
        };
        let (session, mut rx) = HelperSession::start(&identity.name, &self.req.cookie);
        self.session = Some(session);
        self.generation += 1;
        self.prompt = None;
        self.prompted = false;
        self.info = None;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            while let Some(msg) = rx.next().await {
                if this
                    .update(cx, |this, cx| this.on_helper(generation, msg, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn on_helper(&mut self, generation: u64, msg: HelperMsg, cx: &mut Context<Self>) {
        if generation != self.generation {
            return;
        }
        self.checking = false;
        match msg {
            HelperMsg::Prompt { text, echo } => {
                self.prompted = true;
                self.prompt = Some((text, echo));
            }
            HelperMsg::Error(text) => self.error = Some(text),
            HelperMsg::Info(text) => self.info = Some(text),
            HelperMsg::Success => {
                self.close(AuthOutcome::Authenticated, cx);
                return;
            }
            HelperMsg::Failure => {
                self.session = None;
                self.input.clear();
                if self.prompted {
                    // Wrong password: say so and ask again, as other agents do.
                    if self.error.is_none() {
                        self.error = Some("Wrong password, try again".into());
                    }
                    self.start_session(cx);
                } else if self.error.is_none() {
                    self.error = Some("Authentication could not start".into());
                }
            }
        }
        cx.notify();
    }

    /// Send what was typed as the answer to the helper's question.
    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.session.is_none() {
            // The helper could not run before: Authenticate tries again.
            self.error = None;
            self.start_session(cx);
            return;
        }
        if self.prompt.take().is_none() {
            return;
        }
        let answer = self.input.text().to_string();
        self.input.clear();
        self.error = None;
        self.checking = true;
        if let Some(session) = &self.session {
            session.respond(answer);
        }
        cx.notify();
    }

    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == self.selected || index >= self.req.identities.len() {
            return;
        }
        self.selected = index;
        self.input.clear();
        self.error = None;
        self.start_session(cx);
    }

    fn edit(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut InputBuffer)) {
        edit(&mut self.input);
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.control || keystroke.modifiers.alt || self.prompt.is_none() {
            return;
        }
        let ch = keystroke
            .key_char
            .as_ref()
            .and_then(|s| s.chars().next())
            .or_else(|| {
                let mut chars = keystroke.key.chars();
                chars.next().filter(|_| chars.next().is_none())
            });
        if let Some(ch) = ch.filter(|ch| !ch.is_control()) {
            self.edit(cx, |input| input.insert_str(&ch.to_string()));
        }
    }
}

impl Focusable for AuthDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for AuthDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(cx.theme().font_size);
        let theme = cx.theme();
        let border = if self.error.is_some() {
            theme.colors.status.error
        } else {
            theme.colors.accent
        };
        let field_bg = theme.colors.background;
        let text = theme.colors.text;

        let message = if self.req.message.trim().is_empty() {
            "An application wants to perform a privileged action".to_string()
        } else {
            self.req.message.clone()
        };
        let mut modal = Modal::new()
            .title("Authentication required")
            .child(Label::new(message));
        if let Some(command) = self.req.command() {
            modal = modal.child(
                Label::new(command.to_string())
                    .size(TextSize::Small)
                    .color(Color::Muted),
            );
        }
        modal = modal.child(
            Label::new(self.req.action_id.clone())
                .size(TextSize::XSmall)
                .color(Color::Muted),
        );

        if self.req.identities.len() > 1 {
            let mut row = h_flex()
                .gap(Spacing::Small.pixels())
                .items_center()
                .flex_wrap()
                .child(Label::new("Authenticate as").color(Color::Muted));
            for (i, identity) in self.req.identities.iter().enumerate() {
                row = row.child(
                    Button::new(("polkit-identity", i), identity.name.clone())
                        .style(if i == self.selected {
                            ButtonStyle::Tinted(TintColor::Accent)
                        } else {
                            ButtonStyle::Outlined
                        })
                        .on_click(cx.listener(move |this, _, _, cx| this.select(i, cx))),
                );
            }
            modal = modal.child(row);
        } else if let Some(identity) = self.req.identities.first() {
            modal = modal.child(
                Label::new(format!("Authenticate as {}", identity.name)).color(Color::Muted),
            );
        }

        let (label, echo) = self
            .prompt
            .clone()
            .unwrap_or_else(|| ("Password:".to_string(), false));
        let line = if echo {
            render_input_line(&self.input, "", cx)
        } else {
            render_masked_input_line(&self.input, "", '•', cx)
        };
        modal = modal.child(
            v_flex()
                .gap(Spacing::XSmall.pixels())
                .child(Label::new(label.trim().to_string()).size(TextSize::Small))
                .child(
                    div()
                        .w_full()
                        .px(Spacing::Medium.pixels())
                        .py(Spacing::Small.pixels())
                        .bg(field_bg)
                        .rounded(Radius::Small.pixels())
                        .border_1()
                        .border_color(border)
                        .text_size(TextSize::Small.rems())
                        .text_color(text)
                        .child(line),
                ),
        );
        if let Some(error) = &self.error {
            modal = modal.child(Label::new(error.clone()).color(Color::Error));
        }
        if let Some(info) = &self.info {
            modal = modal.child(Label::new(info.clone()).color(Color::Muted));
        }
        if self.checking {
            modal = modal.child(Label::new("Checking…").color(Color::Muted));
        }

        let waiting = self.session.is_some() && self.prompt.is_none();
        let cancel = Button::new("polkit-cancel", "Cancel")
            .style(ButtonStyle::Outlined)
            .on_click(cx.listener(|this, _, _, cx| this.close(AuthOutcome::Cancelled, cx)));
        let authenticate = Button::new("polkit-authenticate", "Authenticate")
            .style(ButtonStyle::Tinted(TintColor::Accent))
            .disabled(waiting)
            .on_click(cx.listener(|this, _, _, cx| this.submit(cx)));
        let modal = modal.footer(
            h_flex()
                .justify_end()
                .gap(Spacing::Small.pixels())
                .child(cancel)
                .child(authenticate),
        );

        let entity = cx.entity();
        div()
            .size_full()
            .track_focus(&self.focus)
            .key_context(KEY_CONTEXT)
            .on_action(
                cx.listener(|this, _: &Cancel, _, cx| this.close(AuthOutcome::Cancelled, cx)),
            )
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.submit(cx)))
            .on_action(
                cx.listener(|this, _: &Backspace, _, cx| this.edit(cx, InputBuffer::backspace)),
            )
            .on_action(cx.listener(|this, _: &DeleteWordBack, _, cx| {
                this.edit(cx, InputBuffer::delete_word_back)
            }))
            .on_action(
                cx.listener(|this, _: &CursorLeft, _, cx| this.edit(cx, |i| i.move_left(false))),
            )
            .on_action(
                cx.listener(|this, _: &CursorRight, _, cx| this.edit(cx, |i| i.move_right(false))),
            )
            .on_action(
                cx.listener(|this, _: &WordLeft, _, cx| this.edit(cx, |i| i.move_word_left(false))),
            )
            .on_action(
                cx.listener(|this, _: &WordRight, _, cx| {
                    this.edit(cx, |i| i.move_word_right(false))
                }),
            )
            .on_action(cx.listener(|this, _: &SelectWordLeft, _, cx| {
                this.edit(cx, |i| i.move_word_left(true))
            }))
            .on_action(cx.listener(|this, _: &SelectWordRight, _, cx| {
                this.edit(cx, |i| i.move_word_right(true))
            }))
            .on_action(
                cx.listener(|this, _: &SelectAll, _, cx| this.edit(cx, InputBuffer::select_all)),
            )
            .on_action(
                cx.listener(|this, _: &SelectLeft, _, cx| this.edit(cx, |i| i.move_left(true))),
            )
            .on_action(
                cx.listener(|this, _: &SelectRight, _, cx| this.edit(cx, |i| i.move_right(true))),
            )
            .on_key_down(cx.listener(Self::on_key_down))
            .child(modal_overlay(self.focus.clone(), modal, move |_, cx| {
                // Backdrop click cancels.
                entity.update(cx, |this, cx| this.close(AuthOutcome::Cancelled, cx));
            }))
    }
}
