//! `Editable` for every config section the editor covers. Kept together so
//! the key list is easy to check against the structs and the docs.

use gpui::App;
use ui::IconName;

use super::bar_preview::bar_preview;
use super::config::ConfigEditorConfig;
use super::dock_preview::dock_preview;
use super::frame_preview::frame_preview;
use super::launcher_preview::launcher_preview;
use super::layout_page::LayoutPage;
use super::look_preset::{apply_look, look_preview};
use super::notification_preview::notification_preview;
use super::osd_preview::osd_preview;
use super::schema::{ConfigPath, Editable, Field, Group, TextHint, flatten, lens};
use crate::bar::config::{BarConfig, BarFloatingConfig, BarPosition, BarSegmentsConfig, BarStyle};
use crate::bar::look::Look;
use crate::bar::modules::{
    self as bar_widgets, ActiveWindowConfig, BarWidgetMeta, BatteryConfig, ClockConfig,
    KeyboardLayoutConfig, MprisConfig as BarMprisConfig, SysInfoConfig as BarSysInfoConfig,
    TrayConfig, WorkspacesConfig as BarWorkspacesConfig,
};
use crate::config::ActiveConfig;
use crate::config::{Config, FrameConfig};
use crate::control_center::ControlCenterConfig;
use crate::dock::config::{
    DockAlign, DockConfig, DockHoverEffect, DockMonitors, DockStyle, DockVisibility,
};
use crate::launcher::action::{Action, Outcome};
use crate::launcher::config::{LauncherConfig, ModulesConfig as LauncherModulesConfig};
use crate::launcher::content::Item;
use crate::launcher::modules::{
    self, apps::config::AppsConfig, calculator::config::CalculatorConfig,
    clipboard::config::ClipboardConfig, help::config::HelpConfig, mpris::config::MprisConfig,
    services::config::ServicesConfig, shell::config::ShellConfig, sysinfo::config::SysInfoConfig,
    theme::config::ThemesConfig, wallpaper::config::WallpaperConfig, web::config::WebConfig,
    workspaces::config::WorkspacesConfig,
};
use crate::notification::{NotificationConfig, NotificationPopupPosition};
use crate::osd::{OsdConfig, OsdPosition};
use crate::windows::PanelsConfig;

impl Editable for Config {
    fn title() -> &'static str {
        "Config"
    }

    /// `theme.toml` has its own module; link to it from here.
    fn shortcuts(cx: &App) -> Vec<Item> {
        let prefix = cx.config().launcher.modules.themes.prefix.clone();
        vec![
            Item::new("themes", "Themes")
                .subtitle("Browse and apply themes, saved to theme.toml")
                .icon(IconName::Palette)
                .keywords("theme colors palette")
                .action(
                    Action::new("Open themes", move |_, _| Outcome::SetQuery(prefix.clone()))
                        .icon(IconName::ChevronRight),
                ),
        ]
    }

    fn fields() -> Vec<Field<Self>> {
        flatten(Self::groups())
    }

    fn groups() -> Vec<Group<Self>> {
        vec![
            Group::new(
                "",
                vec![
                    look_field(|c: &Self| &c.bar, |c: &mut Self| &mut c.bar),
                    Field::nested("bar", "Bar", |c: &Self| &c.bar, |c: &mut Self| &mut c.bar)
                        .icon(IconName::Layout)
                        .describe("Size, position, style and widgets"),
                    Field::nested(
                        "launcher",
                        "Launcher",
                        |c: &Self| &c.launcher,
                        |c: &mut Self| &mut c.launcher,
                    )
                    .icon(IconName::MagnifyingGlass)
                    .describe("Window geometry and module prefixes"),
                    Field::nested(
                        "dock",
                        "Dock",
                        |c: &Self| &c.dock,
                        |c: &mut Self| &mut c.dock,
                    )
                    .icon(IconName::Layers)
                    .describe("Position, shape, visibility and hover effect"),
                    Field::nested(
                        "frame",
                        "Frame",
                        |c: &Self| &c.frame,
                        |c: &mut Self| &mut c.frame,
                    )
                    .icon(IconName::Square)
                    .describe("A band around the screen the bar and dock can be sides of"),
                    Field::nested(
                        "notification",
                        "Notifications",
                        |c: &Self| &c.notification,
                        |c: &mut Self| &mut c.notification,
                    )
                    .icon(IconName::Bell)
                    .describe("Popup placement and sizes"),
                    Field::nested(
                        "control_center",
                        "Control center",
                        |c: &Self| &c.control_center,
                        |c: &mut Self| &mut c.control_center,
                    )
                    .icon(IconName::Settings)
                    .describe("Power action commands"),
                    Field::nested(
                        "osd",
                        "On-screen display",
                        |c: &Self| &c.osd,
                        |c: &mut Self| &mut c.osd,
                    )
                    .icon(IconName::Sliders),
                    Field::nested(
                        "panels",
                        "Panels",
                        |c: &Self| &c.panels,
                        |c: &mut Self| &mut c.panels,
                    )
                    .icon(IconName::Layout),
                ],
            ),
            Group::new(
                "Files",
                vec![
                    Field::bundle(
                        "watch",
                        "Hot reload",
                        vec![
                            Field::toggle("watch_config", "config.toml", lens!(watch_config)),
                            Field::toggle("watch_theme", "theme.toml", lens!(watch_theme)),
                        ],
                    )
                    .icon(IconName::Refresh)
                    .describe(
                        "Reload a file when it changes on disk. Takes effect after a restart",
                    ),
                ],
            ),
        ]
    }
}

