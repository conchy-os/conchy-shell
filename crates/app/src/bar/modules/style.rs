//! Content helpers for bar widgets. Every size and colour comes from the
//! [`BarCtx`] the bar handed the widget; the chip around the content is the
//! host's business.

use gpui::{AnyElement, Hsla, IntoElement, SharedString, prelude::*};
use ui::{Color, Divider, Icon, IconSource, Label, LabelCommon, h_flex, v_flex};

use super::ctx::BarCtx;

/// The flex container a widget lays its content out in: a column on a
/// vertical bar, a row on a horizontal one, centred either way.
pub fn stack(ctx: &BarCtx) -> gpui::Div {
    if ctx.vertical {
        v_flex().items_center()
    } else {
        h_flex()
    }
}

/// A label in the bar's text size and ink.
pub fn label(ctx: &BarCtx, text: impl Into<SharedString>) -> Label {
    label_in(ctx, text, ctx.ink.text)
}

/// [`label`] in an explicit colour, for a status or muted line.
pub fn label_in(ctx: &BarCtx, text: impl Into<SharedString>, color: Hsla) -> Label {
    Label::new(text)
        .size(ctx.metrics.label)
        .color(Color::Custom(color))
}

/// An icon in the bar's icon size and ink.
pub fn icon(ctx: &BarCtx, icon: impl Into<IconSource>) -> Icon {
    icon_in(ctx, icon, ctx.ink.text)
}

/// [`icon`] in an explicit colour.
pub fn icon_in(ctx: &BarCtx, icon: impl Into<IconSource>, color: Hsla) -> Icon {
    Icon::new(icon)
        .size(ctx.metrics.icon)
        .color(Color::Custom(color))
}

/// A label placed the way the orientation wants: on a vertical bar every
/// line is centred in one fixed-width column so stacked values share an axis.
pub fn value(ctx: &BarCtx, label: Label) -> AnyElement {
    if ctx.vertical {
        text_line(ctx, label)
    } else {
        label.into_any_element()
    }
}

/// An icon/value pair, the shape of most status widgets.
pub fn stat(
    ctx: &BarCtx,
    icon_name: impl Into<IconSource>,
    text: impl Into<SharedString>,
    color: Hsla,
) -> AnyElement {
    stack(ctx)
        .justify_center()
        .gap(ctx.metrics.gap)
        .child(icon_in(ctx, icon_name, color))
        .child(value(ctx, label_in(ctx, text, color)))
        .into_any_element()
}

/// The hairline between a dense widget's sections.
pub fn divider(ctx: &BarCtx) -> Divider {
    Divider::vertical()
        .color(ctx.divider_color())
        .length(ctx.metrics.divider)
}

/// Centre a vertical text row to a fixed axis so stacked labels do not
/// drift; a value wider than the axis overflows it centred, never wraps.
pub fn text_line(ctx: &BarCtx, content: impl IntoElement) -> AnyElement {
    h_flex()
        .w(ctx.metrics.text_line)
        .flex_none()
        .justify_center()
        .whitespace_nowrap()
        .child(content)
        .into_any_element()
}

/// `62%` on a horizontal bar, `62` on a vertical one.
pub fn compact_percent(ctx: &BarCtx, value: u32) -> String {
    if ctx.vertical {
        value.to_string()
    } else {
        format!("{value}%")
    }
}

/// Icon and text colours for a battery readout: neutral unless the charge
/// needs attention; a charging icon shows it in the success colour.
pub fn battery_ink(ctx: &BarCtx, battery: Option<&services::BatteryData>) -> (Hsla, Hsla) {
    let Some(battery) = battery else {
        return (ctx.ink.muted, ctx.ink.muted);
    };
    if battery.is_critical() {
        (ctx.ink.status.error, ctx.ink.status.error)
    } else if battery.is_low() {
        (ctx.ink.status.warning, ctx.ink.status.warning)
    } else if battery.is_charging() {
        (ctx.ink.status.success, ctx.ink.text)
    } else {
        (ctx.ink.text, ctx.ink.text)
    }
}

/// `text` cut to at most `max_chars` characters at a word boundary, with an
/// ellipsis. The cut falls back to mid-word when the last boundary would
/// drop more than half the room.
pub fn fit_words(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if max_chars == 0 || text.chars().count() <= max_chars {
        return text.to_string();
    }
    let room = max_chars.saturating_sub(1);
    let hard: String = text.chars().take(room).collect();
    let cut = match hard.rfind(char::is_whitespace) {
        Some(at) if at >= room / 2 => hard[..at].trim_end(),
        _ => hard.as_str(),
    };
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::fit_words;

    #[test]
    fn fit_words_cuts_between_words() {
        assert_eq!(
            fit_words("Strawberry Fields Forever", 40),
            "Strawberry Fields Forever"
        );
        assert_eq!(
            fit_words("Strawberry Fields Forever", 20),
            "Strawberry Fields…"
        );
        assert_eq!(fit_words("Strawberry Fields Forever", 12), "Strawberry…");
        // One long word: no boundary worth keeping, so cut it.
        assert_eq!(fit_words("Antidisestablishmentarianism", 10), "Antidises…");
        assert_eq!(fit_words("  padded  ", 10), "padded");
        assert_eq!(fit_words("anything", 0), "anything");
    }
}
