//! The window manager's own config file (`wm.kdl`), which Conchy WM reads and
//! reloads when it changes. Settings the shell offers for Conchy WM are
//! written there, so they apply at once and survive a restart.

use std::path::PathBuf;

use anyhow::{Context, anyhow};
use kdl::KdlDocument;

use crate::{fsutil, kdl_edit};

/// Where Conchy WM reads its config: `$CONCHY_CONFIG`, `$NIRI_CONFIG`, else
/// `~/.config/conchy/wm.kdl` (or `/etc/conchy/wm.kdl` when there is no user file).
pub fn path() -> anyhow::Result<PathBuf> {
    if let Some(explicit) = std::env::var_os("CONCHY_CONFIG")
        .or_else(|| std::env::var_os("NIRI_CONFIG"))
        .filter(|p| !p.is_empty())
    {
        return Ok(PathBuf::from(explicit));
    }
    let user = fsutil::config_home()?.join("conchy/wm.kdl");
    if user.exists() {
        return Ok(user);
    }
    let system = PathBuf::from("/etc/conchy/wm.kdl");
    if system.exists() {
        return Ok(system);
    }
    Ok(user)
}

/// The file's document; an empty one when the file is not there yet.
pub fn load() -> anyhow::Result<(PathBuf, KdlDocument)> {
    let path = path()?;
    let text = fsutil::read_optional(&path)?.unwrap_or_default();
    let doc =
        kdl_edit::parse(&text).with_context(|| format!("{} does not parse", path.display()))?;
    Ok((path, doc))
}

/// Edit the config with `edit` and write it back. A user config that is not
/// there is an error: a new file with only one section would replace the
/// whole config the compositor falls back to.
pub fn update(edit: impl FnOnce(&mut KdlDocument) -> anyhow::Result<()>) -> anyhow::Result<()> {
    let (path, mut doc) = load()?;
    if !path.exists() {
        return Err(anyhow!(
            "{} does not exist yet; start Conchy WM once to create it",
            path.display()
        ));
    }
    // A system-wide file is not the user's to change from here.
    if path.starts_with("/etc") {
        return Err(anyhow!(
            "{} is system-wide; copy it to ~/.config/conchy/wm.kdl to change settings",
            path.display()
        ));
    }
    edit(&mut doc)?;
    fsutil::write_through(&path, &doc.to_string())
}
