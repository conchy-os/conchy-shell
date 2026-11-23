//! Small file helpers shared by the services that edit user config files.
//!
//! Many of the user's config files are symlinks into a dotfiles repository,
//! so a write must go through the link: renaming a temporary file over the
//! link's path would replace the link with a plain file.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};

/// `$XDG_CONFIG_HOME`, else `~/.config`.
pub fn config_home() -> anyhow::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("$HOME is not set"))?;
    Ok(PathBuf::from(home).join(".config"))
}

/// `$XDG_DATA_HOME`, else `~/.local/share`.
pub fn data_home() -> anyhow::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("$HOME is not set"))?;
    Ok(PathBuf::from(home).join(".local/share"))
}

/// `~`.
pub fn home() -> anyhow::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("$HOME is not set"))
}

/// Where a write to `path` lands: the end of the symlink chain, also when the
/// last link points at a file that does not exist yet.
pub fn write_target(path: &Path) -> PathBuf {
    if let Ok(resolved) = fs::canonicalize(path) {
        return resolved;
    }
    match fs::read_link(path) {
        Ok(link) => path
            .parent()
            .map_or(link.clone(), |parent| parent.join(&link)),
        Err(_) => path.to_path_buf(),
    }
}

/// The file's text; `None` when it is not there. A file that exists but
/// cannot be read is an error and is never to be overwritten blindly.
pub fn read_optional(path: &Path) -> anyhow::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(anyhow!(err).context(format!("Failed to read {}", path.display()))),
    }
}

/// Write `text` to `path` through a temporary file next to the target and a
/// rename, so readers see the old file or the new one and never half of
/// either. A symlink at `path` is followed and survives.
pub fn write_through(path: &Path, text: &str) -> anyhow::Result<()> {
    let target = write_target(path);
    let dir = target
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", target.display()))?;
    fs::create_dir_all(dir).with_context(|| format!("Failed to create {}", dir.display()))?;

    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let tmp = dir.join(format!(".{name}.conchy-tmp"));
    fs::write(&tmp, text).with_context(|| format!("Failed to write {}", tmp.display()))?;
    if let Ok(meta) = fs::metadata(&target) {
        let _ = fs::set_permissions(&tmp, meta.permissions());
    }
    fs::rename(&tmp, &target).map_err(|err| {
        let _ = fs::remove_file(&tmp);
        anyhow!(err).context(format!("Failed to replace {}", target.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_through_a_symlink_and_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("repo/settings.ini");
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, "old").unwrap();
        let link = dir.path().join("settings.ini");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        write_through(&link, "new").unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
    }

    #[test]
    fn creates_a_missing_file_and_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/c.kdl");
        write_through(&path, "x").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "x");
    }

    #[test]
    fn a_missing_file_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_optional(&dir.path().join("nope")).unwrap(), None);
    }
}
