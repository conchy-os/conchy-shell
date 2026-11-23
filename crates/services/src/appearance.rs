//! Fonts, font DPI, and the cursor and icon themes.
//!
//! Desktop programs do not share one place to look these up, so a change is
//! written to each place a program may read it from: GNOME's `gsettings` (GTK
//! 4 and the settings portal), GTK 3's and 4's `settings.ini`, `xsettingsd`
//! (X11 programs), Xresources, the cursor theme named by
//! `~/.icons/default/index.theme`, the compositor itself, and the session's
//! environment files for the next login. Each place is best effort: a missing
//! tool or file is skipped, a failing one is reported, and neither stops the
//! rest.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::compositor::CompositorBackend;
use crate::{fsutil, ini, kdl_edit, wm_config};

const GSETTINGS_SCHEMA: &str = "org.gnome.desktop.interface";

// -- model --------------------------------------------------------------------

/// A font as GTK writes it: `JetBrainsMono Nerd Font 10.5`.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// The family, with any style words that came before the size.
    pub family: String,
    /// Points.
    pub size: f64,
}

impl Font {
    pub fn new(family: impl Into<String>, size: f64) -> Self {
        Self {
            family: family.into(),
            size,
        }
    }

    pub fn parse(text: &str) -> Self {
        let text = text.trim().trim_matches('\'').trim();
        if let Some((family, size)) = text.rsplit_once(' ')
            && let Ok(size) = size.parse::<f64>()
        {
            return Self::new(family.trim(), size);
        }
        Self::new(text, 11.0)
    }

    /// `JetBrainsMono Nerd Font 10.5`, `Cantarell 11`.
    pub fn description(&self) -> String {
        format!("{} {}", self.family, trim_number(self.size))
    }
}

