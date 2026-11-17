//! conchy-vpnx section: subscription, state, mode, proxy groups and latency.
//!
//! An entity, like the WiFi section, because it owns the subscription link
//! editor. It lives only while the section is expanded.

use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, App, Context, Div, FocusHandle, KeyDownEvent, MouseButton, SharedString, Window,
    div, prelude::*, px,
};
use services::{
    ProxyGroup, SubscriptionInfo, VpnxCommand, VpnxData, VpnxMode, VpnxState, VpnxSubscriber,
};
use ui::{
    ActiveTheme, IconName, IconSize, InputBuffer, Radius, Spacing, Spinner, TextSize,
    render_input_line,
};

use crate::keybinds::{
    Backspace, Cancel, Confirm, CursorLeft, CursorRight, DeleteWordBack, SelectAll, SelectLeft,
    SelectRight, SelectWordLeft, SelectWordRight, WordLeft, WordRight,
};
use crate::state::{AppState, watch};

use super::list;
use super::tooltip::control_center_tooltip;

/// Latency above this is shown as a warning.
const SLOW_MS: u32 = 300;

pub struct VpnxSection {
    focus_handle: FocusHandle,
    /// Gets focus back when the editor closes.
    parent_focus: FocusHandle,
    /// Present while the subscription link is being edited.
    link: Option<InputBuffer>,
    /// Applied on the next render, which has the window.
    pending_focus: Option<PendingFocus>,
}

enum PendingFocus {
    Link,
    Parent,
}

impl VpnxSection {
    pub fn new(parent_focus: FocusHandle, cx: &mut Context<Self>) -> Self {
        let service = AppState::vpnx(cx).clone();
        // The selected proxy can change from the CLI without a signal.
        if service.get().state == VpnxState::Up {
            service.dispatch(VpnxCommand::RefreshGroups);
        }
        watch(cx, service.subscribe(), |_, _, cx| cx.notify());
        watch(cx, service.pending_signal(), |_, _, cx| cx.notify());
        Self {
            focus_handle: cx.focus_handle(),
            parent_focus,
            link: None,
            pending_focus: None,
        }
    }

    fn open_editor(&mut self, cx: &mut Context<Self>) {
        // Never prefill: the URL carries the account's secret token.
        self.link = Some(InputBuffer::default());
        self.pending_focus = Some(PendingFocus::Link);
        cx.notify();
    }

    fn close_editor(&mut self, cx: &mut Context<Self>) {
        if self.link.take().is_some() {
            self.pending_focus = Some(PendingFocus::Parent);
        }
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.link.as_ref().map(|l| l.text().trim().to_string()) else {
            return;
        };
        if url.is_empty() {
            return;
        }
        AppState::vpnx(cx).dispatch(VpnxCommand::SetSubscription(url));
        self.close_editor(cx);
    }

