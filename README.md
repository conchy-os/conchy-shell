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
```

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
