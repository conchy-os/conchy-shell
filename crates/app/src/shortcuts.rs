//! Keyboard shortcuts of the window manager, edited from the settings.
//!
//! The binds belong to `conchy-wm`. It reads `wm.kdl`, and then `shortcuts.kdl`
//! next to it: once that file has a `binds` block, those binds are the whole set
//! (they replace the ones in `wm.kdl`). The shell owns `shortcuts.kdl`. The first
//! edit copies the binds of `wm.kdl` into it, so nothing the user had goes away.
//!
//! A bind is kept as text (`Mod+Q`, `spawn "alacritty"`) and checked for shape
//! only. The compositor has the full list of actions; [`validate_with_wm`] asks
//! it when `conchy-wm` is installed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, anyhow, bail};
use kdl::{KdlDocument, KdlNode, KdlValue};

use crate::config::{config_dir, write_atomic};

/// The key that opens the launcher: a tap of `Mod`. On a real screen `Mod` is Super; in a
/// nested window it is Alt, because the host compositor keeps Super for itself. A literal
/// `Super` tap would never fire there.
pub const SUPER_KEY: &str = "Mod";
/// What this key was called before it became `Mod`. Still understood, and moved to
/// `Mod` on start.
const LEGACY_KEY: &str = "Super";
/// What the Super tap runs.
pub const LAUNCHER_ACTION: &str = r#"spawn "conchy-shell" "toggle-launcher""#;

const WM_FILE: &str = "wm.kdl";
const SHORTCUTS_FILE: &str = "shortcuts.kdl";

const HEADER: &str = "\
// Keyboard shortcuts, managed by conchy-shell (Settings > Shortcuts).
// conchy-wm reads this file after wm.kdl. The binds below are ALL the binds:
// a bind that is not here is not active, even if it is in wm.kdl.
// Comments inside `binds` are not kept when the shell saves.

";

const MODIFIERS: &[&str] = &["mod", "ctrl", "control", "shift", "alt", "super", "win"];

/// One shortcut: a key, the action it runs and the bind properties.
#[derive(Debug, Clone, PartialEq)]
pub struct Bind {
    /// `Mod+Shift+T`, or a lone modifier like `Super` for a tap.
    pub key: String,
    /// The action as KDL: `spawn "alacritty"`, `focus-column-left`.
    pub action: String,
    /// Bind properties other than the key: `repeat=false`, `allow-inhibiting=false`.
    pub props: Vec<(String, Prop)>,
}

/// A property value. Only what binds use.
#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    Bool(bool),
    Int(i64),
    Text(String),
}

impl Prop {
    fn to_kdl(&self) -> KdlValue {
        match self {
            Prop::Bool(b) => KdlValue::Bool(*b),
            Prop::Int(i) => KdlValue::Base10(*i),
            Prop::Text(s) => KdlValue::String(s.clone()),
        }
    }

    fn from_kdl(value: &KdlValue) -> Option<Self> {
        match value {
            KdlValue::Bool(b) => Some(Prop::Bool(*b)),
            KdlValue::Base10(i) | KdlValue::Base2(i) | KdlValue::Base8(i) | KdlValue::Base16(i) => {
                Some(Prop::Int(*i))
            }
            KdlValue::String(s) | KdlValue::RawString(s) => Some(Prop::Text(s.clone())),
            _ => None,
        }
    }
}

