//! Clipboard history module: text and images copied since the service
//! started recording, newest first, with a preview beside the list. Enter
//! puts an entry back on the clipboard.

pub mod config;

use std::time::SystemTime;

use gpui::{Context, SharedString};
use services::{ClipContent, ClipEntry, ClipboardCommand, ManagedService, ServiceStatus};
use ui::IconName;

use self::config::ClipboardConfig;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{
    Accessory, Content, Detail, Dropdown, DropdownItem, EmptyView, Item, ItemIcon, List, Metadata,
};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};
use crate::state::{AppState, watch};

/// Characters of an entry the search sees and the preview shows.
const SEARCHED_CHARS: usize = 2000;
const PREVIEW_CHARS: usize = 4000;
const TITLE_CHARS: usize = 120;

pub fn descriptor(config: &ClipboardConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "clipboard",
            prefix: config.prefix.clone().into(),
            name: "Clipboard",
            icon: IconName::Clipboard,
            description: "Search clipboard history, text and images",
            default: false,
            in_help: true,
        },
        ClipboardModule::new,
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Text,
    Images,
}

impl Filter {
    const ALL: [Filter; 3] = [Filter::All, Filter::Text, Filter::Images];

    fn value(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Text => "text",
            Filter::Images => "images",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Text => "Text",
            Filter::Images => "Images",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Filter::All => IconName::Clipboard,
            Filter::Text => IconName::FileText,
            Filter::Images => IconName::Image,
        }
    }

    fn keeps(self, entry: &ClipEntry) -> bool {
        matches!(
            (self, &entry.content),
            (Filter::All, _)
                | (Filter::Text, ClipContent::Text(_) | ClipContent::Files(_))
                | (Filter::Images, ClipContent::Image { .. })
        )
    }
}

pub struct ClipboardModule {
    entries: Vec<ClipEntry>,
    status: ServiceStatus,
    filter: Filter,
}

impl ClipboardModule {
    fn new(cx: &mut Context<Self>) -> Self {
        let service = AppState::clipboard(cx);
        let (entries, status) = (service.get().entries, service.status());
        let (data, status_signal) = (service.subscribe(), service.status_signal());
        watch(cx, data, |this, data, cx| {
            this.entries = data.entries;
            cx.notify();
        });
        watch(cx, status_signal, |this, status, cx| {
            this.status = status;
            cx.notify();
        });
        Self {
            entries,
            status,
            filter: Filter::All,
        }
    }

    fn empty_view(&self) -> EmptyView {
        match &self.status {
            ServiceStatus::Error(message) => EmptyView::new("Clipboard history is unavailable")
                .description(message.clone().unwrap_or_default())
                .icon(IconName::CircleAlert),
            ServiceStatus::Stopped | ServiceStatus::Unavailable => {
                EmptyView::new("Clipboard service is off")
                    .description("Start it from the services module (;s)")
                    .icon(IconName::Clipboard)
            }
            _ if self.entries.is_empty() => EmptyView::new("Nothing copied yet")
                .description("Text and images you copy show up here")
                .icon(IconName::Clipboard),
            _ => EmptyView::new("No matching entries").icon(IconName::Clipboard),
        }
    }

    fn dropdown(&self, cx: &Context<Self>) -> Dropdown {
        Dropdown::new(self.filter.value(), cx, |this, value, _, cx| {
            if let Some(filter) = Filter::ALL
                .into_iter()
                .find(|f| f.value() == value.as_ref())
            {
                this.filter = filter;
                cx.notify();
            }
        })
        .items(Filter::ALL.into_iter().map(|filter| {
            let count = self.entries.iter().filter(|e| filter.keeps(e)).count();
            DropdownItem::new(filter.value(), filter.title())
                .icon(filter.icon())
                .badge(count.to_string())
        }))
    }
}

impl LauncherPage for ClipboardModule {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let view_actions = if self.entries.is_empty() {
            ActionPanel::new()
        } else {
            ActionPanel::new().action(clear_action())
        };
        List::new()
            .show_detail(true)
            .placeholder("Search clipboard history…")
            .loading(self.status == ServiceStatus::Initializing)
            .dropdown(self.dropdown(cx))
            .empty(self.empty_view())
            .actions(view_actions)
            .items(
                self.entries
                    .iter()
                    .filter(|entry| self.filter.keeps(entry))
                    .map(item),
            )
            .into()
    }
}

fn item(entry: &ClipEntry) -> Item {
    let item = Item::new(entry.id.to_string(), title(entry))
        .accessory(Accessory::text(format_age(entry.copied_at)))
        .actions(actions(entry))
        .detail(detail(entry));
    match &entry.content {
        ClipContent::Text(text) => item
            .icon(IconName::FileText)
            .keywords(truncate(text, SEARCHED_CHARS)),
        ClipContent::Files(uris) => {
            let paths: Vec<_> = uris.iter().map(|uri| file_path(uri)).collect();
            let item = item
                .icon(if uris.len() == 1 {
                    IconName::File
                } else {
                    IconName::Folder
                })
                .keywords(truncate(&paths.join(" "), SEARCHED_CHARS));
            match paths.as_slice() {
                [path] => item.subtitle(path.clone()),
                _ => item,
            }
        }
        ClipContent::Image {
            path, mime, bytes, ..
        } => item
            .subtitle(format!("{} · {}", format_name(mime), format_bytes(*bytes)))
            .icon(ItemIcon::Thumbnail {
                path: path.clone(),
                mtime: None,
            })
            .keywords(format!("image {mime}")),
    }
}