/// The Look preset row, for any section `S` that can reach the bar config:
/// Blocky / Rounded / Floating buttons, "Custom" when none matches, a
/// miniature of each in the detail pane.
fn look_field<S: Editable>(
    get: fn(&S) -> &BarConfig,
    get_mut: fn(&mut S) -> &mut BarConfig,
) -> Field<S> {
    let labels: Vec<&'static str> = Look::ALL.iter().map(|look| look.label()).collect();
    Field::presets(
        "look",
        "Look",
        &labels,
        move |s: &S| {
            Look::current(get(s), ui::radius_scale())
                .and_then(|look| Look::ALL.iter().position(|l| *l == look))
                .unwrap_or(Look::ALL.len())
        },
        move |path: &ConfigPath<S>, ix, cx| {
            if let Some(look) = Look::ALL.get(ix) {
                apply_look(*look, &path.then("bar", get, get_mut), cx);
            }
        },
    )
    .icon(IconName::Sparkles)
    .describe(
        "Blocky, rounded or floating: one preset for the bar's style, borders, chrome and \
         corner radius. Widgets and their order are kept",
    )
    .preview(move |path: &ConfigPath<S>, cx: &App| {
        look_preview(&path.then("bar", get, get_mut), cx)
    })
}

impl Editable for BarConfig {
    fn title() -> &'static str {
        "Bar"
    }

    fn fields() -> Vec<Field<Self>> {
        flatten(Self::groups())
    }

    fn groups() -> Vec<Group<Self>> {
        vec![
            Group::new("Look", vec![look_field(|c: &Self| c, |c: &mut Self| c)]),
            Group::new(
                "Widgets",
                vec![
                    Field::custom("start", "Layout", LayoutPage::open)
                        .describe("Which widgets sit in each slot, and their order"),
                    Field::nested(
                        "modules",
                        "Widget settings",
                        |c: &Self| &c.modules,
                        |c: &mut Self| &mut c.modules,
                    )
                    .describe("Options of each bar widget"),
                ],
            ),
            Group::new(
                "Placement",
                vec![
                    Field::bundle(
                        "placement",
                        "Position and style",
                        vec![
                            Field::choice(
                                "position",
                                "Position",
                                lens!(position),
                                &BarPosition::ALL,
                                BarPosition::label,
                            ),
                            Field::choice(
                                "style",
                                "Style",
                                lens!(style),
                                &BarStyle::ALL,
                                BarStyle::label,
                            )
                            .describe(
                                "Attached to the edge, floating, or bubbles. An attached bar \
                                 becomes a side of the frame when that is on",
                            ),
                        ],
                    )
                    .icon(IconName::Layout)
                    .describe("Click an edge of the preview to move the bar there")
                    .preview(bar_preview),
                    Field::number("size", "Size", lens!(size as f64))
                        .range(16.0, 96.0)
                        .unit("px")
                        .describe("Thickness along the bar's main axis"),
                    Field::number("padding", "End padding", lens!(padding or 2.0))
                        .range(0.0, 64.0)
                        .unit("px")
                        .describe(
                            "Space before the first and after the last widget. Unset, it \
                             matches the space between widgets",
                        ),
                ],
            ),
            Group::new(
                "Appearance",
                vec![
                    opacity_field("Bar opacity", lens!(opacity or 1.0)),
                    Field::toggle("show_border", "Border", lens!(show_border))
                        .describe("Hairline on the inner edge, or around a floating bar"),
                    Field::toggle(
                        "widget_background",
                        "Widget background",
                        lens!(widget_background),
                    ),
                    Field::toggle("widget_border", "Widget border", lens!(widget_border)),
                    Field::toggle("widget_stretch", "Widget stretch", lens!(widget_stretch))
                        .describe(
                            "Chips and workspace cells fill the bar's thickness and sit flush, \
                             for a blocky bar",
                        ),
                ],
            ),
            Group::new(
                "Styles",
                vec![
                    Field::nested(
                        "segments",
                        "Segments",
                        |c: &Self| &c.segments,
                        |c: &mut Self| &mut c.segments,
                    )
                    .describe("Gap, inset and padding of the boxes widgets sit in"),
                    Field::nested(
                        "floating",
                        "Floating",
                        |c: &Self| &c.floating,
                        |c: &mut Self| &mut c.floating,
                    )
                    .describe("Gap and radius when the style is floating"),
                ],
            ),
        ]
    }
}

