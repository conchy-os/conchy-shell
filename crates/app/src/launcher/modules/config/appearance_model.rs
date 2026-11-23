//! Fonts, DPI, cursor and icon theme as programs see them, and changes on
//! their way to every place programs read them from.

use std::mem::Discriminant;
use std::rc::Rc;

use gpui::{Context, prelude::*};
use services::appearance::{
    self, AppearanceSettings, Change, FontFamilies, FontRole, Paths,
};

use super::change_queue::{ChangeQueue, Supersedes, kind_of};
use crate::state::AppState;

impl Supersedes for Change {
    /// A new size for one font replaces an older one, not the other fonts'.
    type Kind = (Discriminant<Change>, Option<FontRole>);

    fn kind(&self) -> Self::Kind {
        let role = match self {
            Change::Font(role, _) => Some(*role),
            _ => None,
        };
        (kind_of(self), role)
    }
}

pub struct Loaded {
    pub settings: AppearanceSettings,
    pub cursor_themes: Vec<String>,
    pub icon_themes: Vec<String>,
    pub fonts: Rc<FontFamilies>,
}

pub enum State {
    Loading,
    Ready(Loaded),
    Failed(String),
}

pub struct AppearanceModel {
    backend: services::CompositorBackend,
    paths: Option<Paths>,
    pub state: State,
    /// Where the last change did not reach, or could not be made.
    pub notice: Option<String>,
    queue: ChangeQueue<Change>,
}

impl AppearanceModel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let backend = AppState::compositor(cx).backend();
        let paths = Paths::standard().ok();
        let mut model = Self {
            backend,
            paths,
            state: State::Loading,
            notice: None,
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

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(paths) = self.paths.clone() else {
            self.state = State::Failed("$HOME is not set".to_string());
            return;
        };
        let task = cx.background_spawn(async move {
            (
                appearance::load(&paths),
                appearance::cursor_themes(),
                appearance::icon_themes(),
                appearance::font_families(),
            )
        });
        cx.spawn(async move |this, cx| {
            let (settings, cursor_themes, icon_themes, fonts) = task.await;
            this.update(cx, |this, cx| {
                this.state = State::Ready(Loaded {
                    settings,
                    cursor_themes,
                    icon_themes,
                    fonts: Rc::new(fonts),
                });
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
        self.notice = None;
        cx.notify();
        ChangeQueue::push(
            self,
            cx,
            |model| &mut model.queue,
            change,
            |model, batch| {
                let paths = model.paths.clone();
                let backend = model.backend;
                Box::new(move || {
                    let paths = paths.ok_or_else(|| anyhow::anyhow!("$HOME is not set"))?;
                    let mut failed = Vec::new();
                    let mut reached = 0;
                    for change in &batch {
                        let report = appearance::apply(&paths, backend, change);
                        reached += report.applied.len();
                        failed.extend(report.failed);
                    }
                    if reached == 0 && !failed.is_empty() {
                        anyhow::bail!("{}", failed.join("; "));
                    }
                    if !failed.is_empty() {
                        tracing::warn!("Some appearance targets failed: {}", failed.join("; "));
                    }
                    Ok(())
                })
            },
            |model, result, cx| {
                if let Err(err) = result {
                    tracing::warn!("Failed to apply appearance settings: {err:#}");
                    model.notice = Some(format!("{err:#}"));
                    model.reload(cx);
                }
                cx.notify();
            },
        );
    }
}
