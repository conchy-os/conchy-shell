# Conchy Shell Architecture & Developer Guide

`conchy-shell` is the desktop shell, status bar, OSD, and control center for **Conchy OS**.
It provides the visual interface around `conchy-wm` (Wayland compositor) and is managed reactively via `conchy-config` (unified KDL configuration engine).

---

## 1. System Topology

```
+---------------------------------------------------------------------------------+
|                                 conchy-session                                  |
|                            (Process Supervisor)                                 |
+---------------------+-------------------------------+---------------------------+
                      |                               |
                      v                               v
        +----------------------------+  +----------------------------+
        |         conchy-wm          |  |    conchy-config daemon    |
        |    (Wayland Compositor)    |  |     (org.conchy.Config)    |
        +--------------+-------------+  +--------------+-------------+
                       |                               |
           CONCHY_SOCKET (UNIX)                  D-Bus Session
           EventStream & Actions            PropertyChanged("shell", ...)
                       |                               |
                       +--------------+----------------+
                                      |
                                      v
                        +----------------------------+
                        |        conchy-shell        |
                        |   - Layer Shell Top Bar    |
                        |   - Workspace Indicator    |
                        |   - Window Title / Taskbar |
                        |   - OSD Volume/Brightness  |
                        |   - Live Reactive Styles   |
                        +----------------------------+
```

---

## 2. Core Responsibilities

1. **Wayland Layer Shell Surface:**
   - Surfaces are registered under the `zwlr_layer_shell_v1` protocol.
   - Typically anchored to `Top | Left | Right` with exclusive zone set to the bar height.
2. **Workspace & Window Tracking:**
   - Connects to `conchy-wm` via `conchy-ipc`.
   - Subscribes to `EventStream` to receive instant notifications when workspaces change, windows open, close, or switch focus.
   - Dispatches user clicks (e.g. clicking workspace #3 sends `Action::FocusWorkspace`).
3. **Live Configuration Reactivity:**
   - Connects to D-Bus service `org.conchy.Config`.
   - On startup, loads settings from channel `"shell"` (which reflects `~/.config/conchy/shell.kdl`).
   - Listens to `PropertyChanged` D-Bus signal to update colors, height, and font on the fly without restarting!

---

## 3. Recommended UI Toolkits for Wayland

When building the graphical front-end for `conchy-shell`, the team has several proven options:

### Option A: `relm4` + `gtk4-layer-shell` (Recommended for fast feature velocity)
- **Pros:** Full widget library (buttons, sliders, popovers, system tray), CSS styling support, native Wayland layer shell integration via `gtk4-layer-shell`.
- **Ideal for:** Rapidly building rich bars, volume sliders, menus, and notification popups.

### Option B: Smithay Client Toolkit (`sctk`) + `tiny-skia` / `cairo`
- **Pros:** Ultra-lightweight, zero heavy C dependencies, total control over rendering and memory footprint.
- **Ideal for:** A hyper-minimalist bar with sub-millisecond launch time.

### Option C: `iced` with layer-shell backend (`iced-layershell`)
- **Pros:** 100% pure Rust Elm-architecture GUI toolkit, modern, type-safe state management.

---

## 4. Project Structure

```
conchy-shell/
├── Cargo.toml                  # Workspace & crate dependencies
├── README.md                   # Getting started & contributor instructions
├── docs/
│   ├── ARCHITECTURE.md         # System design and toolkit recommendations (this file)
│   ├── IPC_GUIDE.md            # Complete reference for conchy-ipc socket
│   └── CONFIG_SPEC.md          # Specification for ~/.config/conchy/shell.kdl
├── config/
│   └── shell.kdl               # Default template configuration
├── examples/
│   ├── workspace_listener.rs   # Quick listener for WM events
│   ├── send_action.rs          # Quick sender for WM commands
│   └── config_watcher.rs       # Quick listener for D-Bus config changes
└── src/
    ├── lib.rs                  # Exported library modules
    ├── main.rs                 # CLI entry point (daemon, status, action)
    ├── client.rs               # conchy-wm IPC socket client
    ├── config.rs               # org.conchy.Config D-Bus client
    ├── state.rs                # Local desktop state tracker
    └── ui/
        ├── mod.rs
        └── bar.rs              # Status bar model & formatters
```
