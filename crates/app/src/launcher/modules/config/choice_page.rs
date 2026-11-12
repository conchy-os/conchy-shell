//! Pick one value for an enum field: a row per option, the current one tagged.

use gpui::Context;
use ui::Color;

use super::schema::{ConfigPath, Editable, Editor, Field};
use super::section_page::watch_revision;
use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Accessory, Content, Item, List, Section};
use crate::launcher::page::LauncherPage;

#[allow(dead_code)]
pub struct ChoicePage<S: Editable> {
    path: ConfigPath<S>,
    field: Field<S>,
}

#[allow(dead_code)]
impl<S: Editable> ChoicePage<S> {
    pub fn new(path: ConfigPath<S>, field: Field<S>, cx: &mut Context<Self>) -> Self {
        watch_revision(cx);
        Self { path, field }
    }
}

impl<S: Editable> LauncherPage for ChoicePage<S> {
    fn content(&mut self, _query: &str, cx: &mut Context<Self>) -> Content {
        let list = List::new()
            .title(self.field.label)
            .placeholder("Filter options…");
        let Editor::Choice {
            current, options, ..
        } = &self.field.editor
        else {
            return list.into();
        };
        let selected = current(self.path.read(cx));
        let items = options.iter().enumerate().map(|(ix, option)| {
            let path = self.path.clone();
            let field = self.field.clone();

            let mut item = Item::new(option.label, option.label)
                .icon(self.field.editor.icon())
                .actions(
                    ActionPanel::new().action(Action::new("Select", move |_, cx| {
                        field.choose(&path, ix, cx);
                        Outcome::Pop
                    })),
                );
            if ix == selected {
                item = item.accessory(Accessory::tag("current").color(Color::Accent));
            }
            item
        });
        list.section(Section::untitled().items(items)).into()
    }
}
