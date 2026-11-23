//! Displays: resolution, refresh rate, scale, rotation, arrangement, and
//! profiles that put a whole setup back.

use gpui::{App, AppContext, Context, Entity, SharedString};
use services::Monitor;
use ui::{Color, IconName};

use super::displays_model::{DisplayModel, State};
use super::prompt_page::PromptPage;
use super::settings_form as rows;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Detail, EmptyView, Form, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};

pub struct DisplaysPage {
    model: Entity<DisplayModel>,
}

impl DisplaysPage {
    pub fn open(cx: &mut App) -> Page {
        Page::new(cx.new(Self::new))
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        let model = cx.new(DisplayModel::new);
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        Self { model }
    }
}

const SCALES: [f64; 7] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

pub(super) fn rotation_label(transform: u16) -> &'static str {
    match transform {
        0 => "Normal",
        1 => "90°",
        2 => "180°",
        3 => "270°",
        4..=7 => "Flipped",
        _ => "Normal",
    }
}

fn mode_text(m: &Monitor) -> String {
    if m.width == 0 {
        "Off".to_string()
    } else {
        format!("{}×{} @ {:.0} Hz", m.width, m.height, m.refresh_rate)
    }
}

fn title_of(m: &Monitor) -> String {
    let model = [m.make.as_str(), m.model.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    if model.is_empty() {
        m.name.clone()
    } else {
        format!("{} — {}", m.name, model)
    }
}

impl LauncherPage for DisplaysPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let model = self.model.clone();
        let m = model.read(cx);

        let mut list = List::new()
            .show_detail(true)
            .click_selects(true)
            .title("Displays & Monitors")
            .placeholder("Search displays…");

        if !m.supported() {
            return list
                .empty(
                    EmptyView::new("Not available on this compositor")
                        .description("Mango has no way to change display settings from outside.")
                        .icon(IconName::ScreenShare),
                )
                .into();
        }
        let monitors = match &m.state {
            State::Loading => return list.loading(true).into(),
            State::Failed(err) => {
                return list
                    .empty(
                        EmptyView::new("Could not read the displays")
                            .description(err.clone())
                            .icon(IconName::ScreenShare),
                    )
                    .into();
            }
            State::Ready(monitors) => monitors.clone(),
        };
        if monitors.is_empty() {
            return list
                .empty(
                    EmptyView::new("No displays detected")
                        .description("The compositor reported no video outputs.")
                        .icon(IconName::ScreenShare),
                )
                .into();
        }

        // 1. The countdown, first, so Enter keeps the setup.
        if let Some(revert) = &m.revert {
            let (keep, undo) = (model.clone(), model.clone());
            list = list.section(Section::new("Keep these settings?").item(
                Item::new("revert", format!("Reverting in {} s", revert.seconds_left))
                    .subtitle("If the screen looks right, keep the new settings")
                    .icon(IconName::Timer)
                    .actions(
                        ActionPanel::new()
                            .action(
                                Action::new("Keep settings", move |_, cx| {
                                    keep.update(cx, |m, cx| m.keep(cx));
                                    Outcome::Stay
                                })
                                .icon(IconName::Check),
                            )
                            .action(
                                Action::new("Revert now", move |_, cx| {
                                    undo.update(cx, |m, cx| m.revert_now(cx));
                                    Outcome::Stay
                                })
                                .icon(IconName::RotateCcw),
                            ),
                    )
                    .detail(Detail::new().markdown(
                        "The display settings changed. Unless you keep them, the previous ones come back on their own.",
                    )),
            ));
        }

        // 2. The displays.
        let others_on = monitors.iter().filter(|m| !m.is_disabled).count();
        let mut items = Vec::new();
        for mon in &monitors {
            let name = mon.name.clone();
            let mut item = Item::new(format!("display-{name}"), title_of(mon))
                .icon(IconName::ScreenShare)
                .subtitle(format!(
                    "{} • {:.2}× • {}",
                    mode_text(mon),
                    mon.scale,
                    rotation_label(mon.transform)
                ))
                .keywords(format!("{} {} {} display monitor", mon.name, mon.make, mon.model));
            if mon.is_focused {
                item = item.accessory(Accessory::tag("Focused").color(Color::Success));
            }
            if mon.is_disabled {
                item = item.accessory(Accessory::tag("Off").color(Color::Muted));
            }
            if mon.vrr_enabled {
                item = item.accessory(Accessory::tag("VRR").color(Color::Warning));
            }

            let mut actions = ActionPanel::new().action(
                Action::new("Edit", |_, _| Outcome::FocusDetail).icon(IconName::ChevronRight),
            );
            if !mon.is_disabled {
                let (model, name, next) = (model.clone(), name.clone(), (mon.transform + 1) % 4);
                actions = actions.action(
                    Action::new(
                        format!("Rotate to {}", rotation_label(next)),
                        move |_, cx| {
                            model.update(cx, |m, cx| {
                                m.change_monitor(&name, true, |c, _| c.transform = next, cx)
                            });
                            Outcome::Stay
                        },
                    )
                    .icon(IconName::RotateCw),
                );
            }
            // The last screen on cannot be turned off.
            if mon.is_disabled || others_on > 1 {
                let (model, name, on) = (model.clone(), name.clone(), mon.is_disabled);
                actions = actions.action(
                    Action::new(
                        if on { "Turn on" } else { "Turn off" },
                        move |_, cx| {
                            model.update(cx, |m, cx| {
                                m.change_monitor(&name, true, |c, _| c.enabled = on, cx)
                            });
                            Outcome::Stay
                        },
                    )
                    .icon(IconName::Power),
                );
            }

            let detail_model = model.clone();
            items.push(item.actions(actions).detail(Detail::new().form(move |form, cx| {
                monitor_form(form, &detail_model, &name, cx)
            })));
        }
        list = list.section(Section::new("Displays").items(items));

        // 3. The profiles.
        list = list.section(Section::new("Profiles").items(profile_items(&model, &monitors, cx)));
        list.into()
    }
}

