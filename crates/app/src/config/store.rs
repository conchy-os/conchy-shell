//! Key-path edits to `config.toml`, written through `toml_edit` so only the
//! edited keys change and the hand-written rest of the file survives.

use std::fs;
use std::sync::{Mutex, MutexGuard, PoisonError};

use anyhow::{Context, anyhow};
use gpui::{App, AppContext};
use services::LocalSlot;
use toml_edit::{DocumentMut, Item, Table, TableLike, Value};

use super::{Config, ConfigSignals, one_line, persistence};
use crate::state::AppState;

#[derive(Debug, Clone)]
pub enum Edit {
    /// Set the value at the key path, creating intermediate tables.
    Set(Vec<&'static str>, Value),
    /// Drop the key so the default applies again.
    Remove(Vec<&'static str>),
}

impl Edit {
    fn path(&self) -> &[&'static str] {
        match self {
            Self::Set(path, _) | Self::Remove(path) => path,
        }
    }
}

/// An edit applied to the live config but not yet written. The sequence
/// number tells a save which entries it wrote, since a value has no equality.
#[derive(Debug, Clone)]
struct Pending {
    seq: u64,
    edit: Edit,
}

static PENDING: Mutex<Vec<Pending>> = Mutex::new(Vec::new());
static NEXT_SEQ: Mutex<u64> = Mutex::new(0);

fn pending_lock() -> MutexGuard<'static, Vec<Pending>> {
    PENDING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Apply `mutate` to the live config now and remember the key edit until
/// [`save`] or [`discard`].
pub fn apply(cx: &mut App, edit: Edit, mutate: impl FnOnce(&mut Config)) {
    mutate(cx.global_mut::<Config>());
    ConfigSignals::publish(cx);

    let seq = {
        let mut next = NEXT_SEQ.lock().unwrap_or_else(PoisonError::into_inner);
        *next += 1;
        *next
    };
    remember(&mut pending_lock(), Pending { seq, edit });
}

/// One entry per key path: a slider drag leaves one edit, and a Remove after
/// a Set leaves just the Remove.
fn remember(pending: &mut Vec<Pending>, entry: Pending) {
    match pending
        .iter_mut()
        .find(|p| p.edit.path() == entry.edit.path())
    {
        Some(slot) => *slot = entry,
        None => pending.push(entry),
    }
}

/// Number of key edits not yet written. Zero means the file matches the live
/// config.
pub fn pending() -> usize {
    pending_lock().len()
}

pub fn has_pending() -> bool {
    pending() > 0
}

/// Write every pending edit to `config.toml` in one `toml_edit` pass, then
/// forget them. A failed write keeps them pending and says so in a sticky
/// notification; the live config is already what the user asked for.
pub fn save(cx: &mut App) {
    let snapshot = pending_lock().clone();
    if snapshot.is_empty() {
        return;
    }

    cx.spawn(async move |cx| {
        let edits: Vec<Edit> = snapshot.iter().map(|p| p.edit.clone()).collect();
        let result = cx.background_spawn(async move { write(&edits) }).await;

        cx.update(|cx| match result {
            Ok(()) => {
                // Edits applied while the write ran are not in the file yet.
                let written: Vec<u64> = snapshot.iter().map(|p| p.seq).collect();
                pending_lock().retain(|p| !written.contains(&p.seq));
                AppState::notification(cx).clear_local(LocalSlot::ConfigWriteError);
                ConfigSignals::publish(cx);
            }
            Err(err) => {
                tracing::warn!("Failed to write config.toml: {err:#}");
                AppState::notification(cx).post_local(
                    LocalSlot::ConfigWriteError,
                    "config.toml was not written".into(),
                    one_line(&err),
                );
            }
        });
    })
    .detach();
}

/// Forget pending edits and reload the live config from disk.
pub fn discard(cx: &mut App) {
    pending_lock().clear();
    Config::reload(cx);
}

fn write(edits: &[Edit]) -> anyhow::Result<()> {
    let path = persistence::config_path()?;
    let text =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let text = rewrite(&text, edits)?;
    persistence::write_atomic(&path, &text)
}

/// Apply `edits` to the document text. A file that does not parse is refused
/// rather than overwritten, and so is an edit that would make it stop parsing.
fn rewrite(text: &str, edits: &[Edit]) -> anyhow::Result<String> {
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|err| anyhow!("config.toml does not parse, not written: {err}"))?;

    for edit in edits {
        let (last, parents) = edit
            .path()
            .split_last()
            .ok_or_else(|| anyhow!("empty key path"))?;
        let table = descend(doc.as_table_mut(), parents)?;
        match edit {
            Edit::Set(_, value) => {
                let mut value = value.clone();
                // Replacing in place keeps the trailing comment on the old value.
                if let Some(Item::Value(old)) = table.get(last) {
                    *value.decor_mut() = old.decor().clone();
                }
                table.insert(last, Item::Value(value));
            }
            Edit::Remove(_) => {
                table.remove(last);
            }
        }
    }

    let text = doc.to_string();
    toml::from_str::<Config>(&text)
        .map_err(|err| anyhow!("edit would break config.toml: {err}"))?;
    Ok(text)
}

/// Walk `path` from the root, creating implicit tables on the way so a new
/// `bar.modules.clock.x` renders as `[bar.modules.clock]` and never as an empty
/// `[bar.modules]`.
fn descend<'a>(root: &'a mut Table, path: &[&str]) -> anyhow::Result<&'a mut dyn TableLike> {
    let mut table: &mut dyn TableLike = root;
    for (depth, key) in path.iter().enumerate() {
        // Old files spell `launcher.modules` as `views`; serde rejects both at once.
        let key = if depth == 1
            && path[0] == "launcher"
            && *key == "modules"
            && !table.contains_key("modules")
            && table.contains_key("views")
        {
            "views"
        } else {
            key
        };

        let item = table.entry(key).or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        });
        table = item
            .as_table_like_mut()
            .ok_or_else(|| anyhow!("`{}` is not a table", path[..=depth].join(".")))?;
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use toml_edit::Array;

    use super::*;

    const FIXTURE: &str = "\
# My shell config

