use conchy_ipc::{Event, Window, Workspace};
use conchy_shell::{BarModel, ShellConfig, ShellState};

#[test]
fn test_shell_config_defaults_and_updates() {
    let mut config = ShellConfig::default();
    assert_eq!(config.bar_position, "top");
    assert_eq!(config.bar_height, 32);
    assert_eq!(config.bar_background, "#1a1b26");

    config.update_key("bar.height", "40");
    assert_eq!(config.bar_height, 40);

    config.update_key("bar.background", "#24283b");
    assert_eq!(config.bar_background, "#24283b");

    config.update_key("bar.position", "bottom");
    assert_eq!(config.bar_position, "bottom");
}

#[test]
fn test_shell_state_workspace_transitions() {
    let mut state = ShellState::new();

    let ws1 = Workspace {
        id: 1,
        idx: 1,
        name: Some("1".to_string()),
        output: Some("eDP-1".to_string()),
        is_active: true,
        is_focused: true,
        is_urgent: false,
        active_window_id: None,
    };
    let ws2 = Workspace {
        id: 2,
        idx: 2,
        name: Some("2".to_string()),
        output: Some("eDP-1".to_string()),
        is_active: false,
        is_focused: false,
        is_urgent: false,
        active_window_id: None,
    };

    state.handle_event(Event::WorkspacesChanged {
        workspaces: vec![ws1, ws2],
    });

    assert_eq!(state.workspaces.len(), 2);
    assert_eq!(state.focused_workspace_id, Some(1));

    // Activate workspace 2
    state.handle_event(Event::WorkspaceActivated {
        id: 2,
        focused: true,
    });
    assert_eq!(state.focused_workspace_id, Some(2));
    assert!(state.workspaces.iter().find(|w| w.id == 2).unwrap().is_focused);
    assert!(!state.workspaces.iter().find(|w| w.id == 1).unwrap().is_focused);
}

#[test]
fn test_shell_state_window_lifecycle() {
    let mut state = ShellState::new();

    let json_window = r#"{
        "id": 100,
        "title": "Alacritty Terminal",
        "app_id": "Alacritty",
        "pid": 1234,
        "workspace_id": 1,
        "is_focused": true,
        "is_floating": false,
        "is_fullscreen": false,
        "is_urgent": false,
        "is_minimized": false,
        "layout": {
            "pos_in_scrolling_layout": [1, 1],
            "tile_size": [800.0, 600.0],
            "window_size": [800, 600],
            "tile_pos_in_workspace_view": [0.0, 0.0],
            "window_offset_in_tile": [0.0, 0.0]
        }
    }"#;

    let window: Window = serde_json::from_str(json_window).expect("Failed to deserialize test window");

    state.handle_event(Event::WindowOpenedOrChanged { window });
    assert_eq!(state.focused_window_id, Some(100));
    assert_eq!(state.active_window_title(), Some("Alacritty Terminal"));
    assert_eq!(state.active_window_app_id(), Some("Alacritty"));

    // Window closed
    state.handle_event(Event::WindowClosed { id: 100 });
    assert_eq!(state.focused_window_id, None);
    assert_eq!(state.active_window_title(), None);
}

#[test]
fn test_bar_model_rendering() {
    let config = ShellConfig::default();
    let mut state = ShellState::new();

    let ws = Workspace {
        id: 1,
        idx: 1,
        name: Some("code".to_string()),
        output: Some("eDP-1".to_string()),
        is_active: true,
        is_focused: true,
        is_urgent: false,
        active_window_id: None,
    };
    state.workspaces = vec![ws];

    let bar = BarModel::new(config, state);
    let preview = bar.render_preview();
    assert!(preview.contains("[code]"));
    assert!(preview.contains("Conchy Desktop"));
}