fn profile_items(model: &Entity<DisplayModel>, monitors: &[Monitor], cx: &App) -> Vec<Item> {
    let m = model.read(cx);
    let mut items = Vec::new();

    let next_name = {
        let mut n = m.profiles.len() + 1;
        while m.profiles.iter().any(|p| p.name == format!("Setup {n}")) {
            n += 1;
        }
        format!("Setup {n}")
    };
    let save_model = model.clone();
    items.push(
        Item::new("save-profile", "Save current setup as a profile…")
            .icon(IconName::Save)
            .subtitle("Remember these displays, their modes, scales and arrangement")
            .keywords("profile save preset layout")
            .actions(ActionPanel::new().action(
                Action::new("Save profile…", move |_, cx| {
                    let model = save_model.clone();
                    PromptPage::ask(
                        cx,
                        "Save display profile",
                        "Name this setup…",
                        "Save as",
                        next_name.clone(),
                        move |name| {
                            if name.len() > 60 {
                                Err("Keep the name under 60 characters".into())
                            } else {
                                Ok(())
                            }
                        },
                        move |name, cx| {
                            model.update(cx, |m, cx| m.save_profile(&name, cx));
                            Outcome::Pop
                        },
                    )
                })
                .icon(IconName::Plus),
            ))
            .detail(Detail::new().markdown(
                "# Profiles\n\nA profile remembers every display's mode, scale, rotation and position, \
                 and which monitors were connected.\n\n\
                 Profiles are applied automatically when exactly those monitors are connected again, \
                 in whichever port, and when the shell starts. Turn that off per profile.",
            )),
    );

    for profile in &m.profiles {
        let name = profile.name.clone();
        let fits = profile.fits(monitors);
        let applied = fits && profile.is_applied(monitors);
        let mut item = Item::new(format!("profile-{name}"), name.clone())
            .icon(IconName::Layers)
            .subtitle(format!(
                "{} display{}{}",
                profile.outputs.len(),
                if profile.outputs.len() == 1 { "" } else { "s" },
                if profile.auto { " • applied automatically" } else { "" }
            ))
            .keywords(format!("profile {name}"));
        if applied {
            item = item.accessory(Accessory::tag("Active").color(Color::Success));
        } else if !fits {
            item = item.accessory(Accessory::tag("other displays").color(Color::Muted));
        }

        let mut actions = ActionPanel::new();
        if fits {
            let (model, name) = (model.clone(), name.clone());
            actions = actions.action(
                Action::new("Apply profile", move |_, cx| {
                    model.update(cx, |m, cx| m.apply_profile(&name, cx));
                    Outcome::Stay
                })
                .icon(IconName::Check),
            );
        }
        {
            let (model, name, auto) = (model.clone(), name.clone(), profile.auto);
            actions = actions.action(
                Action::new(
                    if auto { "Stop applying automatically" } else { "Apply automatically" },
                    move |_, cx| {
                        model.update(cx, |m, cx| m.set_profile_auto(&name, !auto, cx));
                        Outcome::Stay
                    },
                )
                .icon(IconName::Refresh),
            );
        }
        {
            let (model, name) = (model.clone(), name.clone());
            actions = actions.action(
                Action::new("Update with the current setup", move |_, cx| {
                    model.update(cx, |m, cx| m.save_profile(&name, cx));
                    Outcome::Stay
                })
                .icon(IconName::Save),
            );
        }
        {
            let (model, name) = (model.clone(), name.clone());
            actions = actions.action(
                Action::new("Delete profile", move |_, cx| {
                    model.update(cx, |m, cx| m.delete_profile(&name, cx));
                    Outcome::Stay
                })
                .icon(IconName::Trash)
                .destructive(),
            );
        }

        let mut text = format!("# {name}\n\n");
        for o in &profile.outputs {
            let c = &o.config;
            if c.enabled {
                text.push_str(&format!(
                    "- **{}** ({}) — {}, {:.2}×, {}, at {}, {}\n",
                    c.output,
                    o.identity,
                    c.mode
                        .as_ref()
                        .map_or("preferred mode".to_string(), |m| m.label()),
                    c.scale,
                    rotation_label(c.transform),
                    format_args!("{}×{}", c.x, c.y),
                    if c.vrr { "VRR on" } else { "VRR off" },
                ));
            } else {
                text.push_str(&format!("- **{}** ({}) — off\n", c.output, o.identity));
            }
        }
        if !fits {
            text.push_str("\nThe monitors connected now are not the ones this profile was made for.");
        }
        items.push(item.actions(actions).detail(Detail::new().markdown(text)));
    }
    items
}

