//! Theme browser module: the active theme up top and the list filtered by
//! where a scheme comes from; the button beside the filters opens the
//! registries page that downloads and updates providers. Schemes, the stylix
//! palette and registry status load off the UI thread; so does fetching a
//! provider.

mod appearance;
pub mod config;
mod registry;

use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use gpui::{
    AnyElement, App, Context, FontWeight, Hsla, Pixels, SharedString, Window, div, prelude::*, px,
};
use services::{THEME_PROVIDERS, ThemeProvider, ThemeRepository, load_stylix_scheme};
use ui::{
    ActiveTheme, Base16Palette, Color, Icon, IconName, IconSize, Radius, Spacing, TextSize, Theme,
    builtin_schemes,
};

use self::config::ThemesConfig;
use self::registry::RegistryPage;
use crate::config::Config;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Content, Dropdown, DropdownItem, EmptyView, Item, List, Section};
use crate::launcher::modules::wallpaper::{
    GeneratedTheme, generated_palettes, generated_theme_name, wallpaper_of_theme_name,
};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};

const THEME_ICON: IconName = IconName::Palette;

/// Preview tile in a row and in the current-theme card.
const ROW_TILE: (Pixels, Pixels) = (px(40.), px(30.));
const HERO_TILE: (Pixels, Pixels) = (px(58.), px(40.));

pub fn descriptor(config: &ThemesConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "themes",
            prefix: config.prefix.clone().into(),
            name: "Themes",
            icon: THEME_ICON,
            description: "Browse and apply themes",
            default: false,
            in_help: true,
        },
        ThemeModule::new,
    )
}

/// Where a scheme comes from; also the order of the filter chips.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    BuiltIn,
    Registry,
    Wallpaper,
    Stylix,
}

impl Source {
    const ALL: [Source; 4] = [
        Source::BuiltIn,
        Source::Registry,
        Source::Wallpaper,
        Source::Stylix,
    ];

    fn label(self) -> &'static str {
        match self {
            Source::BuiltIn => "Built-in",
            Source::Registry => "Tinted",
            Source::Wallpaper => "Wallpaper",
            Source::Stylix => "Stylix",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Source::BuiltIn => IconName::Layers,
            Source::Registry => IconName::Globe,
            Source::Wallpaper => IconName::Image,
            Source::Stylix => IconName::Sparkles,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Source(Source),
}

impl Filter {
    fn value(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Source(source) => source.label(),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Source(source) => source.label(),
        }
    }

    fn from_value(value: &str) -> Self {
        Source::ALL
            .into_iter()
            .find(|source| source.label() == value)
            .map_or(Filter::All, Filter::Source)
    }

    fn accepts(self, source: Source) -> bool {
        match self {
            Filter::All => true,
            Filter::Source(wanted) => wanted == source,
        }
    }
}

/// One applicable palette.
#[derive(Clone)]
struct Entry {
    /// The name the theme gets when applied.
    name: SharedString,
    /// Author, variant or origin: whatever tells it apart from its neighbours.
    detail: SharedString,
    source: Source,
    palette: Base16Palette,
}

impl Entry {
    /// What the row shows; wallpaper themes drop their `Wallpaper:` prefix
    /// because the source already says it.
    fn title(&self) -> SharedString {
        match wallpaper_of_theme_name(&self.name) {
            Some(wallpaper) => wallpaper.to_string().into(),
            None => self.name.clone(),
        }
    }
}

/// A provider as of the last load.
#[derive(Clone)]
struct Registry {
    provider: &'static ThemeProvider,
    cached: bool,
    schemes: usize,
    updated: Option<SystemTime>,
}

/// What the background load hands back.
struct Loaded {
    entries: Vec<Entry>,
    registries: Vec<Registry>,
}

pub struct ThemeModule {
    /// Everything on disk: built-in, registry and stylix schemes.
    loaded: Vec<Entry>,
    /// `loaded` plus the palettes generated from the wallpaper; item ids
    /// index into this.
    entries: Vec<Entry>,
    registries: Vec<Registry>,
    filter: Filter,
    /// Providers being cloned or pulled right now.
    fetching: HashSet<&'static str>,
    /// Why the last fetch of a provider failed, by provider id.
    fetch_errors: HashMap<&'static str, SharedString>,
}

impl ThemeModule {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.default_global::<GeneratedTheme>();
        cx.observe_global::<GeneratedTheme>(|this, cx| this.rebuild(cx))
            .detach();

