//! Keyboard shortcuts of the window manager: the Super key, and one row per
//! bind to rebind, change or remove, plus a way to add one.
//!
//! Keys and actions are typed in the search line, like any other value. The
//! compositor grabs its own binds before this window sees them, so recording a
//! key press would not work for the keys that matter most.

use gpui::{AppContext, Context, WeakEntity};
use services::LocalSlot;
use ui::{Color, IconName};

use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, EmptyView, Item, List, Section};
use crate::launcher::page::{LauncherPage, Page};
use crate::shortcuts::{self, Bind, LAUNCHER_ACTION, Shortcuts, Store};
use crate::state::AppState;

/// Actions worth offering by name: what, and the KDL the compositor takes.
const CATALOG: &[(&str, &str, IconName)] = &[
    (
        "Open the launcher",
        LAUNCHER_ACTION,
        IconName::MagnifyingGlass,
    ),
    (
        "Open the control center",
        r#"spawn "conchy-shell" "toggle-control-center""#,
        IconName::Sliders,
    ),
    ("Show the overview", "toggle-overview", IconName::Layout),
    ("Close the window", "close-window", IconName::Close),
    (
        "Fullscreen the window",
        "fullscreen-window",
        IconName::Maximize,
    ),
    ("Maximize the column", "maximize-column", IconName::Maximize),
    (
        "Float or tile the window",
        "toggle-window-floating",
        IconName::Layers,
    ),
    (
        "Minimize or restore the window",
        "toggle-window-minimized",
        IconName::Minimize,
    ),
    (
        "Focus the column to the left",
        "focus-column-left",
        IconName::ArrowLeft,
    ),
    (
        "Focus the column to the right",
        "focus-column-right",
        IconName::ArrowRight,
    ),
    (
        "Focus the window above",
        "focus-window-up",
        IconName::ArrowUp,
    ),
    (
        "Focus the window below",
        "focus-window-down",
        IconName::ArrowDown,
    ),
    (
        "Move the column left",
        "move-column-left",
        IconName::ArrowLeft,
    ),
    (
        "Move the column right",
        "move-column-right",
        IconName::ArrowRight,
    ),
    (
        "Go to the workspace above",
        "focus-workspace-up",
        IconName::ArrowUp,
    ),
    (
        "Go to the workspace below",
        "focus-workspace-down",
        IconName::ArrowDown,
    ),
    (
        "Switch the column width",
        "switch-preset-column-width",
        IconName::ChevronsLeftRight,
    ),
    ("Center the column", "center-column", IconName::Layout),
    (
        "Turn the monitors off",
        "power-off-monitors",
        IconName::Power,
    ),
    ("Log out", "quit", IconName::Power),
];

/// The page: the Super key up top, then every shortcut.
pub struct ShortcutsPage {
    store: Store,
    shortcuts: Shortcuts,
    /// Why the file could not be read, when it could not.
    load_error: Option<String>,
    /// `wm.kdl` has includes, whose binds this list cannot show.
    includes: bool,
}

impl ShortcutsPage {
    pub fn open(cx: &mut gpui::App) -> Page {
        Page::new(cx.new(|_| Self::load()))
    }

    fn load() -> Self {
        let store = match Store::standard() {
            Ok(store) => store,
            Err(err) => {
                return Self {
                    store: Store::new("."),
                    shortcuts: Shortcuts::default(),
                    load_error: Some(format!("{err:#}")),
                    includes: false,
                };
            }
        };
        let (shortcuts, load_error) = match store.load() {
            Ok(shortcuts) => (shortcuts, None),
            Err(err) => (Shortcuts::default(), Some(format!("{err:#}"))),
        };
        let includes = !shortcuts.owned && shortcuts::wm_has_includes(&store);
        Self {
            store,
            shortcuts,
            load_error,
            includes,
        }
    }

