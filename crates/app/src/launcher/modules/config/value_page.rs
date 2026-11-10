//! Edit a number or text field: the launcher's search input is the value,
//! prefilled; Enter saves what parses.

use gpui::{AnyElement, Context, Window};
use ui::IconName;

use super::schema::{ConfigPath, Editable, Editor, Field};
use super::section_page::{file_actions, unsaved_banner, watch_revision};
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Content, EmptyView, Item, List};
use crate::launcher::page::LauncherPage;

pub struct ValuePage<S: Editable> {
    path: ConfigPath<S>,
    field: Field<S>,
}

/// A query that parsed: what the row says and what Enter does.
struct Commit {
    title: String,
    subtitle: Option<String>,
    action: Action,
}

impl<S: Editable> ValuePage<S> {
    pub fn new(path: ConfigPath<S>, field: Field<S>, cx: &mut Context<Self>) -> Self {
        watch_revision(cx);
        Self { path, field }
    }

    fn parse(&self, query: &str) -> Result<Commit, String> {
        let query = query.trim();
        let path = self.path.clone();
        let field = self.field.clone();
        match &self.field.editor {
            Editor::Number {
                lens,
                min,
                max,
                integer,
                unit,
                ..
            } => {
                let value: f64 = query
                    .parse()
                    .map_err(|_| format!("{query:?} is not a number"))?;
                if *integer && value.fract() != 0.0 {
                    return Err("Must be a whole number".into());
                }
                if value < *min || value > *max {
                    return Err(format!("Must be between {min} and {max}"));
                }
                let lens = *lens;
                let shown = match unit {
                    Some(unit) => format!("{query} {unit}"),
                    None => query.to_string(),
                };
                Ok(Commit {
                    title: format!("Set to {shown}"),
                    subtitle: None,
                    action: Action::new("Set", move |_, cx| {
                        path.write(&field, |s| (lens.set)(s, value), cx);
                        Outcome::Pop
                    }),
                })
            }
            Editor::Text { lens, hint } => {
                hint.validate(query)?;
                let lens = *lens;
                let text = query.to_string();
                Ok(Commit {
                    title: format!("Set to {query:?}"),
                    subtitle: hint.preview(query),
                    action: Action::new("Set", move |_, cx| {
                        path.write(&field, |s| (lens.set)(s, text.clone()), cx);
                        Outcome::Pop
                    }),
                })
            }
            _ => Err("This field has no text editor".into()),
        }
    }
}

impl<S: Editable> LauncherPage for ValuePage<S> {
    fn content(&mut self, query: &str, cx: &mut Context<Self>) -> Content {
        let current = self.field.display(self.path.read(cx));
        let list = List::new()
            .filtering(false)
            .title(self.field.label)
            .placeholder(format!("Currently {current}"))
            .actions(file_actions(ActionPanel::new()));
        match self.parse(query) {
            Ok(commit) => {
                let mut item = Item::new("set", commit.title).icon(IconName::Check);
                if let Some(subtitle) = commit.subtitle {
                    item = item.subtitle(subtitle);
                }
                list.item(item.actions(file_actions(ActionPanel::new().action(commit.action))))
            }
            Err(reason) => list.empty(
                EmptyView::new(reason)
                    .description(self.field.description)
                    .icon(IconName::Pencil),
            ),
        }
        .into()
    }

    fn render_header(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        unsaved_banner(cx)
    }
}