        let mut module = Self {
            loaded: builtin_entries(),
            entries: Vec::new(),
            registries: THEME_PROVIDERS
                .iter()
                .map(|provider| Registry {
                    provider,
                    cached: false,
                    schemes: 0,
                    updated: None,
                })
                .collect(),
            filter: Filter::All,
            fetching: HashSet::new(),
            fetch_errors: HashMap::new(),
        };
        module.rebuild(cx);
        module.reload(cx);
        module
    }

    /// Recombine the disk schemes with the wallpaper palettes.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.entries = self.loaded.clone();
        if let Some((wallpaper, palettes)) = generated_palettes(cx) {
            let name: SharedString = generated_theme_name(&wallpaper).into();
            for (variant, palette) in [("Dark", palettes.dark), ("Light", palettes.light)] {
                self.entries.push(Entry {
                    name: name.clone(),
                    detail: format!("Generated by matugen · {variant}").into(),
                    source: Source::Wallpaper,
                    palette,
                });
            }
        }
        cx.notify();
    }

    fn reload(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let loaded = cx.background_spawn(async { load() }).await;
            this.update(cx, |this, cx| {
                this.loaded = loaded.entries;
                this.registries = loaded.registries;
                this.rebuild(cx);
            })
            .ok();
        })
        .detach();
    }

    fn fetch(&mut self, provider: &'static ThemeProvider, cx: &mut Context<Self>) {
        if !self.fetching.insert(provider.id) {
            return;
        }
        self.fetch_errors.remove(provider.id);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { ThemeRepository::new(provider).fetch_and_cache() })
                .await;
            let error = match result {
                Ok(schemes) => {
                    tracing::info!("Fetched {} schemes from {}", schemes.len(), provider.name);
                    None
                }
                Err(err) => {
                    tracing::error!("Failed to fetch from {}: {err}", provider.name);
                    Some(first_line(&err.to_string()).to_string())
                }
            };
            this.update(cx, |this, cx| {
                this.fetching.remove(provider.id);
                if let Some(error) = error {
                    this.fetch_errors.insert(provider.id, error.into());
                }
                this.reload(cx);
            })
            .ok();
        })
        .detach();
    }

    fn count(&self, source: Source) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.source == source)
            .count()
    }

    /// The chips on offer: stylix only appears once a palette exists, the
    /// rest always, so their empty states can say how to fill them.
    fn filters(&self) -> Vec<Filter> {
        let mut filters = vec![Filter::All];
        filters.extend(
            Source::ALL
                .into_iter()
                .filter(|source| *source != Source::Stylix || self.count(*source) > 0)
                .map(Filter::Source),
        );
        filters
    }

    fn set_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        if self.filter == filter {
            return;
        }
        self.filter = filter;
        cx.notify();
    }

    fn entry(&self, item: &Item) -> Option<&Entry> {
        self.entries.get(item.id.parse::<usize>().ok()?)
    }

    /// The entry the active palette came from, if any still matches it.
    fn current_entry(&self, theme: &Theme) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|entry| palette_is_active(&entry.palette, theme))
    }

    /// Opens the registries page.
    fn registry_action(&self, cx: &Context<Self>) -> Action {
        let module = cx.weak_entity();
        Action::new("Manage Registries", move |_, cx| match module.upgrade() {
            Some(module) => Outcome::push(cx.new(|cx| RegistryPage::new(module, cx))),
            None => Outcome::Stay,
        })
        .icon(Source::Registry.icon())
    }

    /// The button beside the filters; its label says what the registries
    /// are up to.
    fn registry_button(&self, cx: &Context<Self>) -> Action {
        let (title, icon) = if !self.fetching.is_empty() {
            ("Updating…", IconName::Refresh)
        } else if !self.fetch_errors.is_empty() {
            ("Update failed", IconName::CircleAlert)
        } else if self.registries.iter().all(|registry| !registry.cached) {
            ("Download themes", IconName::Download)
        } else {
            ("Registries", Source::Registry.icon())
        };
        self.registry_action(cx).title(title).icon(icon)
    }
}