    /// Try `change` on a copy; keep it only if the compositor would accept it and
    /// it could be written. The reason otherwise, for the caller to show.
    fn apply(
        &mut self,
        change: impl FnOnce(&mut Shortcuts) -> Result<(), String>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let mut next = self.shortcuts.clone();
        change(&mut next)?;
        for bind in &next.binds {
            shortcuts::check(bind).map_err(|err| format!("{}: {err}", bind.key))?;
        }
        shortcuts::validate_with_wm(&self.store, &next)?;
        self.store.save(&next).map_err(|err| format!("{err:#}"))?;
        next.owned = true;
        self.shortcuts = next;
        self.includes = false;
        cx.notify();
        Ok(())
    }

    /// [`apply`](Self::apply), telling the user in a notification if it fails, for
    /// actions that have no input line to show the reason in.
    fn apply_or_notify(
        &mut self,
        change: impl FnOnce(&mut Shortcuts) -> Result<(), String>,
        cx: &mut Context<Self>,
    ) -> Outcome {
        if let Err(reason) = self.apply(change, cx) {
            AppState::notification(cx).post_local(
                LocalSlot::ConfigWriteError,
                "Shortcut was not saved".into(),
                reason,
            );
        }
        Outcome::Stay
    }

    fn launcher_tap_state(&self) -> TapState {
        match self.shortcuts.tap() {
            None => TapState::Off,
            Some(bind) if bind.action.trim() == LAUNCHER_ACTION => TapState::Launcher,
            Some(_) => TapState::Other,
        }
    }

    fn super_rows(&self, cx: &Context<Self>) -> Vec<Item> {
        let state = self.launcher_tap_state();
        let mut rows = Vec::new();

        let (tag, color) = match state {
            TapState::Launcher => ("on", Color::Accent),
            TapState::Off => ("off", Color::Muted),
            TapState::Other => ("custom", Color::Warning),
        };
        let subtitle = match state {
            TapState::Other => format!(
                "Super does something else now: {}",
                self.shortcuts.tap().map_or("", |b| b.action.as_str())
            ),
            _ => "Tap Super, on its own, to open the launcher. Holding it and dragging still moves windows".into(),
        };
        rows.push(
            Item::new("super", "Super opens the launcher")
                .subtitle(subtitle)
                .icon(IconName::Keyboard)
                .keywords("super windows key launcher tap")
                .accessory(Accessory::tag(tag).color(color))
                .action(
                    Action::listener(
                        if state == TapState::Launcher {
                            "Turn off"
                        } else {
                            "Turn on"
                        },
                        cx,
                        move |page, _, cx| {
                            page.apply_or_notify(
                                |s| {
                                    if state == TapState::Launcher {
                                        s.remove(shortcuts::SUPER_KEY);
                                    } else {
                                        s.put(Bind::launcher_tap(true));
                                    }
                                    Ok(())
                                },
                                cx,
                            )
                        },
                    )
                    .icon(IconName::Check),
                ),
        );

        if state == TapState::Launcher {
            let over = self.shortcuts.tap().is_none_or(Bind::works_in_fullscreen);
            rows.push(
                Item::new("super-fullscreen", "Also over fullscreen apps")
                    .subtitle(if over {
                        "Super opens the launcher even while a game or video is fullscreen"
                    } else {
                        "In a fullscreen app Super goes to the app"
                    })
                    .icon(IconName::Maximize)
                    .keywords("super fullscreen game video")
                    .accessory(
                        Accessory::tag(if over { "on" } else { "off" }).color(if over {
                            Color::Accent
                        } else {
                            Color::Muted
                        }),
                    )
                    .action(
                        Action::listener("Toggle", cx, move |page, _, cx| {
                            page.apply_or_notify(
                                |s| {
                                    let mut tap = Bind::launcher_tap(!over);
                                    if let Some(old) = s.tap() {
                                        tap.key = old.key.clone();
                                    }
                                    s.put(tap);
                                    Ok(())
                                },
                                cx,
                            )
                        })
                        .icon(IconName::Check),
                    ),
            );
        }
        rows
    }

