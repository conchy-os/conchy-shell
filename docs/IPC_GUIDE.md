# Conchy WM IPC Protocol Guide

`conchy-wm` exposes a fast Unix Domain Socket IPC interface allowing external tools—most importantly `conchy-shell`—to query desktop state, stream events, and dispatch actions.

---

## 1. Socket Location & Discovery

The compositor automatically exports the path of its active IPC socket into the environment variable:

```bash
echo $CONCHY_SOCKET
# Example output: /run/user/1000/conchy.wayland-1.12345.sock
```

In Rust code, you can connect using the helper provided by the `conchy-ipc` crate:

```rust
use conchy_ipc::socket::Socket;

// Reads $CONCHY_SOCKET and opens the UnixStream
let mut socket = Socket::connect()?;
```

---

## 2. Wire Protocol

- **Format:** Newline-delimited JSON (`\n`).
- **Encoding:** UTF-8 text.
- **Request:** Client writes a JSON-serialized `Request` object followed by `\n`.
- **Response:** Compositor responds with a JSON-serialized `Reply` object followed by `\n`.
- **Event Streaming:** When `Request::EventStream` is sent, the compositor replies with `Ok(Response::Handled)` and then continuously sends `Event` objects as individual JSON lines until the socket closes.

---

## 3. Core Requests

### `Request::Workspaces`
Retrieves all workspaces across all monitors.

```rust
use conchy_ipc::{Request, Response};

let reply = socket.send(Request::Workspaces)?;
if let Ok(Response::Workspaces(workspaces)) = reply {
    for ws in workspaces {
        println!("Workspace #{}: active={}, focused={}, urgent={}",
            ws.idx, ws.is_active, ws.is_focused, ws.is_urgent);
    }
}
```

### `Request::Windows`
Retrieves all open toplevel windows.

```rust
let reply = socket.send(Request::Windows)?;
if let Ok(Response::Windows(windows)) = reply {
    for win in windows {
        println!("Window [{}]: title={:?}, app_id={:?}, focused={}",
            win.id, win.title, win.app_id, win.is_focused);
    }
}
```

### `Request::Outputs`
Retrieves all connected display monitors and their modes.

```rust
let reply = socket.send(Request::Outputs)?;
if let Ok(Response::Outputs(outputs)) = reply {
    for (name, output) in outputs {
        println!("Monitor {}: resolution={:?}, scale={:?}",
            name, output.current_mode, output.scale);
    }
}
```

---

## 4. Dispatching Actions

Actions allow the shell to control the window manager in response to user clicks, keyboard shortcuts, or gestures.

### Switching Workspaces
```rust
use conchy_ipc::{Action, Request, WorkspaceReferenceArg};

// Switch to workspace by 1-based index:
socket.send(Request::Action(Action::FocusWorkspace {
    reference: WorkspaceReferenceArg::Index(2),
}))?;

// Or switch to workspace by name:
socket.send(Request::Action(Action::FocusWorkspace {
    reference: WorkspaceReferenceArg::Name("code".to_string()),
}))?;
```

### Focusing & Closing Windows
```rust
// Focus window by its unique ID
socket.send(Request::Action(Action::FocusWindow { id: 42 }))?;

// Close the active window (or pass Some(id) to close a specific window)
socket.send(Request::Action(Action::CloseWindow { id: None }))?;
```

### Toggling Overview
```rust
// Toggles the overview / workspace grid
socket.send(Request::Action(Action::ToggleOverview {}))?;
```

### Spawning Programs
```rust
// Launch application via compositor
socket.send(Request::Action(Action::Spawn {
    command: vec!["alacritty".to_string()],
}))?;
```

---

## 5. Streaming Live Events (`EventStream`)

To keep status bars and workspace switchers completely reactive without polling, subscribe to `EventStream`:

```rust
use conchy_ipc::socket::Socket;
use conchy_ipc::{Event, Request, Response};

fn main() -> std::io::Result<()> {
    let mut socket = Socket::connect()?;

    let reply = socket.send(Request::EventStream)?;
    if matches!(reply, Ok(Response::Handled)) {
        let mut read_event = socket.read_events();

        while let Ok(event) = read_event() {
            match event {
                Event::WorkspacesChanged { workspaces } => {
                    // Called whenever workspaces are created, removed, or reordered
                }
                Event::WorkspaceActivated { id, focused } => {
                    // Called when active workspace switches
                }
                Event::WorkspaceUrgencyChanged { id, urgent } => {
                    // Called when an app requests attention
                }
                Event::WindowOpenedOrChanged { window } => {
                    // New window opened or title changed
                }
                Event::WindowClosed { id } => {
                    // Window closed
                }
                Event::WindowFocusChanged { id } => {
                    // Focus moved to another window
                }
                Event::OutputsChanged { outputs } => {
                    // Display monitor plugged/unplugged
                }
                _ => {}
            }
        }
    }
    Ok(())
}
```

> **Note on Initial State:** `EventStream` automatically emits complete initial state updates (e.g. `WorkspacesChanged`, `WindowsChanged`, `OutputsChanged`) immediately upon subscription, so you never miss current desktop state.
