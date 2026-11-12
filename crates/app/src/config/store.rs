//! Key-path edits to `shell.kdl`, written one node at a time so only the
//! edited keys change and the hand-written rest of the file survives.
//!
//! An edit is live the moment it is made and on disk a moment later: there is
//! no Save. Edits made in quick succession (a slider drag) are written once.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, anyhow};
use gpui::{App, AppContext};
use services::LocalSlot;
use toml_edit::Value;

use super::{Config, ConfigSignals, codec, one_line, persistence};
use crate::state::AppState;

/// How long an edit waits for the next one before it is written.
const SAVE_DELAY: Duration = Duration::from_millis(300);

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
/// The newest edit's generation: a timer that wakes to find a newer one has
/// been overtaken, and leaves the write to that edit's timer.
static LATEST_EDIT: AtomicU64 = AtomicU64::new(0);

fn pending_lock() -> MutexGuard<'static, Vec<Pending>> {
    PENDING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Apply `mutate` to the live config now, remember the key edit, and write it
/// shortly.
pub fn apply(cx: &mut App, edit: Edit, mutate: impl FnOnce(&mut Config)) {
    mutate(cx.global_mut::<Config>());
    ConfigSignals::publish(cx);

    let seq = {
        let mut next = NEXT_SEQ.lock().unwrap_or_else(PoisonError::into_inner);
        *next += 1;
        *next
    };
    remember(&mut pending_lock(), Pending { seq, edit });
    save_soon(cx);
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

fn save_soon(cx: &mut App) {
    let generation = LATEST_EDIT.fetch_add(1, Ordering::Relaxed) + 1;
    cx.spawn(async move |cx| {
        cx.background_executor().timer(SAVE_DELAY).await;
        if LATEST_EDIT.load(Ordering::Relaxed) == generation {
            cx.update(save);
        }
    })
    .detach();
}

/// Write every pending edit to `shell.kdl` in one pass, then forget them. A
/// failed write keeps them pending and says so in a sticky notification; the
/// live config is already what the user asked for.
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
                tracing::warn!("Failed to write configuration: {err:#}");
                AppState::notification(cx).post_local(
                    LocalSlot::ConfigWriteError,
                    "Configuration was not written".into(),
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
    LATEST_EDIT.fetch_add(1, Ordering::Relaxed);
    Config::reload(cx);
}

fn write(edits: &[Edit]) -> anyhow::Result<()> {
    let path = persistence::config_path()?;
    let text = if path.exists() {
        std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?
    } else {
        String::new()
    };
    let text = rewrite(&text, edits)?;
    persistence::write_atomic(&path, &text)
}

/// Apply `edits` to the document text. A file that does not parse is refused
/// rather than overwritten, and so is an edit that would make it stop parsing.
fn rewrite(text: &str, edits: &[Edit]) -> anyhow::Result<String> {
    let mut doc = persistence::parse_document(text)
        .map_err(|err| anyhow!("Configuration does not parse, not written: {err}"))?;

    for edit in edits {
        match edit {
            Edit::Set(path, value) => codec::set(&mut doc, path, &tree_value(value)?)?,
            Edit::Remove(path) => {
                codec::remove(&mut doc, path);
            }
        }
    }

    let text = doc.to_string();
    persistence::config_from_text(&text)
        .map_err(|err| anyhow!("edit would break configuration: {err:#}"))?;
    Ok(text)
}

/// An editor value as the settings tree spells it.
fn tree_value(value: &Value) -> anyhow::Result<toml::Value> {
    let table: toml::Table = format!("v = {value}")
        .parse()
        .map_err(|err| anyhow!("value does not encode: {err}"))?;
    table
        .into_iter()
        .next()
        .map(|(_, value)| value)
        .ok_or_else(|| anyhow!("value does not encode"))
}

#[cfg(test)]
mod tests {
    use toml_edit::Array;

    use super::*;

    const FIXTURE: &str = "\
// My shell config

bar {
    position \"top\" // keep me
}

launcher {
    width 700.0
}
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
    fn a_deep_key_gets_its_own_node_and_the_rest_stays_put() {
        let out = rewrite(
            FIXTURE,
            &[set(
                &["bar", "modules", "clock", "format_horizontal"],
                "%H:%M",
            )],
        )
        .expect("rewrite succeeds");

        assert!(out.contains("format_horizontal \"%H:%M\"\n"), "{out}");
        assert!(out.contains("// keep me"), "{out}");
        assert!(keeps_lines_in_order(FIXTURE, &out), "{out}");
    }

    #[test]
    fn replacing_a_value_keeps_its_trailing_comment() {
        let out = rewrite(FIXTURE, &[set(&["bar", "position"], "bottom")]).expect("rewrite");

        assert!(out.contains("position \"bottom\" // keep me\n"), "{out}");
        assert!(!out.contains("\"top\""), "{out}");
        assert_eq!(out.lines().count(), FIXTURE.lines().count(), "{out}");
    }

    #[test]
    fn a_float_stays_a_float_and_an_integer_an_integer() {
        let out = rewrite(
            FIXTURE,
            &[
                set(&["launcher", "width"], 640.0),
                set(&["launcher", "height"], 480.5),
            ],
        )
        .expect("rewrite");

        assert!(out.contains("width 640.0\n"), "{out}");
        assert!(out.contains("height 480.5\n"), "{out}");
    }

    #[test]
    fn nested_arrays_round_trip() {
        let group = Array::from_iter(["Workspaces", "Clock"]);
        let start = Array::from_iter([Value::from("Launcher"), Value::from(group)]);

        let out = rewrite(FIXTURE, &[set(&["bar", "start"], start)]).expect("rewrite");

        let config = persistence::config_from_text(&out).expect("config parses");
        assert_eq!(config.bar.start.len(), 2, "{out}");
        assert_eq!(
            config.bar.start[1].names(),
            ["Workspaces", "Clock"],
            "{out}"
        );
    }

    #[test]
    fn a_list_of_scalars_is_written_as_arguments() {
        let list = Array::from_iter(["Clock", "Mpris"]);

        let out = rewrite(FIXTURE, &[set(&["bar", "end"], list)]).expect("rewrite");

        assert!(out.contains("end \"Clock\" \"Mpris\"\n"), "{out}");
    }

    #[test]
    fn remove_drops_only_that_line() {
        let out = rewrite(FIXTURE, &[remove(&["launcher", "width"])]).expect("rewrite");

        assert!(!out.contains("width"), "{out}");
        assert!(out.contains("position \"top\" // keep me"), "{out}");
        assert!(
            !out.contains("launcher"),
            "an emptied table goes too: {out}"
        );
    }

    #[test]
    fn removing_an_absent_key_changes_nothing() {
        let out =
            rewrite(FIXTURE, &[remove(&["bar", "modules", "clock", "nope"])]).expect("rewrite");

        assert_eq!(out, FIXTURE);
    }

    #[test]
    fn a_file_that_does_not_parse_is_refused() {
        let err = rewrite(
            "bar {\n    position \"top",
            &[set(&["bar", "position"], "top")],
        )
        .expect_err("broken input");

        assert!(err.to_string().contains("not written"), "{err}");
    }

    #[test]
    fn an_edit_that_breaks_the_config_is_refused() {
        let err = rewrite(FIXTURE, &[set(&["bar", "position"], "sideways")])
            .expect_err("invalid variant");

        assert!(
            err.to_string().contains("edit would break configuration"),
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
    fn an_empty_file_takes_the_first_edit() {
        let out = rewrite("", &[set(&["bar", "position"], "bottom")]).expect("rewrite");

        assert_eq!(out, "bar {\n    position \"bottom\"\n}\n");
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