impl Editable for FrameConfig {
    fn title() -> &'static str {
        "Frame"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::bundle(
                "shape",
                "Shape",
                vec![
                    Field::toggle("enabled", "Enabled", lens!(enabled)).describe(
                        "A band around the work area. An attached bar and a taskbar dock \
                         become its sides; a floating bar or dock sits inside it",
                    ),
                    Field::number("thickness", "Thickness", lens!(thickness as f64))
                        .range(0.0, 32.0)
                        .unit("px")
                        .describe("Width of the band on the edges nothing sits on"),
                    Field::number(
                        "radius",
                        "Corner radius",
                        lens!(radius or f64::from(FrameConfig::RADIUS.base())),
                    )
                    .range(0.0, 48.0)
                    .unit("px")
                    .describe(RADIUS_FOLLOWS_THEME),
                ],
            )
            .icon(IconName::Square)
            .preview(frame_preview),
            opacity_field("Opacity", lens!(opacity or 1.0))
                .describe("Unset, the band matches the bar's background"),
        ]
    }
}

impl Editable for BarSegmentsConfig {
    fn title() -> &'static str {
        "Bar segments"
    }

    fn fields() -> Vec<Field<Self>> {
        const PER_STYLE: &str = "Unset: 6 / 3 / 4 px with bubbles, 0 with every other style. Reset to default to \
             follow the style again";
        vec![
            Field::number("gap", "Gap", lens!(gap or 0.0))
                .range(0.0, 32.0)
                .unit("px")
                .describe(PER_STYLE),
            Field::number("inset", "Inset", lens!(inset or 0.0))
                .range(0.0, 16.0)
                .unit("px")
                .describe("Space between a segment and the bar's long edges"),
            Field::number("padding", "Padding", lens!(padding or 0.0))
                .range(0.0, 32.0)
                .unit("px")
                .describe("Space inside a segment's two ends"),
        ]
    }
}

impl Editable for BarFloatingConfig {
    fn title() -> &'static str {
        "Floating bar"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::number("gap", "Gap", lens!(gap as f64))
                .range(0.0, 64.0)
                .unit("px")
                .describe("Distance from the screen edges"),
            Field::number(
                "radius",
                "Corner radius",
                lens!(radius or f64::from(BarFloatingConfig::RADIUS.base())),
            )
            .range(0.0, 48.0)
            .unit("px")
            .describe(RADIUS_FOLLOWS_THEME),
        ]
    }
}

/// Why an unset radius or opacity shows a number the user never typed.
const RADIUS_FOLLOWS_THEME: &str =
    "Unset, the corner follows the theme's radius scale. Reset to default to follow it again";
const OPACITY_FOLLOWS_THEME: &str =
    "Unset, the surface follows the theme's opacity. Reset to default to follow it again";

/// The per-surface override of the theme's surface opacity.
fn opacity_field<S: Editable>(label: &'static str, lens: super::schema::Lens<S, f64>) -> Field<S> {
    Field::number("opacity", label, lens)
        .range(0.0, 1.0)
        .step(0.05)
        .describe(OPACITY_FOLLOWS_THEME)
}