fn trim_number(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontRole {
    Interface,
    Document,
    Monospace,
}

impl FontRole {
    fn gsettings_key(self) -> &'static str {
        match self {
            Self::Interface => "font-name",
            Self::Document => "document-font-name",
            Self::Monospace => "monospace-font-name",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppearanceSettings {
    pub interface_font: Font,
    pub document_font: Font,
    pub monospace_font: Font,
    /// Multiplier on every font's size, GTK 4's setting.
    pub text_scale: f64,
    /// Dots per inch fonts are drawn at, X11's and GTK 3's setting.
    pub dpi: u32,
    pub cursor_theme: String,
    pub cursor_size: u32,
    pub icon_theme: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            interface_font: Font::new("Sans", 11.0),
            document_font: Font::new("Sans", 11.0),
            monospace_font: Font::new("Monospace", 11.0),
            text_scale: 1.0,
            dpi: 96,
            cursor_theme: "default".to_string(),
            cursor_size: 24,
            icon_theme: "hicolor".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Font(FontRole, Font),
    TextScale(f64),
    Dpi(u32),
    CursorTheme(String),
    CursorSize(u32),
    IconTheme(String),
}

impl AppearanceSettings {
    pub fn apply(&mut self, change: &Change) {
        match change {
            Change::Font(FontRole::Interface, f) => self.interface_font = f.clone(),
            Change::Font(FontRole::Document, f) => self.document_font = f.clone(),
            Change::Font(FontRole::Monospace, f) => self.monospace_font = f.clone(),
            Change::TextScale(v) => self.text_scale = *v,
            Change::Dpi(v) => self.dpi = *v,
            Change::CursorTheme(v) => self.cursor_theme = v.clone(),
            Change::CursorSize(v) => self.cursor_size = *v,
            Change::IconTheme(v) => self.icon_theme = v.clone(),
        }
    }
}

/// What a change reached and what it could not.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub applied: Vec<String>,
    pub failed: Vec<String>,
}

impl Report {
    fn ok(&mut self, place: &str) {
        self.applied.push(place.to_string());
    }

    fn fail(&mut self, place: &str, why: impl std::fmt::Display) {
        self.failed.push(format!("{place}: {why}"));
    }

    /// Nothing took the change.
    pub fn is_failure(&self) -> bool {
        self.applied.is_empty() && !self.failed.is_empty()
    }
}

// -- where the files are ------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Paths {
    pub gtk3: PathBuf,
    pub gtk4: PathBuf,
    pub xsettingsd: PathBuf,
    /// Files whose `[Icon Theme]` `Inherits=` names the default cursor theme.
    pub cursor_defaults: Vec<PathBuf>,
    pub uwsm_env: PathBuf,
    pub session_kdl: PathBuf,
}

impl Paths {
    pub fn standard() -> anyhow::Result<Self> {
        let config = fsutil::config_home()?;
        let home = fsutil::home()?;
        Ok(Self {
            gtk3: config.join("gtk-3.0/settings.ini"),
            gtk4: config.join("gtk-4.0/settings.ini"),
            xsettingsd: config.join("xsettingsd/xsettingsd.conf"),
            cursor_defaults: vec![
                home.join(".icons/default/index.theme"),
                fsutil::data_home()?.join("icons/default/index.theme"),
            ],
            uwsm_env: config.join("uwsm/env"),
            session_kdl: config.join("conchy/session.kdl"),
        })
    }
}

// -- reading ------------------------------------------------------------------

/// The settings as programs would see them now.
pub fn load(paths: &Paths) -> AppearanceSettings {
    let mut s = AppearanceSettings::default();
    let gtk = |key: &str| -> Option<String> {
        [&paths.gtk3, &paths.gtk4].into_iter().find_map(|p| {
            fs::read_to_string(p)
                .ok()
                .and_then(|t| ini::get_key(&t, "Settings", key))
        })
    };

    s.interface_font = gsettings_get("font-name")
        .or_else(|| gtk("gtk-font-name"))
        .map(|t| Font::parse(&t))
        .unwrap_or(s.interface_font);
    s.document_font = gsettings_get("document-font-name")
        .map(|t| Font::parse(&t))
        .unwrap_or_else(|| s.interface_font.clone());
    s.monospace_font = gsettings_get("monospace-font-name")
        .map(|t| Font::parse(&t))
        .unwrap_or(s.monospace_font);
    s.text_scale = gsettings_get("text-scaling-factor")
        .and_then(|t| t.parse().ok())
        .unwrap_or(1.0);
    s.cursor_theme = gsettings_get("cursor-theme")
        .map(|t| t.trim_matches('\'').to_string())
        .or_else(|| gtk("gtk-cursor-theme-name"))
        .unwrap_or(s.cursor_theme);
    s.cursor_size = gsettings_get("cursor-size")
        .and_then(|t| t.parse().ok())
        .or_else(|| gtk("gtk-cursor-theme-size").and_then(|t| t.parse().ok()))
        .unwrap_or(s.cursor_size);
    s.icon_theme = gsettings_get("icon-theme")
        .map(|t| t.trim_matches('\'').to_string())
        .or_else(|| gtk("gtk-icon-theme-name"))
        .unwrap_or(s.icon_theme);
    s.dpi = fs::read_to_string(&paths.xsettingsd)
        .ok()
        .and_then(|t| xsettings_get(&t, "Xft/DPI"))
        .and_then(|v| v.parse::<u32>().ok())
        .or_else(|| gtk("gtk-xft-dpi").and_then(|v| v.parse().ok()))
        .map_or(96, |v| (v + 512) / 1024);
    s
}

fn gsettings_get(key: &str) -> Option<String> {
    let out = Command::new("gsettings")
        .args(["get", GSETTINGS_SCHEMA, key])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|t| !t.is_empty())
}

// -- writing ------------------------------------------------------------------

/// Make `change` take effect everywhere it can.
pub fn apply(paths: &Paths, backend: CompositorBackend, change: &Change) -> Report {
    let mut report = Report::default();
    write_files(paths, change, &mut report);
    run_tools(paths, backend, change, &mut report);
    report
}

