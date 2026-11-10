//! The launcher's default rows, cells and section headers, and icon drawing
//! shared with pages that draw their own.

use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use gpui::{
    AnyElement, App, Div, EntityId, FontWeight, ObjectFit, Pixels, RenderImage, SharedString, Size,
    Stateful, Window, div, img, prelude::*, px, size,
};
use ui::{
    ActiveTheme, Color, HighlightedLabel, Icon, IconSize, Label, LabelCommon, ListItem,
    ListItemSpacing, Radius, Spacing, TextSize, ThumbnailCache, ThumbnailKey, Toggleable,
    inset_radius, thumbnail,
};

use ui::patterns::card_radius;

use super::action::Action;
use super::content::{Accessory, AccessoryKind, Dropdown, EmptyView, Item, ItemIcon, Section};

const ICON_SLOT: Pixels = px(28.);

/// The thumbnail of `path` sized for `slot`, or `None` while it loads.
/// `owner` is notified when it lands.
pub fn thumbnail_image(
    path: &Path,
    mtime: Option<SystemTime>,
    slot: Size<Pixels>,
    owner: EntityId,
    window: &Window,
    cx: &App,
) -> Option<Arc<RenderImage>> {
    let cache = ThumbnailCache::global(cx)?;
    let target = thumbnail::target_size(slot, window.scale_factor());
    let key = ThumbnailKey::new(path.to_path_buf(), mtime, target);
    match cache.get(&key, owner, cx) {
        ui::Thumbnail::Ready(image) => Some(image),
        ui::Thumbnail::Loading | ui::Thumbnail::Failed => None,
    }
}

/// `icon` filling `slot`; `None` for [`ItemIcon::None`].
fn icon_content(
    icon: &ItemIcon,
    slot: Size<Pixels>,
    icon_size: IconSize,
    owner: EntityId,
    window: &Window,
    cx: &App,
) -> Option<AnyElement> {
    let slot_bg = cx.theme().colors.element_background;
    let fill = || div().size_full().bg(slot_bg);
    Some(match icon {
        ItemIcon::None => return None,
        ItemIcon::Icon(source) => fill()
            .flex()
            .items_center()
            .justify_center()
            .child(Icon::new(source.clone()).size(icon_size))
            .into_any_element(),
        ItemIcon::Path(path) => img(path.clone())
            .size_full()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        ItemIcon::Thumbnail { path, mtime } => fill()
            .children(
                thumbnail_image(path, *mtime, slot, owner, window, cx)
                    .map(|image| img(image).size_full().object_fit(ObjectFit::Cover)),
            )
            .into_any_element(),
        ItemIcon::Color(color) => div().size_full().bg(*color).into_any_element(),
    })
}

pub(super) fn list_row(
    item: &Item,
    highlights: &[usize],
    selected: bool,
    compact: bool,
    owner: EntityId,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let icon = icon_content(
        &item.icon,
        size(ICON_SLOT, ICON_SLOT),
        IconSize::Small,
        owner,
        window,
        cx,
    );

    ListItem::new(item.id.clone())
        .spacing(ListItemSpacing::Sparse)
        .toggle_state(selected)
        .when_some(icon, |row, icon| {
            row.start_slot(
                div()
                    .size(ICON_SLOT)
                    .flex_none()
                    .rounded(card_radius())
                    .overflow_hidden()
                    .child(icon),
            )
        })
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(1.))
                .min_w(px(0.))
                .child(
                    HighlightedLabel::new(item.title.clone(), highlights.to_vec())
                        .size(TextSize::Default),
                )
                .when_some(item.subtitle.clone(), |el, subtitle| {
                    el.child(
                        Label::new(subtitle)
                            .size(TextSize::Small)
                            .color(Color::Muted)
                            .truncate(),
                    )
                }),
        )
        .when(!compact && !item.accessories.is_empty(), |row| {
            row.end_slot(
                div()
                    .flex()
                    .items_center()
                    .gap(Spacing::Medium.pixels())
                    .children(item.accessories.iter().map(|a| accessory(a, cx))),
            )
        })
        .into_any_element()
}

fn accessory(accessory: &Accessory, cx: &App) -> AnyElement {
    let color = accessory.color.unwrap_or(Color::Muted);
    let icon = accessory
        .icon
        .map(|icon| Icon::new(icon).size(IconSize::XSmall).color(color));
    let el = div().flex().items_center().gap(px(4.)).children(icon);
    match &accessory.kind {
        AccessoryKind::Icon => el,
        AccessoryKind::Text(text) => {
            el.child(Label::new(text.clone()).size(TextSize::Small).color(color))
        }
        AccessoryKind::Tag(text) => el
            .px(px(6.))
            .py(px(1.))
            .rounded(Radius::Small.pixels())
            .bg(cx.theme().colors.element_background)
            .child(Label::new(text.clone()).size(TextSize::XSmall).color(color)),
    }
    .into_any_element()
}