fn load() -> Loaded {
    let mut entries = builtin_entries();
    let mut registries = Vec::new();

    for provider in THEME_PROVIDERS {
        let repo = ThemeRepository::new(provider);
        let before = entries.len();
        for b16 in repo.load_cached() {
            let Ok(palette) = hex_palette(&b16.palette) else {
                continue;
            };
            let detail = if b16.author.is_empty() {
                provider.repo.to_string()
            } else {
                b16.author
            };
            entries.push(Entry {
                name: b16.name.into(),
                detail: detail.into(),
                source: Source::Registry,
                palette,
            });
        }
        registries.push(Registry {
            provider,
            cached: repo.is_cached(),
            schemes: entries.len() - before,
            updated: repo.last_updated(),
        });
    }

    if let Some(b16) = load_stylix_scheme()
        && let Ok(palette) = hex_palette(&b16.palette)
    {
        let detail = if b16.author.is_empty() {
            "/etc/stylix".to_string()
        } else {
            format!("{} · /etc/stylix", b16.author)
        };
        entries.push(Entry {
            name: b16.name.into(),
            detail: detail.into(),
            source: Source::Stylix,
            palette,
        });
    }

    Loaded {
        entries,
        registries,
    }
}

fn builtin_entries() -> Vec<Entry> {
    builtin_schemes()
        .into_iter()
        .map(|scheme| Entry {
            name: scheme.name,
            detail: "Ships with the shell".into(),
            source: Source::BuiltIn,
            palette: scheme.palette,
        })
        .collect()
}

fn hex_palette(p: &services::Base16Palette) -> Result<Base16Palette, ()> {
    Base16Palette::from_hex(&[
        &p.base00, &p.base01, &p.base02, &p.base03, &p.base04, &p.base05, &p.base06, &p.base07,
        &p.base08, &p.base09, &p.base0a, &p.base0b, &p.base0c, &p.base0d, &p.base0e, &p.base0f,
    ])
    .map_err(drop)
}

/// Make `palette` the theme and save it to `theme.toml`, keeping the user's
/// font size, radius scale, opacity and token overrides.
pub(crate) fn apply_palette(name: SharedString, palette: Base16Palette, cx: &mut App) {
    let mut theme = cx.theme().clone();
    theme.set_palette(name, palette);
    apply_theme(theme, cx);
}

/// Install `theme` and save it to `theme.toml`.
pub(crate) fn apply_theme(theme: Theme, cx: &mut App) {
    Theme::set(theme.clone(), cx);
    cx.background_spawn(async move {
        if let Err(err) = Config::save_theme_value(&theme) {
            tracing::warn!("Failed to persist theme: {err}");
        }
    })
    .detach();
}

pub(crate) fn palette_is_active(palette: &Base16Palette, theme: &Theme) -> bool {
    colors_match(palette.base0d, theme.colors.accent)
        && colors_match(palette.base00, theme.colors.background)
}

fn colors_match(a: Hsla, b: Hsla) -> bool {
    (a.h - b.h).abs() < 0.01 && (a.s - b.s).abs() < 0.01 && (a.l - b.l).abs() < 0.01
}

impl LauncherPage for ThemeModule {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let mut dropdown =
            Dropdown::new(self.filter.value(), cx, |this: &mut Self, value, _, cx| {
                this.set_filter(Filter::from_value(&value), cx);
            });
        for filter in self.filters() {
            let (count, icon) = match filter {
                Filter::All => (self.entries.len(), None),
                Filter::Source(source) => (self.count(source), Some(source.icon())),
            };
            let mut item =
                DropdownItem::new(filter.value(), filter.label()).badge(count.to_string());
            if let Some(icon) = icon {
                item = item.icon(icon);
            }
            dropdown = dropdown.item(item);
        }
        dropdown = dropdown.trailing(self.registry_button(cx));

        let mut list = List::new()
            .placeholder("Search themes...")
            .dropdown(dropdown)
            .empty(self.empty_view());
        for source in Source::ALL.into_iter().filter(|s| self.filter.accepts(*s)) {
            let items: Vec<Item> = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.source == source)
                .map(|(ix, entry)| self.item(ix, entry, cx))
                .collect();
            if !items.is_empty() {
                list = list.section(Section::new(source.label()).items(items));
            }
        }
        // The non-colour knobs live on their own page; the list is schemes.
        // Last, and only when no source filter is on, so it never gets in
        // the way of browsing - searching "font" or "opacity" finds it.
        if self.filter == Filter::All {
            list = list.section(Section::new("More").item(appearance::entry_row(cx)));
        }
        list.into()
    }

    fn render_item(
        &self,
        item: &Item,
        selected: bool,
        _window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let entry = self.entry(item)?;
        let theme = cx.theme();
        Some(
            div()
                .size_full()
                .px(Spacing::Medium.pixels())
                .py(px(2.))
                .child(render_theme_card(
                    entry,
                    selected,
                    palette_is_active(&entry.palette, theme),
                    theme,
                ))
                .into_any_element(),
        )
    }

    fn render_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = cx.theme().clone();
        let current = self.current_entry(&theme).cloned();
        Some(
            div()
                .px(Spacing::Medium.pixels())
                .pt(Spacing::XSmall.pixels())
                .child(render_current_card(&theme, current.as_ref()))
                .into_any_element(),
        )
    }
}