    fn edit(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut InputBuffer)) {
        if let Some(link) = &mut self.link {
            edit(link);
            cx.notify();
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        // Links are pasted, not typed.
        if keystroke.modifiers.control && keystroke.key.eq_ignore_ascii_case("v") {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                let text: String = text.split_whitespace().collect();
                self.edit(cx, |link| link.insert_str(&text));
            }
            return;
        }
        if keystroke.modifiers.control || keystroke.modifiers.alt {
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
        if let Some(ch) = ch.filter(|ch| !ch.is_control() && !ch.is_whitespace()) {
            self.edit(cx, |link| link.insert_str(&ch.to_string()));
        }
    }

    /// Title, traffic and expiry, or the link editor.
    fn render_subscription(
        &self,
        data: &VpnxData,
        setting: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(link) = &self.link {
            return self.render_editor(link, cx);
        }
        let theme = cx.theme();
        let sub = &data.subscription;
        let hover = theme.colors.element_hover;
        let button = |id: &'static str, label: &'static str, cx: &mut Context<Self>| {
            div()
                .id(id)
                .px(Spacing::Medium.pixels())
                .py(px(2.))
                .rounded(Radius::Small.pixels())
                .border_1()
                .border_color(cx.theme().colors.border_variant)
                .text_color(cx.theme().colors.text)
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.open_editor(cx)),
                )
                .child(label)
        };

        let (title, details) = if setting {
            ("Downloading subscription...".to_string(), None)
        } else if !sub.configured {
            (
                "No subscription".to_string(),
                Some("Add the link from your provider".to_string()),
            )
        } else {
            (sub.name().to_string(), subscription_details(sub))
        };
        let title_color = if sub.configured || setting {
            theme.colors.text
        } else {
            theme.colors.text_muted
        };
        let text_muted = theme.colors.text_muted;
        let warning = theme.colors.status.warning;

        div()
            .flex()
            .flex_col()
            .gap(Spacing::XSmall.pixels())
            .text_size(TextSize::XSmall.rems())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(Spacing::Small.pixels())
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_color(title_color)
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(title),
                            )
                            .when_some(details, |el, details| {
                                el.child(div().text_color(text_muted).child(details))
                            }),
                    )
                    .when(setting, |el| {
                        el.child(Spinner::new().size(IconSize::XSmall))
                    })
                    .when(!setting, |el| {
                        el.child(if sub.configured {
                            button("vpnx-change-link", "Change", cx)
                        } else {
                            button("vpnx-add-link", "Add", cx)
                        })
                    }),
            )
            .when_some(sub.error.clone().filter(|_| !setting), |el, err| {
                el.child(
                    div()
                        .id("vpnx-subscription-error")
                        .text_color(warning)
                        .child("Last update failed, using the previous profile")
                        .tooltip(control_center_tooltip(err)),
                )
            })
            .into_any_element()
    }

    fn render_editor(&self, link: &InputBuffer, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let line = render_input_line(link, "https://...", cx);
        let can_save = !link.text().trim().is_empty();
        let hover = theme.colors.element_hover;
        let accent = theme.colors.accent;
        let border = theme.colors.border_variant;
        let text = theme.colors.text;
        let text_muted = theme.colors.text_muted;
        let background = theme.colors.background;

        div()
            .id("vpnx-link")
            .track_focus(&self.focus_handle)
            .key_context("VpnxLink")
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
                cx.listener(|this, _: &SelectLeft, _, cx| this.edit(cx, |i| i.move_left(true))),
            )
            .on_action(
                cx.listener(|this, _: &SelectRight, _, cx| this.edit(cx, |i| i.move_right(true))),
            )
            .on_action(
                cx.listener(|this, _: &SelectAll, _, cx| this.edit(cx, InputBuffer::select_all)),
            )
            .on_action(cx.listener(|this, _: &Cancel, _, cx| this.close_editor(cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.submit(cx)))
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, _, cx| this.on_key_down(event, cx)),
            )
            .flex()
            .flex_col()
            .gap(Spacing::Small.pixels())
            .text_size(TextSize::XSmall.rems())
            .child(
                div()
                    .text_color(text_muted)
                    .child("Subscription link (Ctrl+V to paste)"),
            )
            .child(
                div()
                    .w_full()
                    .px(Spacing::Medium.pixels())
                    .py(Spacing::Small.pixels())
                    .bg(background)
                    .rounded(Radius::Small.pixels())
                    .border_1()
                    .border_color(accent)
                    .overflow_hidden()
                    .text_size(TextSize::Small.rems())
                    .text_color(text)
                    .child(line),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(Spacing::Small.pixels())
                    .child(
                        div()
                            .id("vpnx-link-cancel")
                            .px(Spacing::Medium.pixels())
                            .py(px(2.))
                            .rounded(Radius::Small.pixels())
                            .border_1()
                            .border_color(border)
                            .text_color(text_muted)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.close_editor(cx)),
                            )
                            .child("Cancel"),
                    )
                    .child(
                        div()
                            .id("vpnx-link-save")
                            .px(Spacing::Medium.pixels())
                            .py(px(2.))
                            .rounded(Radius::Small.pixels())
                            .border_1()
                            .border_color(if can_save { accent } else { border })
                            .text_color(if can_save { text } else { text_muted })
                            .when(can_save, |el| {
                                el.cursor_pointer()
                                    .hover(move |s| s.bg(hover))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.submit(cx)),
                                    )
                            })
                            .child("Save"),
                    ),
            )
            .into_any_element()
    }
}

