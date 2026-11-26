//! hyprpaper, the wallpaper engine of a Hyprland desktop. Built only with the
//! `hyprland` cargo feature: it is driven through `hyprctl`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use tracing::{debug, warn};

use crate::lifecycle::RunToken;

/// hyprpaper is normally started by the session; start it only if it isn't up.
pub(super) fn start(token: RunToken) {
    thread::spawn(move || {
        let running = Command::new("pgrep")
            .args(["-x", "hyprpaper"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if running {
            token.active();
            return;
        }
        match Command::new("hyprpaper").spawn() {
            Ok(_) => token.active(),
            Err(e) => {
                warn!("Failed to start hyprpaper: {e}");
                token.error(e.to_string());
            }
        }
    });
}

/// Set wallpaper through hyprpaper's IPC on every monitor.
pub(super) fn set_wallpaper(path: &Path) -> anyhow::Result<()> {
    let output = Command::new("hyprctl")
        .args(["hyprpaper", "wallpaper"])
        .arg(format!(",{},cover", path.display()))
        .output()?;
    let reply = String::from_utf8_lossy(&output.stdout);
    // Silence and "ok" are both success; hyprpaper words its errors.
    let reply_ok = matches!(reply.trim(), "" | "ok" | "OK");
    anyhow::ensure!(
        output.status.success() && reply_ok,
        "hyprctl hyprpaper wallpaper: {}",
        reply.trim()
    );
    debug!("Wallpaper set to: {}", path.display());
    Ok(())
}

/// The wallpaper hyprpaper shows now.
pub(super) fn query() -> Option<PathBuf> {
    let output = Command::new("hyprctl")
        .args(["hyprpaper", "listactive"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_listactive(&String::from_utf8_lossy(&output.stdout))
}

/// First existing path in `hyprctl hyprpaper listactive`, whose lines read
/// `eDP-1: /path/a.jpg`.
fn parse_listactive(text: &str) -> Option<PathBuf> {
    text.lines().find_map(|line| {
        let (_, path) = line.split_once(": ")?;
        let path = PathBuf::from(path.trim());
        path.is_file().then_some(path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listactive_picks_the_first_existing_path() {
        let exe = std::env::current_exe().unwrap();
        let text = format!("DP-1: /nope/a.jpg\neDP-1: {}\n", exe.display());
        assert_eq!(parse_listactive(&text), Some(exe));
        assert_eq!(parse_listactive("no wallpapers"), None);
    }
}
