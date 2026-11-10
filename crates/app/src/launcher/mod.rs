//! Keyboard-driven launcher. The launcher owns the window, the input, the
//! page stack, the selection and the drawing; pages (`modules/`) describe
//! their [`Content`](content::Content) and actions.

pub mod action;
pub mod config;
pub mod content;
mod detail;
mod matcher;
pub mod modules;
pub mod page;
mod route;
pub mod row;

use std::ops::Range;

use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, ElementId, EntityId, FocusHandle, Focusable,
    MouseButton, Pixels, Point, ScrollHandle, SharedString, Size, Subscription, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, layer_shell::*,
    prelude::*, px, relative, size,
};
use ui::patterns::{LauncherFrame, footer_hints};
use ui::{
    ActiveTheme, Breadcrumb, BreadcrumbItem, Clickable, Color, Icon, IconSize, InputBuffer, Label,
    LabelCommon, Radius, ScrollStrategy, Scrollbar, Spacing, TextSize, ThumbnailCache, VirtualList,
    VirtualListScrollHandle, render_input_line,
};

use self::action::{Action, ActionPanel, Outcome};
use self::content::{
    Chrome, Collection, Content, Custom, Detail, Dropdown, EmptyView, GridLayout, Item, ListLayout,
};
use self::matcher::{Ranked, Ranker};
use self::page::{ModuleDescriptor, ModuleMeta, Page};
use self::route::Route;
use crate::config::Config;
use crate::keybinds::{
    Backspace, Cancel, Confirm, CursorDown, CursorLeft, CursorRight, CursorUp, DeleteWordBack,
    PageDown, PageUp, SecondaryConfirm, SelectAll, SelectLeft, SelectRight, SelectWordLeft,
    SelectWordRight, ToggleActions, WordLeft, WordRight,
};

/// Rows to jump on Page Up/Down.
const ROWS_PER_PAGE: usize = 7;
/// Share of the width the list keeps beside an item's detail.
const DETAIL_SPLIT: f32 = 0.40;

#[derive(Clone, Copy)]
enum Layout {
    List { show_detail: bool },
    Grid { columns: usize, aspect_ratio: f32 },
}

/// A page's content with the list/grid distinction folded into [`Layout`].
enum Body {
    Collection(Collection<Layout>),
    Detail(Detail),
    Custom(Custom),
}

impl From<Content> for Body {
    fn from(content: Content) -> Self {
        match content {
            Content::List(list) => Body::Collection(
                list.map_layout(|ListLayout { show_detail }| Layout::List { show_detail }),
            ),
            Content::Grid(grid) => Body::Collection(grid.map_layout(
                |GridLayout {
                     columns,
                     aspect_ratio,
                 }| Layout::Grid {
                    columns,
                    aspect_ratio,
                },
            )),
            Content::Detail(detail) => Body::Detail(detail),
            Content::Custom(custom) => Body::Custom(custom),
        }
    }
}

impl Body {
    fn chrome(&self) -> &Chrome {
        match self {
            Body::Collection(c) => &c.chrome,
            Body::Detail(d) => &d.chrome,
            Body::Custom(c) => &c.chrome,
        }
    }

    fn collection(&self) -> Option<&Collection<Layout>> {
        match self {
            Body::Collection(c) => Some(c),
            _ => None,
        }
    }

    fn dropdown(&self) -> Option<&Dropdown> {
        self.collection()?.dropdown.as_deref()
    }
}

/// A matched item, in display order.
struct Entry {
    section: usize,
    index: usize,
    highlights: Vec<usize>,
}

enum Row {
    Header(usize),
    /// Entries drawn side by side; one in a list.
    Items(Range<usize>),
}

/// A page pushed over the module, with what to restore when it pops.
struct Pushed {
    page: Page,
    _observe: Subscription,
    input: String,
    selected: Option<SharedString>,
    title: SharedString,
}

#[derive(Default)]
struct ActionMenu {
    filter: String,
    selected: usize,
}

pub struct Launcher {
    input: InputBuffer,
    focus_handle: FocusHandle,
    /// Scroll state for detail and custom bodies.
    body_scroll: ScrollHandle,
    /// Scroll state for the detail pane beside a list.
    detail_scroll: ScrollHandle,
    list: VirtualListScrollHandle,
    registry: Vec<ModuleDescriptor>,
    metas: Vec<ModuleMeta>,
    /// Built on first route, so modules nobody opens never start their services.
    modules: Vec<Option<Page>>,
    subscriptions: Vec<Subscription>,
    stack: Vec<Pushed>,
    /// Input text the query was taken from, so it is parsed once per edit.
    routed_input: String,
    route: Route,
    body: Option<Body>,
    entries: Vec<Entry>,
    rows: Vec<Row>,
    /// The row each entry is drawn in.
    entry_rows: Vec<usize>,
    selected: usize,
    /// The page notified, or a search it filters itself changed.
    dirty: bool,
    /// The next rebuild starts at the top instead of keeping the selection.
    reset_selection: bool,
    /// Selected on the next rebuild, after a pop.
    restore: Option<SharedString>,
    menu: Option<ActionMenu>,
    ranker: Ranker,
    owner: EntityId,
}