    fn bind_row(&self, bind: &Bind, cx: &Context<Self>) -> Item {
        let key = bind.key.clone();
        let title = if bind.is_tap() {
            format!("Tap {}", bind.key)
        } else {
            bind.key.clone()
        };
        let mut row = Item::new(format!("bind:{}", bind.key), title)
            .subtitle(describe(&bind.action))
            .icon(if bind.is_tap() {
                IconName::Keyboard
            } else {
                IconName::Hash
            })
            .keywords(format!("{} {}", bind.key, bind.action));
        if !bind.works_in_fullscreen() {
            row = row.accessory(Accessory::tag("not in fullscreen"));
        }

        let page = cx.weak_entity();
        let (page_a, page_b) = (page.clone(), page.clone());
        let (key_a, key_b) = (key.clone(), key.clone());
        let action = bind.action.clone();
        let panel = ActionPanel::new()
            .action(
                Action::new("Change key", move |_, cx| {
                    let target = Target::Rebind(key_a.clone());
                    Outcome::push_with_input(
                        cx.new(|_| KeyPage::new(page_a.clone(), target)),
                        key_a.clone(),
                    )
                })
                .icon(IconName::Keyboard),
            )
            .action(
                Action::new("Change action", move |_, cx| {
                    let target = Target::Rebind(key_b.clone());
                    Outcome::push_with_input(
                        cx.new(|_| ActionPage::new(page_b.clone(), target)),
                        action.clone(),
                    )
                })
                .icon(IconName::Pencil),
            )
            .action(
                Action::listener("Remove", cx, move |page, _, cx| {
                    page.apply_or_notify(
                        |s| {
                            s.remove(&key);
                            Ok(())
                        },
                        cx,
                    )
                })
                .icon(IconName::Trash)
                .shortcut("ctrl-x")
                .destructive(),
            );
        row.actions(panel)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TapState {
    Off,
    Launcher,
    Other,
}

/// `spawn "alacritty"` as a sentence, when it helps; the KDL as it is otherwise.
fn describe(action: &str) -> String {
    match CATALOG.iter().find(|(_, kdl, _)| *kdl == action) {
        Some((label, _, _)) => format!("{label} ({action})"),
        None => action.to_string(),
    }
}

impl LauncherPage for ShortcutsPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let mut list = List::new()
            .title("Shortcuts")
            .placeholder("Search shortcuts…");

        if let Some(reason) = &self.load_error {
            return list
                .empty(
                    EmptyView::new("Shortcuts could not be read")
                        .description(reason.clone())
                        .icon(IconName::CircleAlert),
                )
                .into();
        }

        list = list.section(Section::new("Super key").items(self.super_rows(cx)));

        let page = cx.weak_entity();
        let note = if self.includes {
            "wm.kdl includes other files. Their binds are not listed, and stop applying once you edit here"
        } else if !self.shortcuts.owned {
            "The first change copies these binds from wm.kdl to shortcuts.kdl"
        } else {
            "Saved to shortcuts.kdl"
        };
        let rows = self
            .shortcuts
            .binds
            .iter()
            .filter(|bind| !(bind.is_tap() && bind.key.eq_ignore_ascii_case(shortcuts::SUPER_KEY)))
            .map(|bind| self.bind_row(bind, cx))
            .chain(std::iter::once(
                Item::new("add", "Add a shortcut…")
                    .icon(IconName::Plus)
                    .subtitle("Pick a key, then what it does")
                    .action(
                        Action::new("Choose a key", move |_, cx| {
                            let page = page.clone();
                            Outcome::push(cx.new(|_| KeyPage::new(page, Target::New)))
                        })
                        .icon(IconName::ChevronRight),
                    ),
            ));
        list = list.section(Section::new("Shortcuts").subtitle(note).items(rows));
        list.into()
    }
}

// ============================================================================
// Key
// ============================================================================

/// What a key or action page is for.
#[derive(Clone)]
enum Target {
    /// A new bind: the key page leads on to the action page.
    New,
    /// An existing bind, named by its key.
    Rebind(String),
}

