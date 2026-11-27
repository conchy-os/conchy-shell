# AGENTS.md — `conchy-shell`

Instructions for AI coding agents and contributors working in `conchy-shell`.

`conchy-shell` is the GPUI-based Wayland desktop shell for **Conchy OS** (`github.com/conchy-os`): status bar, dock, launcher, OSD, control center, notifications, session settings UI, and `org.conchy.Portal.ShellHost` dialogs.

---

## Shared Conchy OS Rules

- **Rolling release**: Conchy OS is a rolling distribution with date-based builds (`YYYYMMDD`) and commit hashes, with no semantic OS release version numbers. Do not introduce version-tag release workflows, versioned changelogs, or milestone version numbers. (`Cargo.toml` keeps `version = "0.1.0"` solely because Cargo requires a semver field.)
- **Rust-only for our own code**: write all first-party Conchy OS components in pure Rust. Never introduce GTK or Qt dependencies into our code (`conchy-shell` renders via GPUI on Wayland/Vulkan).
- **One utility = one task**: tools compose via pipes, IPC, D-Bus, and KDL files. `conchy-shell` owns the desktop shell UI and user-facing hardware/service controls. Keep compositor window management in `conchy-wm`, KDL persistence/`org.conchy.Config` in `conchy-config`, portal routing in `conchy-portal` (`xdg-desktop-portal-conchy`), file management/`FilesHost` in `conchy-files`, session supervision in `conchy-session`, screen locking in `conchy-lock`, screenshots in `conchy-shot`, and VPN daemon/client logic in `conchy-vpnx`.
- **Target OS priority**: **Void Linux (`glibc` / `runit` / `xbps`) first**, **Arch Linux second**. Never add `systemd`-only assumptions (power controls must work with `loginctl` / `zzz` / `elogind`).
- **Git workflow**: single long-lived branch `main`. Work on short-lived PR branches and delete them immediately after merge. Never force-push `main`, and do not run `git commit` or `git push` unless explicitly instructed.
- **Roadmap workflow**: project-wide tasks, `depends`, architectural `decisions`, and `done_when` criteria live in [`../conchy-os/ROADMAP.md`](../conchy-os/ROADMAP.md) and [`../conchy-os/roadmap.json`](../conchy-os/roadmap.json). Verify dependencies and `done_when` criteria before completing roadmap work; never modify `../conchy-os` or sibling repositories unless explicitly tasked to do so.
- **Mandatory Rust verification** after every code change (zero warnings):
  ```sh
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
  > Note: `cargo fmt --all` also targets local path dependencies outside this repo when writing changes. Run `cargo fmt` (without `--all`) to format only `conchy-shell` crates, then run `cargo fmt --all -- --check` to verify.

---

## Safety Rules (CRITICAL)

- **Never touch the live user session**:
  - Do **NOT** invoke `hyprctl`, send IPC commands to the user's running compositor or shell, or kill/restart live session processes (`conchy-wm`, `conchy-shell`, `conchy-session`, `conchy-config`).
  - Do **NOT** start daemons on the host session bus or write to `~/.config`, `~/.local`, `~/.cache`, `/usr`, `/etc`, or sibling repositories.
- **Never run `conchy-shell daemon` against a live session**:
  - Running `conchy-shell daemon` touches the single-instance IPC socket (`$XDG_RUNTIME_DIR/conchy-shell.sock`), claims session D-Bus names (notifications, SNI tray watcher/host, portal shell host), and binds `zwlr_layer_shell_v1` surfaces on the active Wayland display.
- **Isolate XDG directories (and remember the `dconf` / `gsettings` trap)**:
  - Always isolate `XDG_CONFIG_HOME` and `XDG_STATE_HOME` (plus `XDG_DATA_HOME`, `XDG_CACHE_HOME`, and `HOME` when appropriate) in a temporary directory created with `mktemp -d` (and clean it up afterward).
  - **The `dconf` / `gsettings` trap**: redirecting `XDG_CONFIG_HOME` alone does **not** isolate GLib / `gsettings` / `dconf` subprocesses (for example when testing appearance sync in `services::appearance`) while `DBUS_SESSION_BUS_ADDRESS` points to the host session bus. Unset `DBUS_SESSION_BUS_ADDRESS` (`env -u DBUS_SESSION_BUS_ADDRESS`) and/or set `GSETTINGS_BACKEND=memory` whenever isolating tests or tools that could invoke `gsettings`, `dconf`, or D-Bus portals.
- **Never touch `~/.config/conchy/*` or `~/.config/mimeapps.list` during tests**:
  - Shell configuration (`shell.kdl`, `state.kdl`, `theme.kdl`, `shortcuts.kdl`), equalizer/mic presets, display profiles, and default-app associations (`mimeapps.list`) must only ever be read/written inside isolated temporary directories in tests.
- **Never use `pkill -f` matching your own command**: a broad `pkill -f` pattern can match the calling shell or command line itself.
- **Never load unverified `.so` files**: do not load untrusted shared libraries (e.g., via `LD_PRELOAD` or unverified LADSPA/LV2 plugins).

---

## Repository Specifics (`conchy-shell`)

### Workspace Crates

Workspace manifest (`Cargo.toml`, `edition = "2024"`, `resolver = "3"`):

- `crates/app` (`conchy-shell`): main shell binary — status bar, calendar, config/KDL codec & persistence, control center, dock, window frame, app icons, single-instance IPC, keybinds, launcher & modules (including `;cfg`, `;equ`, `;mic`, `;wp`), notifications, OSD, `portal_host`, session settings, shortcuts editor, state, and multi-monitor windows.
- `crates/services` (`services`): background hardware and desktop services — `conchy-ipc` client, audio (PipeWire / PulseAudio), 10-band `equalizer`, `mic` effects chain, Bluetooth, brightness, MPRIS, NetworkManager, privacy, sysinfo, SNI tray, UPower, appearance sync, autostart, display profiles, input, `mimeapps`, wallpaper, `wm_config`, and XKB.
- `crates/ui` (`ui`): reusable GPUI UI components, typography, spacing/radius tokens, Base16 palettes, and in-process Material You theme extraction.
- `crates/assets` (`assets`): embedded SVG icons and vector assets via `rust-embed`.
- `crates/story` (`story`, `publish = false`): standalone UI development binaries `story` (per-component gallery) and `showcase` (all interactive components on one shared surface).

### Cross-Repo Path Dependencies & Workspace Symlinks

`conchy-shell` depends on sibling repositories via local paths:

- `conchy-ipc = { path = "../compositor/conchy-ipc" }` → `conchy-wm/conchy-ipc`
- `conchy-config = { path = "../config" }` → `conchy-config`
- `vpnx-client = { path = "../vpnx/crates/client" }` → `conchy-vpnx/crates/client`
- `conchy-portal-proto = { path = "../../../conchy-portal/crates/proto" }` (in `crates/app/Cargo.toml`) → `conchy-portal/crates/proto`

This layout relies on parent workspace symlinks alongside the sibling checkouts:
- `compositor -> conchy-wm`
- `config -> conchy-config`
- `vpnx -> conchy-vpnx`

Never modify any sibling repository when working on `conchy-shell`.

### Compositor Backends & the `hyprland` Feature

- The default build supports Conchy WM (and niri, through the same IPC) and Mango. Conchy OS ships only Conchy WM.
- The Hyprland backend (`hyprctl`, `hyprpaper`, the `hyprland` crate) is behind the `hyprland` cargo feature (`services/hyprland`, forwarded by `conchy-shell/hyprland`). Its code lives in `services/src/compositor/hyprland.rs`, `services/src/input/hyprland.rs`, and `services/src/wallpaper/hyprpaper.rs`; keep new Hyprland-only code there or behind `#[cfg(feature = "hyprland")]`.
- Verify both builds: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo clippy --workspace --all-targets --features services/hyprland -- -D warnings`.

### Comment & Documentation Language

- **Comment language: English.** All comments (`//`), docstrings (`///`, `//!`), and internal documentation in `conchy-shell` must be written in **English**.
- Preserve existing comments and docstrings unrelated to your changes.

### Configuration & Protocol References

- [`docs/CONFIG_SPEC.md`](docs/CONFIG_SPEC.md) — KDL configuration (`shell.kdl`, `state.kdl`, `theme.kdl`, `shortcuts.kdl`), `org.conchy.Config` D-Bus API, and wallpaper/Material You integration.
- [`docs/IPC_GUIDE.md`](docs/IPC_GUIDE.md) — `conchy-wm` newline-delimited JSON IPC protocol over `$CONCHY_SOCKET`.
- `config/shell.kdl` — default KDL configuration template.

### Examples, Probes, and Tools

- **Audio & Mic probes (live PipeWire)**:
  ```sh
  XDG_CONFIG_HOME=$(mktemp -d) cargo run -p services --example equalizer_probe
  XDG_CONFIG_HOME=$(mktemp -d) cargo run -p services --example mic_probe
  ```
  > **Warning:** `equalizer_probe` and `mic_probe` interact with **live PipeWire** (creating/removing filter-chain sinks/sources). Always isolate `XDG_CONFIG_HOME=$(mktemp -d)` and run them only when intentionally testing audio.
- **Service restart leak probe**:
  ```sh
  cargo run -p services --example restart_probe [rounds]
  ```
  Explicitly omits tray and notification services so it does not claim session D-Bus names owned by a running shell.
- **UI component gallery & showcase**:
  ```sh
  cargo run -p story --bin story
  cargo run -p story --bin showcase
  ```
- **Config validation (no daemon / no single-instance lock)**:
  ```sh
  tmp=$(mktemp -d)
  XDG_CONFIG_HOME="$tmp" cargo run -p conchy-shell -- --validate
  rm -rf "$tmp"
  ```
- **Portal-only dev mode**:
  - `CONCHY_SHELL_PORTAL_ONLY=1` starts only `portal_host` (`org.conchy.Portal.ShellHost`) without setting up compositor IPC or hardware services. Still never run it against a live session where `$XDG_RUNTIME_DIR/conchy-shell.sock` or the portal host bus name is active.
- **Polkit agent testing:**
  - The shell registers as the polkit agent of its login session (system bus). If another agent already holds the session, it logs `An authentication agent already exists for the given subject` and runs without one.
  - To test without taking over the session, set `CONCHY_POLKIT_PROCESS=<pid>`: the agent registers for that one process (like `pkttyagent --process`), and `pkexec` run as a child of it asks in the shell's dialog.

### Isolated Verification Suite

Run tests in both the normal environment and an isolated empty XDG/HOME environment without display or session D-Bus:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

tmp=$(mktemp -d)
env -u WAYLAND_DISPLAY -u DISPLAY -u DBUS_SESSION_BUS_ADDRESS \
  CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
  XDG_CONFIG_HOME="$tmp/cfg" XDG_STATE_HOME="$tmp/state" HOME="$tmp/home" \
  cargo test --workspace
rm -rf "$tmp"
```

### Void Linux Build Dependencies

On Void Linux (`glibc`), building `conchy-shell` requires:

```sh
xbps-install -y \
  pkg-config gcc clang cmake nasm perl \
  pipewire-devel pulseaudio-devel eudev-libudev-devel \
  wayland-devel libxkbcommon-devel libxcb-devel \
  xcb-util-devel xcb-util-wm-devel xcb-util-keysyms-devel \
  xcb-util-renderutil-devel xcb-util-image-devel \
  libX11-devel libXext-devel libXcursor-devel libXi-devel libXrandr-devel \
  fontconfig-devel freetype-devel vulkan-loader-devel
```

Minimal/CI containers starting from `ghcr.io/void-linux/void-glibc-full:latest` also need `rust cargo git bash zstd`.

### Void CI

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs on `push` and `pull_request` targeting `main` inside `ghcr.io/void-linux/void-glibc-full:latest`. It checks out `conchy-shell` along with `conchy-config`, `conchy-wm`, `conchy-vpnx`, and `conchy-portal`, creates the `config`, `compositor`, and `vpnx` symlinks, caches Cargo and `target`, and enforces `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
