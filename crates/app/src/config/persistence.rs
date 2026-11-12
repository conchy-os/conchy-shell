//! Reading and writing the shell's KDL files, and carrying an old TOML
//! config over to `shell.kdl` the first time it is found.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use kdl::KdlDocument;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{Config, codec};

/// Top-level keys of [`Config`]. A `shell.kdl` with none of them is the
/// template `conchy-config` creates, not somebody's settings.
const CONFIG_KEYS: &[&str] = &[
    "bar",
    "launcher",
    "osd",
    "notification",
    "control_center",
    "dock",
    "frame",
    "panels",
    "services",
    "watch_config",
    "watch_theme",
];

/// Directory holding every file the shell reads or writes.
pub(super) fn config_dir() -> anyhow::Result<PathBuf> {
    let base = config_home()?;
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

fn config_home() -> anyhow::Result<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        Ok(PathBuf::from(xdg))
    } else if let Some(home) = std::env::var_os("HOME") {
        Ok(PathBuf::from(home).join(".config"))
    } else {
        Err(anyhow!(
            "Unable to determine config path (XDG_CONFIG_HOME/HOME not set)"
        ))
    }
}

/// `shell.kdl`, the one file `conchy-config` calls the `shell` channel.
pub fn config_path() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("shell.kdl"))
}

/// Where a TOML config may still be, newest name first.
fn legacy_paths() -> anyhow::Result<Vec<PathBuf>> {
    let dir = config_dir()?;
    Ok(vec![
        dir.join("shell.toml"),
        dir.join("config.toml"),
        config_home()?.join("gpuishell").join("config.toml"),
    ])
}

/// Load `shell.kdl`, writing the defaults when there is none yet. Startup
/// only: on a reload an absent file is a save in flight, see [`load`].
pub fn load_or_create() -> anyhow::Result<Config> {
    let path = config_path()?;
    if let Err(err) = migrate_legacy(&path) {
        tracing::warn!(
            "Could not carry the old config over to {}: {err:#}",
            path.display()
        );
    }
    match parse_config(&path)? {
        Some(config) => Ok(config),
        None => {
            let config = Config::default();
            write_kdl(&path, &config)?;
            Ok(config)
        }
    }
}

/// Load `shell.kdl`. An absent file is an error, not the defaults: editors
/// that save by rename leave it missing for a moment, and applying the
/// defaults then would silently move the bar and drop every setting.
pub fn load() -> anyhow::Result<Config> {
    let path = config_path()?;
    parse_config(&path)?.ok_or_else(|| anyhow!("{} does not exist", path.display()))
}

/// [`parse_kdl`] for `shell.kdl`, which may carry nodes meant for other
/// readers (the template `conchy-config` writes has a `panel`): only the
/// shell's own sections are read.
pub(super) fn parse_config(path: &Path) -> anyhow::Result<Option<Config>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let config =
        config_from_text(&raw).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(Some(config))
}

/// The shell's sections of `shell.kdl` text.
pub(super) fn config_from_text(text: &str) -> anyhow::Result<Config> {
    let mut doc = parse_document(text)?;
    doc.nodes_mut()
        .retain(|node| CONFIG_KEYS.contains(&node.name().value()));
    Ok(toml::Value::Table(codec::decode(&doc)?).try_into()?)
}

/// Read and parse a KDL file into `T`. `Ok(None)` when it isn't there, so
/// every caller decides for itself what an absent file means.
pub(super) fn parse_kdl<T: DeserializeOwned>(path: &Path) -> anyhow::Result<Option<T>> {
    if !path.exists() {
        return migrate_sibling_toml(path);
    }

    let raw =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let parsed = from_text(&raw).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(Some(parsed))
}

/// Parse KDL text into `T`.
pub(super) fn from_text<T: DeserializeOwned>(text: &str) -> anyhow::Result<T> {
    let doc = parse_document(text)?;
    let table = codec::decode(&doc)?;
    Ok(toml::Value::Table(table).try_into()?)
}

