use std::path::PathBuf;

use ui::{StoredTheme, Theme};

use crate::config::persistence::{config_dir, parse_kdl, write_kdl};

pub fn theme_path() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("theme.kdl"))
}

/// Read `theme.kdl`. A theme we can't parse is reported, not repaired:
/// hot reload fires on every save, so writing the defaults back here would
/// destroy a file caught mid-edit.
pub fn load_theme() -> anyhow::Result<Theme> {
    Ok(parse_kdl::<StoredTheme>(&theme_path()?)?
        .map_or_else(Theme::default, StoredTheme::into_theme))
}

pub fn save_theme(theme: &Theme) -> anyhow::Result<()> {
    write_kdl(&theme_path()?, &StoredTheme::from_theme(theme))
}
