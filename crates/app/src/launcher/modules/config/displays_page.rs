use gpui::{App, Context, prelude::*};
use services::CompositorCommand;
use ui::{
    ActiveTheme, Color, IconName,
};

use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, EmptyView, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};
use crate::state::{AppState, watch};

pub struct DisplaysPage;

impl DisplaysPage {
    pub fn open(cx: &mut App) -> Page {
        Page::new(cx.new(|cx| Self::new(cx)))
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        let compositor = AppState::compositor(cx);
        watch(cx, compositor.subscribe(), |_, _, cx| {
            cx.notify();
        });
        Self
    }
}

impl LauncherPage for DisplaysPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let service = AppState::compositor(cx);
        let data = service.get();
        let monitors = data.monitors.clone();

        let mut list = List::new()
            .show_detail(true)
            .click_selects(true)
            .title("Displays & Monitors")
            .placeholder("Search displays…");

        if monitors.is_empty() {
            list = list.empty(
                EmptyView::new("No displays detected")
                    .description("The compositor reported 0 active video outputs.")
                    .icon(IconName::Layout),
            );
            return list.into();
        }

        let mut monitor_items = Vec::new();
        for (idx, m) in monitors.into_iter().enumerate() {
            let output_name = m.name.clone();
            let current_scale = m.scale;
            let current_transform = m.transform;
            let vrr_on = m.vrr_enabled;

            let rot_label = match current_transform {
                1 => "90°",
                2 => "180°",
                3 => "270°",
                _ => "Normal 0°",
            };

            let title = if m.model.is_empty() {
                m.name.clone()
            } else {
                format!("{} ({})", m.name, m.model)
            };

            let subtitle = format!(
                "{}x{} @ {:.1} Hz • Scale: {:.2}x • Orientation: {}",
                m.width, m.height, if m.refresh_rate > 0.0 { m.refresh_rate } else { 60.0 },
                current_scale, rot_label
            );

            let mut item = Item::new(format!("display-{idx}"), title)
                .subtitle(subtitle)
                .icon(IconName::Layout)
                .accessory(Accessory::text(format!("{:.2}x", current_scale)));

            if m.is_focused {
                item = item.accessory(Accessory::tag("Focused").color(Color::Custom(
                    cx.theme().colors().status.success,
                )));
            }

            if m.vrr_enabled {
                item = item.accessory(Accessory::tag("VRR").color(Color::Custom(
                    cx.theme().colors().status.warning,
                )));
            }

            // Next scale: 1.0 -> 1.25 -> 1.5 -> 1.75 -> 2.0 -> 1.0
            let next_scale = if (current_scale - 1.0).abs() < 0.05 {
                1.25
            } else if (current_scale - 1.25).abs() < 0.05 {
                1.5
            } else if (current_scale - 1.5).abs() < 0.05 {
                1.75
            } else if (current_scale - 1.75).abs() < 0.05 {
                2.0
            } else {
                1.0
            };

            // Next transform: 0 -> 1 -> 2 -> 3 -> 0
            let next_transform = (current_transform + 1) % 4;

            let s = service.clone();
            let out_name = output_name.clone();

            let item_actions = ActionPanel::default()
                .action(Action::new(
                    format!("Change Scale to {:.2}x", next_scale),
                    {
                        let s = s.clone();
                        let o = out_name.clone();
                        move |_, _| {
                            s.dispatch(CompositorCommand::SetOutputScale {
                                output: o.clone(),
                                scale: next_scale,
                            });
                            Outcome::Stay
                        }
                    },
                ))
                .action(Action::new(
                    format!("Rotate Output (next: {}°)", next_transform * 90),
                    {
                        let s = s.clone();
                        let o = out_name.clone();
                        move |_, _| {
                            s.dispatch(CompositorCommand::SetOutputTransform {
                                output: o.clone(),
                                transform: next_transform,
                            });
                            Outcome::Stay
                        }
                    },
                ))
                .action(Action::new(
                    if vrr_on {
                        "Disable Variable Refresh Rate (VRR)"
                    } else {
                        "Enable Variable Refresh Rate (VRR)"
                    },
                    {
                        let s = s.clone();
                        let o = out_name.clone();
                        move |_, _| {
                            s.dispatch(CompositorCommand::SetOutputVrr {
                                output: o.clone(),
                                enabled: !vrr_on,
                            });
                            Outcome::Stay
                        }
                    },
                ));

            monitor_items.push(item.actions(item_actions));
        }

        list = list.section(Section::new("Connected Outputs").items(monitor_items));
        list.into()
    }
}
