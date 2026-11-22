//! Where `mimeapps.list` lives, how it is written, and which of the files the
//! specification stacks decides what a type opens with.
//!
//! <https://specifications.freedesktop.org/mime-apps-spec/latest/>

use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};

use super::document::{Group, MimeApps};
use super::entries::DesktopIndex;
use crate::applications::get_application_dirs;

const FILE: &str = "mimeapps.list";

/// The user's `mimeapps.list`: the one file this module writes.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// `$XDG_CONFIG_HOME/mimeapps.list`.
    pub fn standard() -> anyhow::Result<Self> {
        Ok(Self::new(config_home()?.join(FILE)))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Where a write lands. A `mimeapps.list` that is a symlink into a dotfiles
    /// repository is written through, so the link survives.
    pub fn target(&self) -> PathBuf {
        if let Ok(resolved) = fs::canonicalize(&self.path) {
            return resolved;
        }
        // A link whose target does not exist yet: follow it one step by hand,
        // so that saving creates the target and keeps the link.
        match fs::read_link(&self.path) {
            Ok(link) => self
                .path
                .parent()
                .map_or(link.clone(), |parent| parent.join(&link)),
            Err(_) => self.path.clone(),
        }
    }

    /// The file as it is now. A file that is not there yet is an empty one; one
    /// that cannot be read as text is an error, and is never overwritten.
    pub fn load(&self) -> anyhow::Result<MimeApps> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Ok(MimeApps::parse(&text)),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(MimeApps::default()),
            Err(err) => {
                Err(anyhow!(err).context(format!("Failed to read {}", self.path.display())))
            }
        }
    }

    /// Write through a temporary file next to the target and a rename, so
    /// programs that read the file while it is saved see the old one or the
    /// new one and never half of either.
    pub fn save(&self, doc: &MimeApps) -> anyhow::Result<()> {
        let target = self.target();
        let dir = target
            .parent()
            .ok_or_else(|| anyhow!("{} has no parent directory", target.display()))?;
        fs::create_dir_all(dir).with_context(|| format!("Failed to create {}", dir.display()))?;

        let name = target.file_name().and_then(|n| n.to_str()).unwrap_or(FILE);
        let tmp = dir.join(format!(".{name}.conchy-tmp"));
        fs::write(&tmp, doc.render())
            .with_context(|| format!("Failed to write {}", tmp.display()))?;
        if let Ok(meta) = fs::metadata(&target) {
            let _ = fs::set_permissions(&tmp, meta.permissions());
        }
        fs::rename(&tmp, &target).map_err(|err| {
            let _ = fs::remove_file(&tmp);
            anyhow!(err).context(format!("Failed to replace {}", target.display()))
        })
    }
}

/// Where a type's application comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The default set in the user's `mimeapps.list`.
    Config,
    /// A `<desktop>-mimeapps.list` in the config directory. It outranks the
    /// user's own file, so changes made there do not take effect.
    Override(PathBuf),
    /// A system-wide `mimeapps.list`.
    System(PathBuf),
    /// No default is set; this application was added to the type.
    Added,
    /// No default is set; this application's desktop entry lists the type.
    Declared,
}

/// The files that decide what a type opens with, from the strongest down.
#[derive(Debug, Clone, Default)]
pub struct Layers {
    /// `<desktop>-mimeapps.list` in the config home: stronger than the user's.
    pub above: Vec<(PathBuf, MimeApps)>,
    pub user: MimeApps,
    /// The system-wide files, strongest first.
    pub below: Vec<(PathBuf, MimeApps)>,
}

impl Layers {
    /// Read the user's file, which must be readable, and every other file in
    /// the stack that is there.
    pub fn load(store: &Store) -> anyhow::Result<Self> {
        let user = store.load()?;
        let desktops: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .filter(|d| !d.is_empty())
            .map(str::to_lowercase)
            .collect();

        let home = store
            .path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let above = candidates(&home, &desktops, false);

        let mut below = Vec::new();
        for dir in config_dirs() {
            below.extend(candidates(&dir, &desktops, true));
        }
        // The deprecated spot, `$XDG_DATA_HOME/applications`, and the system's.
        for dir in get_application_dirs() {
            below.extend(candidates(&dir, &desktops, true));
        }
        Ok(Self { above, user, below })
    }

