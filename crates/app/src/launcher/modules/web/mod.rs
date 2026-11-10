//! Web search module: `!<shebang> query` searches one provider, else the
//! default one. A custom body: the query card, then the providers.

pub mod config;

use gpui::{Context, IntoElement, Render, Window, div, prelude::*, px, rgba};
use ui::{
    ActiveTheme, Color, Icon, IconName, IconSize, Label, LabelCommon, Radius, Spacing, TextSize,
};

use self::config::{WebConfig, WebProviderConfig};
use crate::launcher::action::{Action, ActionPanel};
use crate::launcher::content::{Content, Custom};
use crate::launcher::page::{LauncherPage, ModuleDescriptor, ModuleMeta};

pub fn descriptor(config: &WebConfig) -> ModuleDescriptor {
    let providers = config.providers.clone();
    ModuleDescriptor::new(
        ModuleMeta {
            id: "web",
            prefix: config.prefix.clone().into(),
            name: "Web Search",
            icon: IconName::Globe,
            description: "Search the web (!g, !yt, !gh, !nix, !ddg)",
            default: false,
            in_help: true,
        },
        move |_| WebSearchModule {
            providers: providers.clone(),
            provider: None,
            search: String::new(),
        },
    )
}

pub struct WebSearchModule {
    providers: Vec<WebProviderConfig>,
    /// Index of the provider the query picked.
    provider: Option<usize>,
    /// The query without its shebang.
    search: String,
}

impl WebSearchModule {
    /// The provider a leading shebang picks, else the default one.
    fn split<'a>(&self, query: &'a str) -> (Option<usize>, &'a str) {
        let query = query.trim();
        for (ix, provider) in self.providers.iter().enumerate() {
            if let Some(rest) = query.strip_prefix(provider.shebang.as_str())
                && (rest.is_empty() || rest.starts_with(' '))
            {
                return (Some(ix), rest.trim());
            }
        }
        (self.default_provider(), query)
    }

    fn default_provider(&self) -> Option<usize> {
        let default = self.providers.iter().position(|p| p.default);
        default.or((!self.providers.is_empty()).then_some(0))
    }

    /// The URL the current search opens.
    fn url(&self) -> Option<String> {
        let provider = self.providers.get(self.provider?)?;
        (!self.search.is_empty())
            .then(|| provider.url.replace("{query}", &url_encode(&self.search)))
    }
}

impl LauncherPage for WebSearchModule {
    fn content(&mut self, query: &str, cx: &mut Context<Self>) -> Content {
        let (provider, search) = self.split(query);
        self.provider = provider;
        self.search = search.to_string();
        let actions = match (self.url(), provider.and_then(|ix| self.providers.get(ix))) {
            (Some(url), Some(provider)) => ActionPanel::new()
                .action(Action::open(url.clone()).title(format!("Search {}", provider.name)))
                .action(Action::copy(url).title("Copy URL")),
            _ => ActionPanel::new(),
        };
        Custom::new(cx.entity()).actions(actions).into()
    }
}

impl Render for WebSearchModule {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let interactive_default = colors.element_background;
        let accent_selection = colors.element_selected;
        let has_query = !self.search.is_empty();
        let text_color = |muted| if has_query { muted } else { Color::Disabled };

        let card = self
            .provider
            .and_then(|ix| self.providers.get(ix))
            .map(|provider| {
                div()
                    .w_full()
                    .p(Spacing::Large.pixels())
                    .bg(colors.surface_background)
                    .rounded(Radius::Medium.pixels())
                    .flex()
                    .flex_col()
                    .gap(Spacing::Medium.pixels())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(Spacing::Medium.pixels())
                                    .child(Icon::new(provider.icon()).size(IconSize::Large))
                                    .child(
                                        Label::new(provider.name.clone()).size(TextSize::Default),
                                    )
                                    .child(
                                        div()
                                            .px(px(6.))
                                            .py(px(2.))
                                            .rounded(Radius::Small.pixels())
                                            .bg(interactive_default)
                                            .child(
                                                Label::new(format!("!{}", provider.shebang))
                                                    .size(TextSize::XSmall)
                                                    .color(Color::Muted),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(Spacing::Medium.pixels())
                                    .px(Spacing::Medium.pixels())
                                    .py(px(4.))
                                    .rounded(Radius::Small.pixels())
                                    .bg(if has_query {
                                        interactive_default
                                    } else {
                                        rgba(0x00000033).into()
                                    })
                                    .child(
                                        Label::new("Search")
                                            .size(TextSize::Small)
                                            .color(text_color(Color::Default)),
                                    )
                                    .child(
                                        div()
                                            .px(px(4.))
                                            .py(px(2.))
                                            .rounded(Radius::Small.pixels())
                                            .bg(rgba(0x00000044))
                                            .child(
                                                Label::new("Enter")
                                                    .size(TextSize::XSmall)
                                                    .color(text_color(Color::Muted)),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .p(Spacing::Medium.pixels())
                            .bg(rgba(0x00000066))
                            .rounded(Radius::Small.pixels())
                            .child(if has_query {
                                Label::new(format!("\"{}\"", self.search)).size(TextSize::Default)
                            } else {
                                Label::new("Type your search query...")
                                    .color(Color::Placeholder)
                                    .size(TextSize::Default)
                            }),
                    )
            });

        let chips = self.providers.iter().enumerate().map(|(ix, p)| {
            div()
                .px(Spacing::Medium.pixels())
                .py(px(4.))
                .rounded(Radius::Small.pixels())
                .bg(if Some(ix) == self.provider {
                    accent_selection
                } else {
                    interactive_default
                })
                .flex()
                .items_center()
                .gap(px(4.))
                .child(Icon::new(p.icon()).size(IconSize::Small))
                .child(
                    Label::new(format!("!{}", p.shebang))
                        .size(TextSize::Small)
                        .color(Color::Muted),
                )
        });

        let default_name = self
            .default_provider()
            .map(|ix| self.providers[ix].name.clone())
            .unwrap_or_default();
        let usage = [
            "• Type !<shebang> <query> to search a specific provider".to_string(),
            "• Example: !g rust programming, !yt music".to_string(),
            format!("• Without a shebang, the default provider ({default_name}) is used"),
        ];

        div()
            .flex()
            .flex_col()
            .gap(Spacing::Large.pixels())
            .p(Spacing::Large.pixels())
            .children(card)
            .child(
                div()
                    .w_full()
                    .pt(Spacing::Medium.pixels())
                    .flex()
                    .flex_col()
                    .gap(Spacing::XSmall.pixels())
                    .child(
                        Label::new("AVAILABLE PROVIDERS")
                            .size(TextSize::XSmall)
                            .color(Color::Disabled),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(Spacing::Medium.pixels())
                            .children(chips),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .pt(Spacing::Large.pixels())
                    .flex()
                    .flex_col()
                    .gap(Spacing::XSmall.pixels())
                    .child(
                        Label::new("USAGE")
                            .size(TextSize::XSmall)
                            .color(Color::Disabled),
                    )
                    .children(
                        usage
                            .map(|line| Label::new(line).size(TextSize::Small).color(Color::Muted)),
                    ),
            )
    }
}

fn url_encode(s: &str) -> String {
    let mut encoded = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => encoded.push(c),
            ' ' => encoded.push_str("%20"),
            _ => {
                let mut buf = [0; 4];
                for byte in c.encode_utf8(&mut buf).bytes() {
                    encoded.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encode_escapes_spaces_and_unicode() {
        assert_eq!(url_encode("rust gpui"), "rust%20gpui");
        assert_eq!(url_encode("é&"), "%C3%A9%26");
    }
}
