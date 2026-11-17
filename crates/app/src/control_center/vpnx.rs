//! conchy-vpnx section: state, mode, proxy groups and latency.

use gpui::{App, Div, MouseButton, SharedString, div, prelude::*, px};
use services::{ProxyGroup, VpnxCommand, VpnxData, VpnxMode, VpnxState, VpnxSubscriber};
use ui::{ActiveTheme, IconName, IconSize, Radius, Spacing, Spinner, TextSize};

use crate::state::AppState;

use super::list;
use super::tooltip::control_center_tooltip;

/// Latency above this is shown as a warning.
const SLOW_MS: u32 = 300;

pub fn render_vpnx_section(cx: &App) -> impl IntoElement {
    let service = AppState::vpnx(cx);
    let data = service.get();
    let pending = service.pending();
    let busy = pending.iter().any(|c| {
        matches!(
            c,
            VpnxCommand::Up | VpnxCommand::Down | VpnxCommand::SetMode(_)
        )
    });
    let updating = pending.contains(&VpnxCommand::UpdateSubscription);

    let header = {
        let service = service.clone();
        move |header: Div| {
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
        }
    };

    list::section(IconName::Globe, "VPNX", header, cx)
        .child(render_status(&data, busy, cx))
        .child(render_modes(data.mode, busy, service, cx))
        .when_some(render_tun_hint(&data, &pending, service, cx), |el, hint| {
            el.child(hint)
        })
        .when(data.state == VpnxState::Up, |el| {
            el.children(
                data.groups
                    .iter()
                    .enumerate()
                    .map(|(i, group)| render_group(i, group, &data, service, cx)),
            )
        })
        .when(
            data.state == VpnxState::Up && data.groups.is_empty(),
            |el| el.child(list::empty_state("No proxy groups", cx)),
        )
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