/// The files a change goes into.
pub fn write_files(paths: &Paths, change: &Change, report: &mut Report) {
    // GTK's own files.
    let gtk_edits: Vec<(&str, String)> = match change {
        Change::Font(FontRole::Interface, f) => vec![("gtk-font-name", f.description())],
        Change::Dpi(dpi) => vec![("gtk-xft-dpi", (u64::from(*dpi) * 1024).to_string())],
        Change::CursorTheme(name) => vec![("gtk-cursor-theme-name", name.clone())],
        Change::CursorSize(size) => vec![("gtk-cursor-theme-size", size.to_string())],
        Change::IconTheme(name) => vec![("gtk-icon-theme-name", name.clone())],
        Change::Font(..) | Change::TextScale(_) => Vec::new(),
    };
    if !gtk_edits.is_empty() {
        for (label, path) in [("GTK 3", &paths.gtk3), ("GTK 4", &paths.gtk4)] {
            if !path.exists() {
                continue;
            }
            let edit = |text: &str| {
                gtk_edits.iter().fold(text.to_string(), |text, (key, value)| {
                    ini::set_key(&text, "Settings", key, Some(value))
                })
            };
            match edit_file(path, edit) {
                Ok(()) => report.ok(&format!("{label} settings.ini")),
                Err(err) => report.fail(&format!("{label} settings.ini"), err),
            }
        }
    }

    // xsettingsd, which X11 programs read.
    let xsettings: Vec<(&str, String)> = match change {
        Change::Font(FontRole::Interface, f) => {
            vec![("Gtk/FontName", format!("{:?}", f.description()))]
        }
        Change::Dpi(dpi) => vec![("Xft/DPI", (u64::from(*dpi) * 1024).to_string())],
        Change::CursorTheme(name) => vec![("Gtk/CursorThemeName", format!("{name:?}"))],
        Change::CursorSize(size) => vec![("Gtk/CursorThemeSize", size.to_string())],
        Change::IconTheme(name) => vec![("Net/IconThemeName", format!("{name:?}"))],
        _ => Vec::new(),
    };
    if !xsettings.is_empty() && paths.xsettingsd.exists() {
        let edit = |text: &str| {
            xsettings
                .iter()
                .fold(text.to_string(), |text, (key, value)| xsettings_set(&text, key, value))
        };
        match edit_file(&paths.xsettingsd, edit) {
            Ok(()) => report.ok("xsettingsd.conf"),
            Err(err) => report.fail("xsettingsd.conf", err),
        }
    }

    // The default cursor theme X11 and Xwayland programs fall back to.
    if let Change::CursorTheme(name) = change {
        for path in paths.cursor_defaults.iter().filter(|p| p.exists()) {
            let edit = |text: &str| ini::set_key(text, "Icon Theme", "Inherits", Some(name));
            match edit_file(path, edit) {
                Ok(()) => report.ok("default cursor theme"),
                Err(err) => report.fail("default cursor theme", err),
            }
        }
    }

    // What the next login starts with.
    let env: Vec<(&str, String)> = match change {
        Change::CursorTheme(name) => vec![("XCURSOR_THEME", name.clone()), ("HYPRCURSOR_THEME", name.clone())],
        Change::CursorSize(size) => vec![("XCURSOR_SIZE", size.to_string()), ("HYPRCURSOR_SIZE", size.to_string())],
        _ => Vec::new(),
    };
    if !env.is_empty() {
        if paths.uwsm_env.exists() {
            let edit = |text: &str| {
                env.iter()
                    .fold(text.to_string(), |text, (key, value)| env_file_set(&text, key, value))
            };
            match edit_file(&paths.uwsm_env, edit) {
                Ok(()) => report.ok("uwsm env"),
                Err(err) => report.fail("uwsm env", err),
            }
        }
        if paths.session_kdl.exists() {
            let result = fsutil::read_optional(&paths.session_kdl)
                .and_then(|text| kdl_edit::parse(&text.unwrap_or_default()))
                .and_then(|mut doc| {
                    for (key, value) in env.iter().filter(|(k, _)| k.starts_with("XCURSOR")) {
                        kdl_edit::set_args(&mut doc, &["env", key], vec![kdl_edit::string(value)])?;
                    }
                    fsutil::write_through(&paths.session_kdl, &doc.to_string())
                });
            match result {
                Ok(()) => report.ok("conchy session env"),
                Err(err) => report.fail("conchy session env", err),
            }
        }
    }
}