impl Bind {
    pub fn new(key: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            action: action.into(),
            props: Vec::new(),
        }
    }

    /// A lone modifier: pressed and released with nothing in between.
    pub fn is_tap(&self) -> bool {
        parse_key(&self.key).is_ok_and(|key| key.tap)
    }

    pub fn prop(&self, name: &str) -> Option<&Prop> {
        self.props.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    pub fn set_prop(&mut self, name: &str, value: Option<Prop>) {
        self.props.retain(|(n, _)| n != name);
        if let Some(value) = value {
            self.props.push((name.to_string(), value));
        }
    }

    /// Whether the bind still fires while a fullscreen window has focus.
    pub fn works_in_fullscreen(&self) -> bool {
        self.prop("allow-when-fullscreen") != Some(&Prop::Bool(false))
    }

    /// Let the bind through fullscreen windows. Off also stops it where an app has
    /// grabbed the keyboard (games, VMs); on lets it through there too.
    pub fn set_works_in_fullscreen(&mut self, on: bool) {
        if on {
            self.set_prop("allow-when-fullscreen", None);
            self.set_prop("allow-inhibiting", Some(Prop::Bool(false)));
        } else {
            self.set_prop("allow-when-fullscreen", Some(Prop::Bool(false)));
            self.set_prop("allow-inhibiting", None);
        }
    }

    /// The Super tap that opens the launcher.
    pub fn launcher_tap(works_in_fullscreen: bool) -> Self {
        let mut bind = Self::new(SUPER_KEY, LAUNCHER_ACTION);
        bind.set_works_in_fullscreen(works_in_fullscreen);
        bind
    }

    fn from_node(node: &KdlNode) -> anyhow::Result<Self> {
        let key = node.name().value().to_string();
        let action = node
            .children()
            .and_then(|children| children.nodes().first())
            .ok_or_else(|| anyhow!("{key} has no action"))?;

        let mut props = Vec::new();
        for entry in node.entries() {
            if let Some(name) = entry.name() {
                let value = Prop::from_kdl(entry.value())
                    .ok_or_else(|| anyhow!("{key}: unsupported value for {}", name.value()))?;
                props.push((name.value().to_string(), value));
            }
        }

        Ok(Self {
            key,
            action: node_text(action),
            props,
        })
    }

    fn to_node(&self) -> anyhow::Result<KdlNode> {
        let mut node = KdlNode::new(self.key.as_str());
        for (name, value) in &self.props {
            node.insert(name.as_str(), value.to_kdl());
        }
        let mut action = parse_action(&self.action)?;
        action.clear_fmt_recursive();
        node.ensure_children().nodes_mut().push(action);
        node.fmt();
        Ok(node)
    }
}

/// One line of KDL for a node, without its terminator.
fn node_text(node: &KdlNode) -> String {
    let mut node = node.clone();
    node.clear_fmt_recursive();
    node.fmt();
    node.to_string()
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_string()
}

fn parse_action(text: &str) -> anyhow::Result<KdlNode> {
    let text = text.trim().trim_end_matches(';').trim();
    if text.is_empty() {
        bail!("Say what the shortcut does");
    }
    let doc: KdlDocument = text
        .parse()
        .map_err(|err: kdl::KdlError| anyhow!("Not a valid action: {}", err.kind))?;
    match doc.nodes() {
        [node] if node.children().is_none_or(|c| c.nodes().is_empty()) => Ok(node.clone()),
        [_] => bail!("An action takes arguments and properties, not a block"),
        _ => bail!("One action per shortcut"),
    }
}

/// A key, taken apart.
#[derive(Debug, PartialEq)]
pub struct ParsedKey {
    pub modifiers: Vec<String>,
    /// `None` for a tap.
    pub key: Option<String>,
    pub tap: bool,
}

/// Checks the shape of a key: `Mod+Shift+T`, `Print`, `Super`. Whether the key
/// name exists is the compositor's call.
pub fn parse_key(text: &str) -> Result<ParsedKey, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Type a key, like Mod+T".into());
    }
    if text.chars().any(char::is_whitespace) {
        return Err("No spaces in a key: Mod+Shift+T".into());
    }

    let is_modifier = |part: &str| MODIFIERS.iter().any(|m| m.eq_ignore_ascii_case(part));
    let parts: Vec<&str> = text.split('+').collect();

    if parts.iter().all(|part| is_modifier(part)) {
        return Ok(ParsedKey {
            modifiers: parts.iter().map(|p| p.to_string()).collect(),
            key: None,
            tap: true,
        });
    }

    let (key, modifiers) = parts.split_last().expect("split gives a part");
    if key.is_empty() {
        return Err("A key has to follow the +".into());
    }
    if let Some(bad) = modifiers.iter().find(|part| !is_modifier(part)) {
        return Err(if bad.is_empty() {
            "An empty part between + signs".into()
        } else {
            format!("{bad:?} is not a modifier (Mod, Ctrl, Shift, Alt, Super)")
        });
    }
    Ok(ParsedKey {
        modifiers: modifiers.iter().map(|p| p.to_string()).collect(),
        key: Some(key.to_string()),
        tap: false,
    })
}

/// Why a bind cannot be saved, if it cannot.
pub fn check(bind: &Bind) -> Result<(), String> {
    parse_key(&bind.key)?;
    parse_action(&bind.action).map_err(|err| format!("{err}"))?;
    Ok(())
}

