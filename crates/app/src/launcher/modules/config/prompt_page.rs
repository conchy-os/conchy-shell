//! Ask for one line of text: the launcher's search field is the input,
//! prefilled; Enter on the row it offers hands the text over.

use std::rc::Rc;

use gpui::{App, AppContext, Context, SharedString};
use ui::IconName;

use crate::launcher::action::{Action, ActionPanel, Outcome};
use crate::launcher::content::{Content, EmptyView, Item, List};
use crate::launcher::page::LauncherPage;

type Validate = Rc<dyn Fn(&str) -> Result<(), String>>;
type Submit = Rc<dyn Fn(String, &mut App) -> Outcome>;

pub struct PromptPage {
    title: SharedString,
    hint: SharedString,
    verb: SharedString,
    validate: Validate,
    submit: Submit,
}

impl PromptPage {
    /// The outcome that shows the prompt, `initial` already typed.
    pub fn ask(
        cx: &mut App,
        title: impl Into<SharedString>,
        hint: impl Into<SharedString>,
        verb: impl Into<SharedString>,
        initial: impl Into<String>,
        validate: impl Fn(&str) -> Result<(), String> + 'static,
        submit: impl Fn(String, &mut App) -> Outcome + 'static,
    ) -> Outcome {
        let page = cx.new(|_| Self {
            title: title.into(),
            hint: hint.into(),
            verb: verb.into(),
            validate: Rc::new(validate),
            submit: Rc::new(submit),
        });
        Outcome::push_with_input(page, initial)
    }
}

impl LauncherPage for PromptPage {
    fn content(&mut self, query: &str, _cx: &mut Context<Self>) -> Content {
        let text = query.trim().to_string();
        let list = List::new()
            .filtering(false)
            .title(self.title.clone())
            .placeholder(self.hint.clone());
        if text.is_empty() {
            return list
                .empty(EmptyView::new(self.hint.clone()).icon(IconName::Pencil))
                .into();
        }
        match (self.validate)(&text) {
            Err(reason) => list
                .empty(EmptyView::new(reason).icon(IconName::Pencil))
                .into(),
            Ok(()) => {
                let submit = self.submit.clone();
                let value = text.clone();
                list.item(
                    Item::new("submit", format!("{} “{text}”", self.verb))
                        .icon(IconName::Check)
                        .actions(
                            ActionPanel::new()
                                .action(Action::new(self.verb.clone(), move |_, cx| {
                                    submit(value.clone(), cx)
                                })),
                        ),
                )
                .into()
            }
        }
    }
}