/// The settings of one bar widget, if it has any; keyed by the widget's
/// config name, which is also its `[bar.modules.<key>]` table.
fn widget_field(widget: &BarWidgetMeta) -> Option<Field<crate::bar::config::ModulesConfig>> {
    type M = crate::bar::config::ModulesConfig;
    let label = widget.label;
    let field = match widget.name {
        "Clock" => Field::nested("clock", label, |m: &M| &m.clock, |m: &mut M| &mut m.clock),
        "Battery" => Field::nested(
            "battery",
            label,
            |m: &M| &m.battery,
            |m: &mut M| &mut m.battery,
        ),
        "Workspaces" => Field::nested(
            "workspaces",
            label,
            |m: &M| &m.workspaces,
            |m: &mut M| &mut m.workspaces,
        ),
        "Systray" => Field::nested("tray", label, |m: &M| &m.tray, |m: &mut M| &mut m.tray),
        "SysInfo" => Field::nested(
            "sysinfo",
            label,
            |m: &M| &m.sysinfo,
            |m: &mut M| &mut m.sysinfo,
        ),
        "Mpris" => Field::nested("mpris", label, |m: &M| &m.mpris, |m: &mut M| &mut m.mpris),
        "ActiveWindow" => Field::nested(
            "active_window",
            label,
            |m: &M| &m.active_window,
            |m: &mut M| &mut m.active_window,
        ),
        "KeyboardLayout" => Field::nested(
            "keyboard_layout",
            label,
            |m: &M| &m.keyboard_layout,
            |m: &mut M| &mut m.keyboard_layout,
        ),
        _ => return None,
    };
    Some(field.icon(widget.icon).describe(widget.description))
}

impl Editable for crate::bar::config::ModulesConfig {
    fn title() -> &'static str {
        "Bar widgets"
    }

    fn fields() -> Vec<Field<Self>> {
        bar_widgets::widgets().filter_map(widget_field).collect()
    }
}

impl Editable for ClockConfig {
    fn title() -> &'static str {
        "Clock"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::text(
                "format_horizontal",
                "Horizontal format",
                lens!(format_horizontal),
            )
            .hint(TextHint::Strftime)
            .describe("strftime pattern when the bar is top or bottom"),
            Field::text("format_vertical", "Vertical format", lens!(format_vertical))
                .hint(TextHint::Strftime)
                .describe("strftime pattern when the bar is left or right"),
        ]
    }
}

impl Editable for BatteryConfig {
    fn title() -> &'static str {
        "Battery"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::toggle("show_icon", "Icon", lens!(show_icon)),
            Field::toggle("show_percentage", "Percentage", lens!(show_percentage)),
        ]
    }
}

impl Editable for BarWorkspacesConfig {
    fn title() -> &'static str {
        "Workspaces"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::toggle("show_icons", "Window icons", lens!(show_icons)),
            Field::toggle("show_numbers", "Workspace numbers", lens!(show_numbers)),
        ]
    }
}

impl Editable for TrayConfig {
    fn title() -> &'static str {
        "System tray"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::number("icon_size", "Icon size", lens!(icon_size or 16.0))
                .range(8.0, 48.0)
                .unit("px")
                .describe("Unset, tray icons match the bar's other icons. Reset to default to follow them again"),
        ]
    }
}

impl Editable for BarSysInfoConfig {
    fn title() -> &'static str {
        "System info"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::toggle("show_cpu", "CPU", lens!(show_cpu)),
            Field::toggle("show_memory", "Memory", lens!(show_memory)),
            Field::toggle("show_temp", "Temperature", lens!(show_temp)),
        ]
    }
}

impl Editable for BarMprisConfig {
    fn title() -> &'static str {
        "Media"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::toggle("show_cover", "Album cover", lens!(show_cover)),
            Field::number("max_width", "Maximum width", lens!(max_width as f64))
                .range(80.0, 600.0)
                .unit("px"),
        ]
    }
}

impl Editable for ActiveWindowConfig {
    fn title() -> &'static str {
        "Active window"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::integer("max_length", "Title length", lens!(max_length as f64))
                .range(8.0, 256.0)
                .unit("chars")
                .describe("Longer titles are cut with an ellipsis"),
            Field::toggle("show_app_icon", "App icon", lens!(show_app_icon)),
        ]
    }
}

impl Editable for KeyboardLayoutConfig {
    fn title() -> &'static str {
        "Keyboard layout"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![Field::toggle("show_flag", "Flag", lens!(show_flag))]
    }
}