/// Two keys name the same bind when they differ only in case or modifier order.
pub fn same_key(a: &str, b: &str) -> bool {
    fn canon(key: &str) -> Option<(Vec<String>, String)> {
        let parsed = parse_key(key).ok()?;
        let mut modifiers: Vec<String> = parsed
            .modifiers
            .iter()
            .map(|m| match m.to_ascii_lowercase().as_str() {
                "control" => "ctrl".to_string(),
                "win" => "super".to_string(),
                other => other.to_string(),
            })
            .collect();
        modifiers.sort();
        modifiers.dedup();
        Some((
            modifiers,
            parsed.key.unwrap_or_default().to_ascii_lowercase(),
        ))
    }
    match (canon(a), canon(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a.eq_ignore_ascii_case(b),
    }
}

/// The binds as the compositor will have them, and where they came from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shortcuts {
    pub binds: Vec<Bind>,
    /// They are in `shortcuts.kdl` already, not just in `wm.kdl`.
    pub owned: bool,
}

impl Shortcuts {
    pub fn find(&self, key: &str) -> Option<&Bind> {
        self.binds.iter().find(|bind| same_key(&bind.key, key))
    }

    pub fn tap(&self) -> Option<&Bind> {
        self.binds
            .iter()
            .find(|bind| same_key(&bind.key, SUPER_KEY))
            .or_else(|| {
                self.binds
                    .iter()
                    .find(|bind| same_key(&bind.key, LEGACY_KEY))
            })
    }

    /// Drop the launcher tap, whichever way it is spelled.
    pub fn remove_tap(&mut self) {
        self.remove(SUPER_KEY);
        self.remove(LEGACY_KEY);
    }

    /// Whether `bind` is the launcher tap's key.
    pub fn is_tap_key(key: &str) -> bool {
        same_key(key, SUPER_KEY) || same_key(key, LEGACY_KEY)
    }

    /// Add `bind`, or replace the one on the same key.
    pub fn put(&mut self, bind: Bind) {
        match self.binds.iter_mut().find(|b| same_key(&b.key, &bind.key)) {
            Some(slot) => *slot = bind,
            None => self.binds.push(bind),
        }
    }

    /// Change a bind in place, for a new key or action. `old` names the bind.
    pub fn replace(&mut self, old: &str, bind: Bind) -> Result<(), String> {
        let Some(at) = self.binds.iter().position(|b| same_key(&b.key, old)) else {
            return Err(format!("{old} is not bound"));
        };
        if !same_key(old, &bind.key) && self.find(&bind.key).is_some() {
            return Err(format!("{} already does something else", bind.key));
        }
        self.binds[at] = bind;
        Ok(())
    }

    pub fn remove(&mut self, key: &str) {
        self.binds.retain(|bind| !same_key(&bind.key, key));
    }
}

/// Where the files are. A value, so tests can point it at a temp dir.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn standard() -> anyhow::Result<Self> {
        Ok(Self::new(config_dir()?))
    }

    pub fn wm_path(&self) -> PathBuf {
        self.dir.join(WM_FILE)
    }

    pub fn shortcuts_path(&self) -> PathBuf {
        self.dir.join(SHORTCUTS_FILE)
    }

    /// The binds in effect: `shortcuts.kdl` if it has a `binds` block, else `wm.kdl`.
    pub fn load(&self) -> anyhow::Result<Shortcuts> {
        if let Some(binds) = read_binds(&self.shortcuts_path())? {
            return Ok(Shortcuts { binds, owned: true });
        }
        let binds = read_binds(&self.wm_path())?.unwrap_or_default();
        Ok(Shortcuts {
            binds,
            owned: false,
        })
    }

    /// Write `shortcuts` to `shortcuts.kdl`, keeping the rest of that file.
    pub fn save(&self, shortcuts: &Shortcuts) -> anyhow::Result<()> {
        let path = self.shortcuts_path();
        let (mut doc, header) = match fs::read_to_string(&path) {
            Ok(text) => (
                text.parse::<KdlDocument>().map_err(|err| {
                    anyhow!(
                        "{} is not valid KDL ({}); not overwriting it",
                        path.display(),
                        err.kind
                    )
                })?,
                "",
            ),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (KdlDocument::new(), HEADER),
            Err(err) => {
                return Err(err).with_context(|| format!("Failed to read {}", path.display()));
            }
        };

        let mut binds = KdlNode::new("binds");
        let children = binds.ensure_children();
        for bind in &shortcuts.binds {
            check(bind).map_err(|err| anyhow!("{}: {err}", bind.key))?;
            children.nodes_mut().push(bind.to_node()?);
        }
        binds.fmt();

        let nodes = doc.nodes_mut();
        match nodes.iter().position(|n| n.name().value() == "binds") {
            Some(at) => {
                // Keep the comment above the old block.
                if let Some(leading) = nodes[at].leading() {
                    binds.set_leading(leading);
                }
                nodes[at] = binds;
            }
            None => nodes.push(binds),
        }

        write_atomic(&path, &format!("{header}{doc}"))
    }
}