[bar]
position = \"top\" # keep me

[launcher]
width = 700.0
";

    fn set(path: &[&'static str], value: impl Into<Value>) -> Edit {
        Edit::Set(path.to_vec(), value.into())
    }

    fn remove(path: &[&'static str]) -> Edit {
        Edit::Remove(path.to_vec())
    }

    /// Every line of `before` is still in `after`, in the same order.
    fn keeps_lines_in_order(before: &str, after: &str) -> bool {
        let mut after = after.lines();
        before
            .lines()
            .all(|line| after.by_ref().any(|candidate| candidate == line))
    }

    #[test]
    fn a_deep_key_gets_its_own_header_without_an_empty_parent() {
        let out = rewrite(
            FIXTURE,
            &[set(
                &["bar", "modules", "clock", "format_horizontal"],
                "%H:%M",
            )],
        )
        .expect("rewrite succeeds");

        assert!(out.contains("[bar.modules.clock]\n"), "{out}");
        assert!(out.contains("format_horizontal = \"%H:%M\"\n"), "{out}");
        assert!(!out.lines().any(|l| l.trim() == "[bar.modules]"), "{out}");
        assert!(out.contains("# keep me"), "{out}");
        assert!(keeps_lines_in_order(FIXTURE, &out), "{out}");
    }

    #[test]
    fn replacing_a_value_keeps_its_trailing_comment() {
        let out = rewrite(FIXTURE, &[set(&["bar", "position"], "bottom")]).expect("rewrite");

        assert!(out.contains("position = \"bottom\" # keep me\n"), "{out}");
        assert!(!out.contains("\"top\""), "{out}");
        assert_eq!(out.lines().count(), FIXTURE.lines().count(), "{out}");
    }

    #[test]
    fn nested_arrays_round_trip() {
        let group = Array::from_iter(["Workspaces", "Clock"]);
        let start = Array::from_iter([Value::from("Launcher"), Value::from(group)]);

        let out = rewrite(FIXTURE, &[set(&["bar", "start"], start)]).expect("rewrite");

        assert!(
            out.contains("start = [\"Launcher\", [\"Workspaces\", \"Clock\"]]"),
            "{out}"
        );
        let config: Config = toml::from_str(&out).expect("config parses");
        assert_eq!(config.bar.start.len(), 2);
        assert_eq!(config.bar.start[1].names(), ["Workspaces", "Clock"]);
    }

    #[test]
    fn remove_drops_only_that_line() {
        let out = rewrite(FIXTURE, &[remove(&["bar", "position"])]).expect("rewrite");

        assert!(!out.contains("position"), "{out}");
        assert!(out.contains("[bar]\n"), "{out}");
        assert!(out.contains("width = 700.0\n"), "{out}");
        assert_eq!(out.lines().count(), FIXTURE.lines().count() - 1, "{out}");
    }

    #[test]
    fn removing_an_absent_key_changes_nothing() {
        let out =
            rewrite(FIXTURE, &[remove(&["bar", "modules", "clock", "nope"])]).expect("rewrite");

        assert_eq!(out, FIXTURE);
    }

    #[test]
    fn a_file_that_does_not_parse_is_refused() {
        let err = rewrite("[bar\nposition = ", &[set(&["bar", "position"], "top")])
            .expect_err("broken input");

        assert!(err.to_string().contains("not written"), "{err}");
    }

    #[test]
    fn an_edit_that_breaks_the_config_is_refused() {
        let err = rewrite(FIXTURE, &[set(&["bar", "position"], "sideways")])
            .expect_err("invalid variant");

        assert!(
            err.to_string().contains("edit would break config.toml"),
            "{err}"
        );
        assert!(err.to_string().contains("sideways"), "{err}");
    }

    #[test]
    fn a_non_table_on_the_path_is_refused() {
        let err = rewrite(FIXTURE, &[set(&["bar", "position", "x"], 1)]).expect_err("scalar");

        assert!(
            err.to_string().contains("`bar.position` is not a table"),
            "{err}"
        );
    }

    #[test]
    fn launcher_modules_follow_the_views_alias() {
        let fixture = "[launcher.views.apps]\nprefix = \"@\"\n";

        let out = rewrite(
            fixture,
            &[set(&["launcher", "modules", "apps", "prefix"], "!")],
        )
        .expect("rewrite");

        assert_eq!(out, "[launcher.views.apps]\nprefix = \"!\"\n");
        assert!(!out.contains("modules"), "{out}");
    }

    fn entry(seq: u64, edit: Edit) -> Pending {
        Pending { seq, edit }
    }

    #[test]
    fn pending_edits_coalesce_per_key_path() {
        let mut pending = Vec::new();

        remember(&mut pending, entry(1, set(&["bar", "position"], "top")));
        remember(&mut pending, entry(2, set(&["launcher", "width"], 700)));
        remember(&mut pending, entry(3, set(&["bar", "position"], "bottom")));
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].seq, 3, "same key replaces in place");
        assert!(matches!(&pending[0].edit, Edit::Set(_, v) if v.as_str() == Some("bottom")));

        remember(&mut pending, entry(4, remove(&["bar", "position"])));
        assert_eq!(pending.len(), 2);
        assert!(matches!(pending[0].edit, Edit::Remove(_)));
        assert_eq!(pending[1].seq, 2, "other keys are untouched");
    }
}
