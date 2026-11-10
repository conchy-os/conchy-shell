//! Draws a [`Detail`]: its markdown, then its metadata.

use std::process::Command;

use gpui::{AnyElement, App, FontWeight, ObjectFit, SharedString, div, img, prelude::*, px};
use ui::{
    ActiveTheme, Color, Icon, IconSize, Label, LabelCommon, Radius, Spacing, TextSize, image_source,
};

use super::content::{Detail, Metadata};

const METADATA_LABEL_WIDTH: f32 = 110.;
const IMAGE_MAX_HEIGHT: f32 = 220.;

pub(super) fn render_detail(detail: &Detail, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let blocks = parse(&detail.markdown);
    let has_metadata = !detail.metadata.is_empty();
    // A widget alone opens the pane; it only needs a rule when text precedes it.
    let widget_follows_text = !blocks.is_empty() || has_metadata;

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(Spacing::Large.pixels())
        .p(Spacing::XLarge.pixels())
        .when(!blocks.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(Spacing::Medium.pixels())
                    .children(blocks.into_iter().map(|block| render_block(block, cx))),
            )
        })
        .when(has_metadata, |el| {
            el.child(div().w_full().h(px(1.)).bg(colors.border_variant))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(Spacing::Medium.pixels())
                        .children(
                            detail
                                .metadata
                                .iter()
                                .enumerate()
                                .map(|(ix, entry)| render_metadata(ix, entry, cx)),
                        ),
                )
        })
        .when_some(detail.widget.as_ref(), |el, widget| {
            el.when(widget_follows_text, |el| {
                el.child(div().w_full().h(px(1.)).bg(colors.border_variant))
            })
            .child(widget(cx))
        })
        .into_any_element()
}

fn render_metadata(ix: usize, entry: &Metadata, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let row = |title: &SharedString, value: AnyElement| {
        div()
            .flex()
            .items_start()
            .gap(Spacing::Large.pixels())
            .child(
                div().w(px(METADATA_LABEL_WIDTH)).flex_none().child(
                    Label::new(title.clone())
                        .size(TextSize::Small)
                        .color(Color::Muted),
                ),
            )
            .child(div().flex_1().min_w(px(0.)).child(value))
            .into_any_element()
    };

    match entry {
        Metadata::Label {
            title,
            text,
            icon,
            color,
        } => {
            let color = color.unwrap_or(Color::Default);
            row(
                title,
                div()
                    .flex()
                    .items_center()
                    .gap(Spacing::Small.pixels())
                    .children(icon.map(|icon| Icon::new(icon).size(IconSize::XSmall).color(color)))
                    .child(Label::new(text.clone()).size(TextSize::Small).color(color))
                    .into_any_element(),
            )
        }
        Metadata::Tags { title, tags } => row(
            title,
            div()
                .flex()
                .flex_wrap()
                .gap(Spacing::XSmall.pixels())
                .children(tags.iter().map(|(text, color)| {
                    div()
                        .px(px(6.))
                        .py(px(1.))
                        .rounded(Radius::Small.pixels())
                        .bg(colors.element_background)
                        .child(
                            Label::new(text.clone())
                                .size(TextSize::XSmall)
                                .color(*color),
                        )
                }))
                .into_any_element(),
        ),
        Metadata::Link {
            title,
            text,
            target,
        } => {
            let target = target.clone();
            row(
                title,
                div()
                    .id(("detail-link", ix))
                    .cursor_pointer()
                    .text_size(TextSize::Small.rems())
                    .text_color(colors.text_accent)
                    .hover(|s| s.underline())
                    .child(text.clone())
                    .on_click(move |_, _, _| {
                        let mut open = Command::new("xdg-open");
                        open.arg(target.as_ref());
                        if let Err(err) = services::spawn_reaped(open, "xdg-open") {
                            tracing::warn!("Failed to open {target}: {err}");
                        }
                    })
                    .into_any_element(),
            )
        }
        Metadata::Separator => div()
            .w_full()
            .h(px(1.))
            .bg(colors.border_variant)
            .into_any_element(),
    }
}

#[derive(Debug, PartialEq)]
enum Block {
    Heading(u8, String),
    Paragraph(String),
    Bullet(String),
    Quote(String),
    Code(String),
    /// `![alt](path)` on its own line.
    Image(String),
}

