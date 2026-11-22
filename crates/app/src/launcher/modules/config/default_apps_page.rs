//! Default applications: which program opens links, folders, images and every
//! other type, kept in the user's `mimeapps.list`.
//!
//! The root page lists the everyday cases (browser, mail, files…) and leads to
//! two more: every file type one by one, and the entries of the file itself.
//! All of them write through [`DefaultAppsPage::apply`], which reads the file
//! again first, so a change made by another program (or by hand, in a dotfiles
//! repository) while the page was open is built upon and not lost.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use gpui::{App, AppContext, Context, Entity};
use services::LocalSlot;
use services::mimeapps::{
    DesktopEntry, DesktopIndex, Group, Layers, MimeApps, Source, Store, describe, known_mime_types,
};
use ui::{Color, IconName};

use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, EmptyView, Item, ItemIcon, List, Section};
use crate::launcher::page::{LauncherPage, Page};
use crate::state::AppState;

/// One everyday setting: a name, and the types that move together under it.
pub(super) struct Role {
    id: &'static str,
    pub(super) name: &'static str,
    pub(super) icon: IconName,
    /// What the row matches in search, besides its name.
    words: &'static str,
    pub(super) mimes: &'static [&'static str],
}

pub(super) const ROLES: &[Role] = &[
    Role {
        id: "web",
        name: "Web browser",
        icon: IconName::Globe,
        words: "browser internet links http https html url",
        mimes: &[
            "x-scheme-handler/http",
            "x-scheme-handler/https",
            "text/html",
            "application/xhtml+xml",
        ],
    },
    Role {
        id: "mail",
        name: "Email",
        icon: IconName::Mail,
        words: "mail mailto client",
        mimes: &["x-scheme-handler/mailto"],
    },
    Role {
        id: "files",
        name: "File manager",
        icon: IconName::Folder,
        words: "folders directories files explorer",
        mimes: &["inode/directory"],
    },
    Role {
        id: "text",
        name: "Text editor",
        icon: IconName::FileText,
        words: "text plain editor notes",
        mimes: &["text/plain"],
    },
    Role {
        id: "images",
        name: "Images",
        icon: IconName::Image,
        words: "pictures photos viewer png jpeg jpg gif webp svg",
        mimes: &[
            "image/png",
            "image/jpeg",
            "image/gif",
            "image/webp",
            "image/bmp",
            "image/tiff",
            "image/svg+xml",
            "image/avif",
            "image/heif",
            "image/jxl",
        ],
    },
    Role {
        id: "video",
        name: "Video",
        icon: IconName::Play,
        words: "movies player mp4 mkv webm",
        mimes: &[
            "video/mp4",
            "video/x-matroska",
            "video/webm",
            "video/quicktime",
            "video/x-msvideo",
            "video/mpeg",
            "video/ogg",
        ],
    },
    Role {
        id: "audio",
        name: "Music",
        icon: IconName::Music,
        words: "audio player mp3 flac ogg wav",
        mimes: &[
            "audio/mpeg",
            "audio/flac",
            "audio/ogg",
            "audio/x-wav",
            "audio/mp4",
            "audio/opus",
            "audio/x-vorbis+ogg",
        ],
    },
    Role {
        id: "pdf",
        name: "PDF documents",
        icon: IconName::BookOpen,
        words: "pdf reader documents",
        mimes: &["application/pdf"],
    },
    Role {
        id: "archives",
        name: "Archives",
        icon: IconName::Paperclip,
        words: "zip tar rar 7z gzip compressed",
        mimes: &[
            "application/zip",
            "application/x-tar",
            "application/x-7z-compressed",
            "application/vnd.rar",
            "application/gzip",
            "application/x-compressed-tar",
            "application/x-xz-compressed-tar",
        ],
    },
];

/// What a role currently opens with.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Choice {
    Unset,
    One(String),
    /// Its types do not agree: every distinct application, in type order.
    Mixed(Vec<String>),
}

/// Fold the application of each of the role's types into one answer.
pub(super) fn role_choice(role: &Role, resolve: impl Fn(&str) -> Option<String>) -> Choice {
    let mut ids: Vec<String> = Vec::new();
    let mut unset = false;
    for mime in role.mimes {
        match resolve(mime) {
            Some(id) if !ids.contains(&id) => ids.push(id),
            Some(_) => {}
            None => unset = true,
        }
    }
    match (ids.len(), unset) {
        (0, _) => Choice::Unset,
        (1, false) => Choice::One(ids.remove(0)),
        _ => Choice::Mixed(ids),
    }
}