    /// The application `mime` opens with according to the files alone: the
    /// first default, in the strongest file, that `installed` accepts; failing
    /// that the first added application that is not removed. What a type's own
    /// desktop entries say is [`resolve`](Self::resolve)'s business.
    pub fn configured(
        &self,
        mime: &str,
        installed: impl Fn(&str) -> bool,
    ) -> Option<(String, Source)> {
        for (source, doc) in self.stack() {
            if let Some(id) = doc
                .get(Group::Default, mime)
                .and_then(|ids| ids.iter().find(|id| installed(id)))
            {
                return Some((id.clone(), source));
            }
        }
        let removed = self.removed(mime);
        self.stack()
            .filter_map(|(_, doc)| doc.get(Group::Added, mime))
            .flatten()
            .find(|id| installed(id) && !removed.contains(&id.as_str()))
            .map(|id| (id.clone(), Source::Added))
    }

    /// [`configured`](Self::configured), then the first application whose
    /// desktop entry lists the type.
    pub fn resolve(&self, mime: &str, index: &DesktopIndex) -> Option<(String, Source)> {
        self.configured(mime, |id| index.get(id).is_some())
            .or_else(|| {
                let removed = self.removed(mime);
                index
                    .opening(mime)
                    .find(|entry| !removed.contains(&entry.id.as_str()))
                    .map(|entry| (entry.id.clone(), Source::Declared))
            })
    }

    fn removed(&self, mime: &str) -> Vec<&str> {
        self.stack()
            .filter_map(|(_, doc)| doc.get(Group::Removed, mime))
            .flatten()
            .map(String::as_str)
            .collect()
    }

    fn stack(&self) -> impl Iterator<Item = (Source, &MimeApps)> {
        let above = self
            .above
            .iter()
            .map(|(path, doc)| (Source::Override(path.clone()), doc));
        let below = self
            .below
            .iter()
            .map(|(path, doc)| (Source::System(path.clone()), doc));
        above
            .chain(std::iter::once((Source::Config, &self.user)))
            .chain(below)
    }
}

/// The files of one directory in the order the specification reads them: each
/// desktop's `<desktop>-mimeapps.list`, then `mimeapps.list`.
fn candidates(dir: &Path, desktops: &[String], with_plain: bool) -> Vec<(PathBuf, MimeApps)> {
    let mut names: Vec<String> = desktops.iter().map(|d| format!("{d}-{FILE}")).collect();
    if with_plain {
        names.push(FILE.to_string());
    }
    names
        .into_iter()
        .filter_map(|name| {
            let path = dir.join(name);
            let text = fs::read_to_string(&path).ok()?;
            Some((path, MimeApps::parse(&text)))
        })
        .collect()
}

fn config_home() -> anyhow::Result<PathBuf> {
    if let Some(dir) = env_path("XDG_CONFIG_HOME") {
        return Ok(dir);
    }
    let home =
        env_path("HOME").ok_or_else(|| anyhow!("Neither XDG_CONFIG_HOME nor HOME is set"))?;
    Ok(home.join(".config"))
}

fn config_dirs() -> Vec<PathBuf> {
    match env_path("XDG_CONFIG_DIRS") {
        Some(dirs) => std::env::split_paths(&dirs).collect(),
        None => vec![PathBuf::from("/etc/xdg")],
    }
}