impl Launcher {
    pub fn new(initial_input: Option<String>, cx: &mut Context<Self>) -> Self {
        let config = Config::global(cx).launcher.clone();
        let registry = modules::registry(&config.modules);
        let metas: Vec<ModuleMeta> = registry.iter().map(|d| d.meta.clone()).collect();
        let input = InputBuffer::new(initial_input.unwrap_or_default());
        let routed_input = input.text().to_string();
        let route = route::route(&routed_input, &metas);

        let mut launcher = Launcher {
            input,
            focus_handle: cx.focus_handle(),
            body_scroll: ScrollHandle::new(),
            detail_scroll: ScrollHandle::new(),
            list: VirtualListScrollHandle::new(),
            modules: registry.iter().map(|_| None).collect(),
            registry,
            metas,
            subscriptions: Vec::new(),
            stack: Vec::new(),
            routed_input,
            route,
            body: None,
            entries: Vec::new(),
            rows: Vec::new(),
            entry_rows: Vec::new(),
            selected: 0,
            dirty: true,
            reset_selection: true,
            restore: None,
            menu: None,
            ranker: Ranker::default(),
            owner: cx.entity_id(),
        };
        launcher.build_root(cx);
        launcher
    }

    /// Replace the input (IPC `--input`).
    pub fn set_input(&mut self, input: String, cx: &mut Context<Self>) {
        self.pop_to_root(cx);
        self.input.set_text(input);
        self.sync(cx);
    }

    fn build_root(&mut self, cx: &mut Context<Self>) {
        let ix = self.route.module;
        if self.modules.get(ix).is_some_and(Option::is_none) {
            let page = self.registry[ix].build(cx);
            self.subscriptions.push(page.observe(cx));
            self.modules[ix] = Some(page);
        }
    }

    fn current_page(&self) -> Option<&Page> {
        match self.stack.last() {
            Some(pushed) => Some(&pushed.page),
            None => self.modules.get(self.route.module)?.as_ref(),
        }
    }

    /// The search the current page sees: the input without the module
    /// prefix, or all of it on a pushed page.
    fn query(&self) -> &str {
        if self.stack.is_empty() {
            &self.route.query
        } else {
            self.input.text()
        }
    }

    pub(crate) fn page_notified(&mut self, page: EntityId, cx: &mut Context<Self>) {
        if self.current_page().map(Page::entity_id) == Some(page) {
            self.dirty = true;
            cx.notify();
        }
    }

    /// Re-route after an edit. Cheap when the text did not change.
    fn sync(&mut self, cx: &mut Context<Self>) {
        if self.input.text() == self.routed_input {
            return;
        }
        self.routed_input = self.input.text().to_string();
        cx.notify();

        if self.stack.is_empty() {
            let route = route::route(&self.routed_input, &self.metas);
            if route == self.route {
                return;
            }
            let module_changed = route.module != self.route.module;
            self.route = route;
            if module_changed {
                self.build_root(cx);
                self.clear_body();
                self.invalidate(true, cx);
                return;
            }
        }

        let filtered = self
            .body
            .as_ref()
            .and_then(Body::collection)
            .is_some_and(|c| c.filtering);
        if filtered {
            self.rank();
            self.select_first();
        } else {
            self.invalidate(true, cx);
        }
    }

    fn invalidate(&mut self, reset_selection: bool, cx: &mut Context<Self>) {
        self.dirty = true;
        self.reset_selection |= reset_selection;
        cx.notify();
    }

    fn clear_body(&mut self) {
        self.body = None;
        self.entries.clear();
        self.rows.clear();
        self.entry_rows.clear();
        self.menu = None;
    }