/// Type a key; Enter takes it.
struct KeyPage {
    page: WeakEntity<ShortcutsPage>,
    target: Target,
}

impl KeyPage {
    fn new(page: WeakEntity<ShortcutsPage>, target: Target) -> Self {
        Self { page, target }
    }
}

impl LauncherPage for KeyPage {
    fn content(&mut self, query: &str, cx: &mut Context<Self>) -> Content {
        let list = List::new()
            .filtering(false)
            .title(match self.target {
                Target::New => "New shortcut: key",
                Target::Rebind(_) => "Change key",
            })
            .placeholder("Mod+Shift+T, Print, Super…");

        let key = query.trim().to_string();
        if let Err(reason) = shortcuts::parse_key(&key) {
            return list
                .empty(
                    EmptyView::new(reason)
                        .description("Modifiers: Mod, Ctrl, Shift, Alt, Super. Mod is Super on a real screen. A modifier alone is a tap")
                        .icon(IconName::Keyboard),
                )
                .into();
        }

        // A key that is already bound, other than the one being changed.
        let taken = self.page.read_with(cx, |page, _| {
            let own = match &self.target {
                Target::Rebind(old) => Some(old.as_str()),
                Target::New => None,
            };
            page.shortcuts
                .find(&key)
                .filter(|bind| !own.is_some_and(|own| shortcuts::same_key(own, &bind.key)))
                .map(|bind| describe(&bind.action))
        });
        let taken = taken.ok().flatten();

        let weak = self.page.clone();
        let target = self.target.clone();
        let chosen = key.clone();
        let action = match &target {
            Target::New => Action::new("Next: what it does", move |_, cx| {
                let (page, target, key) = (weak.clone(), target.clone(), chosen.clone());
                Outcome::push(cx.new(|_| ActionPage::new_with_key(page, target, key)))
            })
            .icon(IconName::ChevronRight),
            Target::Rebind(old) => {
                let (weak, old, chosen) = (weak.clone(), old.clone(), chosen.clone());
                Action::new("Set key", move |_, cx| {
                    let result = weak.update(cx, |page, cx| {
                        page.apply(
                            |s| {
                                let mut bind = s
                                    .find(&old)
                                    .cloned()
                                    .ok_or_else(|| format!("{old} is not bound any more"))?;
                                bind.key = chosen.clone();
                                s.replace(&old, bind)
                            },
                            cx,
                        )
                    });
                    match result {
                        Ok(Ok(())) => Outcome::Pop,
                        Ok(Err(reason)) => {
                            AppState::notification(cx).post_local(
                                LocalSlot::ConfigWriteError,
                                "Shortcut was not saved".into(),
                                reason,
                            );
                            Outcome::Stay
                        }
                        Err(_) => Outcome::Pop,
                    }
                })
                .icon(IconName::Check)
            }
        };

        // A key that is taken is not offered again: replacing a bind by accident is
        // not a surprise to give. Rebinding to the same key is a no-op, not a clash.
        if let Some(existing) = taken {
            return list
                .empty(
                    EmptyView::new(format!("{key} is already bound"))
                        .description(existing)
                        .icon(IconName::Keyboard),
                )
                .into();
        }

        let mut item = Item::new("use", format!("Use {key}")).icon(IconName::Check);
        if shortcuts::parse_key(&key).is_ok_and(|k| k.tap) {
            item = item.subtitle("Fires when the modifier is pressed and released on its own");
        }
        list.item(item.actions(ActionPanel::new().action(action)))
            .into()
    }
}

// ============================================================================
// Action
// ============================================================================

/// Type or pick the action; Enter takes it.
struct ActionPage {
    page: WeakEntity<ShortcutsPage>,
    target: Target,
    /// The key of a new bind, once the key page has set it.
    key: Option<String>,
}

impl ActionPage {
    fn new(page: WeakEntity<ShortcutsPage>, target: Target) -> Self {
        Self {
            page,
            target,
            key: None,
        }
    }

    fn new_with_key(page: WeakEntity<ShortcutsPage>, target: Target, key: String) -> Self {
        Self {
            page,
            target,
            key: Some(key),
        }
    }