/// The programs and compositors that take a change at run time.
fn run_tools(paths: &Paths, backend: CompositorBackend, change: &Change, report: &mut Report) {
    // gsettings: GTK 4, libadwaita, the settings portal.
    let gsettings: Option<(&str, String)> = match change {
        Change::Font(role, f) => Some((role.gsettings_key(), format!("{:?}", f.description()))),
        Change::TextScale(v) => Some(("text-scaling-factor", format!("{v:.2}"))),
        Change::CursorTheme(name) => Some(("cursor-theme", format!("{name:?}"))),
        Change::CursorSize(size) => Some(("cursor-size", size.to_string())),
        Change::IconTheme(name) => Some(("icon-theme", format!("{name:?}"))),
        Change::Dpi(_) => None,
    };
    if let Some((key, value)) = gsettings {
        match run("gsettings", &["set", GSETTINGS_SCHEMA, key, &value]) {
            Ok(()) => report.ok("gsettings"),
            Err(err) => report.fail("gsettings", err),
        }
    }

    // Xresources, for X11 programs that read it.
    if let Change::Dpi(dpi) = change
        && std::env::var_os("DISPLAY").is_some()
    {
        match run_with_input("xrdb", &["-merge"], &format!("Xft.dpi: {dpi}\n")) {
            Ok(()) => report.ok("Xresources"),
            Err(err) => report.fail("Xresources", err),
        }
    }

    // Tell a running xsettingsd to read its file again.
    if matches!(
        change,
        Change::Font(FontRole::Interface, _)
            | Change::Dpi(_)
            | Change::CursorTheme(_)
            | Change::CursorSize(_)
            | Change::IconTheme(_)
    ) && paths.xsettingsd.exists()
    {
        // No daemon running is not a failure.
        let _ = run("pkill", &["-HUP", "-x", "xsettingsd"]);
    }

    // The compositor's own cursor.
    if matches!(change, Change::CursorTheme(_) | Change::CursorSize(_)) {
        apply_cursor_to_compositor(paths, backend, change, report);
    }
}

fn apply_cursor_to_compositor(
    paths: &Paths,
    backend: CompositorBackend,
    change: &Change,
    report: &mut Report,
) {
    // The full pair is needed whichever half changed.
    let current = load(paths);
    let mut s = current;
    s.apply(change);
    match backend {
        CompositorBackend::Hyprland => {
            match run("hyprctl", &["setcursor", &s.cursor_theme, &s.cursor_size.to_string()]) {
                Ok(()) => report.ok("Hyprland cursor"),
                Err(err) => report.fail("Hyprland cursor", err),
            }
        }
        CompositorBackend::Niri => {
            let result = wm_config::update(|doc| {
                kdl_edit::set_args(
                    doc,
                    &["cursor", "xcursor-theme"],
                    vec![kdl_edit::string(&s.cursor_theme)],
                )?;
                kdl_edit::set_args(
                    doc,
                    &["cursor", "xcursor-size"],
                    vec![kdl_edit::int(i64::from(s.cursor_size))],
                )
            });
            match result {
                Ok(()) => report.ok("Conchy WM cursor"),
                Err(err) => report.fail("Conchy WM cursor", err),
            }
        }
        CompositorBackend::Mango => {}
    }
}

