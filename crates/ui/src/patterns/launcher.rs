//! Launcher chrome: the bordered surface that holds a query line, a body, and
//! a footer hint bar.

use gpui::{
    AnyElement, App, Hsla, IntoElement, RenderOnce, SharedString, Window, div, prelude::*, px,
};

use super::surface_radius;
use crate::{ActiveTheme, Color, Icon, IconName, IconSize, Radius, Spacing, Spinner, TextSize};

/// The launcher surface: query row, body, footer hint bar.
#[derive(IntoElement)]
#[must_use = "LauncherFrame does nothing unless rendered"]
pub struct LauncherFrame {
    query: AnyElement,
    icon: IconName,
    loading: bool,
    badge: Option<SharedString>,
    hints: Option<SharedString>,
    actions: Option<AnyElement>,
    background: Option<Hsla>,
    body: Vec<AnyElement>,
}

impl LauncherFrame {
    /// Build a frame around `query` - typically
    /// [`render_input_line`](crate::render_input_line).
    pub fn new(query: impl IntoElement) -> Self {
        Self {
            query: query.into_any_element(),
            icon: IconName::MagnifyingGlass,
            loading: false,
            badge: None,
            hints: None,
            actions: None,
            background: None,
            body: Vec::new(),
        }
    }

    /// Surface fill in place of the theme's, e.g. at a configured opacity.
    pub fn background(mut self, background: Hsla) -> Self {
        self.background = Some(background);
        self
    }

    /// Replace the search icon.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = icon;
        self
    }

    /// Spin in place of the search icon.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Pill on the right of the query row, naming the active view.
    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// Left side of the footer: which prefixes are available.
    pub fn hints(mut self, hints: impl Into<SharedString>) -> Self {
        self.hints = Some(hints.into());
        self
    }

    /// Right side of the footer: what the current view does with a key.
    pub fn actions(mut self, actions: impl IntoElement) -> Self {
        self.actions = Some(actions.into_any_element());
        self
    }
}

impl ParentElement for LauncherFrame {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl RenderOnce for LauncherFrame {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors();
        let (text, muted) = (colors.text, colors.text_muted);
        let background = self.background.unwrap_or_else(|| theme.surface(None));
        let (border, badge_bg) = (colors.border, colors.element_background);
        let hairline = move || div().w_full().h(px(1.)).bg(border);
        let has_footer = self.hints.is_some() || self.actions.is_some();

        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(background)
            .border_1()
            .border_color(border)
            .rounded(surface_radius())
            .text_color(text)
            // Query row
            .child(
                div()
                    .w_full()
                    .px(Spacing::XLarge.pixels())
                    .py(Spacing::Large.pixels())
                    .flex()
                    .items_center()
                    .gap(Spacing::Large.pixels())
                    .child(if self.loading {
                        Spinner::new()
                            .size(IconSize::Medium)
                            .color(Color::Custom(muted))
                            .into_any_element()
                    } else {
                        Icon::new(self.icon)
                            .size(IconSize::Medium)
                            .color(Color::Custom(muted))
                            .into_any_element()
                    })
                    .child(
                        div()
                            .flex_1()
                            .text_size(TextSize::Medium.rems())
                            .child(self.query),
                    )
                    .when_some(self.badge, |el, badge| {
                        el.child(
                            div()
                                .px(Spacing::Medium.pixels())
                                .py(Spacing::XXSmall.pixels())
                                .rounded(Radius::Small.pixels())
                                .bg(badge_bg)
                                .text_size(TextSize::Small.rems())
                                .child(badge),
                        )
                    }),
            )
            .child(hairline())
            // Body. Owns no scroll of its own - which element scrolls is the
            // app's call.
            .child(
                div()
                    .id("launcher-body")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .py(Spacing::XSmall.pixels())
                    .children(self.body),
            )
            .when(has_footer, |el| {
                el.child(hairline()).child(
                    div()
                        .w_full()
                        .px(Spacing::XLarge.pixels())
                        .py(Spacing::Medium.pixels())
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(Spacing::XLarge.pixels())
                        .child(
                            // Hints yield to the actions: clip rather than
                            // push them off the edge.
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(TextSize::XSmall.rems())
                                .text_color(muted)
                                .children(self.hints),
                        )
                        .children(self.actions),
                )
            })
    }
}

/// Render `(action, key)` pairs as the footer's right-hand hints - `Open ⏎ Close Esc`.
pub fn footer_hints<A, K>(actions: impl IntoIterator<Item = (A, K)>, cx: &App) -> AnyElement
where
    A: Into<SharedString>,
    K: Into<SharedString>,
{
    let colors = cx.theme().colors();
    let muted = colors.text_muted;
    let key_bg = colors.element_background;

    div()
        .flex()
        .items_center()
        .gap(Spacing::XLarge.pixels())
        .text_size(TextSize::Small.rems())
        .text_color(muted)
        .children(actions.into_iter().map(|(action, key)| {
            div()
                .flex()
                .items_center()
                .gap(Spacing::Small.pixels())
                .child(action.into())
                .child(
                    div()
                        .px(Spacing::Small.pixels())
                        .py(Spacing::XXSmall.pixels())
                        .rounded(Radius::XSmall.pixels())
                        .bg(key_bg)
                        .text_size(TextSize::XSmall.rems())
                        .child(key.into()),
                )
        }))
        .into_any_element()
}
