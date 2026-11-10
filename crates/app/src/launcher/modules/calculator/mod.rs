//! Calculator module: a custom body with no items. `= 2 * (3 + 4)` shows the
//! steps and the result as you type; Enter copies it.

pub mod config;
mod eval;

use gpui::{Context, FontWeight, IntoElement, Render, SharedString, Window, div, prelude::*};
use ui::{ActiveTheme, Color, IconName, Label, LabelCommon, Radius, Spacing, TextSize};

use self::config::CalculatorConfig;
use self::eval::{Evaluation, evaluate, format_number};
use crate::launcher::action::{Action, ActionPanel};
use crate::launcher::content::{Content, Custom};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};

pub fn descriptor(config: &CalculatorConfig) -> ModuleDescriptor {
    ModuleDescriptor::new(
        ModuleMeta {
            id: "calculator",
            prefix: config.prefix.clone().into(),
            name: "Calculator",
            icon: IconName::Hash,
            description: "Evaluate arithmetic: + - * / % ^ ( )",
            default: false,
            in_help: true,
        },
        |_| CalculatorModule {
            expression: String::new(),
            evaluation: None,
        },
    )
}

pub struct CalculatorModule {
    expression: String,
    evaluation: Option<Evaluation>,
}

impl CalculatorModule {
    fn formatted(&self) -> Option<String> {
        match self.evaluation.as_ref()?.result {
            Ok(value) => Some(format_number(value)),
            Err(_) => None,
        }
    }
}

impl LauncherPage for CalculatorModule {
    fn content(&mut self, query: &str, cx: &mut Context<Self>) -> Content {
        self.expression = query.trim().to_string();
        self.evaluation = (!self.expression.is_empty()).then(|| evaluate(&self.expression));
        let actions = match self.formatted() {
            Some(value) => ActionPanel::new().action(Action::copy(value).title("Copy Result")),
            None => ActionPanel::new(),
        };
        Custom::new(cx.entity()).actions(actions).into()
    }
}

impl Render for CalculatorModule {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let (result, color): (SharedString, Color) = match &self.evaluation {
            None => (
                "Type an expression, e.g. 2 * (3 + 4)".into(),
                Color::Placeholder,
            ),
            Some(Evaluation {
                result: Ok(value), ..
            }) => (format_number(*value).into(), Color::Default),
            Some(Evaluation {
                result: Err(err), ..
            }) => ((*err).into(), Color::Error),
        };
        let steps = self
            .evaluation
            .iter()
            .flat_map(|eval| &eval.steps)
            .map(|step| {
                Label::new(format!("= {step}"))
                    .size(TextSize::Small)
                    .color(Color::Muted)
            });
        let steps: Vec<_> = steps.collect();

        div().p(Spacing::Large.pixels()).child(
            div()
                .w_full()
                .p(Spacing::XLarge.pixels())
                .rounded(Radius::Medium.pixels())
                .bg(colors.surface_background)
                .flex()
                .flex_col()
                .gap(Spacing::Medium.pixels())
                .child(
                    Label::new(if self.expression.is_empty() {
                        SharedString::from("=")
                    } else {
                        SharedString::from(self.expression.clone())
                    })
                    .size(TextSize::Small)
                    .color(Color::Muted),
                )
                .when(!steps.is_empty(), |el| {
                    el.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(Spacing::XSmall.pixels())
                            .pl(Spacing::Medium.pixels())
                            .border_l_2()
                            .border_color(colors.border)
                            .children(steps),
                    )
                })
                .child(
                    div()
                        .text_size(TextSize::Large.rems())
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color.hsla(colors))
                        .child(result),
                ),
        )
    }
}