/// A grid card: the item's icon as content, its title underneath.
pub(super) fn grid_cell(
    item: &Item,
    selected: bool,
    content: Size<Pixels>,
    owner: EntityId,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors();
    div()
        .size_full()
        .p(Spacing::XSmall.pixels())
        .rounded(card_radius())
        .border_1()
        .border_color(if selected {
            colors.accent
        } else {
            gpui::transparent_black()
        })
        .when(selected, |el| el.bg(colors.element_selected))
        .when(!selected, |el| el.hover(|s| s.bg(colors.element_hover)))
        .cursor_pointer()
        .flex()
        .flex_col()
        .gap(Spacing::XSmall.pixels())
        .child(
            div()
                .w_full()
                .h(content.height)
                .rounded(inset_radius(card_radius(), Spacing::XSmall.pixels()))
                .overflow_hidden()
                .bg(colors.element_background)
                .children(icon_content(
                    &item.icon,
                    content,
                    IconSize::XLarge,
                    owner,
                    window,
                    cx,
                )),
        )
        .child(
            Label::new(item.title.clone())
                .size(TextSize::Small)
                .truncate(),
        )
        .when_some(item.subtitle.clone(), |el, subtitle| {
            el.child(
                Label::new(subtitle)
                    .size(TextSize::XSmall)
                    .color(Color::Muted)
                    .truncate(),
            )
        })
        .into_any_element()
}

pub(super) fn section_header(section: &Section, first: bool) -> AnyElement {
    div()
        .px(Spacing::Large.pixels())
        .pt(if first {
            Spacing::Small.pixels()
        } else {
            Spacing::Large.pixels()
        })
        .pb(Spacing::XSmall.pixels())
        // As tall as an item row; the title sits on its bottom edge.
        .size_full()
        .flex()
        .items_end()
        .gap(Spacing::Medium.pixels())
        .children(section.title.clone().map(|title| {
            Label::new(SharedString::from(title.to_uppercase()))
                .size(TextSize::XSmall)
                .color(Color::Disabled)
        }))
        .children(section.subtitle.clone().map(|subtitle| {
            Label::new(subtitle)
                .size(TextSize::XSmall)
                .color(Color::Hint)
        }))
        .into_any_element()
}

pub(super) fn empty_view(empty: Option<&EmptyView>) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(Spacing::Small.pixels())
        .px(Spacing::XLarge.pixels())
        .when_some(empty, |el, empty| {
            el.children(empty.icon.map(|icon| {
                Icon::new(icon)
                    .size(IconSize::XLarge)
                    .color(Color::Disabled)
            }))
            .child(
                Label::new(empty.title.clone())
                    .size(TextSize::Small)
                    .color(Color::Muted),
            )
            .children(empty.description.clone().map(|description| {
                Label::new(description)
                    .size(TextSize::XSmall)
                    .color(Color::Disabled)
            }))
        })
        .into_any_element()
}

/// The dropdown's choices as a row of chips; `trailing` is its button, drawn
/// by the launcher so the click can run the action.
pub(super) fn dropdown_chips(
    dropdown: &Dropdown,
    trailing: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors();
    let chips = dropdown.items.iter().enumerate().map(|(ix, item)| {
        let selected = item.value == dropdown.value;
        let on_change = dropdown.on_change.clone();
        let value = item.value.clone();
        div()
            .id(("launcher-dropdown", ix))
            .flex()
            .items_center()
            .gap(Spacing::XSmall.pixels())
            .px(Spacing::Medium.pixels())
            .py(px(3.))
            .rounded(Radius::Full.pixels())
            .border_1()
            .cursor_pointer()
            .text_size(TextSize::XSmall.rems())
            .when(selected, |el| {
                el.bg(colors.element_selected)
                    .border_color(colors.accent)
                    .text_color(colors.text)
            })
            .when(!selected, |el| {
                el.border_color(colors.border_variant)
                    .text_color(colors.text_muted)
                    .hover(|s| s.bg(colors.element_hover).border_color(colors.border))
            })
            .children(item.icon.map(|icon| {
                Icon::new(icon).size(IconSize::XSmall).color(if selected {
                    Color::Accent
                } else {
                    Color::Muted
                })
            }))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(item.title.clone()),
            )
            .children(item.badge.clone().map(|badge| {
                div()
                    .text_color(if selected {
                        colors.text_muted
                    } else {
                        colors.text_disabled
                    })
                    .child(badge)
            }))
            .on_click(move |_, window, cx| on_change(value.clone(), window, cx))
    });

    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(Spacing::XSmall.pixels())
        .px(Spacing::Large.pixels())
        .py(Spacing::Small.pixels())
        .children(chips)
        .children(trailing)
        .into_any_element()
}

/// An action as a chip at the right end of the row.
pub(super) fn dropdown_button(action: &Action, cx: &App) -> Stateful<Div> {
    let colors = cx.theme().colors();
    div()
        .id("launcher-dropdown-button")
        .ml_auto()
        .flex()
        .items_center()
        .gap(Spacing::XSmall.pixels())
        .px(Spacing::Medium.pixels())
        .py(px(3.))
        .rounded(Radius::Full.pixels())
        .border_1()
        .border_color(colors.border_variant)
        .bg(colors.element_background)
        .cursor_pointer()
        .text_size(TextSize::XSmall.rems())
        .text_color(colors.text_accent)
        .font_weight(FontWeight::MEDIUM)
        .hover(|s| s.bg(colors.element_hover).border_color(colors.border))
        .children(
            action
                .icon
                .map(|icon| Icon::new(icon).size(IconSize::XSmall).color(Color::Accent)),
        )
        .child(action.title.clone())
}