impl Editable for LauncherConfig {
    fn title() -> &'static str {
        "Launcher"
    }

    fn fields() -> Vec<Field<Self>> {
        flatten(Self::groups())
    }

    fn groups() -> Vec<Group<Self>> {
        vec![
            Group::new(
                "",
                vec![
                    Field::nested(
                        "modules",
                        "Modules",
                        |l: &Self| &l.modules,
                        |l: &mut Self| &mut l.modules,
                    )
                    .describe("Prefix and options of each launcher module"),
                ],
            ),
            Group::new(
                "Window",
                vec![
                    Field::bundle(
                        "window",
                        "Window",
                        vec![
                            Field::number("width", "Width", lens!(width as f64))
                                .range(300.0, 2000.0)
                                .unit("px"),
                            Field::number("height", "Height", lens!(height as f64))
                                .range(200.0, 1600.0)
                                .unit("px"),
                            Field::number("margin_top", "Margin top", lens!(margin_top as f64))
                                .range(0.0, 1000.0)
                                .unit("px"),
                            Field::number(
                                "margin_right",
                                "Margin right",
                                lens!(margin_right as f64),
                            )
                            .range(0.0, 1000.0)
                            .unit("px"),
                            Field::number(
                                "margin_bottom",
                                "Margin bottom",
                                lens!(margin_bottom as f64),
                            )
                            .range(0.0, 1000.0)
                            .unit("px"),
                            Field::number("margin_left", "Margin left", lens!(margin_left as f64))
                                .range(0.0, 1000.0)
                                .unit("px"),
                        ],
                    )
                    .icon(IconName::Layout)
                    .describe("Size of the launcher and its distance from the screen edges")
                    .preview(launcher_preview),
                    opacity_field("Opacity", lens!(opacity or 1.0)),
                ],
            ),
        ]
    }
}

/// One launcher module's settings, decorated from the registry so the icon
/// and description are the module's own.
fn module_field(id: &str) -> Option<Field<LauncherModulesConfig>> {
    type M = LauncherModulesConfig;
    let field = match id {
        "apps" => Field::nested("apps", "Apps", |m: &M| &m.apps, |m: &mut M| &mut m.apps),
        "shell" => Field::nested("shell", "Shell", |m: &M| &m.shell, |m: &mut M| &mut m.shell),
        "web" => Field::nested("web", "Web", |m: &M| &m.web, |m: &mut M| &mut m.web),
        "workspaces" => Field::nested(
            "workspaces",
            "Workspaces",
            |m: &M| &m.workspaces,
            |m: &mut M| &mut m.workspaces,
        ),
        "wallpaper" => Field::nested(
            "wallpaper",
            "Wallpaper",
            |m: &M| &m.wallpaper,
            |m: &mut M| &mut m.wallpaper,
        ),
        "themes" => Field::nested(
            "themes",
            "Themes",
            |m: &M| &m.themes,
            |m: &mut M| &mut m.themes,
        ),
        "services" => Field::nested(
            "services",
            "Services",
            |m: &M| &m.services,
            |m: &mut M| &mut m.services,
        ),
        "sysinfo" => Field::nested(
            "sysinfo",
            "System info",
            |m: &M| &m.sysinfo,
            |m: &mut M| &mut m.sysinfo,
        ),
        "mpris" => Field::nested("mpris", "Media", |m: &M| &m.mpris, |m: &mut M| &mut m.mpris),
        "calculator" => Field::nested(
            "calculator",
            "Calculator",
            |m: &M| &m.calculator,
            |m: &mut M| &mut m.calculator,
        ),
        "clipboard" => Field::nested(
            "clipboard",
            "Clipboard",
            |m: &M| &m.clipboard,
            |m: &mut M| &mut m.clipboard,
        ),
        "config" => Field::nested(
            "config",
            "Config",
            |m: &M| &m.config,
            |m: &mut M| &mut m.config,
        ),
        "help" => Field::nested("help", "Help", |m: &M| &m.help, |m: &mut M| &mut m.help),
        _ => return None,
    };
    Some(field)
}

impl Editable for LauncherModulesConfig {
    fn title() -> &'static str {
        "Launcher modules"
    }

    fn fields() -> Vec<Field<Self>> {
        // The registry is the one place that knows each module's icon and
        // description; building it only creates closures.
        modules::registry(&Self::default())
            .into_iter()
            .filter_map(|descriptor| {
                let meta = descriptor.meta;
                module_field(meta.id).map(|field| field.icon(meta.icon).describe(meta.description))
            })
            .collect()
    }
}