impl Render for VpnxSection {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.pending_focus.take() {
            Some(PendingFocus::Link) => self.focus_handle.focus(window, cx),
            Some(PendingFocus::Parent) => self.parent_focus.focus(window, cx),
            None => {}
        }

        let service = AppState::vpnx(cx).clone();
        let data = service.get();
        let pending = service.pending();
        let busy = pending.iter().any(|c| {
            matches!(
                c,
                VpnxCommand::Up | VpnxCommand::Down | VpnxCommand::SetMode(_)
            )
        });
        let updating = pending.contains(&VpnxCommand::UpdateSubscription);
        let setting = pending
            .iter()
            .any(|c| matches!(c, VpnxCommand::SetSubscription(_)));
        let subscription = self.render_subscription(&data, setting, cx);

        let cx: &App = cx;
        let header = {
            let service = service.clone();
            let configured = data.subscription.configured;
            move |header: Div| {
                header.when(configured, |header| {
                    header.child(list::action_button(
                        "vpnx-update-subscription",
                        if updating {
                            IconName::LoaderCircle
                        } else {
                            IconName::Download
                        },
                        cx.theme().colors.text_muted,
                        "Update subscription",
                        move |_| service.dispatch(VpnxCommand::UpdateSubscription),
                        cx,
                    ))
                })
            }
        };

        list::section(IconName::Globe, "VPNX", header, cx)
            .child(subscription)
            .child(render_status(&data, busy, cx))
            .child(render_modes(data.mode, busy, &service, cx))
            .when_some(
                render_tun_hint(&data, &pending, &service, cx),
                |el, hint| el.child(hint),
            )
            .when(data.state == VpnxState::Up, |el| {
                el.children(
                    data.groups
                        .iter()
                        .enumerate()
                        .map(|(i, group)| render_group(i, group, &data, &service, cx)),
                )
            })
            .when(
                data.state == VpnxState::Up && data.groups.is_empty(),
                |el| el.child(list::empty_state("No proxy groups", cx)),
            )
    }
}

/// "281 GB used · 22 days left · updated 2 h ago".
fn subscription_details(sub: &SubscriptionInfo) -> Option<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let mut parts = Vec::new();
    match sub.limit() {
        Some(total) => parts.push(format!("{} of {}", size(sub.used()), size(total))),
        None if sub.used() > 0 => parts.push(format!("{} used", size(sub.used()))),
        None => {}
    }
    if let Some(expire) = sub.expires() {
        parts.push(if expire <= now {
            "expired".to_string()
        } else {
            format!("{} left", span(expire - now))
        });
    }
    if let Some(at) = sub.updated_at {
        parts.push(format!("updated {} ago", span(now.saturating_sub(at))));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn size(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let gb = bytes as f64 / GB;
    if gb >= 10.0 {
        format!("{gb:.0} GB")
    } else if gb >= 0.1 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", bytes as f64 / 1024.0 / 1024.0)
    }
}

fn span(secs: u64) -> String {
    match secs {
        s if s < 60 => "<1 min".to_string(),
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 2 * 86400 => format!("{} h", s / 3600),
        s => format!("{} days", s / 86400),
    }
}

