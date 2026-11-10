//! What a page shows: a [`List`], a [`Grid`], a [`Detail`] or its own
//! [`Custom`] view. Plain data (plus a [`Detail`]'s optional widget builder),
//! rebuilt whenever the page notifies; the launcher owns ranking, drawing,
//! selection and keys.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use gpui::{AnyElement, AnyView, App, Context, Hsla, SharedString, Window};
use ui::{Color, IconName, IconSource};

use super::action::{Action, ActionPanel};

pub enum Content {
    List(List),
    Grid(Grid),
    Detail(Detail),
    Custom(Custom),
}

impl From<List> for Content {
    fn from(list: List) -> Self {
        Content::List(list)
    }
}

impl From<Grid> for Content {
    fn from(grid: Grid) -> Self {
        Content::Grid(grid)
    }
}

impl From<Detail> for Content {
    fn from(detail: Detail) -> Self {
        Content::Detail(detail)
    }
}

impl From<Custom> for Content {
    fn from(custom: Custom) -> Self {
        Content::Custom(custom)
    }
}

/// What every view carries besides its body.
#[derive(Clone, Default)]
pub(super) struct Chrome {
    /// Shown in the query row's badge; defaults to the module name.
    pub title: Option<SharedString>,
    pub placeholder: Option<SharedString>,
    pub loading: bool,
    /// Run when no item is selected, or the view has none.
    pub actions: ActionPanel,
}

macro_rules! chrome_builders {
    () => {
        pub fn title(mut self, title: impl Into<SharedString>) -> Self {
            self.chrome.title = Some(title.into());
            self
        }

        pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
            self.chrome.placeholder = Some(placeholder.into());
            self
        }

        /// Spins the search icon while a slow source loads.
        pub fn loading(mut self, loading: bool) -> Self {
            self.chrome.loading = loading;
            self
        }

        pub fn actions(mut self, actions: ActionPanel) -> Self {
            self.chrome.actions = actions;
            self
        }
    };
}

/// Items in sections, shared by [`List`] and [`Grid`].
pub struct Collection<L> {
    pub(super) chrome: Chrome,
    pub(super) sections: Vec<Section>,
    /// The launcher fuzzy-ranks items against the search; off, the page
    /// filters in `content` and its order is kept.
    pub(super) filtering: bool,
    pub(super) empty: Option<EmptyView>,
    /// Boxed: it is rare and would otherwise dominate the body's size.
    pub(super) dropdown: Option<Box<Dropdown>>,
    pub(super) layout: L,
}

pub struct ListLayout {
    pub(super) show_detail: bool,
}

pub struct GridLayout {
    pub(super) columns: usize,
    /// Width over height of an item's content.
    pub(super) aspect_ratio: f32,
}

pub type List = Collection<ListLayout>;
pub type Grid = Collection<GridLayout>;

impl<L> Collection<L> {
    fn with_layout(layout: L) -> Self {
        Self {
            chrome: Chrome::default(),
            sections: Vec::new(),
            filtering: true,
            empty: None,
            dropdown: None,
            layout,
        }
    }

    chrome_builders!();

    /// Add to the last section.
    pub fn item(mut self, item: Item) -> Self {
        match self.sections.last_mut() {
            Some(section) => section.items.push(item),
            None => self.sections.push(Section::untitled().item(item)),
        }
        self
    }

    pub fn items(self, items: impl IntoIterator<Item = Item>) -> Self {
        items.into_iter().fold(self, Self::item)
    }

    pub fn section(mut self, section: Section) -> Self {
        self.sections.push(section);
        self
    }

    pub fn filtering(mut self, filtering: bool) -> Self {
        self.filtering = filtering;
        self
    }

    /// Shown in place of the body when nothing matches.
    pub fn empty(mut self, empty: EmptyView) -> Self {
        self.empty = Some(empty);
        self
    }

    /// Filter chips under the search; ← → cycle them while it is empty.
    pub fn dropdown(mut self, dropdown: Dropdown) -> Self {
        self.dropdown = Some(Box::new(dropdown));
        self
    }

    pub(super) fn map_layout<M>(self, f: impl FnOnce(L) -> M) -> Collection<M> {
        Collection {
            chrome: self.chrome,
            sections: self.sections,
            filtering: self.filtering,
            empty: self.empty,
            dropdown: self.dropdown,
            layout: f(self.layout),
        }
    }
}

impl Default for List {
    fn default() -> Self {
        Self::new()
    }
}

impl List {
    pub fn new() -> Self {
        Self::with_layout(ListLayout { show_detail: false })
    }

