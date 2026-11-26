//! The programs the session starts at login, from the XDG autostart
//! directories, and switching them on and off.
//!
//! <https://specifications.freedesktop.org/autostart-spec/latest/>
//!
//! An entry in the user's `autostart` directory replaces the system one with
//! the same file name, and `Hidden=true` in it means "not started". So a
//! system entry is switched off by a user file that says so, and back on by
//! removing that file. The user's own entries are often symlinks into a
//! dotfiles repository; changes are written through the link.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};

use crate::fsutil;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// `~/.config/autostart`: the user's own, or an override of a system one.
    User,
    /// `/etc/xdg/autostart` and the other system directories.
    System,
}

/// Why an entry does not start, when it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Off {
    /// `Hidden=true`, or `X-GNOME-Autostart-enabled=false`: the user's switch.
    Switched,
    /// `OnlyShowIn` / `NotShowIn` rule out this desktop.
    OtherDesktop,
    /// `TryExec` names a program that is not installed.
    NotInstalled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The file name, `blueman.desktop`: what the user's file must be called
    /// to override a system one.
    pub id: String,
    pub name: String,
    pub comment: String,
    pub exec: String,
    pub icon: Option<String>,
    /// Where the entry that counts comes from.
    pub origin: Origin,
    pub path: PathBuf,
    /// The user's file replaces a system entry of the same name.
    pub shadows_system: bool,
    /// `None` when the session starts it.
    pub off: Option<Off>,
}

impl Entry {
    pub fn enabled(&self) -> bool {
        self.off.is_none()
    }

    /// The user can turn it on or off with the switch: only `Hidden` is theirs.
    pub fn switchable(&self) -> bool {
        matches!(self.off, None | Some(Off::Switched))
    }

    /// The file is the user's own and no system entry stands behind it.
    pub fn removable(&self) -> bool {
        self.origin == Origin::User && !self.shadows_system
    }
}

#[derive(Debug, Clone)]
pub struct Autostart {
    user: PathBuf,
    system: Vec<PathBuf>,
    /// `XDG_CURRENT_DESKTOP`, lower case.
    desktops: Vec<String>,
}

impl Autostart {
    pub fn new(user: PathBuf, system: Vec<PathBuf>, desktops: Vec<String>) -> Self {
        Self {
            user,
            system,
            desktops,
        }
    }

    /// The directories and desktop of the running session.
    pub fn standard() -> anyhow::Result<Self> {
        let user = fsutil::config_home()?.join("autostart");
        let dirs = std::env::var("XDG_CONFIG_DIRS").unwrap_or_default();
        let mut system: Vec<PathBuf> = dirs
            .split(':')
            .filter(|d| !d.is_empty())
            .map(|d| PathBuf::from(d).join("autostart"))
            .collect();
        let default = PathBuf::from("/etc/xdg/autostart");
        if !system.contains(&default) {
            system.push(default);
        }
        let desktops = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .filter(|d| !d.is_empty())
            .map(str::to_lowercase)
            .collect();
        Ok(Self::new(user, system, desktops))
    }

    pub fn user_dir(&self) -> &Path {
        &self.user
    }