    /// Ask the page for its content again if it changed since the last time.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let Some(page) = self.current_page().cloned() else {
            return;
        };
        let reset = std::mem::take(&mut self.reset_selection);
        let keep = match self.restore.take() {
            Some(id) => Some(id),
            None if !reset => self.selected_item().map(|item| item.id.clone()),
            None => None,
        };

        let query = self.query().to_string();
        self.body = Some(page.content(&query, cx).into());
        let rows_before = self.rows.len();
        self.rank();

        if let Some(id) = keep
            && let Some(pos) = self
                .entries
                .iter()
                .position(|entry| self.item(entry).is_some_and(|item| item.id == id))
        {
            self.selected = pos;
        }
        if reset {
            self.select_first();
        } else {
            self.selected = self.selected.min(self.entries.len().saturating_sub(1));
            if self.rows.len() != rows_before {
                self.scroll_to_selected();
            }
        }
    }

    /// Rank every section's items against the query and lay them out in
    /// rows.
    fn rank(&mut self) {
        self.entries.clear();
        self.rows.clear();
        self.entry_rows.clear();
        let query = self.query().to_string();
        let Some(Body::Collection(collection)) = &self.body else {
            return;
        };
        let columns = match collection.layout {
            Layout::Grid { columns, .. } => columns,
            Layout::List { .. } => 1,
        };

        for (section_ix, section) in collection.sections.iter().enumerate() {
            let ranked = if collection.filtering {
                self.ranker.rank(&query, &section.items)
            } else {
                (0..section.items.len())
                    .map(|index| Ranked {
                        index,
                        highlights: Vec::new(),
                    })
                    .collect()
            };
            if ranked.is_empty() {
                continue;
            }
            if section.title.is_some() {
                self.rows.push(Row::Header(section_ix));
            }
            let start = self.entries.len();
            self.entries.extend(ranked.into_iter().map(|r| Entry {
                section: section_ix,
                index: r.index,
                highlights: r.highlights,
            }));
            let end = self.entries.len();
            for row_start in (start..end).step_by(columns) {
                let row_end = (row_start + columns).min(end);
                self.entry_rows
                    .extend(std::iter::repeat_n(self.rows.len(), row_end - row_start));
                self.rows.push(Row::Items(row_start..row_end));
            }
        }
    }

    fn select_first(&mut self) {
        self.selected = 0;
        self.list.scroll_to_item(0, ScrollStrategy::Top);
        self.body_scroll.set_offset(gpui::point(px(0.), px(0.)));
    }

    fn item(&self, entry: &Entry) -> Option<&Item> {
        let collection = self.body.as_ref()?.collection()?;
        collection
            .sections
            .get(entry.section)?
            .items
            .get(entry.index)
    }

    fn selected_item(&self) -> Option<&Item> {
        self.item(self.entries.get(self.selected)?)
    }

    /// The selected item's actions, or the view's when it has none.
    fn current_actions(&self) -> Option<&ActionPanel> {
        self.selected_item()
            .map(|item| &item.actions)
            .filter(|actions| !actions.is_empty())
            .or_else(|| Some(&self.body.as_ref()?.chrome().actions))
    }

    fn activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh(cx);
        let action = self
            .current_actions()
            .and_then(|actions| actions.get(ix))
            .cloned();
        if let Some(action) = action {
            self.run(&action, window, cx);
        }
    }

    fn run(&mut self, action: &Action, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        let outcome = action.run(window, cx);
        self.apply_outcome(outcome, window, cx);
    }

    fn apply_outcome(&mut self, outcome: Outcome, window: &mut Window, cx: &mut Context<Self>) {
        match outcome {
            Outcome::Close => close(window, cx),
            Outcome::Stay => cx.notify(),
            Outcome::Push(page) => self.push(page, None, cx),
            Outcome::PushWithInput(page, input) => self.push(page, Some(input), cx),
            Outcome::Pop => self.pop(cx),
            Outcome::PopToRoot => self.pop_to_root(cx),
            Outcome::SetQuery(text) => {
                self.pop_to_root(cx);
                self.input.set_text(text);
                self.sync(cx);
            }
        }
    }

    fn push(&mut self, page: Page, input: Option<String>, cx: &mut Context<Self>) {
        let observe = page.observe(cx);
        let title: SharedString = self
            .body
            .as_ref()
            .and_then(|b| b.chrome().title.clone())
            .unwrap_or_else(|| self.metas[self.route.module].name.into());
        self.stack.push(Pushed {
            page,
            _observe: observe,
            input: self.input.text().to_string(),
            selected: self.selected_item().map(|item| item.id.clone()),
            title,
        });
        let text = input.unwrap_or_default();
        self.input.set_text(text.clone());
        self.input.select_all();
        self.routed_input = text;
        self.clear_body();
        self.invalidate(true, cx);
    }

    fn pop(&mut self, cx: &mut Context<Self>) {
        if let Some(top) = self.stack.pop() {
            self.restore_below(top, cx);
        }
    }

    fn pop_to_root(&mut self, cx: &mut Context<Self>) {
        if !self.stack.is_empty() {
            let bottom = self.stack.swap_remove(0);
            self.stack.clear();
            self.restore_below(bottom, cx);
        }
    }

    fn pop_to_depth(&mut self, depth: usize, cx: &mut Context<Self>) {
        if depth == 0 {
            self.pop_to_root(cx);
            return;
        }
        while self.stack.len() > depth {
            self.pop(cx);
        }
    }

    fn restore_below(&mut self, popped: Pushed, cx: &mut Context<Self>) {
        self.input.set_text(popped.input.clone());
        self.routed_input = popped.input;
        self.restore = popped.selected;
        self.clear_body();
        self.invalidate(false, cx);
    }

    fn navigate(&mut self, key: NavKey, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh(cx);
        if self.menu.is_some() {
            return self.navigate_menu(key, window, cx);
        }

        let grid = self
            .body
            .as_ref()
            .and_then(Body::collection)
            .is_some_and(|c| matches!(c.layout, Layout::Grid { .. }));
        let count = self.entries.len();
        match key {
            NavKey::Enter => return self.activate(0, window, cx),
            NavKey::Left | NavKey::Right
                if self.input.is_empty()
                    && let Some(dropdown) = self.body.as_ref().and_then(Body::dropdown) =>
            {
                let step = if key == NavKey::Left { -1 } else { 1 };
                if let Some(value) = dropdown.step(step) {
                    let on_change = dropdown.on_change.clone();
                    on_change(value, window, cx);
                }
            }
            NavKey::Left if grid => self.selected = self.selected.saturating_sub(1),
            NavKey::Right if grid => {
                self.selected = (self.selected + 1).min(count.saturating_sub(1));
            }
            NavKey::Left if self.input.is_empty() => {
                if let Some(actions) = self.current_actions()
                    && let Some(secondary) = actions.get(1)
                    && (secondary.title == "Step back" || secondary.title == "Previous option")
                {
                    let action = secondary.clone();
                    self.run(&action, window, cx);
                    return;
                }
                self.input.move_left(false);
            }
            NavKey::Right if self.input.is_empty() => {
                if let Some(actions) = self.current_actions()
                    && let Some(secondary) = actions.get(1)
                    && (secondary.title == "Step back" || secondary.title == "Previous option")
                    && let Some(primary) = actions.get(0)
                {
                    let action = primary.clone();
                    self.run(&action, window, cx);
                    return;
                }
                self.input.move_right(false);
            }
            NavKey::Left => self.input.move_left(false),
            NavKey::Right => self.input.move_right(false),
            _ if count == 0 => {}
            // A list wraps; a grid stops at its edges.
            NavKey::Up if !grid => self.selected = (self.selected + count - 1) % count,
            NavKey::Down if !grid => self.selected = (self.selected + 1) % count,
            NavKey::Up => self.move_row(false),
            NavKey::Down => self.move_row(true),
        }
        self.scroll_to_selected();
        cx.notify();
    }

    /// Move to the same column in the next or previous row of items.
    fn move_row(&mut self, down: bool) {
        let Some(&row) = self.entry_rows.get(self.selected) else {
            return;
        };
        let Row::Items(current) = &self.rows[row] else {
            return;
        };
        let column = self.selected - current.start;
        let mut rows: Box<dyn Iterator<Item = &Row>> = if down {
            Box::new(self.rows[row + 1..].iter())
        } else {
            Box::new(self.rows[..row].iter().rev())
        };
        if let Some(Row::Items(target)) = rows.find(|row| matches!(row, Row::Items(_))) {
            self.selected = (target.start + column).min(target.end - 1);
        }
    }

    fn page(&mut self, down: bool, cx: &mut Context<Self>) {
        self.refresh(cx);
        let count = self.entries.len();
        if count == 0 || self.menu.is_some() {
            return;
        }
        let columns = match self.body.as_ref().and_then(Body::collection) {
            Some(Collection {
                layout: Layout::Grid { columns, .. },
                ..
            }) => *columns,
            _ => 1,
        };
        let step = ROWS_PER_PAGE * columns;
        self.selected = if down {
            (self.selected + step).min(count - 1)
        } else {
            self.selected.saturating_sub(step)
        };
        self.scroll_to_selected();
        cx.notify();
    }

    fn scroll_to_selected(&self) {
        let Some(&row) = self.entry_rows.get(self.selected) else {
            return;
        };
        // The first entry brings its section header along.
        let row = if self.selected == 0 { 0 } else { row };
        self.list.scroll_to_item(row, ScrollStrategy::Nearest);
    }

    fn edit(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut InputBuffer)) {
        if self.menu.is_some() {
            return;
        }
        edit(&mut self.input);
        self.sync(cx);
        cx.notify();
    }

    fn type_text(&mut self, text: &str, cx: &mut Context<Self>) {
        match &mut self.menu {
            Some(menu) => {
                menu.filter.push_str(text);
                menu.selected = 0;
                cx.notify();
            }
            None => self.edit(cx, |input| input.insert_str(text)),
        }
    }

    fn backspace(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.menu {
            menu.filter.pop();
            menu.selected = 0;
            cx.notify();
        } else if self.input.is_empty() && !self.stack.is_empty() {
            self.pop(cx);
        } else {
            self.edit(cx, InputBuffer::backspace);
        }
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // An open select on a config form goes first; it cannot hold the
        // keyboard focus itself, since the input keeps it.
        if modules::config::select::close(cx) {
            return;
        }
        if self.menu.take().is_some() {
            cx.notify();
        } else if !self.input.is_empty() {
            self.edit(cx, InputBuffer::clear);
        } else if !self.stack.is_empty() {
            self.pop(cx);
        } else {
            close(window, cx);
        }
    }

    fn toggle_menu(&mut self, cx: &mut Context<Self>) {
        self.refresh(cx);
        self.menu = match self.menu {
            Some(_) => None,
            None => self
                .current_actions()
                .is_some_and(|actions| !actions.is_empty())
                .then(ActionMenu::default),
        };
        cx.notify();
    }

    /// The actions the menu filter keeps, with their section titles.
    fn menu_actions(&self) -> Vec<(Option<SharedString>, Action)> {
        let filter = self
            .menu
            .as_ref()
            .map(|menu| menu.filter.to_lowercase())
            .unwrap_or_default();
        let Some(actions) = self.current_actions() else {
            return Vec::new();
        };
        actions
            .sections
            .iter()
            .flat_map(|section| {
                section
                    .actions
                    .iter()
                    .map(|action| (section.title.clone(), action.clone()))
            })
            .filter(|(_, action)| action.title.to_lowercase().contains(&filter))
            .collect()
    }

    fn navigate_menu(&mut self, key: NavKey, window: &mut Window, cx: &mut Context<Self>) {
        let actions = self.menu_actions();
        let Some(menu) = &mut self.menu else { return };
        let count = actions.len();
        match key {
            NavKey::Enter => {
                if let Some((_, action)) = actions.get(menu.selected) {
                    self.run(action, window, cx);
                }
                return;
            }
            _ if count == 0 => {}
            NavKey::Up => menu.selected = (menu.selected + count - 1) % count,
            NavKey::Down => menu.selected = (menu.selected + 1) % count,
            NavKey::Left | NavKey::Right => {}
        }
        cx.notify();
    }

    /// Run the current action bound to `keystroke`, if any.
    fn run_shortcut(
        &mut self,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.refresh(cx);
        let action = self
            .current_actions()
            .and_then(|actions| actions.iter().find(|a| a.matches(keystroke)))
            .cloned();
        match action {
            Some(action) => {
                self.run(&action, window, cx);
                true
            }
            None => false,
        }
    }

    fn placeholder(&self) -> String {
        if let Some(placeholder) = self
            .body
            .as_ref()
            .and_then(|b| b.chrome().placeholder.clone())
        {
            return placeholder.to_string();
        }
        if !self.stack.is_empty() {
            return "Search...".to_string();
        }
        let default = self.metas.iter().find(|meta| meta.default);
        let help = self.metas.iter().find(|meta| meta.id == "help");
        match (default, help) {
            (Some(default), Some(help)) => format!(
                "Search {} or type {} for commands...",
                default.name.to_lowercase(),
                help.prefix
            ),
            _ => "Search...".to_string(),
        }
    }

    /// Bare prefixes at the root; where you are once a page is pushed.
    fn footer_left(&self) -> String {
        if !self.stack.is_empty() {
            return "Esc to go back".to_string();
        }
        self.metas
            .iter()
            .map(|meta| meta.prefix.trim())
            .filter(|prefix| !prefix.is_empty())
            .collect::<Vec<_>>()
            .join("  ")
    }

    fn footer_right(&self) -> Vec<(SharedString, SharedString)> {
        let back: SharedString = if self.stack.is_empty() {
            "Close".into()
        } else {
            "Back".into()
        };
        if self.menu.is_some() {
            return vec![
                ("Run".into(), "Enter".into()),
                ("Close".into(), "Esc".into()),
            ];
        }
        let mut hints: Vec<(SharedString, SharedString)> = Vec::new();
        if let Some(actions) = self.current_actions() {
            hints.extend(
                actions
                    .iter()
                    .zip(["Enter", "Shift Enter"])
                    .map(|(action, key)| (action.title.clone(), key.into())),
            );
            if actions.iter().nth(1).is_some() {
                hints.push(("Actions".into(), "Tab".into()));
            }
        }
        if self.input.is_empty() && self.body.as_ref().and_then(Body::dropdown).is_some() {
            hints.push(("Filter".into(), "← →".into()));
        }
        hints.push((back, "Esc".into()));
        hints
    }

    fn render_row(&self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (Some(row), Some(Body::Collection(collection))) = (self.rows.get(ix), &self.body)
        else {
            return div().into_any_element();
        };
        let range = match row {
            Row::Header(section) => {
                return row::section_header(&collection.sections[*section], ix == 0);
            }
            Row::Items(range) => range.clone(),
        };

        let page = self.current_page();
        let (columns, content_size, compact) = match collection.layout {
            Layout::List { show_detail } => (1, size(px(0.), px(0.)), show_detail),
            Layout::Grid {
                columns,
                aspect_ratio,
            } => {
                let gap = Spacing::Medium.pixels();
                let width = (window.viewport_size().width - gap * 2. - gap * (columns - 1) as f32)
                    / columns as f32
                    // Cell padding and border.
                    - Spacing::XSmall.pixels() * 2.
                    - px(2.);
                (columns, size(width, width / aspect_ratio), false)
            }
        };

        let cells = range.clone().map(|entry_ix| {
            let entry = &self.entries[entry_ix];
            let item = &collection.sections[entry.section].items[entry.index];
            let selected = entry_ix == self.selected;
            let content = page
                .and_then(|page| page.render_item(item, selected, window, cx))
                .unwrap_or_else(|| {
                    if columns > 1 {
                        row::grid_cell(item, selected, content_size, self.owner, window, cx)
                    } else {
                        row::list_row(
                            item,
                            &entry.highlights,
                            selected,
                            compact,
                            self.owner,
                            window,
                            cx,
                        )
                    }
                });
            div()
                .id(("launcher-item", entry_ix))
                .flex_1()
                .min_w(px(0.))
                .h_full()
                .flex()
                .flex_col()
                .justify_center()
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.selected = entry_ix;
                    this.activate(0, window, cx);
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, _, _, cx| {
                        this.selected = entry_ix;
                        this.menu = None;
                        this.toggle_menu(cx);
                    }),
                )
                .child(content)
        });
        let fillers = (range.len()..columns).map(|_| div().flex_1());

        div()
            .size_full()
            .flex()
            .when(columns > 1, |el| {
                el.gap(Spacing::Medium.pixels())
                    .px(Spacing::Medium.pixels())
                    .pb(Spacing::Small.pixels())
            })
            .children(cells)
            .children(fillers)
            .into_any_element()
    }

    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let scroll = |child: AnyElement, handle: &ScrollHandle| {
            div()
                .id("launcher-scroll")
                .flex_1()
                .overflow_y_scroll()
                .track_scroll(handle)
                .child(child)
                .into_any_element()
        };
        let Some(body) = &self.body else {
            return div().flex_1().into_any_element();
        };
        match body {
            Body::Detail(detail) => scroll(detail::render_detail(detail, cx), &self.body_scroll),
            Body::Custom(custom) => {
                scroll(custom.view.clone().into_any_element(), &self.body_scroll)
            }
            Body::Collection(collection) if self.entries.is_empty() => {
                let fallback = EmptyView::new("No results");
                let empty = match (&collection.empty, collection.chrome.loading) {
                    (Some(empty), _) => Some(empty),
                    (None, false) => Some(&fallback),
                    (None, true) => None,
                };
                row::empty_view(empty)
            }
            Body::Collection(collection) => {
                let this = cx.entity().downgrade();
                // Rows are as tall as the first row of items; a header is
                // never measured.
                let measured = self
                    .rows
                    .iter()
                    .position(|row| matches!(row, Row::Items(_)));
                // The list sizes itself from its own style, so it needs a
                // parent with a definite height.
                let list = div().size_full().child(
                    VirtualList::new(
                        "launcher-list",
                        self.rows.len(),
                        move |range: Range<usize>, window, cx| {
                            this.update(cx, |this, cx| {
                                range
                                    .map(|ix| this.render_row(ix, window, cx))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                        },
                    )
                    .width_from_item(measured)
                    .track_scroll(self.list.clone())
                    .size_full(),
                );
                let Layout::List { show_detail: true } = collection.layout else {
                    return div()
                        .flex_1()
                        .overflow_hidden()
                        .child(list)
                        .into_any_element();
                };
                let detail = self
                    .selected_item()
                    .and_then(|item| item.detail.as_ref())
                    .map(|detail| detail::render_detail(detail, cx));
                div()
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .child(
                        div()
                            .w(relative(DETAIL_SPLIT))
                            .flex_none()
                            .h_full()
                            .child(list),
                    )
                    .child(
                        div()
                            .w(px(1.))
                            .h_full()
                            .bg(cx.theme().colors().border_variant),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .relative()
                            .child(
                                div()
                                    .id("launcher-detail")
                                    .size_full()
                                    .overflow_hidden()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.detail_scroll)
                                    .children(detail),
                            )
                            // Overlaid on the pane's padding like the list's
                            // bar, so content never shifts when it appears;
                            // sized from the last frame, so it shows once the
                            // content has overflowed.
                            .when(self.detail_scroll.max_offset().y > px(0.), |el| {
                                el.child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .bottom_0()
                                        .right_0()
                                        .child(Scrollbar::vertical(self.detail_scroll.clone())),
                                )
                            }),
                    )
                    .into_any_element()
            }
        }
    }

    fn render_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.menu.as_ref()?;
        let colors = cx.theme().colors();
        let actions = self.menu_actions();
        let mut children = Vec::new();
        let mut last_section: Option<Option<SharedString>> = None;

        for (ix, (section, action)) in actions.into_iter().enumerate() {
            if last_section.as_ref() != Some(&section) {
                if let Some(title) = &section {
                    children.push(
                        div()
                            .px(Spacing::Medium.pixels())
                            .pt(Spacing::Small.pixels())
                            .child(
                                Label::new(SharedString::from(title.to_uppercase()))
                                    .size(TextSize::XSmall)
                                    .color(Color::Disabled),
                            )
                            .into_any_element(),
                    );
                }
                last_section = Some(section);
            }
            let selected = ix == menu.selected;
            let color = if action.destructive {
                Color::Error
            } else {
                Color::Default
            };
            let shortcut = action.shortcut_label();
            children.push(
                div()
                    .id(("launcher-action", ix))
                    .flex()
                    .items_center()
                    .gap(Spacing::Medium.pixels())
                    .px(Spacing::Medium.pixels())
                    .py(px(5.))
                    .rounded(Radius::Small.pixels())
                    .cursor_pointer()
                    .when(selected, |el| el.bg(colors.element_selected))
                    .when(!selected, |el| el.hover(|s| s.bg(colors.element_hover)))
                    .children(
                        action
                            .icon
                            .map(|icon| Icon::new(icon).size(IconSize::Small).color(color)),
                    )
                    .child(
                        div().flex_1().min_w(px(0.)).child(
                            Label::new(action.title.clone())
                                .size(TextSize::Small)
                                .color(color)
                                .truncate(),
                        ),
                    )
                    .children(shortcut.map(|shortcut| {
                        div()
                            .px(px(4.))
                            .py(px(1.))
                            .rounded(Radius::Small.pixels())
                            .bg(colors.element_background)
                            .child(
                                Label::new(shortcut)
                                    .size(TextSize::XSmall)
                                    .color(Color::Muted),
                            )
                    }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.run(&action, window, cx);
                    }))
                    .into_any_element(),
            );
        }

        let filter: SharedString = if menu.filter.is_empty() {
            "Search actions...".into()
        } else {
            menu.filter.clone().into()
        };
        Some(
            div()
                .absolute()
                .right(Spacing::Large.pixels())
                .bottom(px(48.))
                .w(px(300.))
                .p(Spacing::XSmall.pixels())
                .rounded(Radius::Large.pixels())
                .border_1()
                .border_color(colors.border)
                .bg(colors.elevated_surface_background)
                .shadow_lg()
                .flex()
                .flex_col()
                .gap(px(1.))
                .children(children)
                .child(
                    div()
                        .w_full()
                        .h(px(1.))
                        .my(Spacing::XSmall.pixels())
                        .bg(colors.border_variant),
                )
                .child(
                    div().px(Spacing::Medium.pixels()).py(px(3.)).child(
                        Label::new(filter)
                            .size(TextSize::Small)
                            .color(if menu.filter.is_empty() {
                                Color::Placeholder
                            } else {
                                Color::Default
                            }),
                    ),
                )
                .into_any_element(),
        )
    }

    pub fn target_size(&self, cx: &App) -> Size<Pixels> {
        let config = &Config::global(cx).launcher;
        launcher_target_size(&self.route, &self.metas, config)
    }

    fn render_breadcrumbs(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.stack.is_empty() {
            return None;
        }

        let mut breadcrumb = Breadcrumb::new();
        let entity = cx.entity().clone();

        for (depth, pushed) in self.stack.iter().enumerate() {
            let title = pushed.title.clone();
            let mut item = BreadcrumbItem::new(
                ElementId::NamedInteger("breadcrumb".into(), depth as u64),
                title,
            );
            if depth == 0 {
                item = item.icon(self.metas[self.route.module].icon);
            }
            let entity = entity.clone();
            item = item.on_click(move |_, _window, cx| {
                entity.update(cx, |this, cx| {
                    this.pop_to_depth(depth, cx);
                });
            });
            breadcrumb = breadcrumb.child(item);
        }

        let current_title = self
            .body
            .as_ref()
            .and_then(|b| b.chrome().title.clone())
            .unwrap_or_else(|| self.metas[self.route.module].name.into());

        breadcrumb = breadcrumb.child(
            BreadcrumbItem::new(
                ElementId::NamedInteger("breadcrumb".into(), self.stack.len() as u64),
                current_title,
            )
            .current(true),
        );

        Some(
            div()
                .w_full()
                .px(Spacing::XLarge.pixels())
                .py(Spacing::Small.pixels())
                .child(breadcrumb)
                .into_any_element(),
        )
    }
}