/// "Connected · TUN", errors and merge warnings.
fn render_status(data: &VpnxData, busy: bool, cx: &App) -> Div {
    let theme = cx.theme();
    let (text, color) = match data.state {
        VpnxState::Up => (
            match data.effective_mode {
                Some(mode) => format!("Connected · {}", mode.label()),
                None => "Connected".to_string(),
            },
            theme.colors.status.success,
        ),
        VpnxState::Starting => ("Starting...".to_string(), theme.colors.text_muted),
        VpnxState::Restarting => (
            "mihomo crashed, restarting...".to_string(),
            theme.colors.status.warning,
        ),
        VpnxState::Error => (data.last_error.clone(), theme.colors.status.error),
        VpnxState::Down => ("Off".to_string(), theme.colors.text_muted),
    };
    let spinning = busy || matches!(data.state, VpnxState::Starting | VpnxState::Restarting);

    div()
        .flex()
        .flex_col()
        .gap(Spacing::XSmall.pixels())
        .text_size(TextSize::XSmall.rems())
        .child(
            div()
                .flex()
                .items_center()
                .gap(Spacing::XSmall.pixels())
                .text_color(color)
                .child(div().flex_1().child(text))
                .when(spinning, |el| {
                    el.child(Spinner::new().size(IconSize::XSmall))
                }),
        )
        .when_some(data.command_error.clone(), |el, err| {
            el.child(div().text_color(theme.colors.status.error).child(err))
        })
        .when(!data.warnings.is_empty(), |el| {
            let summary = match data.warnings.len() {
                1 => "1 rule warning".to_string(),
                n => format!("{n} rule warnings"),
            };
            let details: SharedString = data.warnings.join("\n").into();
            el.child(
                div()
                    .id("vpnx-warnings")
                    .text_color(theme.colors.status.warning)
                    .child(summary)
                    .tooltip(control_center_tooltip(details)),
            )
        })
}

/// Why TUN is not in use, with a way to fix it.
fn render_tun_hint(
    data: &VpnxData,
    pending: &[VpnxCommand],
    service: &VpnxSubscriber,
    cx: &App,
) -> Option<Div> {
    let theme = cx.theme();
    if data.mihomo_path.is_empty() {
        return Some(
            div()
                .text_size(TextSize::XSmall.rems())
                .text_color(theme.colors.status.error)
                .child("mihomo is not installed"),
        );
    }
    if data.can_tun || data.mode == VpnxMode::Proxy {
        return None;
    }

    let granting = pending.contains(&VpnxCommand::GrantTun);
    let text = if data.mode == VpnxMode::Tun {
        "TUN needs permission"
    } else {
        "No TUN permission, using system proxy"
    };
    let service = service.clone();
    let hover = theme.colors.element_hover;
    Some(
        div()
            .flex()
            .items_center()
            .gap(Spacing::Small.pixels())
            .text_size(TextSize::XSmall.rems())
            .child(
                div()
                    .flex_1()
                    .text_color(theme.colors.status.warning)
                    .child(text),
            )
            .child(
                div()
                    .id("vpnx-grant-tun")
                    .px(Spacing::Medium.pixels())
                    .py(px(2.))
                    .rounded(Radius::Small.pixels())
                    .border_1()
                    .border_color(theme.colors.accent)
                    .text_color(theme.colors.text)
                    .tooltip(control_center_tooltip(
                        "Run setcap on mihomo (asks for your password)",
                    ))
                    .when(granting, |el| {
                        el.child(Spinner::new().size(IconSize::XSmall))
                    })
                    .when(!granting, |el| {
                        el.cursor_pointer()
                            .hover(move |s| s.bg(hover))
                            .on_mouse_down(MouseButton::Left, move |_, _, _| {
                                service.dispatch(VpnxCommand::GrantTun);
                            })
                            .child("Allow")
                    }),
            ),
    )
}