/// The `binds` of a config file, or `None` when it has none (or is absent).
fn read_binds(path: &Path) -> anyhow::Result<Option<Vec<Bind>>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("Failed to read {}", path.display())),
    };
    let doc: KdlDocument = text
        .parse()
        .map_err(|err: kdl::KdlError| anyhow!("{}: {}", path.display(), err.kind))?;
    let Some(binds) = doc.get("binds") else {
        return Ok(None);
    };
    let Some(children) = binds.children() else {
        return Ok(Some(Vec::new()));
    };
    children
        .nodes()
        .iter()
        .map(Bind::from_node)
        .collect::<anyhow::Result<Vec<_>>>()
        .map(Some)
        .with_context(|| format!("In {}", path.display()))
}

/// Does `wm.kdl` pull in other files? Their binds are not visible here, and they
/// are replaced once `shortcuts.kdl` has binds.
pub fn wm_has_includes(store: &Store) -> bool {
    let Ok(text) = fs::read_to_string(store.wm_path()) else {
        return false;
    };
    // A file this parser cannot read may well have includes; assume so.
    text.parse::<KdlDocument>().map_or(true, |doc| {
        doc.nodes().iter().any(|n| n.name().value() == "include")
    })
}

/// On a first start: give the shortcuts file the Super tap, so that Super opens the
/// launcher out of the box. Does nothing if the user already has a Super tap, a
/// `shortcuts.kdl`, no `wm.kdl`, or a `wm.kdl` with includes (which this could not
/// copy whole).
pub fn ensure_launcher_tap(store: &Store) -> anyhow::Result<bool> {
    if store.shortcuts_path().exists() {
        return migrate_legacy_tap(store);
    }
    if !store.wm_path().exists() || wm_has_includes(store) {
        return Ok(false);
    }
    let mut shortcuts = store.load()?;
    if shortcuts.tap().is_some() {
        return Ok(false);
    }
    shortcuts.put(Bind::launcher_tap(true));
    // An older conchy-wm does not know a tap, and a config it rejects is worse than no
    // Super key.
    validate_with_wm(store, &shortcuts).map_err(|reason| anyhow!("conchy-wm: {reason}"))?;
    store.save(&shortcuts)?;
    Ok(true)
}

/// An earlier version wrote the launcher tap as `Super`, which never fires in a nested
/// window. Write it as `Mod` instead, keeping the bind's other properties.
fn migrate_legacy_tap(store: &Store) -> anyhow::Result<bool> {
    let mut shortcuts = store.load()?;
    if !shortcuts.owned || shortcuts.find(SUPER_KEY).is_some() {
        return Ok(false);
    }
    let Some(slot) = shortcuts.binds.iter_mut().find(|bind| {
        bind.is_tap() && same_key(&bind.key, LEGACY_KEY) && bind.action == LAUNCHER_ACTION
    }) else {
        return Ok(false);
    };
    slot.key = SUPER_KEY.to_string();
    validate_with_wm(store, &shortcuts).map_err(|reason| anyhow!("conchy-wm: {reason}"))?;
    store.save(&shortcuts)?;
    Ok(true)
}

/// Ask `conchy-wm validate` whether the compositor accepts `shortcuts`. `Ok(())`
/// also when `conchy-wm` is not installed; the compositor would say so itself on
/// reload.
pub fn validate_with_wm(store: &Store, shortcuts: &Shortcuts) -> Result<(), String> {
    let Ok(dir) = tempdir() else {
        return Ok(());
    };
    let probe = Store::new(&dir);
    // The real wm.kdl, by include, so relative paths in it behave.
    let include = format!("include {:?}\n", store.wm_path().display().to_string());
    let result = (|| {
        fs::write(probe.wm_path(), include)?;
        probe.save(shortcuts).map_err(std::io::Error::other)?;
        Command::new("conchy-wm")
            .arg("validate")
            .arg("--config")
            .arg(probe.wm_path())
            .output()
    })();
    let _ = fs::remove_dir_all(&dir);

    match result {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stderr);
            let reason = text
                .lines()
                .map(str::trim)
                .find(|line| {
                    !line.is_empty() && !line.starts_with("WARN") && !line.starts_with("INFO")
                })
                .unwrap_or("conchy-wm rejected the shortcuts");
            Err(reason.to_string())
        }
        Err(_) => Ok(()),
    }
}

