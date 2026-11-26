//! Wallpaper service with pluggable engine support.
//!
//! Manages wallpaper setting via external tools: `awww`, or `hyprpaper` when
//! only it is installed and the shell is built with the `hyprland` feature. Starts the daemon on startup and provides commands to
//! set wallpapers.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use tracing::{debug, warn};

use crate::jobs::JobQueue;
use crate::lifecycle::{Lifecycle, ManagedService, RunToken};

#[cfg(feature = "hyprland")]
mod hyprpaper;

/// Supported wallpaper engines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WallpaperEngine {
    #[default]
    Awww,
    /// Built only with the `hyprland` cargo feature: hyprpaper is driven
    /// through `hyprctl`.
    #[cfg(feature = "hyprland")]
    Hyprpaper,
}

impl WallpaperEngine {
    /// The engine this machine has: awww if installed, else hyprpaper (with
    /// the `hyprland` feature).
    pub fn detect() -> Self {
        #[cfg(feature = "hyprland")]
        if !in_path("awww") && in_path("hyprpaper") {
            return Self::Hyprpaper;
        }
        Self::Awww
    }

    /// The engine's program name, for the services page.
    pub fn name(self) -> &'static str {
        match self {
            Self::Awww => "awww",
            #[cfg(feature = "hyprland")]
            Self::Hyprpaper => "hyprpaper",
        }
    }
}

#[cfg(feature = "hyprland")]
fn in_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// Current wallpaper state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WallpaperData {
    /// Path to the currently set wallpaper, if known.
    pub current: Option<PathBuf>,
    /// Active wallpaper engine.
    pub engine: WallpaperEngine,
}

/// Commands for the wallpaper service.
#[derive(Debug, Clone)]
pub enum WallpaperCommand {
    /// Set wallpaper to the given image path.
    SetWallpaper(PathBuf),
}

/// Reactive wallpaper subscriber.
#[derive(Debug, Clone)]
pub struct WallpaperSubscriber {
    core: crate::ServiceCore<WallpaperData, WallpaperCommand>,
}

impl std::ops::Deref for WallpaperSubscriber {
    type Target = crate::ServiceCore<WallpaperData, WallpaperCommand>;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

impl Default for WallpaperSubscriber {
    fn default() -> Self {
        Self {
            core: crate::ServiceCore::with_jobs(
                // Clicking through the picker only needs the last pick applied.
                JobQueue::new("Wallpaper").coalesce(|_, _| true),
            ),
        }
    }
}

impl WallpaperSubscriber {
    /// Create a new wallpaper subscriber. The daemon is spawned by
    /// [`ManagedService::start`].
    pub fn new() -> Self {
        Self::default()
    }

    async fn execute(&self, command: WallpaperCommand) -> anyhow::Result<()> {
        match command {
            WallpaperCommand::SetWallpaper(path) => {
                let shown = path.clone();
                let engine = self.data.lock_ref().engine;
                tokio::task::spawn_blocking(move || match engine {
                    WallpaperEngine::Awww => set_wallpaper_awww(&shown),
                    #[cfg(feature = "hyprland")]
                    WallpaperEngine::Hyprpaper => hyprpaper::set_wallpaper(&shown),
                })
                .await??;
                save_current(&path);
                self.data.lock_mut().current = Some(path);
            }
        }
        Ok(())
    }
}

impl ManagedService for WallpaperSubscriber {
    fn name(&self) -> &'static str {
        "Wallpaper"
    }

    fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    fn memory_bytes(&self) -> usize {
        size_of::<WallpaperData>()
            + self
                .data
                .lock_ref()
                .current
                .as_ref()
                .map_or(0, |path| path.as_os_str().len())
    }

    /// Stopping only detaches the shell: `awww-daemon` is an external process
    /// and keeps the current wallpaper up.
    fn start(&self) {
        let token = self.lifecycle.begin();
        let engine = WallpaperEngine::detect();
        self.data.lock_mut().engine = engine;

        let subscriber = self.clone();
        self.jobs.run(token.clone(), move |command| {
            let subscriber = subscriber.clone();
            async move { subscriber.execute(command).await }
        });

        match engine {
            WallpaperEngine::Awww => start_daemon(token),
            #[cfg(feature = "hyprland")]
            WallpaperEngine::Hyprpaper => hyprpaper::start(token),
        }
    }
}

