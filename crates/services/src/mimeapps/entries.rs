//! The desktop entries on the system, with the types each one opens.
//!
//! [`ApplicationsService`](crate::ApplicationsService) lists what a launcher
//! shows and drops `NoDisplay` entries. Those are what this module is for as
//! well: the handler of a `x-scheme-handler/…` type is usually hidden from the
//! launcher and still a fine thing to pick as a default.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::applications::get_application_dirs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    /// The desktop file ID: `firefox.desktop`, or `kde-ark.desktop` for
    /// `kde/ark.desktop`. What `mimeapps.list` calls the application.
    pub id: String,
    pub name: String,
    /// The `Icon` key as written: a theme name or a path.
    pub icon: Option<String>,
    pub path: PathBuf,
    /// The `MimeType` key.
    pub mimes: Vec<String>,
    /// Left out of menus. Such an entry can still be a default.
    pub no_display: bool,
}

impl DesktopEntry {
    pub fn opens(&self, mime: &str) -> bool {
        self.mimes.iter().any(|m| m == mime)
    }
}

/// Every desktop entry, sorted by name.
#[derive(Debug, Clone, Default)]
pub struct DesktopIndex {
    entries: Vec<DesktopEntry>,
    by_id: HashMap<String, usize>,
    by_mime: HashMap<String, Vec<usize>>,
}

impl DesktopIndex {
    /// Scan the XDG application directories.
    pub fn scan() -> Self {
        Self::scan_dirs(&get_application_dirs())
    }

    /// `dirs` in priority order: an entry found in an earlier one hides the
    /// same ID in a later one, as the desktop entry specification says.
    pub fn scan_dirs(dirs: &[PathBuf]) -> Self {
        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        for dir in dirs {
            walk(dir, "", 0, &mut seen, &mut entries);
        }
        entries.sort_by(|a: &DesktopEntry, b| {
            (a.name.to_lowercase(), &a.id).cmp(&(b.name.to_lowercase(), &b.id))
        });

        let mut by_id = HashMap::new();
        let mut by_mime: HashMap<String, Vec<usize>> = HashMap::new();
        for (ix, entry) in entries.iter().enumerate() {
            by_id.insert(entry.id.clone(), ix);
            for mime in &entry.mimes {
                by_mime.entry(mime.clone()).or_default().push(ix);
            }
        }
        Self {
            entries,
            by_id,
            by_mime,
        }
    }

    pub fn all(&self) -> &[DesktopEntry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&DesktopEntry> {
        self.by_id.get(id).map(|&ix| &self.entries[ix])
    }

    /// The entries that list `mime`, by name.
    pub fn opening<'a>(&'a self, mime: &str) -> impl Iterator<Item = &'a DesktopEntry> + 'a {
        self.by_mime
            .get(mime)
            .into_iter()
            .flatten()
            .map(|&ix| &self.entries[ix])
    }

    /// Every type some entry lists.
    pub fn mimes(&self) -> impl Iterator<Item = &str> {
        self.by_mime.keys().map(String::as_str)
    }
}

/// The desktop file of `id`, if one is installed: a stat in each directory
/// instead of a scan, for callers that ask about a handful of IDs.
pub fn find_desktop_file(id: &str) -> Option<PathBuf> {
    get_application_dirs().into_iter().find_map(|dir| {
        // `kde-ark.desktop` may be `kde/ark.desktop`: try each split.
        let direct = dir.join(id);
        if direct.is_file() {
            return Some(direct);
        }
        id.match_indices('-').find_map(|(at, _)| {
            let nested = dir.join(&id[..at]).join(&id[at + 1..]);
            nested.is_file().then_some(nested)
        })
    })
}

/// Entries nest at most this deep; anything deeper is a symlink loop or a
/// mistake.
const MAX_DEPTH: usize = 4;