    /// Every entry, by name. A name in several directories counts once, the
    /// user's first.
    pub fn list(&self) -> Vec<Entry> {
        let mut seen = std::collections::HashSet::new();
        let mut entries = Vec::new();
        let dirs = std::iter::once((&self.user, Origin::User))
            .chain(self.system.iter().map(|d| (d, Origin::System)));
        for (dir, origin) in dirs {
            let Ok(read) = fs::read_dir(dir) else {
                continue;
            };
            let mut files: Vec<PathBuf> = read
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "desktop"))
                .collect();
            files.sort();
            for path in files {
                let Some(id) = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
                else {
                    continue;
                };
                if !seen.insert(id.clone()) {
                    continue;
                }
                let Ok(text) = fs::read_to_string(&path) else {
                    continue;
                };
                if let Some(entry) = self.parse(&id, &path, origin, &text) {
                    entries.push(entry);
                }
            }
        }
        entries.sort_by_key(|e| e.name.to_lowercase());
        entries
    }

    fn parse(&self, id: &str, path: &Path, origin: Origin, text: &str) -> Option<Entry> {
        let keys = Keys::read(text);
        if keys.get("Type").is_some_and(|t| t != "Application") {
            return None;
        }
        let exec = keys.get("Exec")?.to_string();
        let name = keys
            .localized("Name")
            .unwrap_or_else(|| id.trim_end_matches(".desktop"))
            .to_string();

        let switched_off = keys.get("Hidden").is_some_and(is_true)
            || keys
                .get("X-GNOME-Autostart-enabled")
                .is_some_and(|v| v.eq_ignore_ascii_case("false"));
        let off = if switched_off {
            Some(Off::Switched)
        } else if !self.shown_here(&keys) {
            Some(Off::OtherDesktop)
        } else if keys.get("TryExec").is_some_and(|t| !program_exists(t)) {
            Some(Off::NotInstalled)
        } else {
            None
        };

        let shadows_system =
            origin == Origin::User && self.system.iter().any(|dir| dir.join(id).is_file());

        Some(Entry {
            id: id.to_string(),
            name,
            comment: keys.localized("Comment").unwrap_or_default().to_string(),
            exec,
            icon: keys.get("Icon").map(str::to_string),
            origin,
            path: path.to_path_buf(),
            shadows_system,
            off,
        })
    }

    /// `OnlyShowIn` and `NotShowIn` against the running desktop.
    fn shown_here(&self, keys: &Keys) -> bool {
        let list = |key: &str| -> Vec<String> {
            keys.get(key)
                .map(|v| {
                    v.split(';')
                        .map(|d| d.trim().to_lowercase())
                        .filter(|d| !d.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };
        let only = list("OnlyShowIn");
        let not = list("NotShowIn");
        if !only.is_empty() && !self.desktops.iter().any(|d| only.contains(d)) {
            return false;
        }
        !self.desktops.iter().any(|d| not.contains(d))
    }

    /// The user's copy says nothing a system entry of the same name does not,
    /// once the switch is cleared: dropping the file loses nothing.
    fn is_pure_override(&self, id: &str, cleared_user_text: &str) -> bool {
        let strip = |text: &str| {
            set_key(
                &set_key(text, "Hidden", None),
                "X-GNOME-Autostart-enabled",
                None,
            )
            .trim()
            .to_string()
        };
        self.system
            .iter()
            .find_map(|dir| fs::read_to_string(dir.join(id)).ok())
            .is_some_and(|system| strip(&system) == strip(cleared_user_text))
    }

    /// Turn an entry on or off.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> anyhow::Result<()> {
        let entry = self
            .list()
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow!("no autostart entry {id}"))?;
        if !entry.switchable() {
            bail!(
                "{} is not shown in this desktop or is not installed; the switch cannot change that",
                entry.name
            );
        }
        if entry.enabled() == enabled {
            return Ok(());
        }
        let user_file = self.user.join(id);
        let text = fs::read_to_string(&entry.path)
            .with_context(|| format!("Failed to read {}", entry.path.display()))?;

        if enabled {
            let cleared = set_key(
                &set_key(&text, "Hidden", None),
                "X-GNOME-Autostart-enabled",
                None,
            );
            // An override that only existed to say "off" goes away with it.
            if entry.origin == Origin::User
                && entry.shadows_system
                && self.is_pure_override(id, &cleared)
            {
                fs::remove_file(&user_file)
                    .with_context(|| format!("Failed to remove {}", user_file.display()))?;
                return Ok(());
            }
            fsutil::write_through(&user_file, &cleared)
        } else {
            let hidden = set_key(
                &set_key(&text, "X-GNOME-Autostart-enabled", None),
                "Hidden",
                Some("true"),
            );
            // A system entry is switched off by a copy in the user's directory.
            fsutil::write_through(&user_file, &hidden)
        }
    }

    /// Add a program of the user's own. Returns the new entry's id.
    pub fn add(&self, name: &str, command: &str) -> anyhow::Result<String> {
        let name = name.trim();
        let command = command.trim();
        if command.is_empty() {
            bail!("the command is empty");
        }
        if command.contains('\n') || name.contains('\n') {
            bail!("a name or command cannot span lines");
        }
        let name = if name.is_empty() {
            command.split_whitespace().next().unwrap_or(command)
        } else {
            name
        };
        let stem = slug(name);
        let mut id = format!("{stem}.desktop");
        let mut n = 2;
        while self.user.join(&id).exists() || self.system.iter().any(|d| d.join(&id).exists()) {
            id = format!("{stem}-{n}.desktop");
            n += 1;
        }
        let text = format!(
            "[Desktop Entry]\nType=Application\nName={name}\nExec={command}\nX-GNOME-Autostart-enabled=true\n"
        );
        fsutil::write_through(&self.user.join(&id), &text)?;
        Ok(id)
    }

    /// Delete the user's own entry. A system entry cannot be removed, only
    /// switched off.
    pub fn remove(&self, id: &str) -> anyhow::Result<()> {
        let entry = self
            .list()
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow!("no autostart entry {id}"))?;
        if !entry.removable() {
            bail!(
                "{} comes from the system; switch it off instead",
                entry.name
            );
        }
        // `remove_file` on a symlink removes the link and leaves the file it
        // points at, which is what a dotfiles repository wants.
        fs::remove_file(&entry.path)
            .with_context(|| format!("Failed to remove {}", entry.path.display()))
    }
}

