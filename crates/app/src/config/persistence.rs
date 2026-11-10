use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::Config;

/// Directory holding every file the shell reads or writes.
pub(super) fn config_dir() -> anyhow::Result<PathBuf> {
    let base = if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        return Err(anyhow!(
            "Unable to determine config path (XDG_CONFIG_HOME/HOME not set)"
        ));
    };

    let conchy_dir = base.join("conchy");
    if conchy_dir.exists() {
        return Ok(conchy_dir);
    }

    let gpui_dir = base.join("gpuishell");
    if gpui_dir.exists() {
        return Ok(gpui_dir);
    }

    Ok(conchy_dir)
}

pub fn config_path() -> anyhow::Result<PathBuf> {
    let dir = config_dir()?;
    let shell_toml = dir.join("shell.toml");
    if shell_toml.exists() {
        return Ok(shell_toml);
    }
    let config_toml = dir.join("config.toml");
    if config_toml.exists() {
        return Ok(config_toml);
    }
    Ok(config_toml)
}

/// Load `config.toml`, writing the defaults when there is none yet. Startup
/// only: on a reload an absent file is a save in flight, see [`load`].
pub fn load_or_create() -> anyhow::Result<Config> {
    let path = config_path()?;
    match parse_toml(&path)? {
        Some(config) => Ok(config),
        None => {
            let config = Config::default();
            write_toml(&path, &config)?;
            Ok(config)
        }
    }
}

/// Load `config.toml`. An absent file is an error, not the defaults: editors
/// that save by rename leave it missing for a moment, and applying the
/// defaults then would silently move the bar and drop every setting.
pub fn load() -> anyhow::Result<Config> {
    let path = config_path()?;
    parse_toml(&path)?.ok_or_else(|| anyhow!("{} does not exist", path.display()))
}

/// Read and parse a TOML file. `Ok(None)` when it isn't there, so every caller
/// decides for itself what an absent file means.
pub(super) fn parse_toml<T: DeserializeOwned>(path: &Path) -> anyhow::Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let parsed =
        toml::from_str(&raw).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(Some(parsed))
}

/// Serialize `value` into `path`, creating the config directory if needed.
pub(super) fn write_toml<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Invalid path has no parent directory: {}", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create directory: {}", parent.display()))?;

    let encoded = toml::to_string_pretty(value).context("Failed to encode TOML")?;
    fs::write(path, encoded)
        .with_context(|| format!("Failed to write file: {}", path.display()))?;
    Ok(())
}

/// Write `text` to `path` through a sibling temp file and a rename, so the
/// file is never absent: [`load`] treats a missing file as an error and the
/// watcher's reload would otherwise race the write.
pub(super) fn write_atomic(path: &Path, text: &str) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Invalid path has no parent directory: {}", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create directory: {}", parent.display()))?;

    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).with_context(|| format!("Failed to write file: {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("Failed to move {} over {}", tmp.display(), path.display()))?;
    Ok(())
}