fn tempdir() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "conchy-shortcuts-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(wm: &str) -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(WM_FILE), wm).unwrap();
        let store = Store::new(dir.path());
        (dir, store)
    }

    const WM: &str = r#"
// layout
layout { gaps 16; }

binds {
    Mod+Return { spawn "alacritty"; }
    Mod+Q repeat=false { close-window; }
    Mod+1 { focus-workspace 1; }
}
"#;

    #[test]
    fn reads_the_binds_of_wm_kdl() {
        let (_dir, store) = store_with(WM);
        let shortcuts = store.load().unwrap();
        assert!(!shortcuts.owned);
        assert_eq!(shortcuts.binds.len(), 3);
        assert_eq!(
            shortcuts.binds[0],
            Bind::new("Mod+Return", r#"spawn "alacritty""#)
        );
        assert_eq!(shortcuts.binds[1].prop("repeat"), Some(&Prop::Bool(false)));
        assert_eq!(shortcuts.binds[2].action, "focus-workspace 1");
    }

    #[test]
    fn save_then_load_round_trips() {
        let (_dir, store) = store_with(WM);
        let mut shortcuts = store.load().unwrap();
        shortcuts.put(Bind::launcher_tap(false));
        shortcuts.remove("Mod+1");
        store.save(&shortcuts).unwrap();

        let loaded = store.load().unwrap();
        assert!(loaded.owned);
        assert_eq!(loaded.binds, shortcuts.binds);
        assert_eq!(loaded.binds.len(), 3);

        let tap = loaded.tap().unwrap();
        assert!(tap.is_tap());
        assert!(!tap.works_in_fullscreen());
        assert_eq!(tap.action, LAUNCHER_ACTION);
        // wm.kdl is never touched.
        assert_eq!(fs::read_to_string(store.wm_path()).unwrap(), WM);
    }

    #[test]
    fn written_file_is_what_the_wm_parses() {
        let (_dir, store) = store_with(WM);
        let mut shortcuts = store.load().unwrap();
        shortcuts.put(Bind::launcher_tap(true));
        store.save(&shortcuts).unwrap();

        let text = fs::read_to_string(store.shortcuts_path()).unwrap();
        assert!(text.contains("Mod+Return"), "{text}");
        assert!(text.contains(r#"Mod allow-inhibiting=false"#), "{text}");
        assert!(
            text.contains(r#"spawn "conchy-shell" "toggle-launcher""#),
            "{text}"
        );
        // KDL v1: bare booleans, as knuffel wants.
        assert!(
            !text.contains("#false") && !text.contains("#true"),
            "{text}"
        );
    }

    #[test]
    fn save_keeps_other_nodes_and_refuses_a_broken_file() {
        let (_dir, store) = store_with(WM);
        fs::write(
            store.shortcuts_path(),
            "// mine\nlayout { gaps 4; }\nbinds {\n    Mod+T { spawn \"foot\"; }\n}\n",
        )
        .unwrap();

        let mut shortcuts = store.load().unwrap();
        assert!(shortcuts.owned);
        assert_eq!(shortcuts.binds.len(), 1);
        shortcuts.put(Bind::new("Mod+E", "toggle-overview"));
        store.save(&shortcuts).unwrap();
        let text = fs::read_to_string(store.shortcuts_path()).unwrap();
        assert!(
            text.contains("// mine") && text.contains("layout"),
            "{text}"
        );
        assert_eq!(store.load().unwrap().binds.len(), 2);

        fs::write(store.shortcuts_path(), "binds {").unwrap();
        assert!(store.save(&shortcuts).is_err());
        assert_eq!(
            fs::read_to_string(store.shortcuts_path()).unwrap(),
            "binds {"
        );
    }

    #[test]
    fn keys() {
        assert_eq!(parse_key("Mod+Shift+T").unwrap().key.as_deref(), Some("T"));
        assert!(parse_key("Super").unwrap().tap);
        assert!(parse_key("Ctrl+Alt").unwrap().tap);
        assert!(!parse_key("Print").unwrap().tap);
        assert!(parse_key("").is_err());
        assert!(parse_key("Mod+").is_err());
        assert!(parse_key("Hyper+T").is_err());
        assert!(parse_key("Mod + T").is_err());

        assert!(same_key("Mod+Shift+T", "shift+mod+t"));
        assert!(same_key("Win", "Super"));
        assert!(!same_key("Mod+T", "Mod+Y"));
    }

    #[test]
    fn actions() {
        assert!(check(&Bind::new("Mod+T", r#"spawn "a" "b""#)).is_ok());
        assert!(check(&Bind::new("Mod+T", "focus-column-left;")).is_ok());
        assert!(check(&Bind::new("Mod+T", "")).is_err());
        assert!(check(&Bind::new("Mod+T", "a; b")).is_err());
        assert!(check(&Bind::new("Mod+T", r#"spawn "unterminated"#)).is_err());
    }

    #[test]
    fn edits() {
        let (_dir, store) = store_with(WM);
        let mut shortcuts = store.load().unwrap();

        // Rebinding to a taken key is refused; to a free one it moves.
        assert!(
            shortcuts
                .replace("Mod+Q", Bind::new("Mod+Return", "close-window"))
                .is_err()
        );
        shortcuts
            .replace("Mod+Q", Bind::new("Mod+W", "close-window"))
            .unwrap();
        assert!(shortcuts.find("Mod+Q").is_none());
        assert!(shortcuts.find("mod+w").is_some());
        assert_eq!(shortcuts.binds[1].key, "Mod+W");
    }

    #[test]
    fn super_tap_is_added_once_on_a_first_start() {
        let (_dir, store) = store_with(WM);
        assert!(ensure_launcher_tap(&store).unwrap());
        let shortcuts = store.load().unwrap();
        assert_eq!(shortcuts.binds.len(), 4);
        assert!(shortcuts.tap().unwrap().works_in_fullscreen());

        // Second start: nothing to do. A user's own choice stays.
        assert!(!ensure_launcher_tap(&store).unwrap());
        let mut shortcuts = store.load().unwrap();
        shortcuts.remove_tap();
        store.save(&shortcuts).unwrap();
        assert!(!ensure_launcher_tap(&store).unwrap());
        assert!(store.load().unwrap().tap().is_none());
    }

    #[test]
    fn a_legacy_super_tap_becomes_mod() {
        let (_dir, store) = store_with(WM);
        let mut shortcuts = store.load().unwrap();
        let mut legacy = Bind::launcher_tap(false);
        legacy.key = "Super".into();
        shortcuts.put(legacy);
        store.save(&shortcuts).unwrap();

        assert!(ensure_launcher_tap(&store).unwrap());
        let text = fs::read_to_string(store.shortcuts_path()).unwrap();
        assert!(text.contains("Mod allow-when-fullscreen=false"), "{text}");
        assert!(!text.contains("Super "), "{text}");
        // Done once.
        assert!(!ensure_launcher_tap(&store).unwrap());
    }

    #[test]
    fn super_tap_is_not_forced_where_it_could_lose_binds() {
        let (_dir, store) =
            store_with("include \"other.kdl\"\nbinds {\n    Mod+T { spawn \"foot\"; }\n}\n");
        assert!(wm_has_includes(&store));
        assert!(!ensure_launcher_tap(&store).unwrap());
        assert!(!store.shortcuts_path().exists());

        let dir = tempfile::tempdir().unwrap();
        assert!(!ensure_launcher_tap(&Store::new(dir.path())).unwrap());

        // A wm.kdl this parser cannot read is left alone too.
        let (_dir, store) = store_with("layout { gaps 16 }\nbinds {}\n");
        assert!(wm_has_includes(&store));
        assert!(!ensure_launcher_tap(&store).unwrap());
    }

    #[test]
    fn fullscreen_switch() {
        let mut tap = Bind::launcher_tap(true);
        assert!(tap.works_in_fullscreen());
        assert_eq!(tap.prop("allow-inhibiting"), Some(&Prop::Bool(false)));
        tap.set_works_in_fullscreen(false);
        assert!(!tap.works_in_fullscreen());
        assert_eq!(tap.prop("allow-inhibiting"), None);
        tap.set_works_in_fullscreen(true);
        assert!(tap.works_in_fullscreen());
    }
}