impl ThemeModule {
    fn item(&self, ix: usize, entry: &Entry, cx: &Context<Self>) -> Item {
        let actions = ActionPanel::new()
            .action(
                Action::listener("Apply", cx, move |this, _, cx| {
                    if let Some(entry) = this.entries.get(ix) {
                        apply_palette(entry.name.clone(), entry.palette, cx);
                    }
                    Outcome::Stay
                })
                .icon(IconName::Check),
            )
            .action(Action::copy(entry.name.clone()).title("Copy Name"))
            .action(self.registry_action(cx).shortcut("ctrl-o"));

        Item::new(ix.to_string(), entry.title())
            .subtitle(entry.detail.clone())
            .keywords(entry.source.label())
            .actions(actions)
    }

    fn empty_view(&self) -> EmptyView {
        let registry = self.registries.first();
        match self.filter {
            Filter::Source(Source::Registry) if registry.is_some_and(|r| !r.cached) => {
                EmptyView::new("No registry schemes yet")
                    .description("Open Registries beside the filters to download them")
                    .icon(IconName::Download)
            }
            Filter::Source(Source::Wallpaper) if self.count(Source::Wallpaper) == 0 => {
                EmptyView::new("No wallpaper themes")
                    .description("Apply a wallpaper with matugen enabled to generate one")
                    .icon(IconName::Image)
            }
            _ => EmptyView::new("No themes match"),
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// The active theme: a preview tile, its name and origin, and all 16
/// colours.
fn render_current_card(theme: &Theme, current: Option<&Entry>) -> AnyElement {
    let colors = theme.colors();
    let p = &theme.palette;
    let name = wallpaper_of_theme_name(&theme.name)
        .map(|wallpaper| wallpaper.to_string().into())
        .unwrap_or_else(|| theme.name.clone());

    // A palette no entry matches was hand-edited in theme.toml, unless its
    // name says it was generated from a wallpaper since lost.
    let source = current
        .map(|entry| entry.source)
        .or_else(|| wallpaper_of_theme_name(&theme.name).map(|_| Source::Wallpaper));
    let mut meta = vec![source.map_or("Custom palette", Source::label)];
    if let Some(entry) = current
        && matches!(entry.source, Source::Registry | Source::Stylix)
    {
        meta.push(&entry.detail);
    }
    meta.push(if theme.appearance.is_dark() {
        "Dark"
    } else {
        "Light"
    });
    if !theme.overrides.is_empty() {
        meta.push("Overrides from theme.toml");
    }

    let row = |colors: [Hsla; 8]| {
        div().flex().gap(px(2.)).children(colors.map(|color| {
            div()
                .size(px(11.))
                .rounded(Radius::XSmall.pixels())
                .bg(color)
        }))
    };

    div()
        .w_full()
        .px(Spacing::Large.pixels())
        .py(Spacing::Medium.pixels())
        .rounded(Radius::Large.pixels())
        .bg(colors.surface_background)
        .border_1()
        .border_color(colors.border_variant)
        .flex()
        .items_center()
        .gap(Spacing::Large.pixels())
        .child(preview_tile(p, HERO_TILE))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(Spacing::Small.pixels())
                        .min_w(px(0.))
                        .children(source.map(|source| {
                            Icon::new(source.icon())
                                .size(IconSize::Small)
                                .color(Color::Accent)
                        }))
                        .child(
                            div()
                                .text_size(TextSize::Medium.rems())
                                .text_color(colors.text)
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(name),
                        ),
                )
                .child(
                    div()
                        .text_size(TextSize::XSmall.rems())
                        .text_color(colors.text_muted)
                        .truncate()
                        .child(meta.join(" · ")),
                ),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(row([
                    p.base00, p.base01, p.base02, p.base03, p.base04, p.base05, p.base06, p.base07,
                ]))
                .child(row([
                    p.base08, p.base09, p.base0a, p.base0b, p.base0c, p.base0d, p.base0e, p.base0f,
                ])),
        )
        .into_any_element()
}

fn render_theme_card(entry: &Entry, selected: bool, active: bool, theme: &Theme) -> AnyElement {
    let colors = theme.colors();
    let p = &entry.palette;
    let appearance = if p.appearance().is_dark() {
        IconName::Moon
    } else {
        IconName::Sun
    };

    div()
        .size_full()
        .px(Spacing::Medium.pixels())
        .py(Spacing::XSmall.pixels())
        .rounded(Radius::Medium.pixels())
        .border_1()
        .cursor_pointer()
        .when(selected, |el| {
            el.bg(colors.element_selected).border_color(colors.accent)
        })
        .when(!selected, |el| {
            el.border_color(colors.border_transparent)
                .hover(|s| s.bg(colors.element_hover))
        })
        .flex()
        .items_center()
        .gap(Spacing::Medium.pixels())
        .child(preview_tile(p, ROW_TILE))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(Spacing::Small.pixels())
                        .min_w(px(0.))
                        .child(
                            div()
                                .text_size(TextSize::Small.rems())
                                .text_color(colors.text)
                                .font_weight(FontWeight::MEDIUM)
                                .truncate()
                                .child(entry.title()),
                        )
                        .child(
                            Icon::new(appearance)
                                .size(IconSize::XSmall)
                                .color(Color::Disabled),
                        ),
                )
                .child(
                    div()
                        .text_size(TextSize::XSmall.rems())
                        .text_color(colors.text_muted)
                        .truncate()
                        .child(format!("{} · {}", entry.source.label(), entry.detail)),
                ),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(Spacing::Medium.pixels())
                .child(swatches(
                    [
                        p.base08, p.base09, p.base0a, p.base0b, p.base0c, p.base0d, p.base0e,
                        p.base0f,
                    ],
                    px(10.),
                ))
                .when(active, |el| el.child(active_badge(theme))),
        )
        .into_any_element()
}