/// KDL text to a document, with the line and column where it gave up.
pub(super) fn parse_document(text: &str) -> anyhow::Result<KdlDocument> {
    text.parse::<KdlDocument>().map_err(|err| {
        let offset = err.span.offset().min(text.len());
        let before = &text.as_bytes()[..offset];
        let line = before.iter().filter(|b| **b == b'\n').count() + 1;
        let column = offset
            - before
                .iter()
                .rposition(|b| *b == b'\n')
                .map_or(0, |i| i + 1)
            + 1;
        let label = err
            .label
            .as_deref()
            .map(|l| format!(": {l}"))
            .unwrap_or_default();
        anyhow!("{} at line {line}, column {column}{label}", err.kind)
    })
}

/// Serialize `value` into `path` as KDL, creating the config directory if
/// needed.
pub(super) fn write_kdl<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let text = to_text(value)?;
    write_atomic(path, &text)
}

/// `value` as KDL text.
pub(super) fn to_text<T: Serialize>(value: &T) -> anyhow::Result<String> {
    match toml::Value::try_from(value).context("Failed to encode settings")? {
        toml::Value::Table(table) => Ok(codec::encode(&table).to_string()),
        _ => Err(anyhow!("settings are a table of keys")),
    }
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

    let tmp = path.with_extension("kdl.tmp");
    fs::write(&tmp, text).with_context(|| format!("Failed to write file: {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("Failed to move {} over {}", tmp.display(), path.display()))?;
    Ok(())
}

// -- the move from TOML -------------------------------------------------------

/// Turn an old TOML config into `shell.kdl`, once. Does nothing when
/// `shell.kdl` already holds settings, or when there is no TOML to carry.
///
/// The TOML file is renamed to `*.toml.migrated` when it lives in the conchy
/// directory, so it is not picked up again; one in the old `gpuishell`
/// directory is not ours to touch. A template `shell.kdl` that is replaced is
/// kept as `shell.kdl.bak`.
fn migrate_legacy(path: &Path) -> anyhow::Result<()> {
    if holds_settings(path)? {
        return Ok(());
    }
    let Some(legacy) = legacy_paths()?.into_iter().find(|p| p.exists()) else {
        return Ok(());
    };

    let raw = fs::read_to_string(&legacy)
        .with_context(|| format!("Failed to read {}", legacy.display()))?;
    let table: toml::Table =
        toml::from_str(&raw).with_context(|| format!("Failed to parse {}", legacy.display()))?;
    // Refuse to write what the shell would not read back.
    toml::Value::Table(table.clone())
        .try_into::<Config>()
        .with_context(|| format!("{} does not describe a valid config", legacy.display()))?;

    let header = format!(
        "// Carried over from {} by conchy-shell.\n// Comments in the old file did not come along.\n\n",
        legacy.display()
    );
    let text = format!("{header}{}", codec::encode(&table));

    if path.exists() {
        fs::rename(path, path.with_extension("kdl.bak"))
            .with_context(|| format!("Failed to back up {}", path.display()))?;
    }
    write_atomic(path, &text)?;

    if legacy.parent() == path.parent() {
        fs::rename(&legacy, legacy.with_extension("toml.migrated"))
            .with_context(|| format!("Failed to rename {}", legacy.display()))?;
    }
    tracing::info!("Carried {} over to {}", legacy.display(), path.display());
    Ok(())
}

/// `theme.kdl` and `state.kdl` used to be `theme.toml` and `state.toml`. When
/// the KDL is missing and the TOML is there, carry it over and read that.
fn migrate_sibling_toml<T: DeserializeOwned>(path: &Path) -> anyhow::Result<Option<T>> {
    let legacy = path.with_extension("toml");
    if !legacy.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&legacy)
        .with_context(|| format!("Failed to read {}", legacy.display()))?;
    let table: toml::Table =
        toml::from_str(&raw).with_context(|| format!("Failed to parse {}", legacy.display()))?;
    let parsed: T = toml::Value::Table(table.clone())
        .try_into()
        .with_context(|| format!("Failed to read {}", legacy.display()))?;

    write_atomic(path, &codec::encode(&table).to_string())?;
    fs::rename(&legacy, legacy.with_extension("toml.migrated"))
        .with_context(|| format!("Failed to rename {}", legacy.display()))?;
    tracing::info!("Carried {} over to {}", legacy.display(), path.display());
    Ok(Some(parsed))
}

/// Whether `path` has any of the shell's settings in it.
fn holds_settings(path: &Path) -> anyhow::Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let raw =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let doc =
        parse_document(&raw).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(doc
        .nodes()
        .iter()
        .any(|node| CONFIG_KEYS.contains(&node.name().value())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_config_survives_a_trip_through_kdl() {
        let defaults = Config::default();

        let text = to_text(&defaults).expect("encodes");
        let back = config_from_text(&text).unwrap_or_else(|err| panic!("{err:#}\n{text}"));

        assert_eq!(
            toml::Value::try_from(&back).unwrap(),
            toml::Value::try_from(&defaults).unwrap(),
            "{text}"
        );
    }

    #[test]
    fn a_syntax_error_says_where() {
        let err = from_text::<Config>("bar {\n    position = \n}\n").expect_err("no value");
        let message = format!("{err:#}");
        assert!(message.contains("line 2"), "{message}");
    }

    #[test]
    fn a_bad_value_says_what_was_wrong() {
        let err = config_from_text("bar {\n    position \"sideways\"\n}\n").expect_err("variant");
        let message = format!("{err:#}");
        assert!(message.contains("unknown variant `sideways`"), "{message}");
    }

    #[test]
    fn a_hand_written_file_reads_the_way_it_is_written() {
        let config = config_from_text(concat!(
            "// mine\n",
            "bar {\n",
            "    position \"bottom\"\n",
            "    start \"LauncherBtn\" \"Workspaces\"\n",
            "    (list)center \"ActiveWindow\"\n",
            "}\n",
            "launcher {\n    width 640\n}\n",
        ))
        .expect("parses");

        assert_eq!(config.bar.position, crate::bar::config::BarPosition::Bottom);
        assert_eq!(config.bar.start.len(), 2);
        assert_eq!(config.bar.center.len(), 1);
        assert_eq!(config.launcher.width, 640.0);
    }

    #[test]
    fn an_old_toml_file_is_carried_over_once() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Theme {
            opacity: f32,
            name: String,
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let kdl = dir.path().join("theme.kdl");
        fs::write(dir.path().join("theme.toml"), "opacity = 0.9\nname = \"dusk\"\n").unwrap();

        let first: Option<Theme> = parse_kdl(&kdl).expect("migrates");
        assert_eq!(first, Some(Theme { opacity: 0.9, name: "dusk".into() }));
        assert!(kdl.exists());
        assert!(dir.path().join("theme.toml.migrated").exists());
        assert!(!dir.path().join("theme.toml").exists());

        let again: Option<Theme> = parse_kdl(&kdl).expect("reads the KDL");
        assert_eq!(again, first);
    }

    #[test]
    fn the_conchy_config_template_is_not_a_settings_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("shell.kdl");

        fs::write(&path, "panel {\n    position \"top\"\n}\n").unwrap();
        assert!(!holds_settings(&path).unwrap());

        fs::write(&path, "bar {\n    size 40.0\n}\n").unwrap();
        assert!(holds_settings(&path).unwrap());
    }

    /// The config this machine had before KDL, if it still has one: it must
    /// convert, and the KDL must read back to the same settings.
    #[test]
    fn the_old_toml_config_converts_without_losing_a_setting() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let Ok(raw) = fs::read_to_string(PathBuf::from(home).join(".config/gpuishell/config.toml"))
        else {
            return;
        };
        let table: toml::Table = toml::from_str(&raw).expect("old config parses");
        let before: Config = toml::Value::Table(table.clone())
            .try_into()
            .expect("is a config");

        let kdl = codec::encode(&table).to_string();
        if std::env::var_os("SHOW_KDL").is_some() {
            println!("{kdl}");
        }
        let after = config_from_text(&kdl).unwrap_or_else(|err| panic!("{err:#}\n{kdl}"));

        assert_eq!(
            toml::Value::try_from(&after).unwrap(),
            toml::Value::try_from(&before).unwrap(),
            "{kdl}"
        );
    }
}