fn title(entry: &ClipEntry) -> SharedString {
    match &entry.content {
        ClipContent::Text(text) => {
            let line = text.lines().map(str::trim).find(|l| !l.is_empty());
            truncate(line.unwrap_or_default(), TITLE_CHARS).into()
        }
        ClipContent::Files(uris) => match uris.as_slice() {
            [uri] => {
                let path = file_path(uri);
                let name = path.rsplit('/').find(|part| !part.is_empty());
                name.unwrap_or(&path).to_string().into()
            }
            _ => format!("{} files", uris.len()).into(),
        },
        ClipContent::Image { width, height, .. } if *width > 0 => {
            format!("Image {width}×{height}").into()
        }
        ClipContent::Image { .. } => "Image".into(),
    }
}

fn actions(entry: &ClipEntry) -> ActionPanel {
    let id = entry.id;
    let mut panel = ActionPanel::new().action(
        Action::new("Copy to Clipboard", move |_, cx| {
            AppState::clipboard(cx).dispatch(ClipboardCommand::Copy(id));
            Outcome::Close
        })
        .icon(IconName::Copy),
    );
    match &entry.content {
        ClipContent::Image { path, .. } => {
            panel = panel
                .action(Action::open(path.to_string_lossy().to_string()).title("Open in Viewer"));
        }
        ClipContent::Files(uris) if uris.len() == 1 => {
            panel = panel.action(Action::open(uris[0].clone()).title("Open File"));
        }
        _ => {}
    }
    panel.section(
        "Manage",
        [
            Action::new("Delete Entry", move |_, cx| {
                AppState::clipboard(cx).dispatch(ClipboardCommand::Delete(id));
                Outcome::Stay
            })
            .icon(IconName::Trash)
            .shortcut("ctrl-d")
            .destructive(),
            clear_action(),
        ],
    )
}

fn clear_action() -> Action {
    Action::new("Clear History", |_, cx| {
        AppState::clipboard(cx).dispatch(ClipboardCommand::Clear);
        Outcome::Stay
    })
    .icon(IconName::Trash)
    .shortcut("ctrl-shift-d")
    .destructive()
}

fn detail(entry: &ClipEntry) -> Detail {
    let copied = Metadata::label("Copied", format_age(entry.copied_at));
    match &entry.content {
        ClipContent::Text(text) => {
            // A line opening a fence would end the preview's code block early.
            let shown: String = truncate(text, PREVIEW_CHARS)
                .lines()
                .map(|line| {
                    if line.trim_start().starts_with("```") {
                        format!("\u{200b}{line}")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            Detail::new()
                .markdown(format!("```\n{shown}\n```"))
                .metadata(Metadata::label(
                    "Characters",
                    text.chars().count().to_string(),
                ))
                .metadata(Metadata::label("Lines", text.lines().count().to_string()))
                .metadata(copied)
        }
        ClipContent::Files(uris) => {
            let list: Vec<_> = uris
                .iter()
                .map(|uri| format!("- {}", file_path(uri)))
                .collect();
            Detail::new()
                .markdown(list.join("\n"))
                .metadata(Metadata::label("Files", uris.len().to_string()))
                .metadata(copied)
        }
        ClipContent::Image {
            path,
            mime,
            width,
            height,
            bytes,
        } => {
            let mut detail = Detail::new().markdown(format!("![image]({})", path.display()));
            if *width > 0 {
                detail =
                    detail.metadata(Metadata::label("Dimensions", format!("{width}×{height}")));
            }
            detail
                .metadata(Metadata::label("Format", format_name(mime)))
                .metadata(Metadata::label("Size", format_bytes(*bytes)))
                .metadata(copied)
        }
    }
}

fn truncate(text: &str, chars: usize) -> String {
    match text.char_indices().nth(chars) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// `file:///a%20b` as `/a b`; other URIs as they are.
fn file_path(uri: &str) -> String {
    let Some(path) = uri.strip_prefix("file://") else {
        return uri.to_string();
    };
    let mut bytes = Vec::with_capacity(path.len());
    let mut rest = path.as_bytes();
    while let [byte, tail @ ..] = rest {
        let escaped = match tail {
            [hi, lo, ..] if *byte == b'%' => std::str::from_utf8(&[*hi, *lo])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok()),
            _ => None,
        };
        match escaped {
            Some(decoded) => {
                bytes.push(decoded);
                rest = &tail[2..];
            }
            None => {
                bytes.push(*byte);
                rest = tail;
            }
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn format_name(mime: &str) -> String {
    mime.strip_prefix("image/").unwrap_or(mime).to_uppercase()
}

fn format_bytes(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.0} KB", bytes as f64 / 1024.),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_counts_characters_not_bytes() {
        assert_eq!(truncate("ação", 2), "aç…");
        assert_eq!(truncate("abc", 3), "abc");
    }

    #[test]
    fn file_uris_are_decoded() {
        assert_eq!(
            file_path("file:///home/a%20b/%C3%A7.txt"),
            "/home/a b/ç.txt"
        );
        assert_eq!(file_path("file:///100%"), "/100%");
        assert_eq!(file_path("https://x.y"), "https://x.y");
    }
}