/// The root page, and the state the pages it opens share.
pub struct DefaultAppsPage {
    store: Store,
    layers: Layers,
    index: DesktopIndex,
    /// Icon files by desktop file, borrowed from the launcher's catalog so the
    /// theme is not searched again for each of the apps.
    icons: HashMap<PathBuf, PathBuf>,
    /// Readable names of types, each read from the MIME database once.
    names: RefCell<HashMap<String, Option<String>>>,
    /// Why the file could not be read, when it could not.
    load_error: Option<String>,
}

impl DefaultAppsPage {
    pub fn open(cx: &mut App) -> Page {
        let page = Self::load(cx);
        Page::new(cx.new(|_| page))
    }

    fn load(cx: &App) -> Self {
        let icons = AppState::applications(cx)
            .all()
            .iter()
            .filter_map(|app| Some((app.desktop_file.clone(), app.icon_path.clone()?)))
            .collect();
        let index = DesktopIndex::scan();
        let loaded = Store::standard().and_then(|store| {
            let layers = Layers::load(&store)?;
            Ok((store, layers))
        });
        match loaded {
            Ok((store, layers)) => Self {
                store,
                layers,
                index,
                icons,
                names: RefCell::default(),
                load_error: None,
            },
            Err(err) => Self {
                store: Store::new("mimeapps.list"),
                layers: Layers::default(),
                index,
                icons,
                names: RefCell::default(),
                load_error: Some(format!("{err:#}")),
            },
        }
    }

    // -- what the page knows ------------------------------------------------

    fn resolve(&self, mime: &str) -> Option<(String, Source)> {
        self.layers.resolve(mime, &self.index)
    }

    /// `PNG image` for `image/png`; the type itself when the database does not
    /// know it.
    fn type_name(&self, mime: &str) -> String {
        self.names
            .borrow_mut()
            .entry(mime.to_string())
            .or_insert_with(|| describe(mime))
            .clone()
            .unwrap_or_else(|| mime.to_string())
    }

    /// The line under a type's name: what it is for, and the type, which the
    /// name alone does not give (and which is what the file contains).
    fn type_subtitle(&self, mime: &str, what: &str) -> String {
        if self.type_name(mime) == mime {
            what.to_string()
        } else {
            format!("{what} · {mime}")
        }
    }

    fn app_name(&self, id: &str) -> String {
        self.index.get(id).map_or_else(
            || id.trim_end_matches(".desktop").to_string(),
            |entry| entry.name.clone(),
        )
    }

    fn icon(&self, id: &str, fallback: IconName) -> ItemIcon {
        self.index
            .get(id)
            .and_then(|entry| self.icons.get(&entry.path))
            .map_or_else(|| fallback.into(), |path| ItemIcon::Path(path.clone()))
    }

    /// Entries of the user's file that name applications which are not
    /// installed. The removed list is not counted: a line there is a promise
    /// about an application that may come back.
    fn missing_ids(&self) -> HashSet<String> {
        [Group::Default, Group::Added]
            .into_iter()
            .flat_map(|group| self.layers.user.entries(group))
            .flat_map(|(_, ids)| ids.iter())
            .filter(|id| self.index.get(id).is_none())
            .cloned()
            .collect()
    }

    fn missing_count(&self) -> usize {
        let missing = self.missing_ids();
        [Group::Default, Group::Added]
            .into_iter()
            .flat_map(|group| self.layers.user.entries(group))
            .flat_map(|(_, ids)| ids.iter())
            .filter(|id| missing.contains(*id))
            .count()
    }

    // -- writing --------------------------------------------------------------

    /// Read the file again, make `change`, and write it if anything differs.
    fn apply(
        &mut self,
        change: impl FnOnce(&mut MimeApps),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let mut doc = self.store.load().map_err(|err| format!("{err:#}"))?;
        let before = doc.clone();
        change(&mut doc);
        if doc == before {
            return Ok(());
        }
        self.store.save(&doc).map_err(|err| format!("{err:#}"))?;
        self.layers.user = doc;
        cx.notify();
        Ok(())
    }

