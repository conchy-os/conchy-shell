//! What the mouse and keyboard pages share: the input settings as the
//! compositor has them, and changes on their way to it.

use std::mem::Discriminant;
use std::rc::Rc;

use gpui::{Context, prelude::*};
use services::input::{Change, InputBackend, InputSettings};
use services::xkb::XkbCatalog;

use super::change_queue::{ChangeQueue, Supersedes, kind_of};
use crate::state::AppState;

impl Supersedes for Change {
    type Kind = Discriminant<Change>;
    fn kind(&self) -> Self::Kind {
        kind_of(self)
    }
}

/// What a page needs to know before it can show anything.
pub struct Loaded {
    pub settings: InputSettings,
    pub has_touchpad: bool,
    pub catalog: Rc<XkbCatalog>,
}

pub enum State {
    Loading,
    Ready(Loaded),
    Failed(String),
}

pub struct InputModel {
    pub backend: InputBackend,
    pub state: State,
    /// What went wrong the last time a change was applied.
    pub error: Option<String>,
    queue: ChangeQueue<Change>,
}

impl InputModel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let backend = InputBackend::from_compositor(AppState::compositor(cx).backend());
        let mut model = Self {
            backend,
            state: State::Loading,
            error: None,
            queue: ChangeQueue::default(),
        };
        model.reload(cx);
        model
    }

    pub fn loaded(&self) -> Option<&Loaded> {
        match &self.state {
            State::Ready(loaded) => Some(loaded),
            _ => None,
        }
    }

    /// Read everything again, off the UI thread.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend;
        let task = cx.background_spawn(async move {
            let settings = backend.load().map_err(|err| format!("{err:#}"))?;
            Ok::<_, String>((settings, services::input::has_touchpad(), XkbCatalog::load()))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.state = match result {
                    Ok((settings, has_touchpad, catalog)) => State::Ready(Loaded {
                        settings,
                        has_touchpad,
                        catalog: Rc::new(catalog),
                    }),
                    Err(err) => State::Failed(err),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Show `change` now and apply it a moment later.
    pub fn change(&mut self, change: Change, cx: &mut Context<Self>) {
        if let State::Ready(loaded) = &mut self.state {
            loaded.settings.apply(&change);
        }
        self.error = None;
        cx.notify();
        ChangeQueue::push(
            self,
            cx,
            |model| &mut model.queue,
            change,
            |model, batch| {
                let backend = model.backend;
                Box::new(move || {
                    for change in &batch {
                        backend.apply(change)?;
                    }
                    Ok(())
                })
            },
            |model, result, cx| {
                if let Err(err) = result {
                    tracing::warn!("Failed to apply input settings: {err:#}");
                    model.error = Some(format!("{err:#}"));
                    // Show what the system really has, not what was asked for.
                    model.reload(cx);
                }
                cx.notify();
            },
        );
    }
}
