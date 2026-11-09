# Conchy Shell

Minimalist desktop shell, status bar, OSD, and control center for **Conchy OS** (`github.com/conchy-os`).

Built in Rust for Wayland, integrating directly with `conchy-wm` via `conchy-ipc` and synchronized live via `conchy-config` D-Bus service.

---

## Features

- **Wayland-Native:** Built for Wayland layer-shell protocol (`zwlr_layer_shell_v1`).
- **Live Reactivity:** Zero-polling event stream for instant workspace, window focus, and monitor tracking.
- **Unified KDL Config:** Configured via `~/.config/conchy/shell.kdl`.
- **D-Bus Synchronization:** Listens to `org.conchy.Config` for dynamic theming, font, and size updates without restarting.
- **Fast & Minimal:** Written in pure Rust with modular UI separation.

---

## Quick Start

### Build & Run

```bash
# Check compilation (uses throttled build flags)
nice -n 19 ionice -c 3 cargo check --jobs 2

# Run daemon
cargo run --bin conchy-shell -- daemon

# Print current WM status
cargo run --bin conchy-shell -- status

# Switch workspace
cargo run --bin conchy-shell -- action focus-workspace 2
```

### Try Examples

```bash
# 1. Listen to live workspace and window focus events:
cargo run --example workspace_listener

# 2. Dispatch commands to compositor:
cargo run --example send_action -- 3

# 3. Watch live configuration changes:
cargo run --example config_watcher
```

---

## Documentation

- [Architecture & UI Toolkits](docs/ARCHITECTURE.md)
- [Compositor IPC Guide](docs/IPC_GUIDE.md)
- [Configuration Specification](docs/CONFIG_SPEC.md)

---

## License

GPL-3.0-or-later