// -- one display ---------------------------------------------------------------

/// A display's size in the compositor's coordinates: scaled and turned.
pub(super) fn logical_size(m: &Monitor) -> (i32, i32) {
    let scale = if m.scale > 0.0 { f64::from(m.scale) } else { 1.0 };
    let (w, h) = (f64::from(m.width) / scale, f64::from(m.height) / scale);
    let (w, h) = if matches!(m.transform, 1 | 3 | 5 | 7) { (h, w) } else { (w, h) };
    (w.round() as i32, h.round() as i32)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Side {
    RightOf,
    LeftOf,
    Above,
    Below,
}

impl Side {
    const ALL: [Side; 4] = [Self::RightOf, Self::LeftOf, Self::Above, Self::Below];

    fn label(self) -> &'static str {
        match self {
            Self::RightOf => "Right of",
            Self::LeftOf => "Left of",
            Self::Above => "Above",
            Self::Below => "Below",
        }
    }
}

/// Where `me` sits when put on `side` of `other`, touching it.
pub(super) fn place(me: &Monitor, other: &Monitor, side: Side) -> (i32, i32) {
    let (mw, mh) = logical_size(me);
    let (ow, oh) = logical_size(other);
    match side {
        Side::RightOf => (other.x + ow, other.y),
        Side::LeftOf => (other.x - mw, other.y),
        Side::Above => (other.x, other.y - mh),
        Side::Below => (other.x, other.y + oh),
    }
}