// -- .desktop text ------------------------------------------------------------

/// The keys of a file's `[Desktop Entry]` group.
struct Keys(Vec<(String, String)>);

impl Keys {
    fn read(text: &str) -> Self {
        let mut keys = Vec::new();
        let mut in_entry = false;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                in_entry = line == "[Desktop Entry]";
                continue;
            }
            if in_entry && let Some((key, value)) = line.split_once('=') {
                keys.push((key.trim().to_string(), value.trim().to_string()));
            }
        }
        Self(keys)
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// `Name[ru_RU]`, then `Name[ru]`, then `Name`, for the user's locale.
    fn localized(&self, key: &str) -> Option<&str> {
        let locale = std::env::var("LC_MESSAGES")
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default();
        let locale = locale.split('.').next().unwrap_or("");
        let lang = locale.split('_').next().unwrap_or("");
        [locale, lang]
            .into_iter()
            .filter(|l| !l.is_empty() && *l != "C" && *l != "POSIX")
            .find_map(|l| self.get(&format!("{key}[{l}]")))
            .or_else(|| self.get(key))
    }
}

fn is_true(value: &str) -> bool {
    value.eq_ignore_ascii_case("true")
}

fn program_exists(program: &str) -> bool {
    let path = Path::new(program);
    if path.is_absolute() {
        return path.is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// A file name for `name`: lower case letters and digits, dashes between.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "startup".to_string()
    } else {
        out
    }
}

