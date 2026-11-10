//! What the user can do with an item or a view, and where the launcher goes
//! after. Enter runs the first action, Shift+Enter the second, Tab opens them
//! all.

use std::process::Command;
use std::rc::Rc;

use gpui::{App, AppContext, ClipboardItem, Context, Keystroke, SharedString, Window};
use ui::IconName;

use super::content::Detail;
use super::page::{DetailPage, LauncherPage, Page};

/// Where the launcher goes after an action.
pub enum Outcome {
    Close,
    Stay,
    /// Show `page` on top; Esc or Backspace on an empty search comes back.
    Push(Page),
    /// `Push`, with the search starting as `input`, selected, so typing replaces it.
    PushWithInput(Page, String),
    Pop,
    PopToRoot,
    /// Replace the whole input, prefix included. Leaves pushed pages first.
    SetQuery(String),
}

impl Outcome {
    pub fn push<P: LauncherPage>(page: gpui::Entity<P>) -> Self {
        Outcome::Push(Page::new(page))
    }

    pub fn push_with_input<P: LauncherPage>(
        page: gpui::Entity<P>,
        input: impl Into<String>,
    ) -> Self {
        Outcome::PushWithInput(Page::new(page), input.into())
    }
}

type Handler = Rc<dyn Fn(&mut Window, &mut App) -> Outcome>;

#[derive(Clone)]
pub struct Action {
    pub(super) title: SharedString,
    pub(super) icon: Option<IconName>,
    pub(super) shortcut: Option<Keystroke>,
    pub(super) destructive: bool,
    pub(super) handler: Handler,
}

impl Action {
    pub fn new(
        title: impl Into<SharedString>,
        handler: impl Fn(&mut Window, &mut App) -> Outcome + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            icon: None,
            shortcut: None,
            destructive: false,
            handler: Rc::new(handler),
        }
    }

    /// Run `handler` on the page that built the action.
    pub fn listener<V: 'static>(
        title: impl Into<SharedString>,
        cx: &Context<V>,
        handler: impl Fn(&mut V, &mut Window, &mut Context<V>) -> Outcome + 'static,
    ) -> Self {
        let page = cx.weak_entity();
        Self::new(title, move |window, cx| {
            page.update(cx, |page, cx| handler(page, window, cx))
                .unwrap_or(Outcome::Stay)
        })
    }

    /// Push the page `build` makes, built only when the action runs.
    pub fn push<P: LauncherPage>(
        title: impl Into<SharedString>,
        build: impl Fn(&mut Context<P>) -> P + 'static,
    ) -> Self {
        Self::new(title, move |_, cx| Outcome::push(cx.new(&build))).icon(IconName::ChevronRight)
    }

    /// Push a page showing the detail `build` makes.
    pub fn push_detail(
        title: impl Into<SharedString>,
        build: impl Fn(&mut App) -> Detail + 'static,
    ) -> Self {
        Self::new(title, move |_, cx| {
            let detail = build(cx);
            Outcome::push(cx.new(|_| DetailPage(detail)))
        })
        .icon(IconName::Info)
    }

    pub fn copy(text: impl Into<SharedString>) -> Self {
        let text = text.into();
        Self::new("Copy to Clipboard", move |_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
            Outcome::Close
        })
        .icon(IconName::Copy)
    }

    /// Open a URL or path with `xdg-open`.
    pub fn open(target: impl Into<SharedString>) -> Self {
        let target = target.into();
        Self::new("Open", move |_, _| {
            let mut open = Command::new("xdg-open");
            open.arg(target.as_ref());
            if let Err(err) = services::spawn_reaped(open, "xdg-open") {
                tracing::warn!("Failed to open {target}: {err}");
            }
            Outcome::Close
        })
        .icon(IconName::ExternalLink)
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A gpui keystroke such as `"ctrl-r"`. Bindings the launcher already
    /// uses (`ctrl-k`, `ctrl-a`, arrows, ...) win over it.
    pub fn shortcut(mut self, keystroke: &'static str) -> Self {
        match Keystroke::parse(keystroke) {
            Ok(keystroke) => self.shortcut = Some(keystroke),
            Err(err) => tracing::warn!("Bad shortcut {keystroke:?} on {}: {err}", self.title),
        }
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub(super) fn run(&self, window: &mut Window, cx: &mut App) -> Outcome {
        (self.handler)(window, cx)
    }

    pub(super) fn matches(&self, typed: &Keystroke) -> bool {
        self.shortcut
            .as_ref()
            .is_some_and(|s| s.modifiers == typed.modifiers && s.key == typed.key)
    }

    /// `ctrl-r` as `Ctrl R`, for the footer and the actions panel.
    pub(super) fn shortcut_label(&self) -> Option<SharedString> {
        let keystroke = self.shortcut.as_ref()?;
        let m = keystroke.modifiers;
        let mut parts: Vec<String> = [
            (m.control, "Ctrl"),
            (m.alt, "Alt"),
            (m.shift, "Shift"),
            (m.platform, "Super"),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, name)| name.to_string())
        .collect();
        parts.push(keystroke.key.to_uppercase());
        Some(parts.join(" ").into())
    }
}

#[derive(Clone)]
pub(super) struct ActionSection {
    pub title: Option<SharedString>,
    pub actions: Vec<Action>,
}

/// An item's or a view's actions, in sections.
#[derive(Clone, Default)]
pub struct ActionPanel {
    pub(super) sections: Vec<ActionSection>,
}

impl ActionPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add to the last section.
    pub fn action(mut self, action: Action) -> Self {
        match self.sections.last_mut() {
            Some(section) => section.actions.push(action),
            None => self.sections.push(ActionSection {
                title: None,
                actions: vec![action],
            }),
        }
        self
    }

    pub fn section(
        mut self,
        title: impl Into<SharedString>,
        actions: impl IntoIterator<Item = Action>,
    ) -> Self {
        self.sections.push(ActionSection {
            title: Some(title.into()),
            actions: actions.into_iter().collect(),
        });
        self
    }

    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &Action> {
        self.sections.iter().flat_map(|section| &section.actions)
    }

    pub(super) fn get(&self, ix: usize) -> Option<&Action> {
        self.iter().nth(ix)
    }
}