    /// Split the body: the list on the left, the selected item's
    /// [`Item::detail`] on the right.
    pub fn show_detail(mut self, show: bool) -> Self {
        self.layout.show_detail = show;
        self
    }
}

impl Grid {
    pub fn new(columns: usize) -> Self {
        Self::with_layout(GridLayout {
            columns: columns.max(1),
            aspect_ratio: 1.,
        })
    }

    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.layout.aspect_ratio = ratio.max(0.1);
        self
    }
}

pub struct Section {
    pub(super) title: Option<SharedString>,
    pub(super) subtitle: Option<SharedString>,
    pub(super) items: Vec<Item>,
}

impl Section {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: Some(title.into()),
            ..Self::untitled()
        }
    }

    pub fn untitled() -> Self {
        Self {
            title: None,
            subtitle: None,
            items: Vec::new(),
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn item(mut self, item: Item) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = Item>) -> Self {
        self.items.extend(items);
        self
    }
}

#[derive(Clone)]
pub enum ItemIcon {
    None,
    Icon(IconSource),
    /// Small image file drawn as-is (app icons).
    Path(PathBuf),
    /// Large image drawn through the shared thumbnail cache.
    Thumbnail {
        path: PathBuf,
        mtime: Option<SystemTime>,
    },
    Color(Hsla),
}

impl From<IconName> for ItemIcon {
    fn from(icon: IconName) -> Self {
        ItemIcon::Icon(icon.into())
    }
}

/// A list row or a grid cell. In a grid, the icon is the cell's content.
#[derive(Clone)]
pub struct Item {
    /// Stable across rebuilds, so the selection survives them.
    pub id: SharedString,
    pub title: SharedString,
    pub subtitle: Option<SharedString>,
    pub icon: ItemIcon,
    /// Searched by the matcher but never shown.
    pub keywords: Option<SharedString>,
    pub accessories: Vec<Accessory>,
    pub actions: ActionPanel,
    /// Shown beside the list when it has [`List::show_detail`].
    pub detail: Option<Detail>,
}

impl Item {
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            icon: ItemIcon::None,
            keywords: None,
            accessories: Vec::new(),
            actions: ActionPanel::default(),
            detail: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<ItemIcon>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn keywords(mut self, keywords: impl Into<SharedString>) -> Self {
        self.keywords = Some(keywords.into());
        self
    }

    pub fn accessory(mut self, accessory: Accessory) -> Self {
        self.accessories.push(accessory);
        self
    }

    pub fn action(mut self, action: Action) -> Self {
        self.actions = self.actions.action(action);
        self
    }

    pub fn actions(mut self, actions: ActionPanel) -> Self {
        self.actions = actions;
        self
    }

    pub fn detail(mut self, detail: Detail) -> Self {
        self.detail = Some(detail);
        self
    }
}

#[derive(Clone)]
pub(super) enum AccessoryKind {
    Text(SharedString),
    Tag(SharedString),
    Icon,
}

/// Right-aligned extra on a list row.
#[derive(Clone)]
pub struct Accessory {
    pub(super) kind: AccessoryKind,
    pub(super) icon: Option<IconName>,
    pub(super) color: Option<Color>,
}

impl Accessory {
    pub fn text(text: impl Into<SharedString>) -> Self {
        Self::of(AccessoryKind::Text(text.into()))
    }

    pub fn tag(text: impl Into<SharedString>) -> Self {
        Self::of(AccessoryKind::Tag(text.into()))
    }

    pub fn icon(icon: IconName) -> Self {
        Self {
            icon: Some(icon),
            ..Self::of(AccessoryKind::Icon)
        }
    }

    fn of(kind: AccessoryKind) -> Self {
        Self {
            kind,
            icon: None,
            color: None,
        }
    }

    pub fn with_icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

#[derive(Clone)]
pub struct EmptyView {
    pub(super) title: SharedString,
    pub(super) description: Option<SharedString>,
    pub(super) icon: Option<IconName>,
}

impl EmptyView {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
            icon: None,
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
}

type OnChange = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// One choice out of several, drawn as chips under the search.
#[derive(Clone)]
pub struct Dropdown {
    pub(super) value: SharedString,
    pub(super) items: Vec<DropdownItem>,
    pub(super) on_change: OnChange,
    /// A button at the right end of the chip row; not part of the ← → cycle.
    pub(super) trailing: Option<Action>,
}

impl Dropdown {
    pub fn new<V: 'static>(
        value: impl Into<SharedString>,
        cx: &Context<V>,
        on_change: impl Fn(&mut V, SharedString, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        let page = cx.weak_entity();
        Self {
            value: value.into(),
            items: Vec::new(),
            on_change: Rc::new(move |value, window, cx| {
                page.update(cx, |page, cx| on_change(page, value, window, cx))
                    .ok();
            }),
            trailing: None,
        }
    }