    /// The action that saves `kdl` as this page's bind.
    fn save_action(&self, kdl: String) -> Action {
        let weak = self.page.clone();
        let target = self.target.clone();
        let new_key = self.key.clone();
        Action::new("Use this action", move |_, cx| {
            let (target_key, is_new) = match (&target, &new_key) {
                (Target::Rebind(old), _) => (old.clone(), false),
                (Target::New, Some(key)) => (key.clone(), true),
                (Target::New, None) => return Outcome::Pop,
            };
            let kdl = kdl.clone();
            let result = weak.update(cx, |page, cx| {
                page.apply(
                    |s| {
                        if is_new {
                            if s.find(&target_key).is_some() {
                                return Err(format!("{target_key} is already bound"));
                            }
                            s.put(Bind::new(target_key.clone(), kdl));
                        } else {
                            let mut bind = s
                                .find(&target_key)
                                .cloned()
                                .ok_or_else(|| format!("{target_key} is not bound any more"))?;
                            bind.action = kdl;
                            s.put(bind);
                        }
                        Ok(())
                    },
                    cx,
                )
            });
            match result {
                // New: leave the action page and the key page.
                Ok(Ok(())) if is_new => Outcome::PopMany(2),
                Ok(Ok(())) => Outcome::Pop,
                Ok(Err(reason)) => {
                    AppState::notification(cx).post_local(
                        LocalSlot::ConfigWriteError,
                        "Shortcut was not saved".into(),
                        reason,
                    );
                    Outcome::Stay
                }
                Err(_) => Outcome::PopMany(if is_new { 2 } else { 1 }),
            }
        })
        .icon(IconName::Check)
    }
}

impl LauncherPage for ActionPage {
    fn content(&mut self, query: &str, _cx: &mut Context<Self>) -> Content {
        let title = match (&self.target, &self.key) {
            (Target::New, Some(key)) => format!("{key} does…"),
            (Target::Rebind(key), _) => format!("{key} does…"),
            (Target::New, None) => "Action".to_string(),
        };
        let mut list = List::new()
            .title(title)
            .filtering(false)
            .placeholder("Pick one, or type: spawn \"alacritty\"");

        let typed = query.trim();
        let mut items = Vec::new();

        // What was typed, as it is, when it is an action.
        if !typed.is_empty() && !CATALOG.iter().any(|(_, kdl, _)| *kdl == typed) {
            match shortcuts::check(&Bind::new("Mod+X", typed)) {
                Ok(()) => items.push(
                    Item::new("typed", format!("Use {typed}"))
                        .icon(IconName::Pencil)
                        .subtitle("Exactly as typed")
                        .actions(ActionPanel::new().action(self.save_action(typed.to_string()))),
                ),
                Err(reason) if catalog_matches(typed) == 0 => {
                    return list
                        .empty(
                            EmptyView::new(reason)
                                .description("An action of conchy-wm, like close-window, or spawn \"program\" \"arg\"")
                                .icon(IconName::CircleAlert),
                        )
                        .into();
                }
                Err(_) => {}
            }
        }

        let needle = typed.to_lowercase();
        for (label, kdl, icon) in CATALOG {
            if needle.is_empty()
                || label.to_lowercase().contains(&needle)
                || kdl.to_lowercase().contains(&needle)
            {
                items.push(
                    Item::new(*kdl, *label)
                        .icon(*icon)
                        .subtitle(*kdl)
                        .actions(ActionPanel::new().action(self.save_action(kdl.to_string()))),
                );
            }
        }

        list = list.section(Section::untitled().items(items));
        list.into()
    }
}

/// How many catalog entries a typed text narrows down to.
fn catalog_matches(typed: &str) -> usize {
    let needle = typed.to_lowercase();
    CATALOG
        .iter()
        .filter(|(label, kdl, _)| {
            label.to_lowercase().contains(&needle) || kdl.to_lowercase().contains(&needle)
        })
        .count()
}