fn edit_file(path: &Path, edit: impl FnOnce(&str) -> String) -> anyhow::Result<()> {
    let text = fsutil::read_optional(path)?.unwrap_or_default();
    let new = edit(&text);
    if new == text {
        return Ok(());
    }
    fsutil::write_through(path, &new)
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .output()
        .map_err(|err| format!("{program}: {err}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn run_with_input(program: &str, args: &[&str], input: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("{program}: {err}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .map_err(|err| format!("{program}: {err}"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|err| format!("{program}: {err}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{program}: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

// -- small file formats -------------------------------------------------------

fn xsettings_get(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.trim().split_once(char::is_whitespace)?;
        (k == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// xsettingsd's `Key value` lines: the key's line replaced, or one added.
fn xsettings_set(text: &str, key: &str, literal: &str) -> String {
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|line| {
            let matches = line
                .trim()
                .split_once(char::is_whitespace)
                .is_some_and(|(k, _)| k == key);
            if matches && !found {
                found = true;
                format!("{key} {literal}")
            } else {
                line.to_string()
            }
        })
        .collect();
    if !found {
        out.push(format!("{key} {literal}"));
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

/// `export KEY=value` (or `KEY=value`) in a shell environment file: only an
/// existing line is replaced, a variable the user never set is not added.
fn env_file_set(text: &str, key: &str, value: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let body = line.trim_start();
        let (export, rest) = match body.strip_prefix("export ") {
            Some(rest) => ("export ", rest.trim_start()),
            None => ("", body),
        };
        if rest.strip_prefix(key).is_some_and(|r| r.starts_with('=')) {
            out.push_str(&format!("{export}{key}={}\n", shell_quote(value)));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+'))
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

// -- what is installed --------------------------------------------------------

/// Directories icon and cursor themes live in, the user's first.
fn theme_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = fsutil::home() {
        dirs.push(home.join(".icons"));
    }
    if let Ok(data) = fsutil::data_home() {
        dirs.push(data.join("icons"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    dirs.extend(
        data_dirs
            .split(':')
            .filter(|d| !d.is_empty())
            .map(|d| PathBuf::from(d).join("icons")),
    );
    dirs
}

/// The names of the themes under `dirs` that pass `keep`, once each.
fn themes_in(dirs: &[PathBuf], keep: impl Fn(&Path) -> bool) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for dir in dirs {
        let Ok(read) = fs::read_dir(dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && keep(&path)
            {
                names.insert(name.to_string());
            }
        }
    }
    names.into_iter().collect()
}

/// Icon themes: folders with an `index.theme` that lists icon directories.
pub fn icon_themes() -> Vec<String> {
    icon_themes_in(&theme_dirs())
}

fn icon_themes_in(dirs: &[PathBuf]) -> Vec<String> {
    themes_in(dirs, |path| {
        fs::read_to_string(path.join("index.theme")).is_ok_and(|text| {
            ini::get_key(&text, "Icon Theme", "Directories").is_some()
        })
    })
    .into_iter()
    // Fallbacks every theme inherits, not choices.
    .filter(|name| !matches!(name.as_str(), "hicolor" | "default" | "locolor"))
    .collect()
}

/// Cursor themes: folders with a `cursors` directory.
pub fn cursor_themes() -> Vec<String> {
    cursor_themes_in(&theme_dirs())
}

fn cursor_themes_in(dirs: &[PathBuf]) -> Vec<String> {
    themes_in(dirs, |path| path.join("cursors").is_dir())
        .into_iter()
        .filter(|name| name != "default")
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FontFamilies {
    pub all: Vec<String>,
    pub monospace: Vec<String>,
}

/// The font families installed, and those that are monospaced.
pub fn font_families() -> FontFamilies {
    let list = |pattern: &str| -> Vec<String> {
        let out = Command::new("fc-list")
            .args([pattern, "--format", "%{family[0]}\\n"])
            .stderr(Stdio::null())
            .output();
        let mut names: Vec<String> = out
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    };
    let mut monospace = list(":spacing=mono");
    monospace.extend(list(":spacing=100"));
    monospace.sort_by_key(|n| n.to_lowercase());
    monospace.dedup();
    FontFamilies {
        all: list(":"),
        monospace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(root: &Path) -> Paths {
        Paths {
            gtk3: root.join("gtk-3.0/settings.ini"),
            gtk4: root.join("gtk-4.0/settings.ini"),
            xsettingsd: root.join("xsettingsd.conf"),
            cursor_defaults: vec![root.join("icons/default/index.theme")],
            uwsm_env: root.join("uwsm-env"),
            session_kdl: root.join("session.kdl"),
        }
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn fonts_parse_and_print_the_way_gtk_does() {
        let f = Font::parse("'JetBrainsMono Nerd Font 10.5'");
        assert_eq!(f, Font::new("JetBrainsMono Nerd Font", 10.5));
        assert_eq!(f.description(), "JetBrainsMono Nerd Font 10.5");
        assert_eq!(Font::new("Cantarell Bold", 11.0).description(), "Cantarell Bold 11");
        // No size: a plain default.
        assert_eq!(Font::parse("Sans").size, 11.0);
    }

    #[test]
    fn a_cursor_theme_reaches_every_file_that_names_one() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        write(&p.gtk3, "[Settings]\ngtk-cursor-theme-name=old\ngtk-font-name=Keep 10\n");
        write(&p.gtk4, "[Settings]\ngtk-cursor-theme-name=old\n");
        write(&p.xsettingsd, "Net/ThemeName \"x\"\nGtk/CursorThemeName \"old\"\n");
        write(&p.cursor_defaults[0], "[Icon Theme]\nName=Default\nInherits=old\n");
        write(&p.uwsm_env, "export BROWSER=firefox\nexport XCURSOR_THEME=\"old\"\nexport HYPRCURSOR_THEME=\"old\"\n");
        write(&p.session_kdl, "env {\n    XCURSOR_SIZE \"24\"\n}\n");

        let mut report = Report::default();
        write_files(&p, &Change::CursorTheme("Bibata-Modern-Ice".into()), &mut report);

        assert!(report.failed.is_empty(), "{:?}", report.failed);
        let gtk3 = fs::read_to_string(&p.gtk3).unwrap();
        assert!(gtk3.contains("gtk-cursor-theme-name=Bibata-Modern-Ice"));
        assert!(gtk3.contains("gtk-font-name=Keep 10"));
        assert!(fs::read_to_string(&p.gtk4).unwrap().contains("Bibata-Modern-Ice"));
        let xs = fs::read_to_string(&p.xsettingsd).unwrap();
        assert!(xs.contains("Gtk/CursorThemeName \"Bibata-Modern-Ice\""));
        assert!(xs.contains("Net/ThemeName \"x\""));
        assert!(fs::read_to_string(&p.cursor_defaults[0]).unwrap().contains("Inherits=Bibata-Modern-Ice"));
        let env = fs::read_to_string(&p.uwsm_env).unwrap();
        assert!(env.contains("export XCURSOR_THEME=Bibata-Modern-Ice"));
        assert!(env.contains("export HYPRCURSOR_THEME=Bibata-Modern-Ice"));
        assert!(env.contains("export BROWSER=firefox"));
        // The session's env gets a theme next to the size it had.
        let kdl = fs::read_to_string(&p.session_kdl).unwrap();
        assert!(kdl.contains("XCURSOR_SIZE \"24\""));
        assert!(kdl.contains("XCURSOR_THEME \"Bibata-Modern-Ice\""));
    }

    #[test]
    fn dpi_is_written_in_1024ths() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        write(&p.gtk3, "[Settings]\ngtk-theme-name=x\n");
        write(&p.xsettingsd, "Net/ThemeName \"x\"\n");
        let mut report = Report::default();
        write_files(&p, &Change::Dpi(120), &mut report);
        assert!(fs::read_to_string(&p.gtk3).unwrap().contains("gtk-xft-dpi=122880"));
        assert!(fs::read_to_string(&p.xsettingsd).unwrap().contains("Xft/DPI 122880"));
        // And read back as DPI.
        let loaded = load(&p);
        assert_eq!(loaded.dpi, 120);
    }

    #[test]
    fn files_that_are_not_there_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let mut report = Report::default();
        write_files(&p, &Change::IconTheme("Papirus".into()), &mut report);
        assert!(report.applied.is_empty() && report.failed.is_empty());
        assert!(!p.gtk3.exists());
    }

    #[test]
    fn the_env_file_never_gains_variables() {
        let text = "export A=1\n";
        assert_eq!(env_file_set(text, "XCURSOR_SIZE", "32"), text);
        let text = "XCURSOR_SIZE=24\n";
        assert_eq!(env_file_set(text, "XCURSOR_SIZE", "32"), "XCURSOR_SIZE=32\n");
    }

    #[test]
    fn themes_are_found_by_what_is_inside() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        write(&root.join("Papirus/index.theme"), "[Icon Theme]\nName=P\nDirectories=48x48/apps\n");
        write(&root.join("hicolor/index.theme"), "[Icon Theme]\nDirectories=48x48/apps\n");
        write(&root.join("Bibata/index.theme"), "[Icon Theme]\nName=B\n");
        fs::create_dir_all(root.join("Bibata/cursors")).unwrap();
        fs::create_dir_all(root.join("default")).unwrap();
        let dirs = vec![root];
        assert_eq!(icon_themes_in(&dirs), vec!["Papirus"]);
        assert_eq!(cursor_themes_in(&dirs), vec!["Bibata"]);
    }

    #[test]
    fn a_change_updates_the_model() {
        let mut s = AppearanceSettings::default();
        s.apply(&Change::Font(FontRole::Monospace, Font::new("Mono", 9.0)));
        s.apply(&Change::CursorSize(32));
        assert_eq!(s.monospace_font, Font::new("Mono", 9.0));
        assert_eq!(s.cursor_size, 32);
    }
}