/// `text` with `key` in its `[Desktop Entry]` group set to `value`, or the
/// key removed for `None`. Every other line, comments included, is kept.
fn set_key(text: &str, key: &str, value: Option<&str>) -> String {
    crate::ini::set_key(text, "Desktop Entry", key, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "[Desktop Entry]\nType=Application\nName=Blueman\nExec=blueman-applet\n";

    fn setup() -> (tempfile::TempDir, Autostart) {
        let root = tempfile::tempdir().unwrap();
        let user = root.path().join("user");
        let sys = root.path().join("etc");
        fs::create_dir_all(&user).unwrap();
        fs::create_dir_all(&sys).unwrap();
        let a = Autostart::new(user, vec![sys], vec!["conchy".into()]);
        (root, a)
    }

    #[test]
    fn a_user_entry_hides_the_system_one_of_the_same_name() {
        let (root, a) = setup();
        fs::write(root.path().join("etc/blueman.desktop"), APP).unwrap();
        fs::write(
            root.path().join("user/blueman.desktop"),
            format!("{APP}Hidden=true\n"),
        )
        .unwrap();
        let list = a.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].origin, Origin::User);
        assert!(list[0].shadows_system);
        assert_eq!(list[0].off, Some(Off::Switched));
    }

    #[test]
    fn a_system_entry_is_switched_off_by_a_user_copy_and_on_by_removing_it() {
        let (root, a) = setup();
        fs::write(root.path().join("etc/blueman.desktop"), APP).unwrap();

        a.set_enabled("blueman.desktop", false).unwrap();
        let off = &a.list()[0];
        assert!(!off.enabled());
        assert!(root.path().join("user/blueman.desktop").is_file());
        // The system file is never touched.
        assert_eq!(
            fs::read_to_string(root.path().join("etc/blueman.desktop")).unwrap(),
            APP
        );

        a.set_enabled("blueman.desktop", true).unwrap();
        let on = &a.list()[0];
        assert!(on.enabled());
        assert_eq!(on.origin, Origin::System);
        assert!(!root.path().join("user/blueman.desktop").exists());
    }

    #[test]
    fn a_users_own_entry_toggles_in_place_through_a_symlink() {
        let (root, a) = setup();
        let repo = root.path().join("dotfiles/app.desktop");
        fs::create_dir_all(repo.parent().unwrap()).unwrap();
        fs::write(&repo, APP).unwrap();
        let link = root.path().join("user/app.desktop");
        std::os::unix::fs::symlink(&repo, &link).unwrap();

        a.set_enabled("app.desktop", false).unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(fs::read_to_string(&repo).unwrap().contains("Hidden=true"));
        assert!(!a.list()[0].enabled());

        a.set_enabled("app.desktop", true).unwrap();
        assert!(!fs::read_to_string(&repo).unwrap().contains("Hidden"));
        assert!(a.list()[0].enabled());
    }

    #[test]
    fn entries_for_other_desktops_cannot_be_switched() {
        let (root, a) = setup();
        fs::write(
            root.path().join("etc/gnome.desktop"),
            format!("{APP}OnlyShowIn=GNOME;\n"),
        )
        .unwrap();
        let e = &a.list()[0];
        assert_eq!(e.off, Some(Off::OtherDesktop));
        assert!(!e.switchable());
        assert!(a.set_enabled("gnome.desktop", true).is_err());
    }

    #[test]
    fn a_missing_try_exec_is_reported() {
        let (root, a) = setup();
        fs::write(
            root.path().join("etc/x.desktop"),
            format!("{APP}TryExec=/nonexistent/prog\n"),
        )
        .unwrap();
        assert_eq!(a.list()[0].off, Some(Off::NotInstalled));
    }

    #[test]
    fn added_entries_are_listed_and_can_be_removed() {
        let (_root, a) = setup();
        let id = a.add("My Script!", "sh -c 'echo hi'").unwrap();
        assert_eq!(id, "my-script.desktop");
        let e = a.list().remove(0);
        assert_eq!(e.name, "My Script!");
        assert_eq!(e.exec, "sh -c 'echo hi'");
        assert!(e.removable());
        assert_eq!(a.add("My Script!", "true").unwrap(), "my-script-2.desktop");
        a.remove(&id).unwrap();
        assert_eq!(a.list().len(), 1);
    }

    #[test]
    fn system_entries_cannot_be_removed() {
        let (root, a) = setup();
        fs::write(root.path().join("etc/blueman.desktop"), APP).unwrap();
        assert!(a.remove("blueman.desktop").is_err());
    }

    #[test]
    fn set_key_keeps_everything_else() {
        let text =
            "# c\n[Desktop Entry]\nName=A\nHidden=false\nExec=x\n\n[Desktop Action Y]\nName=Y\n";
        let out = set_key(text, "Hidden", Some("true"));
        assert!(out.contains("# c"));
        assert!(out.contains("Hidden=true"));
        assert!(!out.contains("Hidden=false"));
        assert!(out.contains("[Desktop Action Y]\nName=Y"));
        let gone = set_key(&out, "Hidden", None);
        assert!(!gone.contains("Hidden"));
        // A new key lands inside the entry group, not after the action group.
        let added = set_key(text, "Icon", Some("i"));
        assert!(added.find("Icon=i").unwrap() < added.find("[Desktop Action Y]").unwrap());
    }
}
