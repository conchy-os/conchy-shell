//! Wallpaper module: a grid of thumbnails. Applying one generates a matching
//! theme with matugen in the background; it waits above the grid until
//! picked.

pub mod config;

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use gpui::{AnyElement, App, Context, Global, Hsla, SharedString, Window, div, prelude::*, px};
use services::WallpaperCommand;
use ui::{
    ActiveTheme, Base16Palette, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius,
    Spacing, TextSize, WallpaperPalettes,
};

use self::config::WallpaperConfig;
use crate::launcher::action::{Action, Outcome};
use crate::launcher::content::{Content, EmptyView, Grid, Item, ItemIcon};
use crate::launcher::modules::theme::{apply_palette, palette_is_active};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};
use crate::state::AppState;

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

/// Width over height of a thumbnail.
const THUMB_ASPECT_RATIO: f32 = 1.9;

pub fn descriptor(config: &WallpaperConfig) -> ModuleDescriptor {
    let config = config.clone();
    ModuleDescriptor::new(
        ModuleMeta {
            id: "wallpaper",
            prefix: config.prefix.clone().into(),
            name: "Wallpaper",
            icon: IconName::Image,
            description: "Browse and set wallpapers",
            default: false,
            in_help: true,
        },
        move |cx| WallpaperModule::new(&config, cx),
    )
}

/// The theme generated from the last applied wallpaper. A global, so it
/// outlives the launcher window.
#[derive(Default)]
pub(crate) struct GeneratedTheme {
    state: Generation,
    /// Bumped per wallpaper, so a slower earlier run can't land last.
    id: u64,
}

#[derive(Default)]
enum Generation {
    #[default]
    Idle,
    Running(SharedString),
    Ready(SharedString, Box<WallpaperPalettes>),
    Failed(SharedString),
}

impl Global for GeneratedTheme {}

/// The wallpaper the last generated theme came from, with both its palettes.
pub(crate) fn generated_palettes(cx: &App) -> Option<(SharedString, WallpaperPalettes)> {
    match &cx.try_global::<GeneratedTheme>()?.state {
        Generation::Ready(name, palettes) => Some((name.clone(), **palettes)),
        _ => None,
    }
}

const GENERATED_PREFIX: &str = "Wallpaper: ";

/// The theme name `apply_palette` gets for a wallpaper's palette.
pub(crate) fn generated_theme_name(wallpaper: &str) -> String {
    format!("{GENERATED_PREFIX}{wallpaper}")
}

/// The wallpaper a theme name made by [`generated_theme_name`] came from.
pub(crate) fn wallpaper_of_theme_name(name: &str) -> Option<&str> {
    name.strip_prefix(GENERATED_PREFIX)
}

struct WallpaperEntry {
    path: PathBuf,
    name: String,
    extension: String,
    /// Part of the thumbnail key, so a file overwritten in place re-renders.
    mtime: Option<SystemTime>,
}

pub struct WallpaperModule {
    directory: PathBuf,
    entries: Vec<WallpaperEntry>,
    scanning: bool,
    columns: usize,
    matugen: bool,
    matugen_scheme: String,
}

impl WallpaperModule {
    fn new(config: &WallpaperConfig, cx: &mut Context<Self>) -> Self {
        let directory = expand_tilde(&config.directory);
        let scan_dir = directory.clone();
        cx.spawn(async move |this, cx| {
            let entries = cx
                .background_spawn(async move { scan_wallpapers(&scan_dir) })
                .await;
            this.update(cx, |this, cx| {
                this.entries = entries;
                this.scanning = false;
                cx.notify();
            })
            .ok();
        })
        .detach();

        cx.default_global::<GeneratedTheme>();
        cx.observe_global::<GeneratedTheme>(|_, cx| cx.notify())
            .detach();

        Self {
            directory,
            entries: Vec::new(),
            scanning: true,
            columns: config.columns.max(1),
            matugen: config.matugen,
            matugen_scheme: config.matugen_scheme.clone(),
        }
    }

    fn apply(&self, path: PathBuf, name: SharedString, cx: &mut Context<Self>) {
        AppState::wallpaper(cx).dispatch(WallpaperCommand::SetWallpaper(path.clone()));
        if self.matugen {
            generate_theme(path, name, self.matugen_scheme.clone(), cx);
        }
    }
}

/// Run matugen off the UI thread; the result is offered, not applied.
fn generate_theme(path: PathBuf, name: SharedString, scheme: String, cx: &mut App) {
    let global = cx.global_mut::<GeneratedTheme>();
    global.id += 1;
    global.state = Generation::Running(name.clone());
    let id = global.id;

    cx.spawn(async move |cx| {
        let result = cx
            .background_spawn(async move { Base16Palette::generate_from_wallpaper(&path, &scheme) })
            .await;
        cx.update(|cx| {
            let global = cx.global_mut::<GeneratedTheme>();
            if global.id != id {
                return;
            }
            global.state = match result {
                Ok(palettes) => Generation::Ready(name, Box::new(palettes)),
                Err(err) => {
                    tracing::warn!("Failed to generate matugen theme: {err}");
                    Generation::Failed(err.to_string().into())
                }
            };
        });
    })
    .detach();
}

