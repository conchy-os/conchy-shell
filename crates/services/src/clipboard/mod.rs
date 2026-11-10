//! Clipboard history for text and images.
//!
//! `wl-paste --watch` reports every selection change; each one is read back
//! with `wl-paste` and kept, newest first, in `$XDG_CACHE_HOME/gpuishell/
//! clipboard`. Copying an entry hands it to `wl-copy`. Needs wl-clipboard and
//! a compositor with the data-control protocol (Hyprland, Niri).

mod store;

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tracing::debug;

use self::store::Store;
use crate::jobs::JobQueue;
use crate::lifecycle::{Lifecycle, ManagedService, RunToken};

/// Entries kept; the oldest go first.
const MAX_ENTRIES: usize = 200;
const MAX_TEXT_BYTES: usize = 1 << 20;
const MAX_IMAGE_BYTES: usize = 32 << 20;

/// Offered by password managers (KeePassXC, Bitwarden) for secrets.
const SECRET_HINT: &str = "x-kde-passwordManagerHint";
const TEXT_TYPES: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
    "TEXT",
];
const URI_LIST: &str = "text/uri-list";
/// Set by Firefox and Chromium on "Copy Image", which also offers the
/// image's URL as text.
const BROWSER_IMAGE_HINTS: &[&str] = &["text/_moz_htmlcontext", "chromium/x-source-url"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClipContent {
    Text(Arc<str>),
    /// Files copied in a file manager, as `file://` URIs.
    Files(Vec<String>),
    Image {
        path: PathBuf,
        mime: String,
        width: u32,
        height: u32,
        bytes: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipEntry {
    pub id: u64,
    pub content: ClipContent,
    pub copied_at: SystemTime,
    /// FNV-1a of the content, to spot a repeat copy.
    hash: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClipboardData {
    /// Newest first.
    pub entries: Vec<ClipEntry>,
}

#[derive(Debug, Clone)]
pub enum ClipboardCommand {
    /// Put the entry back on the clipboard.
    Copy(u64),
    Delete(u64),
    Clear,
}

#[derive(Debug, Clone)]
pub struct ClipboardSubscriber {
    core: crate::ServiceCore<ClipboardData, ClipboardCommand>,
    store: Arc<Store>,
}

impl std::ops::Deref for ClipboardSubscriber {
    type Target = crate::ServiceCore<ClipboardData, ClipboardCommand>;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl Default for ClipboardSubscriber {
    fn default() -> Self {
        Self {
            core: crate::ServiceCore::with_jobs(JobQueue::new("Clipboard")),
            store: Arc::new(Store::new(store::default_directory())),
        }
    }
}

impl ClipboardSubscriber {
    pub fn new() -> Self {
        Self::default()
    }

    async fn execute(&self, command: ClipboardCommand) -> anyhow::Result<()> {
        match command {
            ClipboardCommand::Copy(id) => {
                let entry = self.entry(id)?;
                let (mime, bytes) = match &entry.content {
                    ClipContent::Text(text) => (
                        "text/plain;charset=utf-8".to_string(),
                        text.as_bytes().to_vec(),
                    ),
                    ClipContent::Files(uris) => {
                        (URI_LIST.to_string(), uris.join("\r\n").into_bytes())
                    }
                    ClipContent::Image { path, mime, .. } => {
                        let path = path.clone();
                        (
                            mime.clone(),
                            tokio::task::spawn_blocking(move || std::fs::read(path)).await??,
                        )
                    }
                };
                wl_copy(&mime, &bytes).await
            }
            ClipboardCommand::Delete(id) => {
                let removed = {
                    let mut data = self.data.lock_mut();
                    let index = data.entries.iter().position(|entry| entry.id == id);
                    index.map(|index| data.entries.remove(index))
                };
                self.persist(removed.into_iter().collect()).await
            }
            ClipboardCommand::Clear => {
                let removed = std::mem::take(&mut self.data.lock_mut().entries);
                self.persist(removed).await
            }
        }
    }

    fn entry(&self, id: u64) -> anyhow::Result<ClipEntry> {
        self.data
            .lock_ref()
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .with_context(|| format!("no clipboard entry {id}"))
    }

    /// Save the index and delete the image files of `removed`.
    async fn persist(&self, removed: Vec<ClipEntry>) -> anyhow::Result<()> {
        let entries = self.data.get_cloned().entries;
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            store.remove_files(&removed);
            store.save(&entries)
        })
        .await?
    }

    async fn listen(&self, token: RunToken) -> anyhow::Result<()> {
        if self.data.lock_ref().entries.is_empty() {
            let store = self.store.clone();
            let entries = tokio::task::spawn_blocking(move || store.load()).await?;
            token.write(&self.data, |data| data.entries = entries);
        }

        // The watch command drains the offer so the source never hits a
        // closed pipe, then prints the line that wakes us.
        let mut child = Command::new("wl-paste")
            .args(["--watch", "sh", "-c", "cat >/dev/null; echo"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("starting wl-paste (is wl-clipboard installed?)")?;
        let stdout = child.stdout.take().context("wl-paste has no stdout")?;
        let mut lines = BufReader::new(stdout).lines();
        token.active();

        while lines.next_line().await?.is_some() {
            if let Err(err) = self.capture(&token).await {
                debug!("Skipped a clipboard change: {err:#}");
            }
        }

        let output = child.wait_with_output().await?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("wl-paste exited with {}: {}", output.status, stderr.trim())
    }

    /// Read the current selection and record it.
    async fn capture(&self, token: &RunToken) -> anyhow::Result<()> {
        let listed = wl_paste(&["--list-types"]).await?;
        let types: Vec<&str> = std::str::from_utf8(&listed)?
            .lines()
            .map(str::trim)
            .collect();
        let captured = match choose(&types) {
            None => return Ok(()),
            Some(Kind::Text(mime)) => {
                let text = read_text(mime).await?;
                if text.trim().is_empty() {
                    return Ok(());
                }
                Captured::Text(text.into())
            }
            Some(Kind::Files) => {
                let uris: Vec<String> = read_text(URI_LIST)
                    .await?
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(String::from)
                    .collect();
                if uris.is_empty() {
                    return Ok(());
                }
                Captured::Files(uris)
            }
            Some(Kind::Image(mime)) => {
                let bytes = wl_paste(&["--type", mime]).await?;
                if bytes.len() > MAX_IMAGE_BYTES {
                    bail!("image of {} bytes is over the limit", bytes.len());
                }
                Captured::Image {
                    mime: mime.to_string(),
                    bytes,
                }
            }
        };

        let hash = captured.hash();
        let mut repeated = false;
        token.write(&self.data, |data| {
            repeated = promote(&mut data.entries, hash)
        });
        if repeated {
            return self.persist(Vec::new()).await;
        }

        let id = self
            .data
            .lock_ref()
            .entries
            .iter()
            .map(|entry| entry.id + 1)
            .max()
            .unwrap_or(1);
        let store = self.store.clone();
        let content = tokio::task::spawn_blocking(move || store.content(id, captured)).await??;
        let entry = ClipEntry {
            id,
            content,
            copied_at: SystemTime::now(),
            hash,
        };

        let mut evicted = Vec::new();
        token.write(&self.data, |data| {
            data.entries.insert(0, entry);
            evicted = data.entries.split_off(data.entries.len().min(MAX_ENTRIES));
        });
        if !token.alive() {
            return Ok(());
        }
        self.persist(evicted).await
    }
}

impl ManagedService for ClipboardSubscriber {
    fn name(&self) -> &'static str {
        "Clipboard"
    }

    fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    fn memory_bytes(&self) -> usize {
        self.data
            .lock_ref()
            .entries
            .iter()
            .map(|entry| {
                size_of::<ClipEntry>()
                    + match &entry.content {
                        ClipContent::Text(text) => text.len(),
                        ClipContent::Files(uris) => uris.iter().map(String::len).sum(),
                        ClipContent::Image { path, mime, .. } => {
                            path.as_os_str().len() + mime.len()
                        }
                    }
            })
            .sum()
    }

    fn start(&self) {
        let token = self.lifecycle.begin();

        let subscriber = self.clone();
        self.jobs.run(token.clone(), move |command| {
            let subscriber = subscriber.clone();
            async move { subscriber.execute(command).await }
        });

        let subscriber = self.clone();
        token.supervise("Clipboard", move |token| {
            let subscriber = subscriber.clone();
            async move { subscriber.listen(token).await }
        });
    }
}

/// A selection read but not stored yet.
enum Captured {
    Text(Arc<str>),
    Files(Vec<String>),
    Image { mime: String, bytes: Vec<u8> },
}

impl Captured {
    fn hash(&self) -> u64 {
        match self {
            Captured::Text(text) => fnv1a(text.as_bytes()),
            // Prefixed so copying a path as text stays its own entry.
            Captured::Files(uris) => fnv1a(format!("{URI_LIST}\n{}", uris.join("\n")).as_bytes()),
            Captured::Image { bytes, .. } => fnv1a(bytes),
        }
    }
}

/// Which offered type to record.
#[derive(Debug, PartialEq)]
enum Kind<'a> {
    Text(&'a str),
    Files,
    Image(&'a str),
}

fn choose<'a>(types: &[&'a str]) -> Option<Kind<'a>> {
    if types.contains(&SECRET_HINT) {
        return None;
    }
    let image = image_type(types);
    if let Some(mime) = image
        && types.iter().any(|t| BROWSER_IMAGE_HINTS.contains(t))
    {
        return Some(Kind::Image(mime));
    }
    if types.contains(&URI_LIST) {
        return Some(Kind::Files);
    }
    let text = TEXT_TYPES
        .iter()
        .find_map(|t| types.iter().find(|o| *o == t));
    text.map(|mime| Kind::Text(mime)).or(image.map(Kind::Image))
}

/// Move the entry with `hash` to the front as if just copied.
fn promote(entries: &mut Vec<ClipEntry>, hash: u64) -> bool {
    let Some(index) = entries.iter().position(|entry| entry.hash == hash) else {
        return false;
    };
    let mut entry = entries.remove(index);
    entry.copied_at = SystemTime::now();
    entries.insert(0, entry);
    true
}

fn image_type<'a>(types: &[&'a str]) -> Option<&'a str> {
    types
        .iter()
        .find(|t| **t == "image/png")
        .or_else(|| types.iter().find(|t| t.starts_with("image/")))
        .copied()
}