fn monitor_form(form: &mut Form, model: &Entity<DisplayModel>, name: &str, cx: &App) {
    let m = model.read(cx);
    let Some(mon) = m.monitor(name) else {
        rows::title(form, name, "This display is not connected any more");
        return;
    };
    rows::title(
        form,
        &title_of(mon),
        "Changes apply at once; keep them within 15 seconds or they are undone",
    );

    let on_off = {
        let (model, name) = (model.clone(), name.to_string());
        move |on: bool, cx: &mut App| {
            let name = name.clone();
            model.update(cx, |m, cx| {
                m.change_monitor(&name, true, move |c, _| c.enabled = on, cx)
            });
        }
    };
    let others_on = m
        .monitors()
        .iter()
        .filter(|o| o.name != mon.name && !o.is_disabled)
        .count();
    if mon.is_disabled || others_on > 0 {
        rows::toggle(
            form,
            format!("disp-on-{name}"),
            "Display on",
            "Turn this screen off or back on",
            !mon.is_disabled,
            on_off,
            cx,
        );
    } else {
        rows::note(form, "This is the only display on, so it cannot be turned off.", Color::Muted);
    }
    if mon.is_disabled {
        rows::note(form, "Turn the display on to change its settings.", Color::Muted);
        footer(form, m);
        return;
    }

    // Resolution and refresh rate.
    rows::heading(form, "Resolution");
    if mon.available_modes.is_empty() {
        rows::note(
            form,
            "The compositor did not list the modes of this display.",
            Color::Muted,
        );
    } else {
        let sizes = mon.resolutions();
        let labels: Vec<SharedString> = sizes
            .iter()
            .map(|(w, h)| SharedString::from(format!("{w} × {h}")))
            .collect();
        let selected = sizes.iter().position(|&(w, h)| (w, h) == (mon.width, mon.height));
        let pick_size = {
            let (model, name, sizes) = (model.clone(), name.to_string(), sizes.clone());
            move |ix: usize, cx: &mut App| {
                let Some(&(w, h)) = sizes.get(ix) else { return };
                let name = name.clone();
                model.update(cx, |m, cx| {
                    m.change_monitor(
                        &name,
                        true,
                        move |c, mon| {
                            // The same refresh rate when the new size has it.
                            let rates = mon.refresh_rates(w, h);
                            let keep = c
                                .mode
                                .as_ref()
                                .and_then(|m| rates.iter().find(|r| (**r - m.refresh_rate).abs() < 0.05));
                            let rate = keep.or(rates.first()).copied().unwrap_or(60.0);
                            c.mode = Some(services::OutputMode {
                                width: w,
                                height: h,
                                refresh_rate: rate,
                            });
                        },
                        cx,
                    )
                });
            }
        };
        rows::choice(
            form,
            format!("disp-res-{name}"),
            "Resolution",
            "The size of the picture in pixels; the largest is the sharpest",
            labels,
            selected,
            pick_size,
            cx,
        );

        let rates = mon.refresh_rates(mon.width, mon.height);
        let rate_labels: Vec<SharedString> = rates
            .iter()
            .map(|r| SharedString::from(format!("{r:.2} Hz")))
            .collect();
        let rate_selected = rates.iter().position(|r| (r - mon.refresh_rate).abs() < 0.05);
        let pick_rate = {
            let (model, name, rates) = (model.clone(), name.to_string(), rates.clone());
            move |ix: usize, cx: &mut App| {
                let Some(&rate) = rates.get(ix) else { return };
                let name = name.clone();
                model.update(cx, |m, cx| {
                    m.change_monitor(
                        &name,
                        true,
                        move |c, mon| {
                            c.mode = Some(services::OutputMode {
                                width: mon.width,
                                height: mon.height,
                                refresh_rate: rate,
                            });
                        },
                        cx,
                    )
                });
            }
        };
        rows::choice(
            form,
            format!("disp-rate-{name}"),
            "Refresh rate",
            "How many times a second the picture is redrawn",
            rate_labels,
            rate_selected,
            pick_rate,
            cx,
        );
    }

    // Scale and rotation.
    rows::heading(form, "Size & orientation");
    let scale_selected = SCALES
        .iter()
        .position(|s| (s - f64::from(mon.scale)).abs() < 0.01);
    rows::choice(
        form,
        format!("disp-scale-{name}"),
        "Scale",
        "Makes everything bigger on a dense screen. Fractions may blur older apps",
        SCALES
            .iter()
            .map(|s| SharedString::from(format!("{}%", (s * 100.0).round() as i32)))
            .collect(),
        scale_selected,
        {
            let (model, name) = (model.clone(), name.to_string());
            move |ix, cx| {
                let name = name.clone();
                model.update(cx, |m, cx| {
                    m.change_monitor(&name, true, move |c, _| c.scale = SCALES[ix], cx)
                });
            }
        },
        cx,
    );
    rows::choice(
        form,
        format!("disp-rot-{name}"),
        "Rotation",
        "Turn the picture for a screen that stands on its side",
        (0..4).map(|t| SharedString::from(rotation_label(t))).collect(),
        (mon.transform < 4).then_some(mon.transform as usize),
        {
            let (model, name) = (model.clone(), name.to_string());
            move |ix, cx| {
                let name = name.clone();
                model.update(cx, |m, cx| {
                    m.change_monitor(&name, true, move |c, _| c.transform = ix as u16, cx)
                });
            }
        },
        cx,
    );

    // Arrangement.
    let neighbours: Vec<&Monitor> = m
        .monitors()
        .iter()
        .filter(|o| o.name != mon.name && !o.is_disabled)
        .collect();
    if !neighbours.is_empty() {
        rows::heading(form, "Arrangement");
        let mut labels: Vec<SharedString> = Vec::new();
        let mut spots: Vec<(i32, i32)> = Vec::new();
        for other in &neighbours {
            for side in Side::ALL {
                labels.push(format!("{} {}", side.label(), other.name).into());
                spots.push(place(mon, other, side));
            }
        }
        let selected = spots.iter().position(|&(x, y)| (x, y) == (mon.x, mon.y));
        rows::choice(
            form,
            format!("disp-pos-{name}"),
            "Position",
            "Where this screen sits next to the others; the pointer crosses between them there",
            labels,
            selected,
            {
                let (model, name) = (model.clone(), name.to_string());
                move |ix, cx| {
                    let Some(&(x, y)) = spots.get(ix) else { return };
                    let name = name.clone();
                    model.update(cx, |m, cx| {
                        m.change_monitor(
                            &name,
                            true,
                            move |c, _| {
                                c.x = x;
                                c.y = y;
                            },
                            cx,
                        )
                    });
                }
            },
            cx,
        );
        if selected.is_none() {
            rows::note(
                form,
                format!("Now at {}, {}: not next to another screen", mon.x, mon.y),
                Color::Muted,
            );
        }
    }

    // Variable refresh rate.
    rows::heading(form, "Adaptive sync");
    if mon.vrr_supported {
        rows::toggle(
            form,
            format!("disp-vrr-{name}"),
            "Variable refresh rate",
            "The screen waits for each frame: smoother games and video",
            mon.vrr_enabled,
            {
                let (model, name) = (model.clone(), name.to_string());
                move |on, cx| {
                    let name = name.clone();
                    // Harmless: nothing to confirm.
                    model.update(cx, |m, cx| {
                        m.change_monitor(&name, false, move |c, _| c.vrr = on, cx)
                    });
                }
            },
            cx,
        );
    } else {
        rows::note(form, "This display does not support variable refresh rate.", Color::Muted);
    }
    footer(form, m);
}