pub fn launcher_target_size(
    route: &Route,
    metas: &[ModuleMeta],
    config: &config::LauncherConfig,
) -> Size<Pixels> {
    let module_id = metas.get(route.module).map(|m| m.id).unwrap_or("");
    let (mod_width, mod_height) = match module_id {
        "config" => (config.modules.config.width, config.modules.config.height),
        "apps" => (config.modules.apps.width, config.modules.apps.height),
        "clipboard" => (config.modules.clipboard.width, config.modules.clipboard.height),
        "wallpaper" => (config.modules.wallpaper.width, config.modules.wallpaper.height),
        "themes" => (config.modules.themes.width, config.modules.themes.height),
        "shell" => (config.modules.shell.width, config.modules.shell.height),
        _ => (None, None),
    };
    let width = mod_width.unwrap_or(config.width);
    let height = mod_height.unwrap_or(config.height);
    Size::new(px(width), px(height))
}

/// Navigation keys, after the launcher's own bindings resolved them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NavKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
}

fn close(window: &mut Window, cx: &mut App) {
    modules::config::select::close(cx);
    crate::windows::ShellWindows::set_launcher(cx, None);
    window.remove_window();
}

impl Focusable for Launcher {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Launcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let desired = self.target_size(cx);
        if window.viewport_size() != desired {
            window.resize(desired);
        }
        window.set_rem_size(cx.theme().font_size);
        if !self.focus_handle.is_focused(window) {
            self.focus_handle.focus(window, cx);
        }
        self.refresh(cx);