    /// [`apply`](Self::apply), telling the user in a notification when it fails.
    /// Whether it worked.
    fn apply_or_notify(
        &mut self,
        change: impl FnOnce(&mut MimeApps),
        cx: &mut Context<Self>,
    ) -> bool {
        match self.apply(change, cx) {
            Ok(()) => true,
            Err(reason) => {
                AppState::notification(cx).post_local(
                    LocalSlot::ConfigWriteError,
                    "Default application was not saved".into(),
                    reason,
                );
                false
            }
        }
    }

    fn set_default(&mut self, mimes: &[String], id: &str, cx: &mut Context<Self>) -> bool {
        let entry = self.index.get(id);
        let listed: Vec<bool> = mimes
            .iter()
            .map(|mime| entry.is_some_and(|e| e.opens(mime)))
            .collect();
        self.apply_or_notify(
            |doc| {
                for (mime, listed) in mimes.iter().zip(listed) {
                    doc.set_default(mime, id, listed);
                }
            },
            cx,
        )
    }

    fn clear(&mut self, group: Group, mimes: &[String], cx: &mut Context<Self>) -> bool {
        self.apply_or_notify(
            |doc| {
                for mime in mimes {
                    doc.set(group, mime, Vec::new());
                }
            },
            cx,
        )
    }

    fn prune(&mut self, cx: &mut Context<Self>) -> bool {
        let missing = self.missing_ids();
        self.apply_or_notify(
            |doc| {
                doc.prune_missing(|id| !missing.contains(id));
            },
            cx,
        )
    }

    // -- rows -----------------------------------------------------------------

    fn role_row(&self, role: &'static Role, page: &Entity<Self>) -> Item {
        let choice = role_choice(role, |mime| self.resolve(mime).map(|(id, _)| id));
        let overridden = role
            .mimes
            .iter()
            .any(|mime| matches!(self.resolve(mime), Some((_, Source::Override(_)))));

        let (subtitle, icon) = match &choice {
            Choice::Unset => ("Not set".to_string(), role.icon.into()),
            Choice::One(id) => (self.app_name(id), self.icon(id, role.icon)),
            Choice::Mixed(ids) => (
                ids.iter()
                    .map(|id| self.app_name(id))
                    .collect::<Vec<_>>()
                    .join(", "),
                role.icon.into(),
            ),
        };
        let mut item = Item::new(format!("role:{}", role.id), role.name)
            .subtitle(subtitle)
            .icon(icon)
            .keywords(role.words);
        match choice {
            Choice::Mixed(_) => {
                item = item.accessory(Accessory::tag("mixed").color(Color::Warning));
            }
            Choice::Unset => {}
            Choice::One(_) => {}
        }
        if overridden {
            item = item.accessory(Accessory::tag("overridden").color(Color::Warning));
        }

        let mimes: Vec<String> = role.mimes.iter().map(|m| m.to_string()).collect();
        let (choose_page, reset_page) = (page.clone(), page.clone());
        let (choose_mimes, reset_mimes) = (mimes.clone(), mimes);
        let name = role.name;
        item.actions(
            ActionPanel::new()
                .action(
                    Action::new("Choose application", move |_, cx| {
                        let (page, mimes) = (choose_page.clone(), choose_mimes.clone());
                        Outcome::push(cx.new(|cx| ChoosePage::new(page, name.into(), mimes, cx)))
                    })
                    .icon(IconName::ChevronRight),
                )
                .action(
                    Action::new("Use the system's choice", move |_, cx| {
                        reset_page.update(cx, |page, cx| {
                            page.clear(Group::Default, &reset_mimes, cx);
                        });
                        Outcome::Stay
                    })
                    .icon(IconName::RotateCcw),
                ),
        )
    }
}

