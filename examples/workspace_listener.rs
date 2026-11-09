//! Example: Workspace & Window Event Listener for Conchy OS
//!
//! Run with:
//!     cargo run --example workspace_listener
//!
//! This example connects to the running Conchy WM compositor via `CONCHY_SOCKET`
//! and prints every workspace and window focus event in real-time.

use anyhow::Result;
use conchy_ipc::socket::Socket;
use conchy_ipc::{Event, Request, Response};

fn main() -> Result<()> {
    println!("Connecting to Conchy WM via CONCHY_SOCKET...");

    // Connect to the compositor IPC socket
    let mut socket = Socket::connect()?;
    println!("Connected successfully!");

    // Subscribe to continuous event stream
    let reply = socket.send(Request::EventStream)?;
    if !matches!(reply, Ok(Response::Handled)) {
        eprintln!("Compositor rejected EventStream: {:?}", reply);
        return Ok(());
    }

    println!("Listening for live compositor events (Ctrl+C to stop)...");

    // Blocking reader function
    let mut read_event = socket.read_events();

    while let Ok(event) = read_event() {
        match event {
            Event::WorkspaceActivated { id, focused } => {
                println!(">> WorkspaceActivated: id={}, focused={}", id, focused);
            }
            Event::WorkspacesChanged { workspaces } => {
                println!(">> WorkspacesChanged: count={}", workspaces.len());
                for ws in workspaces {
                    let active = if ws.is_active { "*" } else { " " };
                    let focused = if ws.is_focused { "[FOCUSED]" } else { "" };
                    println!(
                        "   {} ws #{} {:?} {}",
                        active,
                        ws.idx,
                        ws.name.unwrap_or_default(),
                        focused
                    );
                }
            }
            Event::WindowFocusChanged { id } => {
                println!(">> WindowFocusChanged: active_window={:?}", id);
            }
            Event::WindowOpenedOrChanged { window } => {
                println!(
                    ">> WindowOpenedOrChanged: id={}, title={:?}, app_id={:?}",
                    window.id, window.title, window.app_id
                );
            }
            Event::WindowClosed { id } => {
                println!(">> WindowClosed: id={}", id);
            }
            _ => {}
        }
    }

    Ok(())
}