/// Start the awww daemon if it's not already running.
fn start_daemon(token: RunToken) {
    thread::spawn(move || {
        // Check if awww is already running
        let running = Command::new("awww")
            .arg("query")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if running {
            debug!("awww daemon already running");
            token.active();
            return;
        }

        debug!("Starting awww daemon");
        match Command::new("awww-daemon").spawn() {
            Ok(_) => {
                debug!("awww daemon started");
                token.active();
            }
            Err(e) => {
                warn!("Failed to start awww daemon: {}", e);
                token.error(e.to_string());
            }
        }
    });
}

/// Set wallpaper using awww, waiting for it so the child is reaped and a
/// failure is reported instead of assumed away.
fn set_wallpaper_awww(path: &std::path::Path) -> anyhow::Result<()> {
    let status = Command::new("awww")
        .args([
            "img",
            &path.to_string_lossy(),
            "--transition-type",
            "fade",
            "--transition-duration",
            "1",
        ])
        .status()?;
    anyhow::ensure!(status.success(), "awww img exited with {status}");
    debug!("Wallpaper set to: {}", path.display());
    Ok(())
}

/// The wallpaper that is up right now: what the awww daemon reports, else the
/// last one this shell set (awww forgets it when the daemon restarts).
///
/// Blocks on a subprocess; call it off the UI thread.
pub fn current_wallpaper() -> Option<PathBuf> {
    let queried = match WallpaperEngine::detect() {
        #[cfg(feature = "hyprland")]
        WallpaperEngine::Awww => query_awww().or_else(hyprpaper::query),
        #[cfg(not(feature = "hyprland"))]
        WallpaperEngine::Awww => query_awww(),
        #[cfg(feature = "hyprland")]
        WallpaperEngine::Hyprpaper => hyprpaper::query().or_else(query_awww),
    };
    queried.or_else(load_current)
}

fn query_awww() -> Option<PathBuf> {
    let output = Command::new("awww").arg("query").output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_query(&String::from_utf8_lossy(&output.stdout))
}

/// First image in `awww query` output, which has a line per output like
/// `: eDP-1: 1920x1080, scale: 1, currently displaying: image: /path/a.png`.
fn parse_query(text: &str) -> Option<PathBuf> {
    text.lines().find_map(|line| {
        let (_, path) = line.split_once("image: ")?;
        let path = PathBuf::from(path.trim());
        path.is_file().then_some(path)
    })
}

fn state_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(base.join("conchy/wallpaper"))
}

fn save_current(path: &Path) {
    let Some(file) = state_file() else { return };
    if let Some(dir) = file.parent()
        && let Err(err) = std::fs::create_dir_all(dir)
    {
        warn!("Could not create {}: {err}", dir.display());
        return;
    }
    if let Err(err) = std::fs::write(&file, path.to_string_lossy().as_bytes()) {
        warn!(
            "Could not remember the wallpaper in {}: {err}",
            file.display()
        );
    }
}

fn load_current() -> Option<PathBuf> {
    let text = std::fs::read_to_string(state_file()?).ok()?;
    let path = PathBuf::from(text.trim());
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_picks_the_image_path() {
        let exe = std::env::current_exe().unwrap();
        let line = format!(
            ": eDP-1: 1920x1080, scale: 1, currently displaying: image: {}",
            exe.display()
        );
        assert_eq!(parse_query(&line), Some(exe));
    }

    #[test]
    fn query_ignores_color_fills_and_missing_files() {
        assert_eq!(
            parse_query(": DP-1: 2560x1440, scale: 1, currently displaying: color: 000000"),
            None
        );
        assert_eq!(
            parse_query(": DP-1: 1x1, scale: 1, currently displaying: image: /nope/x.png"),
            None
        );
        assert_eq!(parse_query(""), None);
    }
}