    pub fn item(mut self, item: DropdownItem) -> Self {
        self.items.push(item);
        self
    }

    /// Run `action` from a button after the chips.
    pub fn trailing(mut self, action: Action) -> Self {
        self.trailing = Some(action);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = DropdownItem>) -> Self {
        self.items.extend(items);
        self
    }

    /// The value `step` chips away, wrapping.
    pub(super) fn step(&self, step: isize) -> Option<SharedString> {
        let len = self.items.len() as isize;
        if len == 0 {
            return None;
        }
        let current = self
            .items
            .iter()
            .position(|item| item.value == self.value)
            .unwrap_or(0) as isize;
        let next = (current + step).rem_euclid(len) as usize;
        Some(self.items[next].value.clone())
    }
}

#[derive(Clone)]
pub struct DropdownItem {
    pub(super) value: SharedString,
    pub(super) title: SharedString,
    pub(super) icon: Option<IconName>,
    pub(super) badge: Option<SharedString>,
}

impl DropdownItem {
    pub fn new(value: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            title: title.into(),
            icon: None,
            badge: None,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Small trailing text, such as a count.
    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }
}

/// Builds a [`Detail`]'s mouse widget from the current app state.
pub type DetailWidget = Rc<dyn Fn(&App) -> AnyElement>;

/// Markdown above a key-value list. The markdown is a block subset:
/// headings, paragraphs, `-` lists, `>` quotes, fenced code and
/// `![alt](path)` images on their own line.
#[derive(Clone, Default)]
pub struct Detail {
    pub(super) chrome: Chrome,
    pub(super) markdown: SharedString,
    pub(super) metadata: Vec<Metadata>,
    pub(super) widget: Option<DetailWidget>,
}

impl Detail {
    pub fn new() -> Self {
        Self::default()
    }

    chrome_builders!();

    pub fn markdown(mut self, markdown: impl Into<SharedString>) -> Self {
        self.markdown = markdown.into();
        self
    }

    pub fn metadata(mut self, entry: Metadata) -> Self {
        self.metadata.push(entry);
        self
    }

    /// Drawn under the metadata. Mouse widgets whose handlers take `&mut App`
    /// work here; focusable ones do not, the launcher keeps focus.
    pub fn widget(mut self, build: impl Fn(&App) -> AnyElement + 'static) -> Self {
        self.widget = Some(Rc::new(build));
        self
    }
}

#[derive(Clone)]
pub enum Metadata {
    Label {
        title: SharedString,
        text: SharedString,
        icon: Option<IconName>,
        color: Option<Color>,
    },
    Tags {
        title: SharedString,
        tags: Vec<(SharedString, Color)>,
    },
    Link {
        title: SharedString,
        text: SharedString,
        target: SharedString,
    },
    Separator,
}

impl Metadata {
    pub fn label(title: impl Into<SharedString>, text: impl Into<SharedString>) -> Self {
        Metadata::Label {
            title: title.into(),
            text: text.into(),
            icon: None,
            color: None,
        }
    }

    pub fn tags(
        title: impl Into<SharedString>,
        tags: impl IntoIterator<Item = (SharedString, Color)>,
    ) -> Self {
        Metadata::Tags {
            title: title.into(),
            tags: tags.into_iter().collect(),
        }
    }

    /// Opened with `xdg-open` on click.
    pub fn link(
        title: impl Into<SharedString>,
        text: impl Into<SharedString>,
        target: impl Into<SharedString>,
    ) -> Self {
        Metadata::Link {
            title: title.into(),
            text: text.into(),
            target: target.into(),
        }
    }

    /// Only affects a label.
    pub fn icon(mut self, new: IconName) -> Self {
        if let Metadata::Label { icon, .. } = &mut self {
            *icon = Some(new);
        }
        self
    }

    /// Only affects a label.
    pub fn color(mut self, new: Color) -> Self {
        if let Metadata::Label { color, .. } = &mut self {
            *color = Some(new);
        }
        self
    }
}

/// The page draws the body itself; the launcher keeps the input, footer and
/// actions.
pub struct Custom {
    pub(super) chrome: Chrome,
    pub(super) view: AnyView,
}

impl Custom {
    pub fn new(view: impl Into<AnyView>) -> Self {
        Self {
            chrome: Chrome::default(),
            view: view.into(),
        }
    }

    chrome_builders!();
}