fn footer(form: &mut Form, m: &DisplayModel) {
    if let Some(notice) = &m.notice {
        rows::note(
            form,
            notice.text.clone(),
            if notice.error { Color::Error } else { Color::Muted },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(name: &str, w: u32, h: u32, scale: f32, transform: u16, x: i32, y: i32) -> Monitor {
        Monitor {
            name: name.into(),
            width: w,
            height: h,
            scale,
            transform,
            x,
            y,
            ..Default::default()
        }
    }

    #[test]
    fn logical_size_is_scaled_and_turned() {
        assert_eq!(logical_size(&mon("a", 2880, 1800, 2.0, 0, 0, 0)), (1440, 900));
        assert_eq!(logical_size(&mon("a", 1920, 1080, 1.0, 1, 0, 0)), (1080, 1920));
        assert_eq!(logical_size(&mon("a", 3840, 2160, 1.5, 0, 0, 0)), (2560, 1440));
    }

    #[test]
    fn a_screen_is_placed_touching_its_neighbour() {
        let laptop = mon("eDP-1", 2880, 1800, 2.0, 0, 0, 0);
        let ext = mon("DP-1", 1920, 1080, 1.0, 0, 0, 0);
        assert_eq!(place(&ext, &laptop, Side::RightOf), (1440, 0));
        assert_eq!(place(&ext, &laptop, Side::LeftOf), (-1920, 0));
        assert_eq!(place(&ext, &laptop, Side::Above), (0, -1080));
        assert_eq!(place(&ext, &laptop, Side::Below), (0, 900));
    }
}