fn parse(markdown: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut code: Option<Vec<&str>> = None;

    let flush = |paragraph: &mut Vec<&str>, blocks: &mut Vec<Block>| {
        if !paragraph.is_empty() {
            blocks.push(Block::Paragraph(paragraph.join(" ")));
            paragraph.clear();
        }
    };

    for line in markdown.lines() {
        if let Some(lines) = code.as_mut() {
            if line.trim_start().starts_with("```") {
                blocks.push(Block::Code(lines.join("\n")));
                code = None;
            } else {
                lines.push(line);
            }
            continue;
        }

        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            flush(&mut paragraph, &mut blocks);
            code = Some(Vec::new());
        } else if trimmed.is_empty() {
            flush(&mut paragraph, &mut blocks);
        } else if let Some(path) = image(trimmed) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Image(path.to_string()));
        } else if let Some((level, text)) = heading(trimmed) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Heading(level, text.to_string()));
        } else if let Some(text) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Bullet(text.to_string()));
        } else if let Some(text) = trimmed.strip_prefix('>') {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Quote(text.trim().to_string()));
        } else {
            paragraph.push(trimmed);
        }
    }
    flush(&mut paragraph, &mut blocks);
    // An unclosed fence still shows its lines.
    if let Some(lines) = code {
        blocks.push(Block::Code(lines.join("\n")));
    }
    blocks
}

fn image(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("![")?;
    let (_, rest) = rest.split_once("](")?;
    rest.strip_suffix(')').map(str::trim)
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    let text = line[level..].strip_prefix(' ')?;
    (1..=3)
        .contains(&level)
        .then_some((level as u8, text.trim()))
}

fn render_block(block: Block, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    match block {
        Block::Heading(level, text) => {
            let size = match level {
                1 => TextSize::Large,
                2 => TextSize::Medium,
                _ => TextSize::Default,
            };
            div()
                .text_size(size.rems())
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.text)
                .child(text)
                .into_any_element()
        }
        Block::Paragraph(text) => div()
            .text_size(TextSize::Default.rems())
            .text_color(colors.text)
            .child(text)
            .into_any_element(),
        Block::Bullet(text) => div()
            .flex()
            .gap(Spacing::Medium.pixels())
            .text_size(TextSize::Default.rems())
            .child(div().text_color(colors.text_muted).child("•"))
            .child(div().flex_1().text_color(colors.text).child(text))
            .into_any_element(),
        Block::Quote(text) => div()
            .pl(Spacing::Medium.pixels())
            .border_l_2()
            .border_color(colors.border)
            .text_size(TextSize::Default.rems())
            .text_color(colors.text_muted)
            .child(text)
            .into_any_element(),
        Block::Image(path) => div()
            .w_full()
            .flex()
            .justify_center()
            .child(
                img(image_source(&path))
                    .max_w_full()
                    .max_h(px(IMAGE_MAX_HEIGHT))
                    .object_fit(ObjectFit::Contain),
            )
            .into_any_element(),
        Block::Code(text) => div()
            .w_full()
            .min_w(px(0.))
            .overflow_hidden()
            .p(Spacing::Medium.pixels())
            .rounded(Radius::Small.pixels())
            .bg(colors.surface_background)
            .font_family("monospace")
            .text_size(TextSize::Small.rems())
            .text_color(colors.text)
            .child(text)
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_are_split_by_kind() {
        let md = "# Title\nfirst line\nsecond line\n\n- one\n> quoted\n```\nlet x = 1;\n```";
        assert_eq!(
            parse(md),
            [
                Block::Heading(1, "Title".into()),
                Block::Paragraph("first line second line".into()),
                Block::Bullet("one".into()),
                Block::Quote("quoted".into()),
                Block::Code("let x = 1;".into()),
            ]
        );
    }

    #[test]
    fn an_image_line_is_an_image() {
        assert_eq!(parse("![cover](~/a.png)"), [Block::Image("~/a.png".into())]);
    }

    #[test]
    fn a_hash_without_a_space_is_text() {
        assert_eq!(parse("#tag"), [Block::Paragraph("#tag".into())]);
    }
}
