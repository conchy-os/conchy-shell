# Conchy Shell Configuration Specification

`conchy-shell` is configured via **KDL** at `~/.config/conchy/shell.kdl`.
It is supervised and synchronized in real time by the `conchy-config daemon` (`org.conchy.Config`).

---

## 1. File Format (`shell.kdl`)

```kdl
bar {
    position "top"        // "top" or "bottom"
    height 32             // Height in physical pixels
    background "#1a1b26"  // Hex color or RGBA
    foreground "#c0caf5"  // Text color
    accent "#7aa2f7"      // Highlight / focused workspace color
    urgent "#f7768e"      // Urgent workspace / notification alert color
}

font {
    family "JetBrains Mono"
    size 11.0
}

clock {
    format "%Y-%m-%d %H:%M"
}

widgets {
    workspaces true
    window-title true
    cpu true
    memory true
    battery true
    tray true
}
```

---

## 2. Dynamic D-Bus API (`org.conchy.Config`)

All settings can be read or mutated via D-Bus without touching files manually:

- **Service:** `org.conchy.Config`
- **Object Path:** `/org/conchy/Config`
- **Interface:** `org.conchy.Config`

### Methods:
- `Get(channel: String, key: String) -> String`
- `Set(channel: String, key: String, value: String) -> ()`
- `List(channel: String) -> Vec<(String, String)>`

### Signals:
- `PropertyChanged(channel: String, key: String, value: String)`

---

## 3. CLI Commands

You can interact with shell settings from bash, scripts, or GUI settings tools:

```bash
# Read current bar height
conchy-config get shell bar.height

# Change bar background color live
conchy-config set shell bar.background "#24283b"

# Change bar height live
conchy-config set shell bar.height 36

# List all shell properties
conchy-config list shell

# Watch live updates
conchy-config watch
```

---

## 4. Bidirectional Sync with Text Editors

If a user edits `~/.config/conchy/shell.kdl` directly using Vim, Neovim, or VSCode:
1. The `conchy-config daemon` detects the file modification via inotify.
2. The daemon reparses the KDL AST (preserving all comments and whitespace).
3. The daemon broadcasts `PropertyChanged("shell", key, value)` over D-Bus.
4. `conchy-shell` receives the signal and updates the UI instantly—no restart needed!