impl LauncherPage for DefaultAppsPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let list = List::new()
            .title("Default applications")
            .placeholder("Search default applications…");
        if let Some(reason) = &self.load_error {
            return list
                .empty(
                    EmptyView::new("mimeapps.list could not be read")
                        .description(reason.clone())
                        .icon(IconName::CircleAlert),
                )
                .into();
        }

        let page = cx.entity();
        let common = ROLES.iter().map(|role| self.role_row(role, &page));

        let (types_page, file_page) = (page.clone(), page.clone());
        let missing = self.missing_count();
        let mut file_row = Item::new("file", "mimeapps.list")
            .subtitle(tilde(self.store.path()))
            .icon(IconName::FileText)
            .keywords("mimeapps list file associations added removed edit")
            .action(
                Action::new("Open entries", move |_, cx| {
                    let page = file_page.clone();
                    Outcome::push(cx.new(|cx| FilePage::new(page, cx)))
                })
                .icon(IconName::ChevronRight),
            );
        if missing > 0 {
            file_row = file_row
                .accessory(Accessory::tag(format!("{missing} missing")).color(Color::Warning));
        }
        let types_row = Item::new("types", "All file types")
            .subtitle("Choose an application for any type, one by one")
            .icon(IconName::List)
            .keywords("mime types extensions formats associations")
            .action(
                Action::new("Open file types", move |_, cx| {
                    let page = types_page.clone();
                    Outcome::push(cx.new(|cx| TypesPage::new(page, cx)))
                })
                .icon(IconName::ChevronRight),
            );

        list.section(Section::new("Common").items(common))
            .section(Section::new("Advanced").items([types_row, file_row]))
            .into()
    }
}

// ============================================================================
// Choose an application
// ============================================================================

/// Pick the application for one type, or for all the types of a role.
struct ChoosePage {
    page: Entity<DefaultAppsPage>,
    title: String,
    mimes: Vec<String>,
}

impl ChoosePage {
    fn new(
        page: Entity<DefaultAppsPage>,
        title: String,
        mimes: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&page, |_, _, cx| cx.notify()).detach();
        Self { page, title, mimes }
    }

    fn row(
        &self,
        entry: &DesktopEntry,
        current: &HashSet<String>,
        parent: &DefaultAppsPage,
    ) -> Item {
        let (page, mimes, id) = (self.page.clone(), self.mimes.clone(), entry.id.clone());
        let mut item = Item::new(entry.id.clone(), entry.name.clone())
            .subtitle(entry.id.trim_end_matches(".desktop").to_string())
            .icon(parent.icon(&entry.id, IconName::Layers))
            .keywords(entry.mimes.join(" "));
        if current.contains(&entry.id) {
            item = item.accessory(Accessory::tag("current").color(Color::Accent));
        }
        item.action(
            Action::new("Use as default", move |_, cx| {
                let saved = page.update(cx, |page, cx| page.set_default(&mimes, &id, cx));
                if saved { Outcome::Pop } else { Outcome::Stay }
            })
            .icon(IconName::Check),
        )
    }
}

impl LauncherPage for ChoosePage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let parent = self.page.read(cx);
        let current: HashSet<String> = self
            .mimes
            .iter()
            .filter_map(|mime| parent.resolve(mime).map(|(id, _)| id))
            .collect();

        let (mut suggested, mut others) = (Vec::new(), Vec::new());
        for entry in parent.index.all() {
            if self.mimes.iter().any(|mime| entry.opens(mime)) {
                suggested.push(entry);
            } else if !entry.no_display {
                others.push(entry);
            }
        }
        // What is in use leads; the index is by name already, and the sort is stable.
        suggested.sort_by_key(|entry| !current.contains(&entry.id));

        let mut list = List::new()
            .title(self.title.clone())
            .placeholder("Search applications…");
        if !suggested.is_empty() {
            let title = if self.mimes.len() == 1 {
                "Can open this type"
            } else {
                "Can open these types"
            };
            list = list.section(
                Section::new(title).items(suggested.iter().map(|e| self.row(e, &current, parent))),
            );
        }
        let others_section = if suggested.is_empty() {
            Section::new("Other applications")
                .subtitle("No installed application lists this type. You can still pick one")
        } else {
            Section::new("Other applications")
        };
        list.section(others_section.items(others.iter().map(|e| self.row(e, &current, parent))))
            .into()
    }
}

// ============================================================================
// Every type
// ============================================================================

/// Every MIME type there is, each with what it opens with.
struct TypesPage {
    page: Entity<DefaultAppsPage>,
    types: Vec<String>,
}

impl TypesPage {
    fn new(page: Entity<DefaultAppsPage>, cx: &mut Context<Self>) -> Self {
        cx.observe(&page, |_, _, cx| cx.notify()).detach();
        let parent = page.read(cx);
        let mut types: HashSet<String> = known_mime_types().into_iter().collect();
        types.extend(parent.index.mimes().map(str::to_string));
        for group in Group::ALL {
            types.extend(
                parent
                    .layers
                    .user
                    .entries(group)
                    .into_iter()
                    .map(|(mime, _)| mime.to_string()),
            );
        }
        let mut types: Vec<String> = types.into_iter().collect();
        types.sort();
        Self { page, types }
    }

