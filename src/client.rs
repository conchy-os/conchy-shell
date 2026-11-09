use std::collections::HashMap;
use anyhow::{Context, Result};
use conchy_ipc::socket::Socket;
use conchy_ipc::{Action, Event, Output, Request, Response, Window, Workspace, WorkspaceReferenceArg};
use tokio::sync::mpsc;
use tracing::{error, info};

/// High-level client for communicating with Conchy WM compositor over IPC.
#[derive(Clone, Default)]
pub struct ConchyClient;

impl ConchyClient {
    pub fn new() -> Self {
        Self
    }

    /// Connect and send a single request to Conchy WM, returning the response.
    pub fn send_request(&self, request: Request) -> Result<Response> {
        let mut socket = Socket::connect()
            .context("Failed to connect to Conchy WM socket (is CONCHY_SOCKET set?)")?;
        let reply = socket
            .send(request)
            .context("Failed to send request to Conchy WM")?;
        reply.map_err(|e| anyhow::anyhow!("Conchy WM returned error: {e}"))
    }

    /// Retrieve the current list of active workspaces.
    pub fn get_workspaces(&self) -> Result<Vec<Workspace>> {
        match self.send_request(Request::Workspaces)? {
            Response::Workspaces(ws) => Ok(ws),
            other => anyhow::bail!("Unexpected response for Workspaces: {:?}", other),
        }
    }

    /// Retrieve the current list of open windows.
    pub fn get_windows(&self) -> Result<Vec<Window>> {
        match self.send_request(Request::Windows)? {
            Response::Windows(wins) => Ok(wins),
            other => anyhow::bail!("Unexpected response for Windows: {:?}", other),
        }
    }

    /// Retrieve connected display outputs.
    pub fn get_outputs(&self) -> Result<HashMap<String, Output>> {
        match self.send_request(Request::Outputs)? {
            Response::Outputs(outs) => Ok(outs),
            other => anyhow::bail!("Unexpected response for Outputs: {:?}", other),
        }
    }

    /// Focus a workspace by index (1-based) or by name.
    pub fn focus_workspace(&self, target: &str) -> Result<()> {
        let reference = if let Ok(idx) = target.parse::<u8>() {
            WorkspaceReferenceArg::Index(idx)
        } else {
            WorkspaceReferenceArg::Name(target.to_string())
        };

        self.send_request(Request::Action(Action::FocusWorkspace { reference }))?;
        Ok(())
    }

    /// Focus a specific window by its unique ID.
    pub fn focus_window(&self, id: u64) -> Result<()> {
        self.send_request(Request::Action(Action::FocusWindow { id }))?;
        Ok(())
    }

    /// Close the currently active window.
    pub fn close_window(&self) -> Result<()> {
        self.send_request(Request::Action(Action::CloseWindow { id: None }))?;
        Ok(())
    }

    /// Spawn a program through the compositor.
    pub fn spawn(&self, command: Vec<String>) -> Result<()> {
        self.send_request(Request::Action(Action::Spawn { command }))?;
        Ok(())
    }

    /// Toggle the window overview mode.
    pub fn toggle_overview(&self) -> Result<()> {
        self.send_request(Request::Action(Action::ToggleOverview {}))?;
        Ok(())
    }

    /// Start listening to live events from the compositor.
    ///
    /// Spawns a dedicated background task and returns an unbounded MPSC receiver.
    pub fn subscribe_events(&self) -> Result<mpsc::UnboundedReceiver<Event>> {
        let mut socket = Socket::connect()
            .context("Failed to connect to Conchy WM socket for event stream")?;

        let reply = socket
            .send(Request::EventStream)
            .context("Failed to initiate EventStream request")?;

        if !matches!(reply, Ok(Response::Handled)) {
            anyhow::bail!("Compositor rejected EventStream subscription: {:?}", reply);
        }

        let (tx, rx) = mpsc::unbounded_channel();

        // Socket::read_events() is blocking, so run it on Tokio's blocking thread pool.
        tokio::task::spawn_blocking(move || {
            info!("Conchy IPC EventStream reader thread started");
            let mut reader = socket.read_events();
            loop {
                match reader() {
                    Ok(event) => {
                        if tx.send(event).is_err() {
                            info!("Event channel closed by consumer; exiting reader thread");
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error reading from Conchy IPC stream: {e}");
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }
}
