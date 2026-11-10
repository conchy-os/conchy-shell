//! The history on disk: `history.json` plus one file per image, readable
//! only by the user since a clipboard holds whatever was copied.

use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use tracing::warn;

use super::{Captured, ClipContent, ClipEntry};

const INDEX: &str = "history.json";

pub(super) fn default_directory() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".cache")
        });
    base.join("gpuishell").join("clipboard")
}

#[derive(Debug)]
pub(super) struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Entries whose image is gone are dropped.
    pub fn load(&self) -> Vec<ClipEntry> {
        let Ok(json) = fs::read(self.dir.join(INDEX)) else {
            return Vec::new();
        };
        match serde_json::from_slice::<Vec<ClipEntry>>(&json) {
            Ok(entries) => entries
                .into_iter()
                .filter(|entry| match &entry.content {
                    ClipContent::Image { path, .. } => path.exists(),
                    ClipContent::Text(_) | ClipContent::Files(_) => true,
                })
                .collect(),
            Err(err) => {
                warn!("Ignoring unreadable clipboard history: {err}");
                Vec::new()
            }
        }
    }

    pub fn save(&self, entries: &[ClipEntry]) -> anyhow::Result<()> {
        let tmp = self.dir.join(format!("{INDEX}.tmp"));
        self.write(&tmp, &serde_json::to_vec(entries)?)?;
        fs::rename(&tmp, self.dir.join(INDEX))?;
        Ok(())
    }

    /// Turn a capture into stored content, writing an image to its own file.
    pub fn content(&self, id: u64, captured: Captured) -> anyhow::Result<ClipContent> {
        Ok(match captured {
            Captured::Text(text) => ClipContent::Text(text),
            Captured::Files(uris) => ClipContent::Files(uris),
            Captured::Image { mime, bytes } => {
                let (width, height) = dimensions(&bytes).unwrap_or_default();
                let extension = mime.strip_prefix("image/").unwrap_or("img");
                let path = self.dir.join(format!("{id}.{extension}"));
                self.write(&path, &bytes)?;
                ClipContent::Image {
                    path,
                    mime,
                    width,
                    height,
                    bytes: bytes.len() as u64,
                }
            }
        })
    }

    pub fn remove_files(&self, entries: &[ClipEntry]) {
        for entry in entries {
            if let ClipContent::Image { path, .. } = &entry.content
                && let Err(err) = fs::remove_file(path)
            {
                warn!("Failed to remove {}: {err}", path.display());
            }
        }
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.dir)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
        Ok(())
    }
}

fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn history_round_trips_and_drops_missing_images() {
        let dir = std::env::temp_dir().join(format!("gpuishell-clip-{}", std::process::id()));
        let store = Store::new(dir.clone());
        let text = ClipEntry {
            id: 1,
            content: store.content(1, Captured::Text("hi".into())).unwrap(),
            copied_at: SystemTime::UNIX_EPOCH,
            hash: 1,
        };
        let image = ClipEntry {
            id: 2,
            content: store
                .content(
                    2,
                    Captured::Image {
                        mime: "image/png".into(),
                        bytes: vec![1, 2, 3],
                    },
                )
                .unwrap(),
            copied_at: SystemTime::UNIX_EPOCH,
            hash: 2,
        };
        store.save(&[image.clone(), text.clone()]).unwrap();
        assert_eq!(store.load(), [image.clone(), text.clone()]);

        store.remove_files(std::slice::from_ref(&image));
        assert_eq!(store.load(), [text]);
        fs::remove_dir_all(dir).unwrap();
    }
}