/// A clickable chip previewing one generated palette.
fn palette_chip(
    id: &'static str,
    label: &'static str,
    palette: Base16Palette,
    name: SharedString,
    cx: &App,
) -> impl IntoElement {
    let colors = cx.theme().colors();
    let active = palette_is_active(&palette, cx.theme());
    let swatch = |color: Hsla| div().size(px(10.)).rounded(Radius::Full.pixels()).bg(color);

    div()
        .id(id)
        .flex()
        .items_center()
        .gap(Spacing::Small.pixels())
        .px(Spacing::Medium.pixels())
        .py(px(4.))
        .rounded(Radius::Small.pixels())
        .bg(palette.base00)
        .border_1()
        .border_color(if active { colors.accent } else { colors.border })
        .cursor_pointer()
        .hover(|s| s.border_color(colors.accent))
        .child(
            div().flex().gap(px(2.)).children(
                [
                    palette.base05,
                    palette.base08,
                    palette.base0b,
                    palette.base0d,
                    palette.base0e,
                ]
                .map(swatch),
            ),
        )
        .child(
            div()
                .text_size(TextSize::XSmall.rems())
                .text_color(palette.base05)
                .child(label),
        )
        .on_click(move |_, _, cx| {
            apply_palette(generated_theme_name(&name).into(), palette, cx);
        })
}

impl LauncherPage for WallpaperModule {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let dir = self.directory.display().to_string();
        let empty = if self.scanning {
            EmptyView::new(format!("Scanning {dir}…"))
        } else {
            EmptyView::new(format!("No images in {dir}")).icon(IconName::Image)
        };
        Grid::new(self.columns)
            .aspect_ratio(THUMB_ASPECT_RATIO)
            .loading(self.scanning)
            .empty(empty)
            .items(self.entries.iter().map(|entry| {
                let path = entry.path.clone();
                let name: SharedString = entry.name.clone().into();
                let location = entry.path.to_string_lossy().to_string();
                Item::new(location.clone(), name.clone())
                    .keywords(entry.extension.clone())
                    .icon(ItemIcon::Thumbnail {
                        path: entry.path.clone(),
                        mtime: entry.mtime,
                    })
                    .action(
                        Action::listener("Set Wallpaper", cx, move |this, _, cx| {
                            this.apply(path.clone(), name.clone(), cx);
                            Outcome::Stay
                        })
                        .icon(IconName::Image),
                    )
                    .action(Action::open(location.clone()).title("Open in Viewer"))
                    .action(
                        Action::copy(location)
                            .title("Copy Path")
                            .shortcut("ctrl-shift-c"),
                    )
            }))
            .into()
    }

    fn render_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.matugen {
            return None;
        }
        let colors = cx.theme().colors();
        let (status, status_color): (SharedString, Color) = match &cx
            .global::<GeneratedTheme>()
            .state
        {
            Generation::Idle => ("Apply a wallpaper to generate one".into(), Color::Muted),
            Generation::Running(name) => (format!("Generating from {name}…").into(), Color::Muted),
            Generation::Ready(name, _) => (format!("From {name}").into(), Color::Muted),
            Generation::Failed(err) => (err.clone(), Color::Error),
        };
        let chips = match &cx.global::<GeneratedTheme>().state {
            Generation::Ready(name, palettes) => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(Spacing::Medium.pixels())
                    .child(palette_chip(
                        "wallpaper-theme-dark",
                        "Dark",
                        palettes.dark,
                        name.clone(),
                        cx,
                    ))
                    .child(palette_chip(
                        "wallpaper-theme-light",
                        "Light",
                        palettes.light,
                        name.clone(),
                        cx,
                    )),
            ),
            _ => None,
        };

        Some(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(Spacing::Large.pixels())
                .px(Spacing::XLarge.pixels())
                .py(Spacing::Medium.pixels())
                .bg(colors.surface_background)
                .border_b_1()
                .border_color(colors.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(Spacing::Large.pixels())
                        .min_w(px(0.))
                        .child(
                            Icon::new(IconName::Palette)
                                .size(IconSize::Small)
                                .color(Color::Accent),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w(px(0.))
                                .child(Label::new("Theme from wallpaper").size(TextSize::Small))
                                .child(
                                    Label::new(status)
                                        .size(TextSize::XSmall)
                                        .color(status_color)
                                        .truncate(),
                                ),
                        ),
                )
                .children(chips)
                .into_any_element(),
        )
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

fn scan_wallpapers(dir: &Path) -> Vec<WallpaperEntry> {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        tracing::warn!("Could not read wallpaper directory: {}", dir.display());
        return Vec::new();
    };

    let mut entries: Vec<WallpaperEntry> = read_dir
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let extension = path.extension()?.to_str()?.to_lowercase();
            if !path.is_file() || !IMAGE_EXTENSIONS.contains(&extension.as_str()) {
                return None;
            }
            let name = path.file_stem()?.to_str()?.to_string();
            let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            Some(WallpaperEntry {
                path,
                name,
                extension: extension.to_uppercase(),
                mtime,
            })
        })
        .collect();

    entries.sort_by_cached_key(|entry| entry.name.to_lowercase());
    entries
}