        let chrome = self.body.as_ref().map(Body::chrome);
        let badge = chrome
            .and_then(|chrome| chrome.title.clone())
            .unwrap_or_else(|| self.metas[self.route.module].name.into());
        let loading = chrome.is_some_and(|chrome| chrome.loading);
        let header = self
            .current_page()
            .cloned()
            .and_then(|page| page.render_header(window, cx));
        let dropdown = match self.body.as_ref().and_then(Body::dropdown) {
            Some(dropdown) => {
                let trailing = dropdown.trailing.clone().map(|action| {
                    row::dropdown_button(&action, cx)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run(&action, window, cx);
                        }))
                        .into_any_element()
                });
                Some(row::dropdown_chips(dropdown, trailing, cx))
            }
            None => None,
        };
        let body = self.render_body(cx);
        let menu = self.render_menu(cx);

        div()
            .id("launcher")
            .track_focus(&self.focus_handle)
            .key_context("Launcher")
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.cancel(window, cx)))
            .on_action(cx.listener(|this, _: &Confirm, window, cx| {
                this.navigate(NavKey::Enter, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SecondaryConfirm, window, cx| {
                if this.menu.is_none() {
                    this.activate(1, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleActions, _, cx| this.toggle_menu(cx)))
            .on_action(cx.listener(|this, _: &CursorUp, window, cx| {
                this.navigate(NavKey::Up, window, cx);
            }))
            .on_action(cx.listener(|this, _: &CursorDown, window, cx| {
                this.navigate(NavKey::Down, window, cx);
            }))
            .on_action(cx.listener(|this, _: &CursorLeft, window, cx| {
                this.navigate(NavKey::Left, window, cx);
            }))
            .on_action(cx.listener(|this, _: &CursorRight, window, cx| {
                this.navigate(NavKey::Right, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| this.page(false, cx)))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| this.page(true, cx)))
            .on_action(cx.listener(|this, _: &Backspace, _, cx| this.backspace(cx)))
            .on_action(cx.listener(|this, _: &DeleteWordBack, _, cx| {
                this.edit(cx, InputBuffer::delete_word_back);
            }))
            .on_action(cx.listener(|this, _: &WordLeft, _, cx| {
                this.edit(cx, |input| input.move_word_left(false));
            }))
            .on_action(cx.listener(|this, _: &WordRight, _, cx| {
                this.edit(cx, |input| input.move_word_right(false));
            }))
            .on_action(cx.listener(|this, _: &SelectWordLeft, _, cx| {
                this.edit(cx, |input| input.move_word_left(true));
            }))
            .on_action(cx.listener(|this, _: &SelectWordRight, _, cx| {
                this.edit(cx, |input| input.move_word_right(true));
            }))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| {
                this.edit(cx, |input| input.move_left(true));
            }))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| {
                this.edit(cx, |input| input.move_right(true));
            }))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.edit(cx, InputBuffer::select_all);
            }))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let keystroke = &event.keystroke;
                let m = keystroke.modifiers;
                if m.control || m.alt || m.platform {
                    if this.run_shortcut(keystroke, window, cx) {
                        cx.stop_propagation();
                    }
                    return;
                }
                let typed = keystroke.key_char.clone().or_else(|| {
                    (keystroke.key.chars().count() == 1).then(|| keystroke.key.clone())
                });
                if let Some(text) = typed
                    && !text.chars().any(char::is_control)
                {
                    this.type_text(&text, cx);
                }
            }))
            .size_full()
            .relative()
            .child(
                LauncherFrame::new(render_input_line(&self.input, &self.placeholder(), cx))
                    .background(cx.theme().surface(Config::global(cx).launcher.opacity))
                    .badge(badge)
                    .loading(loading)
                    .hints(self.footer_left())
                    .actions(footer_hints(self.footer_right(), cx))
                    .breadcrumbs(self.render_breadcrumbs(cx))
                    .children(header)
                    .children(dropdown)
                    .child(body),
            )
            .children(menu)
    }
}

