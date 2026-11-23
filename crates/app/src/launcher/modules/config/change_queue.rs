//! Settings changes that reach the system through slow steps (a process, a
//! file) while the user is still dragging a slider.
//!
//! A change shows in the page at once. It is applied a moment later, on a
//! background thread, and only one batch runs at a time: two batches editing
//! the same file would each read it before the other wrote.

use std::mem::Discriminant;
use std::time::Duration;

use gpui::{Context, prelude::*};

/// How long to wait for the next change before applying what has come.
const SETTLE: Duration = Duration::from_millis(180);

/// A change that replaces an earlier, not yet applied one of the same kind:
/// of ten slider positions only the last matters.
pub trait Supersedes {
    type Kind: PartialEq;
    fn kind(&self) -> Self::Kind;
}

/// A change enum whose variants are its kinds.
pub fn kind_of<T>(value: &T) -> Discriminant<T> {
    std::mem::discriminant(value)
}

/// The work of one batch, ready to run off the UI thread.
pub type Job = Box<dyn FnOnce() -> anyhow::Result<()> + Send>;
/// Makes the [`Job`] for a batch of changes from the view's current state.
pub type MakeJob<V, C> = fn(&V, Vec<C>) -> Job;
/// Hears how a batch went.
pub type Finished<V> = fn(&mut V, anyhow::Result<()>, &mut Context<V>);

pub struct ChangeQueue<C> {
    pending: Vec<C>,
    scheduled: bool,
    running: bool,
}

impl<C> Default for ChangeQueue<C> {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            scheduled: false,
            running: false,
        }
    }
}

impl<C: Supersedes + Clone + Send + 'static> ChangeQueue<C> {
    /// Queue `change` on the queue `queue` finds in the view; `apply` makes
    /// the job a batch runs, off the UI thread, and `finished` hears how it went.
    pub fn push<V: 'static>(
        view: &mut V,
        cx: &mut Context<V>,
        queue: fn(&mut V) -> &mut Self,
        change: C,
        apply: MakeJob<V, C>,
        finished: Finished<V>,
    ) {
        let kind = change.kind();
        let q = queue(view);
        q.pending.retain(|c| c.kind() != kind);
        q.pending.push(change);
        if !std::mem::replace(&mut q.scheduled, true) {
            Self::settle(cx, queue, apply, finished);
        }
    }

    /// Wait for the changes to stop coming, then start a batch.
    fn settle<V: 'static>(
        cx: &mut Context<V>,
        queue: fn(&mut V) -> &mut Self,
        apply: MakeJob<V, C>,
        finished: Finished<V>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update(cx, |view, cx| {
                queue(view).scheduled = false;
                Self::run(view, cx, queue, apply, finished);
            })
            .ok();
        })
        .detach();
    }

    fn run<V: 'static>(
        view: &mut V,
        cx: &mut Context<V>,
        queue: fn(&mut V) -> &mut Self,
        apply: MakeJob<V, C>,
        finished: Finished<V>,
    ) {
        let q = queue(view);
        if q.running || q.pending.is_empty() {
            return;
        }
        q.running = true;
        let batch = std::mem::take(&mut q.pending);
        let job = apply(view, batch);
        let task = cx.background_spawn(async move { job() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |view, cx| {
                queue(view).running = false;
                finished(view, result, cx);
                // What came while the batch ran.
                Self::run(view, cx, queue, apply, finished);
            })
            .ok();
        })
        .detach();
    }
}
