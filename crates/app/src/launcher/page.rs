//! The launcher module API.
//!
//! A module is a root [`LauncherPage`], picked by its prefix. A page is a
//! GPUI entity that describes its [`Content`]; actions can push more pages on
//! top, and Esc pops them.

use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext, Context, Entity, EntityId, SharedString, Subscription, Window,
};
use ui::IconName;

use super::Launcher;
use super::content::{Content, Detail, Item};

/// How the launcher knows a module: everything help, routing and the footer
/// need before the module is ever built.
#[derive(Clone)]
pub struct ModuleMeta {
    pub id: &'static str,
    pub prefix: SharedString,
    pub name: &'static str,
    pub icon: IconName,
    pub description: &'static str,
    /// Owns queries without a prefix.
    pub default: bool,
    pub in_help: bool,
}

pub trait LauncherPage: Sized + 'static {
    /// What the page shows for `query`. Called again whenever the page
    /// notifies, and on every search edit unless the content is a list or
    /// grid the launcher filters. Never per frame; must not notify.
    fn content(&mut self, query: &str, cx: &mut Context<Self>) -> Content;

    /// Draw an item instead of the launcher's row or cell. The launcher still
    /// owns its click and selection.
    fn render_item(
        &self,
        _item: &Item,
        _selected: bool,
        _window: &Window,
        _cx: &App,
    ) -> Option<AnyElement> {
        None
    }

    /// Between the search and the body: summaries, toggles.
    fn render_header(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        None
    }
}

/// A built page behind a trait object, so the launcher can stack pages of
/// different types.
trait PageHandle {
    fn content(&self, query: &str, cx: &mut App) -> Content;
    fn render_item(
        &self,
        item: &Item,
        selected: bool,
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement>;
    fn render_header(&self, window: &mut Window, cx: &mut App) -> Option<AnyElement>;
    fn entity_id(&self) -> EntityId;
    fn observe(&self, cx: &mut Context<Launcher>) -> Subscription;
}

impl<P: LauncherPage> PageHandle for Entity<P> {
    fn content(&self, query: &str, cx: &mut App) -> Content {
        self.update(cx, |page, cx| page.content(query, cx))
    }

    fn render_item(
        &self,
        item: &Item,
        selected: bool,
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        self.read(cx).render_item(item, selected, window, cx)
    }

    fn render_header(&self, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        self.update(cx, |page, cx| page.render_header(window, cx))
    }

    fn entity_id(&self) -> EntityId {
        Entity::entity_id(self)
    }

    fn observe(&self, cx: &mut Context<Launcher>) -> Subscription {
        cx.observe(self, |launcher, page, cx| {
            launcher.page_notified(page.entity_id(), cx)
        })
    }
}

#[derive(Clone)]
pub struct Page(Rc<dyn PageHandle>);

impl Page {
    pub fn new<P: LauncherPage>(page: Entity<P>) -> Self {
        Self(Rc::new(page))
    }

    pub(super) fn content(&self, query: &str, cx: &mut App) -> Content {
        self.0.content(query, cx)
    }

    pub(super) fn render_item(
        &self,
        item: &Item,
        selected: bool,
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        self.0.render_item(item, selected, window, cx)
    }

    pub(super) fn render_header(&self, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        self.0.render_header(window, cx)
    }

    pub(super) fn entity_id(&self) -> EntityId {
        self.0.entity_id()
    }

    /// Rebuild the content when the page notifies.
    pub(super) fn observe(&self, cx: &mut Context<Launcher>) -> Subscription {
        self.0.observe(cx)
    }
}

type BuildModule = dyn Fn(&mut App) -> Page;

/// A module the launcher builds on first use.
pub struct ModuleDescriptor {
    pub meta: ModuleMeta,
    build: Box<BuildModule>,
}

impl ModuleDescriptor {
    pub fn new<P: LauncherPage>(
        meta: ModuleMeta,
        build: impl Fn(&mut Context<P>) -> P + 'static,
    ) -> Self {
        Self {
            meta,
            build: Box::new(move |cx| Page::new(cx.new(&build))),
        }
    }

    pub(super) fn build(&self, cx: &mut App) -> Page {
        (self.build)(cx)
    }
}

/// A page that only shows the detail it was given; behind
/// [`Action::push_detail`](super::action::Action::push_detail).
pub(super) struct DetailPage(pub Detail);

impl LauncherPage for DetailPage {
    fn content(&mut self, _query: &str, _cx: &mut Context<Self>) -> Content {
        self.0.clone().into()
    }
}