/// An environment variable that is set and not empty, as a path.
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Every MIME type the shared-mime-info databases know, sorted.
pub fn known_mime_types() -> Vec<String> {
    let mut types = BTreeSet::new();
    for apps in get_application_dirs() {
        let Some(data) = apps.parent() else { continue };
        if let Ok(text) = fs::read_to_string(data.join("mime").join("types")) {
            types.extend(
                text.lines()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(str::to_string),
            );
        }
    }
    types.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("conchy-mimestore-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_loads_empty_and_save_creates_it() {
        let dir = temp("new");
        let store = Store::new(dir.join("conf").join(FILE));
        let mut doc = store.load().unwrap();
        assert_eq!(doc, MimeApps::default());
        doc.set_default("a/b", "x.desktop", true);
        store.save(&doc).unwrap();
        assert_eq!(
            fs::read_to_string(store.path()).unwrap(),
            "[Default Applications]\na/b=x.desktop;\n"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn saving_through_a_symlink_keeps_the_link_and_writes_the_target() {
        let dir = temp("link");
        let repo = dir.join("dotfiles");
        let config = dir.join("config");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&config).unwrap();
        fs::write(repo.join(FILE), "[Default Applications]\na/b=old.desktop\n").unwrap();
        std::os::unix::fs::symlink(repo.join(FILE), config.join(FILE)).unwrap();

        let store = Store::new(config.join(FILE));
        let mut doc = store.load().unwrap();
        doc.set_default("a/b", "new.desktop", true);
        store.save(&doc).unwrap();

        assert!(
            fs::symlink_metadata(config.join(FILE))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(repo.join(FILE)).unwrap(),
            "[Default Applications]\na/b=new.desktop;\n"
        );
        // No temporary file is left in the repository.
        let left: Vec<_> = fs::read_dir(&repo).unwrap().flatten().collect();
        assert_eq!(left.len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_dangling_link_is_followed_not_replaced() {
        let dir = temp("dangling");
        let repo = dir.join("dotfiles");
        fs::create_dir_all(&repo).unwrap();
        std::os::unix::fs::symlink(repo.join(FILE), dir.join(FILE)).unwrap();

        let store = Store::new(dir.join(FILE));
        let mut doc = store.load().unwrap();
        doc.set_default("a/b", "x.desktop", true);
        store.save(&doc).unwrap();

        assert!(
            fs::symlink_metadata(dir.join(FILE))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(repo.join(FILE).is_file());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_file_that_is_not_text_is_an_error_not_an_empty_file() {
        let dir = temp("binary");
        fs::write(dir.join(FILE), [0xff, 0xfe, 0x00]).unwrap();
        assert!(Store::new(dir.join(FILE)).load().is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    fn layers(above: &str, user: &str, below: &str) -> Layers {
        Layers {
            above: vec![(
                PathBuf::from("/c/conchy-mimeapps.list"),
                MimeApps::parse(above),
            )],
            user: MimeApps::parse(user),
            below: vec![(PathBuf::from("/etc/mimeapps.list"), MimeApps::parse(below))],
        }
    }

    #[test]
    fn the_strongest_file_with_an_installed_default_wins() {
        let l = layers(
            "[Default Applications]\na/b=gone.desktop\n",
            "[Default Applications]\na/b=mine.desktop\n",
            "[Default Applications]\na/b=sys.desktop\n",
        );
        // The override names an app that is not installed, so the user's file counts.
        let got = l.configured("a/b", |id| id != "gone.desktop");
        assert_eq!(got, Some(("mine.desktop".into(), Source::Config)));
        // With everything installed the override outranks the user.
        let got = l.configured("a/b", |_| true).unwrap();
        assert_eq!(got.0, "gone.desktop");
        assert!(matches!(got.1, Source::Override(_)));
    }

    #[test]
    fn added_apps_count_unless_removed() {
        let l = layers(
            "",
            "[Added Associations]\na/b=one.desktop;two.desktop;\n",
            "[Removed Associations]\na/b=one.desktop;\n",
        );
        assert_eq!(
            l.configured("a/b", |_| true),
            Some(("two.desktop".into(), Source::Added))
        );
    }

    #[test]
    fn falls_back_to_what_a_desktop_entry_lists() {
        let dir = temp("declared");
        fs::write(
            dir.join("v.desktop"),
            "[Desktop Entry]\nName=Viewer\nMimeType=a/b;\n",
        )
        .unwrap();
        let index = DesktopIndex::scan_dirs(std::slice::from_ref(&dir));

        let none = Layers::default();
        assert_eq!(
            none.resolve("a/b", &index),
            Some(("v.desktop".into(), Source::Declared))
        );
        assert_eq!(none.resolve("c/d", &index), None);

        let hidden = layers("", "[Removed Associations]\na/b=v.desktop;\n", "");
        assert_eq!(hidden.resolve("a/b", &index), None);
        fs::remove_dir_all(dir).unwrap();
    }
}
