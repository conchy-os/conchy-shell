use gpui::{App, KeyBinding, actions};

// Shared keyboard actions used across views.
actions!(
    keybinds,
    [
        Cancel,
        Confirm,
        // Runs a launcher item's second action
        SecondaryConfirm,
        // Opens the launcher actions panel
        ToggleActions,
        // Launcher list navigation
        CursorUp,
        CursorDown,
        PageUp,
        PageDown,
        // Editing
        Backspace,
        DeleteWordBack,
        // Cursor movement / selection
        CursorLeft,
        CursorRight,
        WordLeft,
        WordRight,
        SelectAll,
        SelectWordLeft,
        SelectWordRight,
        SelectLeft,
        SelectRight,
    ]
);

pub fn register(cx: &mut App) {
    // Launcher bindings (arrows + safe vim-ish defaults).
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some("Launcher")),
        KeyBinding::new("enter", Confirm, Some("Launcher")),
        KeyBinding::new("shift-enter", SecondaryConfirm, Some("Launcher")),
        KeyBinding::new("tab", ToggleActions, Some("Launcher")),
        KeyBinding::new("up", CursorUp, Some("Launcher")),
        KeyBinding::new("down", CursorDown, Some("Launcher")),
        KeyBinding::new("pageup", PageUp, Some("Launcher")),
        KeyBinding::new("pagedown", PageDown, Some("Launcher")),
        KeyBinding::new("ctrl-k", CursorUp, Some("Launcher")),
        KeyBinding::new("ctrl-p", CursorUp, Some("Launcher")),
        KeyBinding::new("ctrl-j", CursorDown, Some("Launcher")),
        KeyBinding::new("ctrl-n", CursorDown, Some("Launcher")),
        KeyBinding::new("ctrl-u", PageUp, Some("Launcher")),
        KeyBinding::new("ctrl-d", PageDown, Some("Launcher")),
        KeyBinding::new("backspace", Backspace, Some("Launcher")),
        KeyBinding::new("ctrl-backspace", DeleteWordBack, Some("Launcher")),
        KeyBinding::new("left", CursorLeft, Some("Launcher")),
        KeyBinding::new("right", CursorRight, Some("Launcher")),
        KeyBinding::new("ctrl-left", WordLeft, Some("Launcher")),
        KeyBinding::new("ctrl-right", WordRight, Some("Launcher")),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some("Launcher")),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, Some("Launcher")),
        KeyBinding::new("ctrl-b", WordLeft, Some("Launcher")),
        KeyBinding::new("ctrl-w", WordRight, Some("Launcher")),
        KeyBinding::new("ctrl-a", SelectAll, Some("Launcher")),
        KeyBinding::new("shift-left", SelectLeft, Some("Launcher")),
        KeyBinding::new("shift-right", SelectRight, Some("Launcher")),
        KeyBinding::new("ctrl-f", CursorRight, Some("Launcher")),
        KeyBinding::new("ctrl-h", CursorLeft, Some("Launcher")),
        KeyBinding::new("ctrl-l", CursorRight, Some("Launcher")),
    ]);

    cx.bind_keys([KeyBinding::new("escape", Cancel, Some("DockContextMenu"))]);

    // Control Center WiFi password prompt.
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some("WifiPassword")),
        KeyBinding::new("enter", Confirm, Some("WifiPassword")),
        KeyBinding::new("backspace", Backspace, Some("WifiPassword")),
        KeyBinding::new("ctrl-backspace", DeleteWordBack, Some("WifiPassword")),
        KeyBinding::new("left", CursorLeft, Some("WifiPassword")),
        KeyBinding::new("right", CursorRight, Some("WifiPassword")),
        KeyBinding::new("ctrl-left", WordLeft, Some("WifiPassword")),
        KeyBinding::new("ctrl-right", WordRight, Some("WifiPassword")),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some("WifiPassword")),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, Some("WifiPassword")),
        KeyBinding::new("ctrl-b", WordLeft, Some("WifiPassword")),
        KeyBinding::new("ctrl-w", WordRight, Some("WifiPassword")),
        KeyBinding::new("ctrl-a", SelectAll, Some("WifiPassword")),
        KeyBinding::new("shift-left", SelectLeft, Some("WifiPassword")),
        KeyBinding::new("shift-right", SelectRight, Some("WifiPassword")),
        KeyBinding::new("ctrl-f", CursorRight, Some("WifiPassword")),
        KeyBinding::new("ctrl-h", CursorLeft, Some("WifiPassword")),
        KeyBinding::new("ctrl-l", CursorRight, Some("WifiPassword")),
    ]);

    // Control Center VPNX subscription link (Ctrl+V is handled by the view).
    let ctx = Some("VpnxLink");
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, ctx),
        KeyBinding::new("enter", Confirm, ctx),
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("ctrl-backspace", DeleteWordBack, ctx),
        KeyBinding::new("left", CursorLeft, ctx),
        KeyBinding::new("right", CursorRight, ctx),
        KeyBinding::new("ctrl-left", WordLeft, ctx),
        KeyBinding::new("ctrl-right", WordRight, ctx),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, ctx),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, ctx),
        KeyBinding::new("ctrl-a", SelectAll, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
    ]);
}