pub fn init(cx: &mut App) {
    ThumbnailCache::init(cx);
}

/// Toggle the launcher window with optional prefilled input.
///
/// - Closed: opens it, with `input` if given.
/// - Open with `input`: replaces the input.
/// - Open without `input`: closes it.
pub fn toggle(input: Option<String>, cx: &mut App) {
    if let Some(handle) = crate::windows::ShellWindows::take_launcher(cx) {
        if let Some(input_text) = input {
            let update_result = handle.update(cx, |launcher, _, cx| {
                launcher.set_input(input_text, cx);
            });
            if update_result.is_ok() {
                crate::windows::ShellWindows::set_launcher(cx, Some(handle));
                return;
            }
        }
        let _ = handle.update(cx, |_, window, _| {
            window.remove_window();
        });
        return;
    }

    let cfg = Config::global(cx).launcher.clone();
    let registry = modules::registry(&cfg.modules);
    let metas: Vec<ModuleMeta> = registry.iter().map(|d| d.meta.clone()).collect();
    let initial_input = input.as_deref().unwrap_or_default();
    let initial_route = route::route(initial_input, &metas);
    let target_size = launcher_target_size(&initial_route, &metas, &cfg);

    if let Ok(handle) = cx.open_window(
        WindowOptions {
            titlebar: None,
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::new(px(0.), px(0.)),
                size: target_size,
            })),
            app_id: Some("conchy-shell-launcher".to_string()),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "launcher".to_string(),
                layer: Layer::Overlay,
                anchor: Anchor::TOP,
                exclusive_zone: None,
                margin: Some((
                    px(cfg.margin_top),
                    px(cfg.margin_right),
                    px(cfg.margin_bottom),
                    px(cfg.margin_left),
                )),
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                ..Default::default()
            }),
            focus: true,
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| Launcher::new(input.clone(), cx)),
    ) {
        crate::windows::ShellWindows::set_launcher(cx, Some(handle));
    }
}