/// The prefix field every launcher module has.
fn prefix_field<S: Editable>(lens: super::schema::Lens<S, String>) -> Field<S> {
    Field::text("prefix", "Prefix", lens)
        .hint(TextHint::Prefix)
        .describe("What to type first to reach this module. Applies when the launcher reopens")
}

/// Modules whose only setting is the prefix.
macro_rules! prefix_only {
    ($($ty:ty => $title:literal),+ $(,)?) => {
        $(impl Editable for $ty {
            fn title() -> &'static str {
                $title
            }

            fn fields() -> Vec<Field<Self>> {
                vec![prefix_field(lens!(prefix))]
            }
        })+
    };
}

prefix_only! {
    AppsConfig => "Apps",
    WorkspacesConfig => "Workspaces",
    ThemesConfig => "Themes",
    ServicesConfig => "Services",
    SysInfoConfig => "System info",
    MprisConfig => "Media",
    CalculatorConfig => "Calculator",
    ClipboardConfig => "Clipboard",
    HelpConfig => "Help",
    ConfigEditorConfig => "Config",
    WebConfig => "Web",
}

impl Editable for ShellConfig {
    fn title() -> &'static str {
        "Shell"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            prefix_field(lens!(prefix)),
            Field::text("terminal", "Terminal", lens!(terminal))
                .hint(TextHint::Command)
                .describe("Terminal to run commands in; empty picks one that is installed"),
        ]
    }
}

impl Editable for WallpaperConfig {
    fn title() -> &'static str {
        "Wallpaper"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            prefix_field(lens!(prefix)),
            Field::text("directory", "Directory", lens!(directory))
                .hint(TextHint::Path)
                .describe("Where wallpapers are listed from; ~ is expanded"),
            Field::integer("columns", "Columns", lens!(columns as f64))
                .range(1.0, 8.0)
                .describe("Cards per grid row"),
            Field::toggle("matugen", "Matugen themes", lens!(matugen))
                .describe("Offer dark and light themes generated from the applied wallpaper"),
            Field::text("matugen_scheme", "Matugen scheme", lens!(matugen_scheme))
                .describe("matugen --type, e.g. scheme-tonal-spot or scheme-content"),
        ]
    }
}

impl Editable for OsdConfig {
    fn title() -> &'static str {
        "On-screen display"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::choice(
                "position",
                "Position",
                lens!(position),
                &OsdPosition::ALL,
                OsdPosition::label,
            )
            .describe("Where volume and brightness indicators appear; click an edge of the preview")
            .preview(osd_preview),
            opacity_field("Opacity", lens!(opacity or 1.0)),
        ]
    }
}

impl Editable for NotificationConfig {
    fn title() -> &'static str {
        "Notifications"
    }

    fn fields() -> Vec<Field<Self>> {
        flatten(Self::groups())
    }

    fn groups() -> Vec<Group<Self>> {
        vec![
            Group::new(
                "Popups",
                vec![
                    Field::bundle(
                        "popups",
                        "Popups",
                        vec![
                            Field::choice(
                                "popup_position",
                                "Corner",
                                lens!(popup_position),
                                &NotificationPopupPosition::ALL,
                                NotificationPopupPosition::label,
                            ),
                            Field::number("popup_width", "Width", lens!(popup_width as f64))
                                .range(200.0, 800.0)
                                .unit("px"),
                            Field::number(
                                "popup_height",
                                "Stack height",
                                lens!(popup_height as f64),
                            )
                            .range(100.0, 1200.0)
                            .unit("px")
                            .describe("Most the stack of popups may take at rest"),
                            Field::integer(
                                "popup_stack_limit",
                                "Stack limit",
                                lens!(popup_stack_limit as f64),
                            )
                            .range(1.0, 64.0)
                            .describe("Popups kept on screen before the oldest is dropped"),
                            Field::number(
                                "popup_margin_top",
                                "Margin top",
                                lens!(popup_margin_top as f64),
                            )
                            .range(0.0, 400.0)
                            .unit("px"),
                            Field::number(
                                "popup_margin_right",
                                "Margin right",
                                lens!(popup_margin_right as f64),
                            )
                            .range(0.0, 400.0)
                            .unit("px"),
                            Field::number(
                                "popup_margin_bottom",
                                "Margin bottom",
                                lens!(popup_margin_bottom as f64),
                            )
                            .range(0.0, 400.0)
                            .unit("px"),
                            Field::number(
                                "popup_margin_left",
                                "Margin left",
                                lens!(popup_margin_left as f64),
                            )
                            .range(0.0, 400.0)
                            .unit("px"),
                        ],
                    )
                    .icon(IconName::Layout)
                    .describe("Click a corner of the preview to move the popups there")
                    .preview(notification_preview),
                ],
            ),
            Group::new(
                "Cards",
                vec![
                    Field::number(
                        "popup_card_collapsed_height",
                        "Collapsed height",
                        lens!(popup_card_collapsed_height as f64),
                    )
                    .range(40.0, 400.0)
                    .unit("px"),
                    Field::number(
                        "popup_card_expanded_height",
                        "Expanded height",
                        lens!(popup_card_expanded_height as f64),
                    )
                    .range(40.0, 600.0)
                    .unit("px"),
                    opacity_field("Popup opacity", lens!(opacity or 1.0)),
                ],
            ),
            Group::new(
                "Notification center",
                vec![
                    Field::number("center_width", "Width", lens!(center_width as f64))
                        .range(240.0, 1000.0)
                        .unit("px"),
                    Field::number("center_height", "Height", lens!(center_height as f64))
                        .range(200.0, 1600.0)
                        .unit("px"),
                ],
            ),
        ]
    }
}