/// A tiny mock window in the palette: title bar, two lines of text and the
/// accent dots.
fn preview_tile(p: &Base16Palette, (width, height): (Pixels, Pixels)) -> impl IntoElement {
    let bar = (height * 0.2).round();
    let line = (height * 0.1).round().max(px(2.));
    let dot = (height * 0.14).round();

    div()
        .flex_none()
        .w(width)
        .h(height)
        .rounded(Radius::Small.pixels())
        .border_1()
        .border_color(p.base02)
        .bg(p.base00)
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .h(bar)
                .bg(p.base01)
                .flex()
                .items_center()
                .pl(px(3.))
                .child(div().size(line).rounded(Radius::Full.pixels()).bg(p.base08)),
        )
        .child(
            div()
                .flex_1()
                .px(px(4.))
                .flex()
                .flex_col()
                .justify_center()
                .gap(line)
                .child(
                    div()
                        .h(line)
                        .w(width * 0.55)
                        .rounded(Radius::Full.pixels())
                        .bg(p.base05),
                )
                .child(
                    div()
                        .h(line)
                        .w(width * 0.35)
                        .rounded(Radius::Full.pixels())
                        .bg(p.base03),
                )
                .child(
                    div().flex().gap(px(2.)).children(
                        [p.base0d, p.base0b, p.base0a, p.base0e]
                            .map(|color| div().size(dot).rounded(Radius::Full.pixels()).bg(color)),
                    ),
                ),
        )
}

fn swatches(colors: impl IntoIterator<Item = Hsla>, size: Pixels) -> impl IntoElement {
    div().flex().items_center().gap(px(2.)).children(
        colors
            .into_iter()
            .map(move |color| div().size(size).rounded(Radius::Small.pixels()).bg(color)),
    )
}

fn active_badge(theme: &Theme) -> impl IntoElement {
    let colors = theme.colors();
    div()
        .flex()
        .items_center()
        .gap(px(3.))
        .px(Spacing::Small.pixels())
        .py(px(2.))
        .rounded(Radius::Small.pixels())
        .bg(colors.accent)
        .text_size(TextSize::XSmall.rems())
        .text_color(colors.background)
        .font_weight(FontWeight::BOLD)
        .child(
            Icon::new(IconName::Check)
                .size(IconSize::XSmall)
                .color(Color::Custom(colors.background)),
        )
        .child("Active")
}

fn format_age(time: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(time)
        .unwrap_or_default()
        .as_secs();
    match secs {
        0..60 => "just now".to_string(),
        60..3600 => format!("{}m ago", secs / 60),
        3600..86400 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text).trim()
}