fn walk(
    dir: &Path,
    prefix: &str,
    depth: usize,
    seen: &mut HashSet<String>,
    out: &mut Vec<DesktopEntry>,
) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if path.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, &format!("{prefix}{name}-"), depth + 1, seen, out);
            }
            continue;
        }
        if !name.ends_with(".desktop") {
            continue;
        }
        let id = format!("{prefix}{name}");
        if seen.contains(&id) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        match parse(&id, &path, &text) {
            Parsed::Entry(entry) => {
                seen.insert(id);
                out.push(entry);
            }
            // `Hidden` is how an entry says it is deleted, and it hides the
            // same ID further down the list too.
            Parsed::Deleted => {
                seen.insert(id);
            }
            Parsed::NotAnApplication => {}
        }
    }
}

enum Parsed {
    Entry(DesktopEntry),
    Deleted,
    NotAnApplication,
}

fn parse(id: &str, path: &Path, text: &str) -> Parsed {
    let (mut name, mut icon, mut mimes) = (None, None, Vec::new());
    let (mut no_display, mut hidden, mut kind) = (false, false, None);
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Type" => kind = Some(value.to_string()),
            "Name" if name.is_none() => name = Some(value.to_string()),
            "Icon" => icon = Some(value.to_string()),
            "NoDisplay" => no_display = value == "true",
            "Hidden" => hidden = value == "true",
            "MimeType" => {
                mimes = value
                    .split(';')
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            _ => {}
        }
    }
    if hidden {
        return Parsed::Deleted;
    }
    if kind.as_deref().is_some_and(|kind| kind != "Application") {
        return Parsed::NotAnApplication;
    }
    Parsed::Entry(DesktopEntry {
        id: id.to_string(),
        name: name.unwrap_or_else(|| id.trim_end_matches(".desktop").to_string()),
        icon,
        path: path.to_path_buf(),
        mimes,
        no_display,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn temp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("conchy-mimeapps-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_types_and_keeps_hidden_handlers() {
        let dir = temp("types");
        write(
            &dir,
            "firefox.desktop",
            "[Desktop Entry]\nType=Application\nName=Firefox\nIcon=firefox\nMimeType=text/html;x-scheme-handler/http;\n\n[Desktop Action new]\nName=New window\n",
        );
        write(
            &dir,
            "url.desktop",
            "[Desktop Entry]\nName=Link handler\nNoDisplay=true\nMimeType=x-scheme-handler/clash;\n",
        );
        let index = DesktopIndex::scan_dirs(std::slice::from_ref(&dir));

        let firefox = index.get("firefox.desktop").unwrap();
        assert_eq!(firefox.name, "Firefox");
        assert!(firefox.opens("text/html") && !firefox.opens("image/png"));
        // The action's `Name` did not overwrite the entry's.
        assert!(index.get("url.desktop").unwrap().no_display);
        let names: Vec<_> = index
            .opening("x-scheme-handler/clash")
            .map(|e| &*e.name)
            .collect();
        assert_eq!(names, ["Link handler"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn earlier_directory_wins_and_hidden_deletes() {
        let user = temp("user");
        let system = temp("system");
        write(&user, "a.desktop", "[Desktop Entry]\nName=Mine\n");
        write(&system, "a.desktop", "[Desktop Entry]\nName=Theirs\n");
        write(
            &user,
            "gone.desktop",
            "[Desktop Entry]\nName=Gone\nHidden=true\n",
        );
        write(
            &system,
            "gone.desktop",
            "[Desktop Entry]\nName=Still here\n",
        );
        let index = DesktopIndex::scan_dirs(&[user.clone(), system.clone()]);

        assert_eq!(index.get("a.desktop").unwrap().name, "Mine");
        assert!(index.get("gone.desktop").is_none());
        fs::remove_dir_all(user).unwrap();
        fs::remove_dir_all(system).unwrap();
    }

    #[test]
    fn subdirectory_becomes_a_dash_in_the_id() {
        let dir = temp("nested");
        write(&dir, "kde/ark.desktop", "[Desktop Entry]\nName=Ark\n");
        write(
            &dir,
            "link.desktop",
            "[Desktop Entry]\nType=Link\nName=Not an app\n",
        );
        let index = DesktopIndex::scan_dirs(std::slice::from_ref(&dir));

        assert_eq!(index.get("kde-ark.desktop").unwrap().name, "Ark");
        assert!(index.get("link.desktop").is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