/// Stable across runs, unlike `DefaultHasher`, so dedup survives a restart.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

async fn wl_paste(args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let output = Command::new("wl-paste")
        .args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .context("running wl-paste")?;
    if !output.status.success() {
        bail!(
            "wl-paste {args:?} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

async fn read_text(mime: &str) -> anyhow::Result<String> {
    let bytes = wl_paste(&["--no-newline", "--type", mime]).await?;
    if bytes.len() > MAX_TEXT_BYTES {
        bail!("text of {} bytes is over the limit", bytes.len());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// `wl-copy` forks to serve the selection, so this returns once it has the
/// data.
async fn wl_copy(mime: &str, bytes: &[u8]) -> anyhow::Result<()> {
    let mut child = Command::new("wl-copy")
        .args(["--type", mime])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting wl-copy (is wl-clipboard installed?)")?;
    let mut stdin = child.stdin.take().context("wl-copy has no stdin")?;
    stdin.write_all(bytes).await?;
    drop(stdin);
    let output = child.wait_with_output().await?;
    if !output.status.success() {
        bail!(
            "wl-copy exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, text: &str) -> ClipEntry {
        ClipEntry {
            id,
            content: ClipContent::Text(text.into()),
            copied_at: SystemTime::UNIX_EPOCH,
            hash: fnv1a(text.as_bytes()),
        }
    }

    #[test]
    fn a_repeat_copy_moves_to_the_front() {
        let mut entries = vec![entry(3, "c"), entry(2, "b"), entry(1, "a")];
        assert!(promote(&mut entries, fnv1a(b"a")));
        let ids: Vec<_> = entries.iter().map(|e| e.id).collect();
        assert_eq!(ids, [1, 3, 2]);
        assert!(entries[0].copied_at > SystemTime::UNIX_EPOCH);
        assert!(!promote(&mut entries, fnv1a(b"d")));
    }

    #[test]
    fn png_is_preferred_over_other_images() {
        assert_eq!(image_type(&["image/jpeg", "image/png"]), Some("image/png"));
        assert_eq!(image_type(&["text/html", "image/webp"]), Some("image/webp"));
        assert_eq!(image_type(&["text/html"]), None);
    }

    #[test]
    fn the_recorded_type_follows_the_source() {
        let screenshot = ["image/png"];
        let browser_image = ["text/_moz_htmlcontext", "text/plain", "image/png"];
        let office_cell = ["text/plain;charset=utf-8", "image/png"];
        let file_manager = [
            "x-special/gnome-copied-files",
            "text/uri-list",
            "text/plain",
        ];
        let secret = ["text/plain", SECRET_HINT];

        assert_eq!(choose(&screenshot), Some(Kind::Image("image/png")));
        assert_eq!(choose(&browser_image), Some(Kind::Image("image/png")));
        assert_eq!(
            choose(&office_cell),
            Some(Kind::Text("text/plain;charset=utf-8"))
        );
        assert_eq!(choose(&file_manager), Some(Kind::Files));
        assert_eq!(choose(&secret), None);
        assert_eq!(choose(&["application/x-thing"]), None);
    }
}
