use gpui::{App, FontWeight, div, prelude::*, px};
use services::Monitor;
use ui::patterns::PanelSurface;
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, TextSize,
    h_flex, v_flex,
};

use crate::launcher::content::Detail;
use crate::state::AppState;

pub fn displays_detail(_cx: &App) -> Detail {
    Detail::new().widget(move |cx| {
        let theme = cx.theme();
        let colors = theme.colors();
        let compositor = AppState::compositor(cx).get();
        let monitors = &compositor.monitors;

        let header = v_flex()
            .w_full()
            .gap(Spacing::Small.pixels())
            .child(
                h_flex()
                    .w_full()
                    .gap(Spacing::Small.pixels())
                    .items_center()
                    .child(Icon::new(IconName::Layout).size(IconSize::Medium))
                    .child(
                        Label::new("Displays & Monitors")
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
                                Label::new(format!("{} Connected", monitors.len()))
                                    .size(TextSize::XSmall)
                                    .color(Color::Custom(colors.status.success)),
                            ),
                    ),
            )
            .child(
                Label::new("Resolution, refresh rates, scaling, orientation and layout")
                    .size(TextSize::Small)
                    .color(Color::Muted),
            );

        // Visual layout preview canvas
        let topology_card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Small.pixels())
            .panel_card(cx)
            .child(
                Label::new("Display Arrangement")
                    .size(TextSize::Small)
                    .weight(FontWeight::BOLD),
            )
            .child(render_topology_canvas(monitors, cx));

        let monitors_card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Small.pixels())
            .panel_card(cx)
            .child(
                Label::new("Connected Outputs")
                    .size(TextSize::Small)
                    .weight(FontWeight::BOLD),
            )
            .child(div().h(px(1.)).bg(colors.border_variant).w_full())
            .children(monitors.iter().map(|m| render_monitor_row(m, cx)));

        v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Large.pixels())
            .child(header)
            .child(topology_card)
            .child(monitors_card)
            .into_any_element()
    })
}

/// Miniature interactive layout diagram representing connected monitors in coordinate space
fn render_topology_canvas(monitors: &[Monitor], cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();

    if monitors.is_empty() {
        return div()
            .h(px(80.))
            .flex()
            .items_center()
            .justify_center()
            .child(Label::new("No displays detected").color(Color::Muted));
    }

    // Find bounds to normalize to a 240x120 canvas
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    for m in monitors {
        min_x = min_x.min(m.x);
        min_y = min_y.min(m.y);
        max_x = max_x.max(m.x + m.width as i32);
        max_y = max_y.max(m.y + m.height as i32);
    }

    let total_w = (max_x - min_x).max(1) as f32;
    let total_h = (max_y - min_y).max(1) as f32;

    let canvas_w = 260.0f32;
    let canvas_h = 100.0f32;

    let scale_factor = (canvas_w / total_w).min(canvas_h / total_h);

    div()
        .w_full()
        .h(px(canvas_h + 24.))
        .bg(colors.elevated_surface_background)
        .rounded(Radius::Medium.pixels())
        .border_1()
        .border_color(colors.border_variant)
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .children(monitors.iter().map(|m| {
            let left = (m.x - min_x) as f32 * scale_factor + 12.;
            let top = (m.y - min_y) as f32 * scale_factor + 12.;
            let w = (m.width as f32 * scale_factor).max(36.);
            let h = (m.height as f32 * scale_factor).max(24.);

            let is_focused = m.is_focused;

            div()
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(w))
                .h(px(h))
                .rounded(Radius::Small.pixels())
                .bg(if is_focused {
                    colors.accent
                } else {
                    colors.element_background
                })
                .border_1()
                .border_color(if is_focused {
                    colors.accent
                } else {
                    colors.border
                })
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Label::new(m.name.clone())
                        .size(TextSize::XSmall)
                        .weight(FontWeight::BOLD)
                        .color(if is_focused {
                            Color::Custom(colors.status.success)
                        } else {
                            Color::Custom(colors.text)
                        }),
                )
        }))
}

fn render_monitor_row(m: &Monitor, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let transform_str = match m.transform {
        1 => "Rotated 90°",
        2 => "Rotated 180°",
        3 => "Rotated 270°",
        _ => "Normal (0°)",
    };

    let title = if m.model.is_empty() {
        m.name.clone()
    } else {
        format!("{} ({})", m.name, m.model)
    };

    let mode_str = format!(
        "{}x{} @ {:.1} Hz",
        m.width,
        m.height,
        if m.refresh_rate > 0.0 {
            m.refresh_rate
        } else {
            60.0
        }
    );

    v_flex()
        .w_full()
        .py(Spacing::Small.pixels())
        .gap(px(2.))
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(
                    Label::new(title)
                        .size(TextSize::Small)
                        .weight(FontWeight::BOLD),
                )
                .child(
                    h_flex()
                        .gap(Spacing::XSmall.pixels())
                        .when(m.is_focused, |el| {
                            el.child(
                                div()
                                    .px(Spacing::Small.pixels())
                                    .rounded(Radius::Small.pixels())
                                    .bg(colors.element_background)
                                    .child(
                                        Label::new("Focused")
                                            .size(TextSize::XSmall)
                                            .color(Color::Custom(colors.status.success)),
                                    ),
                            )
                        })
                        .when(m.vrr_enabled, |el| {
                            el.child(
                                div()
                                    .px(Spacing::Small.pixels())
                                    .rounded(Radius::Small.pixels())
                                    .bg(colors.element_background)
                                    .child(
                                        Label::new("VRR")
                                            .size(TextSize::XSmall)
                                            .color(Color::Custom(colors.status.warning)),
                                    ),
                            )
                        }),
                ),
        )
        .child(
            h_flex()
                .justify_between()
                .child(Label::new(mode_str).size(TextSize::XSmall).color(Color::Muted))
                .child(
                    Label::new(format!(
                        "Scale: {:.2}x • {} • Pos: ({}, {})",
                        m.scale, transform_str, m.x, m.y
                    ))
                    .size(TextSize::XSmall)
                    .color(Color::Muted),
                ),
        )
}
