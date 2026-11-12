# Conchy Shell Configuration Specification

`conchy-shell` is configured in **KDL**, like the rest of Conchy OS, through
`conchy-config`. Three files live in `~/.config/conchy/`:

| File | Written by | What it holds |
|---|---|---|
| `shell.kdl` | you, and the settings editor (one node at a time) | everything in the settings window |
| `state.kdl` | the shell | dock pins and service modes set from the UI; each overrides its `shell.kdl` counterpart |
| `theme.kdl` | the theme picker, and you | colors and radius scale |

Anything left out takes its default. `conchy-shell --validate` parses all three
and says where one gave up.

## 1. How a setting is spelled

```kdl
// Comments are kept: the settings editor edits only the node it changes.
watch_config true

bar {                                  // a section is a node with a block
    position "top"                     // a value is one argument
    size 32.0                          // 32.0 is a float, 32 an integer
    start "LauncherBtn" "Workspaces"   // two or more arguments make a list
    (list)center "ActiveWindow"        // a list of one (or none: `(list)center`)
    modules {
        clock format_horizontal="%H:%M"    // properties are a one-line section
        tray                               // a bare node is a section at its defaults
    }
}

launcher {
    modules {
        web {
            providers {                // a list of sections: one `-` node per item
                - shebang="g" name="Google" url="https://www.google.com/search?q={query}"
                - {
                    shebang "w"
                    name "Wikipedia"
                }
            }
        }
    }
}
```

Rules:

- A key can be set once in a section.
- `null` is the same as leaving the key out.
- A list of lists uses `-` items: `- "Systray" "KeyboardLayout"` is a group.
- A file that does not parse is not applied and not overwritten: the shell keeps
  the last good settings and says why in a notification.

Every key, its type and its default are in the settings window (`;cfg`, or the
Settings button in the control center); `conchy-config list shell` prints the
current values.

## 2. Editing

- **The settings window** applies a change at once and writes it to `shell.kdl`
  about 300 ms later (a slider drag is written once). There is no Save. Each
  change rewrites only its own node, so your comments and layout stay.
- **A text editor**: saving the file reloads the shell. The `watch_config`
  switch turns that off.
- **The CLI**: `conchy-config set shell bar.size 36` edits the file, the daemon
  announces it over D-Bus, the shell reloads.

```bash
conchy-config get shell bar.size
conchy-config set shell bar.position bottom
conchy-config list shell
conchy-config watch shell
```

The CLI addresses single values by dotted path. Lists and sections are easier
to change in the editor.

## 3. Dynamic D-Bus API (`org.conchy.Config`)

- **Service:** `org.conchy.Config`
- **Object Path:** `/org/conchy/Config`

Methods: `Get(channel, key)`, `Set(channel, key, value)`, `List(channel)`.
Signal: `PropertyChanged(channel, key, value)`. The shell reads `shell.kdl`
again on any `PropertyChanged` for the `shell` channel.

## 4. Coming from TOML

The first start after the upgrade carries `shell.toml` / `config.toml` (and
`theme.toml`, `state.toml`) over to KDL: the old file is renamed `*.toml.migrated`
and a template `shell.kdl` that is replaced is kept as `shell.kdl.bak`. Comments
in the old file are not carried.
