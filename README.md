# Conchy Shell

GPU-accelerated desktop shell, status bar, dock, OSD, launcher, and control center for **Conchy OS** (`github.com/conchy-os`).

Built in 100% Rust using Zed's GPU-accelerated **GPUI** framework for Wayland (`zwlr_layer_shell_v1`), integrating natively with `conchy-wm` via `conchy-ipc` and synchronized live via `conchy-config` D-Bus service.

---

## 🧭 Key Features & Architecture

- **GPU-Accelerated & Low Latency:** Powered by GPUI and Vulkan/wgpu pipelines, delivering sub-millisecond input response, smooth 120–240Hz frame presentation, and near-zero CPU overhead (~40-50MB RSS).
- **Single-Instance Architecture:** The main shell runs as a persistent daemon holding the GPU context. Secondary CLI invocations communicate over a fast UNIX domain socket (`$XDG_RUNTIME_DIR/conchy-shell.sock`) in `< 1ms` and exit immediately without spawning secondary GPU runtimes.
- **Strict Threading & Concurrency Isolation:**
  - **Zero-lock UI thread:** Background services (`conchy-ipc`, `conchy-config`, PipeWire, Bluetooth, Battery) run in a Tokio pool, publishing updates via lock-free reactive signals (`futures_signals::Mutable<T>`).
  - **Non-blocking commands:** Actions to the window manager run asynchronously through `JobQueue` on `spawn_blocking`.
  - **Leak Prevention:** Long-running socket and bus listeners are supervised via `Lifecycle` and generational `RunToken`s to eliminate ghost/zombie threads on reconnect.
- **Scrolling Ribbon & Window Minimizing:**
  - Native integration with `conchy-wm`'s horizontal scrolling ribbon and column management.
  - Full support for window minimizing (`Action::MinimizeWindow`): minimized windows are surfaced in the dock with subtle translucent styling, unminimizing and focusing in a single click.
- **Shortcuts in the UI:** `;cfg` → Shortcuts rebinds, adds and removes the compositor's keys (stored in `~/.config/conchy/shortcuts.kdl`, read by `conchy-wm`). Tapping **Super** opens the launcher, also over fullscreen apps if you want; see [docs/CONFIG_SPEC.md](docs/CONFIG_SPEC.md#4-keyboard-shortcuts-shortcutskdl).
- **System settings in the UI:** `;cfg` → *System* holds the pages that change the machine, not only the shell:
  - **Mouse & Touchpad** (speed, acceleration, natural scrolling, tap to click…) and **Keyboard** (layouts and variants, the switching key, Caps Lock, repeat delay and rate). On Conchy WM they are written to the `input` block of `~/.config/conchy/wm.kdl`, which the compositor reloads; on Hyprland (a `--features hyprland` build) they are applied through `hyprctl eval` and kept in `~/.config/conchy/input.kdl`, applied again when the shell starts.
  - **Displays & Monitors**: resolution, refresh rate, scale, rotation, arrangement, VRR and on/off, with a 15 s "keep these settings?" countdown. **Profiles** (`~/.config/conchy/displays.kdl`) save a whole setup and put it back by itself when the same monitors are connected, in whichever port.
  - **Startup applications**: the XDG autostart entries (`~/.config/autostart`, `/etc/xdg/autostart`) with a switch each (`Hidden=true` in a user copy), plus your own programs.
  - **Appearance**: fonts, text scaling, DPI, cursor and icon themes, written to gsettings, GTK 3/4 `settings.ini`, `xsettingsd`, Xresources and the compositor.
- **Dynamic Theming & D-Bus Reactivity:** Listens to `org.conchy.Config` for dynamic size, position, and styling hot-reloading without restarting the shell.
- **System Integration for Void Linux:**
  - Native PipeWire and PulseAudio bindings for audio and device management.
  - Native BlueZ, NetworkManager, UPower, and StatusNotifierItem (SNI) tray support.
  - Void Linux-friendly power commands (`loginctl` / `zzz` / `elogind`) without `systemd` dependencies.

---

## 📂 Workspace Crates

```
shell/
├── Cargo.toml               # Workspace manifest
├── crates/
│   ├── app/                 # Main binary (conchy-shell): bar, dock, launcher, control center, OSD
│   ├── services/            # Background hardware & desktop services (conchy-ipc, audio, tray, etc.)
│   ├── ui/                  # GPUI components, typography, and theme definitions
│   └── assets/              # Embedded SVG icons and vector assets
├── config/
│   └── shell.kdl            # Default KDL configuration specification
└── docs/                    # Architecture and developer guides
```

---

## 🚀 Usage

### Build

```bash
# Debug build
cargo build -p conchy-shell

# Optimized release build
cargo build -p conchy-shell --release

# With the Hyprland backend, for development on a Hyprland desktop
cargo build -p conchy-shell --features hyprland
```

Conchy OS ships Conchy WM, so the default build talks to it (and to niri and
Mango) only. The Hyprland backend (`hyprctl`, `hyprpaper`) is behind the
`hyprland` cargo feature.

### Run Daemon

```bash
# Start shell daemon
conchy-shell daemon
```

### Fast CLI Controls (Compositor Keybinds)

When the shell daemon is active, CLI invocations signal the primary instance instantly:

```bash
# Toggle application launcher
conchy-shell toggle-launcher

# Toggle control center panel
conchy-shell toggle-control-center

# Toggle overview mode
conchy-shell toggle-overview

# Minimize active window
conchy-shell toggle-minimize

# Close active window
conchy-shell close-window

# OSD Volume Controls
conchy-shell osd volume-up
conchy-shell osd volume-down
conchy-shell osd volume-mute

# OSD Brightness Controls
conchy-shell osd brightness-up
conchy-shell osd brightness-down
```

---

## 📄 License

GPL-3.0-or-later