impl Editable for ControlCenterConfig {
    fn title() -> &'static str {
        "Control center"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::text(
                "power_actions.sleep",
                "Sleep command",
                lens!(power_actions.sleep),
            )
            .hint(TextHint::Command),
            Field::text(
                "power_actions.reboot",
                "Reboot command",
                lens!(power_actions.reboot),
            )
            .hint(TextHint::Command),
            Field::text(
                "power_actions.poweroff",
                "Power off command",
                lens!(power_actions.poweroff),
            )
            .hint(TextHint::Command),
        ]
    }
}

impl Editable for DockConfig {
    fn title() -> &'static str {
        "Dock"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::bundle(
                "placement",
                "Position and behaviour",
                vec![
                    Field::choice(
                        "position",
                        "Position",
                        lens!(position),
                        &BarPosition::ALL,
                        BarPosition::label,
                    ),
                    Field::choice(
                        "style",
                        "Style",
                        lens!(style),
                        &DockStyle::ALL,
                        DockStyle::label,
                    )
                    .describe("Floating off the edge like a card, or attached flush to it"),
                    Field::toggle("stretch", "Stretch", lens!(stretch))
                        .describe("Span the whole edge"),
                    Field::choice(
                        "align",
                        "Align",
                        lens!(align),
                        &DockAlign::ALL,
                        DockAlign::label,
                    )
                    .describe("Where the icons sit along the edge"),
                    Field::toggle("launcher", "Launcher", lens!(launcher))
                        .describe("A button at the start of the dock that opens the launcher"),
                    Field::toggle("reserve_space", "Reserve", lens!(reserve_space)).describe(
                        "Reserve the dock's space so windows stay off it, as they do the bar. \
                         A reserved dock never hides",
                    ),
                    Field::choice(
                        "visibility",
                        "Visibility",
                        lens!(visibility),
                        &DockVisibility::ALL,
                        DockVisibility::label,
                    )
                    .describe("Ignored while the dock reserves its space"),
                    Field::choice(
                        "hover_effect",
                        "Hover effect",
                        lens!(hover_effect),
                        &DockHoverEffect::ALL,
                        DockHoverEffect::label,
                    ),
                ],
            )
            .icon(IconName::Layout)
            .describe("Click an edge of the preview to move the dock there")
            .preview(dock_preview),
            Field::choice(
                "monitors",
                "Monitors",
                lens!(monitors),
                &DockMonitors::ALL,
                DockMonitors::label,
            ),
            Field::number("icon_size", "Icon size", lens!(icon_size as f64))
                .range(16.0, 96.0)
                .unit("px"),
            opacity_field("Opacity", lens!(opacity or 1.0)),
        ]
    }
}

