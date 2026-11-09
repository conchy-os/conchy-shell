use std::collections::HashMap;
use conchy_ipc::{Event, Output, Window, Workspace};
use tracing::{debug, info};

/// Represents the synchronized live state of Conchy OS desktop.
#[derive(Debug, Clone, Default)]
pub struct ShellState {
    /// Connected outputs mapped by name (e.g., "eDP-1", "DP-1")
    pub outputs: HashMap<String, Output>,
    /// All active workspaces
    pub workspaces: Vec<Workspace>,
    /// Currently focused workspace ID
    pub focused_workspace_id: Option<u64>,
    /// Open windows mapped by window ID
    pub windows: HashMap<u64, Window>,
    /// Currently focused window ID
    pub focused_window_id: Option<u64>,
    /// Whether the overview mode is active
    pub overview_active: bool,
}

impl ShellState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process an incoming compositor event and update local state accordingly.
    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::WorkspacesChanged { workspaces } => {
                debug!("Workspaces updated: {} workspaces", workspaces.len());
                self.workspaces = workspaces;
                // Re-evaluate focused workspace
                self.focused_workspace_id = self
                    .workspaces
                    .iter()
                    .find(|ws| ws.is_focused)
                    .map(|ws| ws.id);
            }
            Event::WorkspaceActivated { id, focused } => {
                debug!("Workspace {} activated (focused: {})", id, focused);
                for ws in &mut self.workspaces {
                    if ws.id == id {
                        ws.is_active = true;
                        if focused {
                            ws.is_focused = true;
                        }
                    } else if focused {
                        ws.is_focused = false;
                    }
                }
                if focused {
                    self.focused_workspace_id = Some(id);
                }
            }
            Event::WorkspaceUrgencyChanged { id, urgent } => {
                debug!("Workspace {} urgency: {}", id, urgent);
                if let Some(ws) = self.workspaces.iter_mut().find(|w| w.id == id) {
                    ws.is_urgent = urgent;
                }
            }
            Event::WindowsChanged { windows } => {
                debug!("Windows updated: {} open windows", windows.len());
                self.windows.clear();
                for win in windows {
                    if win.is_focused {
                        self.focused_window_id = Some(win.id);
                    }
                    self.windows.insert(win.id, win);
                }
            }
            Event::WindowOpenedOrChanged { window } => {
                debug!("Window {} updated: {:?}", window.id, window.title);
                if window.is_focused {
                    self.focused_window_id = Some(window.id);
                }
                self.windows.insert(window.id, window);
            }
            Event::WindowClosed { id } => {
                debug!("Window {} closed", id);
                self.windows.remove(&id);
                if self.focused_window_id == Some(id) {
                    self.focused_window_id = None;
                }
            }
            Event::WindowFocusChanged { id } => {
                debug!("Focused window changed to {:?}", id);
                self.focused_window_id = id;
                for win in self.windows.values_mut() {
                    win.is_focused = Some(win.id) == id;
                }
            }
            Event::OverviewOpenedOrClosed { is_open } => {
                info!("Overview toggled: {}", is_open);
                self.overview_active = is_open;
            }
            other => {
                debug!("Unhandled IPC event: {:?}", other);
            }
        }
    }

    /// Returns the currently active window title, if any.
    pub fn active_window_title(&self) -> Option<&str> {
        self.focused_window_id
            .and_then(|id| self.windows.get(&id))
            .and_then(|w| w.title.as_deref())
    }

    /// Returns the currently active window application ID / class, if any.
    pub fn active_window_app_id(&self) -> Option<&str> {
        self.focused_window_id
            .and_then(|id| self.windows.get(&id))
            .and_then(|w| w.app_id.as_deref())
    }
}