    fn row(&self, mime: &str, parent: &DefaultAppsPage, customized: bool) -> Item {
        let resolved = parent.resolve(mime);
        let (subtitle, icon, words) = match &resolved {
            Some((id, _)) => {
                let name = parent.app_name(id);
                (name.clone(), parent.icon(id, IconName::File), name)
            }
            None => (
                "No application".to_string(),
                IconName::File.into(),
                String::new(),
            ),
        };
        let label = parent.type_name(mime);
        let mut item = Item::new(mime.to_string(), label.clone())
            .subtitle(parent.type_subtitle(mime, &subtitle))
            .icon(icon)
            .keywords(format!("{mime} {words}"));
        if let Some((_, source)) = resolved {
            item = match source {
                Source::Override(_) => {
                    item.accessory(Accessory::tag("overridden").color(Color::Warning))
                }
                Source::System(_) => item.accessory(Accessory::tag("system")),
                Source::Added if !customized => item.accessory(Accessory::tag("added")),
                Source::Declared => item.accessory(Accessory::tag("automatic")),
                Source::Config | Source::Added => item,
            };
        }

        let (choose, reset) = (self.page.clone(), self.page.clone());
        let (choose_mime, reset_mime) = (mime.to_string(), mime.to_string());
        let mut actions = ActionPanel::new().action(
            Action::new("Choose application", move |_, cx| {
                let (page, mime, label) = (choose.clone(), choose_mime.clone(), label.clone());
                Outcome::push(cx.new(|cx| ChoosePage::new(page, label, vec![mime], cx)))
            })
            .icon(IconName::ChevronRight),
        );
        if customized {
            actions = actions.action(
                Action::new("Use the system's choice", move |_, cx| {
                    let mimes = [reset_mime.clone()];
                    reset.update(cx, |page, cx| {
                        page.clear(Group::Default, &mimes, cx);
                    });
                    Outcome::Stay
                })
                .icon(IconName::RotateCcw),
            );
        }
        item.actions(actions)
    }
}

impl LauncherPage for TypesPage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let parent = self.page.read(cx);
        let (mut set, mut rest) = (Vec::new(), Vec::new());
        for mime in &self.types {
            if parent.layers.user.get(Group::Default, mime).is_some() {
                set.push(self.row(mime, parent, true));
            } else {
                rest.push(self.row(mime, parent, false));
            }
        }
        let mut list = List::new()
            .title("All file types")
            .placeholder("Search types or applications…");
        if !set.is_empty() {
            list = list.section(
                Section::new("Chosen by you")
                    .subtitle("Defaults set in mimeapps.list")
                    .items(set),
            );
        }
        list.section(Section::new("Other types").items(rest)).into()
    }
}

// ============================================================================
// The file
// ============================================================================

/// The entries of `mimeapps.list`, group by group, to change or remove.
struct FilePage {
    page: Entity<DefaultAppsPage>,
}

impl FilePage {
    fn new(page: Entity<DefaultAppsPage>, cx: &mut Context<Self>) -> Self {
        cx.observe(&page, |_, _, cx| cx.notify()).detach();
        Self { page }
    }

    fn entry_row(
        &self,
        group: Group,
        mime: &str,
        ids: &[String],
        parent: &DefaultAppsPage,
    ) -> Item {
        let names: Vec<String> = ids.iter().map(|id| parent.app_name(id)).collect();
        let label = parent.type_name(mime);
        let mut item = Item::new(format!("{}:{mime}", group.header()), label.clone())
            .subtitle(parent.type_subtitle(mime, &names.join(", ")))
            .icon(match ids.first() {
                Some(id) => parent.icon(id, IconName::File),
                None => IconName::File.into(),
            })
            .keywords(format!("{mime} {}", ids.join(" ")));
        // The removed list may name applications that are gone; that is fine.
        if group != Group::Removed && ids.iter().any(|id| parent.index.get(id).is_none()) {
            item = item.accessory(Accessory::tag("not installed").color(Color::Warning));
        }

        let mut actions = ActionPanel::new();
        if group == Group::Default {
            let (page, mime) = (self.page.clone(), mime.to_string());
            actions = actions.action(
                Action::new("Choose application", move |_, cx| {
                    let (page, mime, label) = (page.clone(), mime.clone(), label.clone());
                    Outcome::push(cx.new(|cx| ChoosePage::new(page, label, vec![mime], cx)))
                })
                .icon(IconName::Pencil),
            );
        }
        let (page, mime) = (self.page.clone(), mime.to_string());
        actions = actions.action(
            Action::new("Remove entry", move |_, cx| {
                let mimes = [mime.clone()];
                page.update(cx, |page, cx| {
                    page.clear(group, &mimes, cx);
                });
                Outcome::Stay
            })
            .icon(IconName::Trash)
            .shortcut("ctrl-x")
            .destructive(),
        );
        item.actions(actions)
    }
}