impl Editable for PanelsConfig {
    fn title() -> &'static str {
        "Panels"
    }

    fn fields() -> Vec<Field<Self>> {
        vec![
            Field::toggle(
                "close_on_click_outside",
                "Close on click outside",
                lens!(close_on_click_outside),
            )
            .describe("That click is not passed on to what is underneath"),
            opacity_field("Opacity", lens!(opacity or 1.0)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key an `Editable` names must exist in the struct's TOML, or the
    /// store would write a key serde ignores.
    fn keys_resolve<S: Editable>() {
        use super::super::schema::Editor;

        fn has_key(section: &impl serde::Serialize, key: &str) -> bool {
            let table: toml::Table = toml::from_str(&toml::to_string(section).unwrap()).unwrap();
            let mut node = Some(&toml::Value::Table(table));
            for segment in key.split('.') {
                node = node.and_then(|v| v.get(segment));
            }
            node.is_some()
        }

        let fields: Vec<_> = S::fields()
            .iter()
            .flat_map(|field| match &field.editor {
                Editor::Bundle(fields) => fields.to_vec(),
                _ => vec![field.clone()],
            })
            .collect();
        for field in fields {
            // A custom page or a preset writes its own keys; `key` only names the row.
            if matches!(
                field.editor,
                Editor::Custom { .. }
                    | Editor::Choice {
                        commit: Some(_),
                        ..
                    }
            ) {
                continue;
            }
            let mut sample = S::default();
            // An unset `Option` has no key in the default's TOML; give it a
            // value and the key has to appear.
            if !has_key(&sample, field.key)
                && let Editor::Number { lens, .. } = &field.editor
            {
                let shown = (lens.get)(&sample);
                (lens.set)(&mut sample, shown);
            }
            assert!(
                has_key(&sample, field.key),
                "{}: key {:?} missing",
                S::title(),
                field.key
            );
        }
    }

    #[test]
    fn every_field_key_exists_in_its_struct() {
        keys_resolve::<Config>();
        keys_resolve::<BarConfig>();
        keys_resolve::<BarFloatingConfig>();
        keys_resolve::<FrameConfig>();
        keys_resolve::<BarSegmentsConfig>();
        keys_resolve::<crate::bar::config::ModulesConfig>();
        keys_resolve::<ClockConfig>();
        keys_resolve::<BatteryConfig>();
        keys_resolve::<BarWorkspacesConfig>();
        keys_resolve::<TrayConfig>();
        keys_resolve::<BarSysInfoConfig>();
        keys_resolve::<BarMprisConfig>();
        keys_resolve::<ActiveWindowConfig>();
        keys_resolve::<KeyboardLayoutConfig>();
        keys_resolve::<LauncherConfig>();
        keys_resolve::<LauncherModulesConfig>();
        keys_resolve::<ShellConfig>();
        keys_resolve::<WallpaperConfig>();
        keys_resolve::<AppsConfig>();
        keys_resolve::<WebConfig>();
        keys_resolve::<ConfigEditorConfig>();
        keys_resolve::<OsdConfig>();
        keys_resolve::<NotificationConfig>();
        keys_resolve::<ControlCenterConfig>();
        keys_resolve::<DockConfig>();
        keys_resolve::<PanelsConfig>();
    }

    /// Every registered launcher module has a settings row.
    #[test]
    fn every_launcher_module_is_listed() {
        let listed: Vec<&str> = LauncherModulesConfig::fields()
            .iter()
            .map(|field| field.key)
            .collect();
        for descriptor in modules::registry(&LauncherModulesConfig::default()) {
            assert!(
                listed.contains(&descriptor.meta.id),
                "{}",
                descriptor.meta.id
            );
        }
    }

    /// Choice values are written with the enum's serde spelling, whatever
    /// its `rename_all`.
    #[test]
    fn choices_use_serde_spellings() {
        let dock = DockConfig::default();
        // `visibility` sits in the placement bundle, `monitors` on its own.
        let field = |key: &str| {
            DockConfig::fields()
                .into_iter()
                .flat_map(|field| match &field.editor {
                    super::super::schema::Editor::Bundle(fields) => fields.to_vec(),
                    _ => vec![field],
                })
                .find(|f| f.key == key)
                .unwrap()
        };
        assert_eq!(
            field("visibility")
                .current_value(&dock)
                .unwrap()
                .to_string()
                .trim(),
            "\"intelligent-hide\""
        );
        assert_eq!(
            field("monitors")
                .current_value(&dock)
                .unwrap()
                .to_string()
                .trim(),
            "\"primaryonly\""
        );
    }

    /// Numbers keep the field's own TOML type.
    #[test]
    fn integers_are_written_as_integers() {
        let field = ActiveWindowConfig::fields()
            .into_iter()
            .find(|f| f.key == "max_length")
            .unwrap();
        let value = field.current_value(&ActiveWindowConfig::default()).unwrap();
        assert!(value.is_integer(), "{value}");
        assert!(field.is_modified(&ActiveWindowConfig {
            max_length: 7,
            ..Default::default()
        }));
    }
}