/// Auto / TUN / Proxy switch.
fn render_modes(current: VpnxMode, busy: bool, service: &VpnxSubscriber, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .gap(Spacing::XSmall.pixels())
        .children(VpnxMode::ALL.into_iter().map(|mode| {
            let selected = mode == current;
            let service = service.clone();
            let hover = theme.colors.element_hover;
            div()
                .id(SharedString::from(format!("vpnx-mode-{}", mode.as_str())))
                .flex_1()
                .flex()
                .justify_center()
                .py(Spacing::XSmall.pixels())
                .rounded(Radius::Small.pixels())
                .border_1()
                .text_size(TextSize::XSmall.rems())
                .when(selected, |el| {
                    el.border_color(theme.colors.accent)
                        .bg(theme.colors.element_selected)
                        .text_color(theme.colors.text)
                })
                .when(!selected, |el| {
                    el.border_color(theme.colors.border_variant)
                        .text_color(theme.colors.text_muted)
                })
                .when(!selected && !busy, |el| {
                    el.cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .on_mouse_down(MouseButton::Left, move |_, _, _| {
                            service.dispatch(VpnxCommand::SetMode(mode));
                        })
                })
                .tooltip(control_center_tooltip(match mode {
                    VpnxMode::Auto => "TUN when allowed, system proxy otherwise",
                    VpnxMode::Tun => "All traffic through a virtual interface",
                    VpnxMode::Proxy => "HTTP/SOCKS proxy for newly started apps",
                }))
                .child(mode.label())
        }))
}

/// Group title with a latency test button, then its members.
fn render_group(
    index: usize,
    group: &ProxyGroup,
    data: &VpnxData,
    service: &VpnxSubscriber,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let selectable = group.is_selectable();
    let test = {
        let service = service.clone();
        let name = group.name.clone();
        move |_: &mut App| service.dispatch(VpnxCommand::TestDelay(name.clone()))
    };

    div()
        .flex()
        .flex_col()
        .gap(Spacing::XSmall.pixels())
        .child(
            div()
                .flex()
                .items_center()
                .gap(Spacing::Small.pixels())
                .text_size(TextSize::XSmall.rems())
                .child(
                    div()
                        .text_color(theme.colors.text)
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(group.name.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .text_color(theme.colors.text_muted)
                        .child(group.kind.clone()),
                )
                .child(list::action_button(
                    format!("vpnx-test-{index}"),
                    IconName::Gauge,
                    theme.colors.text_muted,
                    "Test latency",
                    test,
                    cx,
                )),
        )
        .child(
            list::list(SharedString::from(format!("vpnx-group-{index}")), cx).children(
                group.all.iter().enumerate().map(|(j, proxy)| {
                    let selected = *proxy == group.now;
                    let delay = data.delays.get(proxy).copied();
                    render_member(
                        index, j, group, proxy, selected, selectable, delay, service, cx,
                    )
                }),
            ),
        )
}

#[allow(clippy::too_many_arguments)]
fn render_member(
    group_index: usize,
    index: usize,
    group: &ProxyGroup,
    proxy: &str,
    selected: bool,
    selectable: bool,
    delay: Option<Option<u32>>,
    service: &VpnxSubscriber,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let (delay_text, delay_color) = match delay {
        None => (String::new(), theme.colors.text_muted),
        Some(None) => ("timeout".to_string(), theme.colors.status.error),
        Some(Some(ms)) if ms > SLOW_MS => (format!("{ms} ms"), theme.colors.status.warning),
        Some(Some(ms)) => (format!("{ms} ms"), theme.colors.status.success),
    };

    list::row(format!("vpnx-{group_index}-{index}"), selected, cx)
        .when(selectable && !selected, |el| {
            let service = service.clone();
            let group = group.name.clone();
            let proxy = proxy.to_string();
            el.on_mouse_down(MouseButton::Left, move |_, _, _| {
                service.dispatch(VpnxCommand::Select {
                    group: group.clone(),
                    proxy: proxy.clone(),
                });
            })
        })
        .when(!selectable, |el| el.cursor_default())
        .child(list::row_label(proxy.to_string(), cx))
        .child(
            div()
                .w(px(56.))
                .text_right()
                .text_size(TextSize::XSmall.rems())
                .text_color(delay_color)
                .child(delay_text),
        )
}