impl LauncherPage for FilePage {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let parent = self.page.read(cx);
        let mut list = List::new()
            .title("mimeapps.list")
            .placeholder("Search entries…");

        let target = parent.store.target();
        let linked = target != parent.store.path();
        let location = if linked {
            format!("{} → {}", tilde(parent.store.path()), tilde(&target))
        } else {
            tilde(parent.store.path())
        };
        let copy = target.to_string_lossy().to_string();
        let mut head = vec![
            Item::new("location", "Location")
                .subtitle(location)
                .icon(IconName::FileText)
                .action(Action::copy(copy).title("Copy path")),
        ];
        let missing = parent.missing_count();
        if missing > 0 {
            let page = self.page.clone();
            head.push(
                Item::new(
                    "prune",
                    format!("Remove {missing} entries for missing applications"),
                )
                .subtitle("Defaults and added applications that are no longer installed")
                .icon(IconName::Eraser)
                .accessory(Accessory::tag(format!("{missing}")).color(Color::Warning))
                .action(
                    Action::new("Remove them", move |_, cx| {
                        page.update(cx, |page, cx| {
                            page.prune(cx);
                        });
                        Outcome::Stay
                    })
                    .icon(IconName::Eraser),
                ),
            );
        }
        list = list.section(Section::untitled().items(head));

        let notes = [
            (Group::Default, "What each type opens with"),
            (
                Group::Added,
                "Applications offered for a type, beyond its own list",
            ),
            (Group::Removed, "Applications kept out of a type's choices"),
        ];
        for (group, note) in notes {
            let rows: Vec<Item> = parent
                .layers
                .user
                .entries(group)
                .into_iter()
                .map(|(mime, ids)| self.entry_row(group, mime, ids, parent))
                .collect();
            if !rows.is_empty() {
                list = list.section(Section::new(group.header()).subtitle(note).items(rows));
            }
        }
        list.into()
    }
}

/// `/home/me/.config/x` as `~/.config/x`.
pub(super) fn tilde(path: &Path) -> String {
    match std::env::var_os("HOME").map(PathBuf::from) {
        Some(home) => match path.strip_prefix(&home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(id: &str) -> &'static Role {
        ROLES.iter().find(|r| r.id == id).unwrap()
    }

    #[test]
    fn a_role_agrees_when_all_its_types_do() {
        let web = role("web");
        assert_eq!(
            role_choice(web, |_| Some("firefox.desktop".into())),
            Choice::One("firefox.desktop".into())
        );
    }

    #[test]
    fn a_role_is_mixed_when_types_disagree_or_one_is_unset() {
        let web = role("web");
        let split = role_choice(web, |m| {
            Some(
                if m.ends_with("https") {
                    "b.desktop"
                } else {
                    "a.desktop"
                }
                .into(),
            )
        });
        assert_eq!(
            split,
            Choice::Mixed(vec!["a.desktop".into(), "b.desktop".into()])
        );

        let partial = role_choice(web, |m| (m == "text/html").then(|| "a.desktop".into()));
        assert_eq!(partial, Choice::Mixed(vec!["a.desktop".into()]));

        assert_eq!(role_choice(web, |_| None), Choice::Unset);
    }

    #[test]
    fn role_ids_are_unique_and_every_role_has_types() {
        let mut ids: Vec<_> = ROLES.iter().map(|r| r.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ROLES.len());
        assert!(ROLES.iter().all(|r| !r.mimes.is_empty()));
    }

    #[test]
    fn tilde_shortens_only_paths_under_home() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(tilde(&Path::new(&home).join(".config/x")), "~/.config/x");
        assert_eq!(tilde(Path::new("/etc/x")), "/etc/x");
    }
}
