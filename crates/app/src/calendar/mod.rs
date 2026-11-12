//! Calendar panel module providing a monthly calendar view and daily agenda.
//!
//! Opened by clicking the clock in the bar, or from the launcher's `;cal` module.

use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use gpui::{
    App, Context, DisplayId, FocusHandle, Focusable, FontWeight, IntoElement, MouseButton,
    Pixels, Point, Render, Window, div, prelude::*, px, layer_shell::Anchor,
};
use ui::patterns::PanelSurface;
use ui::{ActiveTheme, Color, Icon, IconName, IconSize, Radius, Spacing, TextSize};

use crate::config::Config;
use crate::keybinds::Cancel;
use crate::windows::{
    PanelConfig, PanelSize, ShellPanel, ShellWindows, panel_placement_from_event,
};

pub const CALENDAR_PANEL_WIDTH: f32 = 580.0;
pub const CALENDAR_PANEL_MAX_HEIGHT: f32 = 450.0;

pub fn russian_weekday_full(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Понедельник",
        Weekday::Tue => "Вторник",
        Weekday::Wed => "Среда",
        Weekday::Thu => "Четверг",
        Weekday::Fri => "Пятница",
        Weekday::Sat => "Суббота",
        Weekday::Sun => "Воскресенье",
    }
}

pub fn russian_month_nominative(month: u32) -> &'static str {
    match month {
        1 => "Январь",
        2 => "Февраль",
        3 => "Март",
        4 => "Апрель",
        5 => "Май",
        6 => "Июнь",
        7 => "Июль",
        8 => "Август",
        9 => "Сентябрь",
        10 => "Октябрь",
        11 => "Ноябрь",
        12 => "Декабрь",
        _ => "",
    }
}

pub fn russian_month_genitive(month: u32) -> &'static str {
    match month {
        1 => "января",
        2 => "февраля",
        3 => "марта",
        4 => "апреля",
        5 => "мая",
        6 => "июня",
        7 => "июля",
        8 => "августа",
        9 => "сентября",
        10 => "октября",
        11 => "ноября",
        12 => "декабря",
        _ => "",
    }
}

pub const RUSSIAN_WEEKDAYS_SHORT: [&str; 7] = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];

pub fn format_subtitle(date: NaiveDate) -> String {
    format!(
        "{}, {:02}.{:02}.{}",
        russian_weekday_full(date.weekday()),
        date.day(),
        date.month(),
        date.year()
    )
}

pub fn format_month_year(date: NaiveDate) -> String {
    format!("{} {}", russian_month_nominative(date.month()), date.year())
}

pub fn format_selected_header(date: NaiveDate) -> String {
    format!(
        "{} {} {}",
        russian_weekday_full(date.weekday()),
        date.day(),
        russian_month_genitive(date.month())
    )
}

pub fn prev_month(date: NaiveDate) -> NaiveDate {
    let (mut year, mut month) = (date.year(), date.month());
    if month == 1 {
        year -= 1;
        month = 12;
    } else {
        month -= 1;
    }
    NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(date)
}

pub fn next_month(date: NaiveDate) -> NaiveDate {
    let (mut year, mut month) = (date.year(), date.month());
    if month == 12 {
        year += 1;
        month = 1;
    } else {
        month += 1;
    }
    NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(date)
}

pub fn month_grid_days(displayed_month: NaiveDate) -> [NaiveDate; 42] {
    let first_day = NaiveDate::from_ymd_opt(displayed_month.year(), displayed_month.month(), 1)
        .unwrap_or(displayed_month);
    let days_before = (first_day.weekday().number_from_monday() - 1) as i64;
    let start_date = first_day - Duration::days(days_before);
    std::array::from_fn(|i| start_date + Duration::days(i as i64))
}

pub struct CalendarPanel {
    today: NaiveDate,
    selected_date: NaiveDate,
    displayed_month: NaiveDate,
    focus_handle: FocusHandle,
}

impl CalendarPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let today = Local::now().date_naive();
        let displayed_month =
            NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
        Self {
            today,
            selected_date: today,
            displayed_month,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn prev_month(&mut self, cx: &mut Context<Self>) {
        self.displayed_month = prev_month(self.displayed_month);
        cx.notify();
    }

    pub fn next_month(&mut self, cx: &mut Context<Self>) {
        self.displayed_month = next_month(self.displayed_month);
        cx.notify();
    }

    pub fn jump_to_today(&mut self, cx: &mut Context<Self>) {
        self.today = Local::now().date_naive();
        self.selected_date = self.today;
        self.displayed_month =
            NaiveDate::from_ymd_opt(self.today.year(), self.today.month(), 1).unwrap_or(self.today);
        cx.notify();
    }

    pub fn select_date(&mut self, date: NaiveDate, cx: &mut Context<Self>) {
        self.selected_date = date;
        if date.month() != self.displayed_month.month() || date.year() != self.displayed_month.year() {
            self.displayed_month =
                NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date);
        }
        cx.notify();
    }
}

impl Focusable for CalendarPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CalendarPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors();

        let header = div()
            .id("calendar-header")
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(Spacing::Small.pixels())
                    .child(
                        Icon::new(IconName::Calendar)
                            .size(IconSize::Small)
                            .color(Color::Accent),
                    )
                    .child(
                        div()
                            .text_size(TextSize::Medium.rems())
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.text)
                            .child("Календарь"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(Spacing::Small.pixels())
                    .child(
                        div()
                            .id("calendar-today-button")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.))
                            .h(px(28.))
                            .rounded(Radius::Small.pixels())
                            .bg(colors.ghost_element_background)
                            .hover(move |s| s.bg(colors.ghost_element_hover))
                            .active(move |s| s.bg(colors.ghost_element_active))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.jump_to_today(cx);
                                }),
                            )
                            .child(
                                Icon::new(IconName::Calendar)
                                    .size(IconSize::XSmall)
                                    .color(Color::Default),
                            ),
                    )
                    .child(
                        div()
                            .id("calendar-close-button")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.))
                            .h(px(28.))
                            .rounded(Radius::Small.pixels())
                            .bg(colors.ghost_element_background)
                            .hover(move |s| s.bg(colors.ghost_element_hover))
                            .active(move |s| s.bg(colors.ghost_element_active))
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                ShellWindows::close_panel(cx);
                            })
                            .child(
                                Icon::new(IconName::Close)
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted),
                            ),
                    ),
            );

        let grid_days = month_grid_days(self.displayed_month);
        let current_month = self.displayed_month.month();
        let current_year = self.displayed_month.year();
        let selected = self.selected_date;
        let today = self.today;

        let nav_bar = div()
            .flex()
            .items_center()
            .justify_between()
            .py(Spacing::XSmall.pixels())
            .child(
                div()
                    .id("calendar-prev-month")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(28.))
                    .h(px(28.))
                    .rounded(Radius::Small.pixels())
                    .hover(move |s| s.bg(colors.ghost_element_hover))
                    .active(move |s| s.bg(colors.ghost_element_active))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.prev_month(cx);
                        }),
                    )
                    .child(
                        Icon::new(IconName::ChevronLeft)
                            .size(IconSize::XSmall)
                            .color(Color::Default),
                    ),
            )
            .child(
                div()
                    .text_size(TextSize::Default.rems())
                    .font_weight(FontWeight::BOLD)
                    .text_color(colors.text)
                    .child(format_month_year(self.displayed_month)),
            )
            .child(
                div()
                    .id("calendar-next-month")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(28.))
                    .h(px(28.))
                    .rounded(Radius::Small.pixels())
                    .hover(move |s| s.bg(colors.ghost_element_hover))
                    .active(move |s| s.bg(colors.ghost_element_active))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.next_month(cx);
                        }),
                    )
                    .child(
                        Icon::new(IconName::ChevronRight)
                            .size(IconSize::XSmall)
                            .color(Color::Default),
                    ),
            );

        let weekdays_header = div()
            .flex()
            .items_center()
            .justify_between()
            .children(RUSSIAN_WEEKDAYS_SHORT.iter().map(|name| {
                div()
                    .w(px(36.))
                    .text_center()
                    .text_size(TextSize::XSmall.rems())
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.text_muted)
                    .child(*name)
            }));

        let day_rows = (0..6).map(|row_idx| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(3.))
                .children((0..7).map(|col_idx| {
                    let day_date = grid_days[row_idx * 7 + col_idx];
                    let is_this_month =
                        day_date.month() == current_month && day_date.year() == current_year;
                    let is_selected = day_date == selected;
                    let is_today = day_date == today;

                    let text_color = if is_selected {
                        colors.background
                    } else if is_today {
                        colors.text_accent
                    } else if is_this_month {
                        colors.text
                    } else {
                        colors.text_muted
                    };

                    let bg_color = if is_selected {
                        colors.accent
                    } else {
                        colors.ghost_element_background
                    };

                    div()
                        .id(("cal-day", day_date.num_days_from_ce() as usize))
                        .w(px(36.))
                        .h(px(32.))
                        .rounded(Radius::Small.pixels())
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(bg_color)
                        .when(!is_selected && is_today, |el| {
                            el.border_1().border_color(colors.accent)
                        })
                        .when(!is_selected, |el| {
                            el.hover(move |s| s.bg(colors.ghost_element_hover))
                        })
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.select_date(day_date, cx);
                            }),
                        )
                        .child(
                            div()
                                .text_size(TextSize::Small.rems())
                                .when(is_selected || is_today, |el| {
                                    el.font_weight(FontWeight::BOLD)
                                })
                                .text_color(text_color)
                                .child(day_date.day().to_string()),
                        )
                }))
        });

        let month_card = div()
            .id("calendar-month-card")
            .w(px(320.))
            .panel_card(cx)
            .p(Spacing::Medium.pixels())
            .flex()
            .flex_col()
            .gap(Spacing::Small.pixels())
            .child(
                div()
                    .text_size(TextSize::Small.rems())
                    .text_color(colors.text_muted)
                    .child(format_subtitle(self.today)),
            )
            .child(nav_bar)
            .child(weekdays_header)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .children(day_rows),
            );

        let agenda_card = div()
            .id("calendar-agenda-card")
            .flex_1()
            .panel_card(cx)
            .p(Spacing::Medium.pixels())
            .flex()
            .flex_col()
            .gap(Spacing::Medium.pixels())
            .child(
                div()
                    .text_size(TextSize::Default.rems())
                    .font_weight(FontWeight::BOLD)
                    .text_color(colors.text)
                    .child(format_selected_header(self.selected_date)),
            )
            .child(div().h(px(1.)).bg(colors.border_variant))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_start()
                    .justify_start()
                    .pt(Spacing::Small.pixels())
                    .child(
                        div()
                            .text_size(TextSize::Small.rems())
                            .text_color(colors.text_muted)
                            .child("Нет событий."),
                    ),
            );

        div()
            .id("calendar-panel")
            .track_focus(&self.focus_handle)
            .key_context("Calendar")
            .on_action(cx.listener(|_, _: &Cancel, _, cx| {
                ShellWindows::close_panel(cx);
            }))
            .w(px(CALENDAR_PANEL_WIDTH))
            .shell_panel(cx)
            .p(Spacing::Large.pixels())
            .flex()
            .flex_col()
            .gap(Spacing::Medium.pixels())
            .child(header)
            .child(
                div()
                    .flex()
                    .gap(Spacing::Medium.pixels())
                    .child(month_card)
                    .child(agenda_card),
            )
    }
}

pub fn toggle_panel_at(at: Point<Pixels>, window: &Window, cx: &mut App) {
    let config = Config::global(cx);
    let size = PanelSize::FitHeight {
        width: CALENDAR_PANEL_WIDTH,
        max_height: CALENDAR_PANEL_MAX_HEIGHT,
    };
    let (anchor, margin) =
        panel_placement_from_event(config.bar.position, at, window, cx, size.initial());
    let panel_config = PanelConfig {
        size,
        anchor,
        margin,
        namespace: "calendar".to_string(),
    };

    ShellWindows::toggle_panel(cx, window, "calendar", panel_config, move |cx| {
        CalendarPanel::new(cx)
    });
}

pub fn toggle_on_display(display_id: Option<DisplayId>, cx: &mut App) {
    let config = Config::global(cx);
    let size = PanelSize::FitHeight {
        width: CALENDAR_PANEL_WIDTH,
        max_height: CALENDAR_PANEL_MAX_HEIGHT,
    };
    let anchor = match config.bar.position {
        crate::config::BarPosition::Bottom => Anchor::BOTTOM,
        crate::config::BarPosition::Top => Anchor::TOP,
        crate::config::BarPosition::Left => Anchor::LEFT | Anchor::TOP,
        crate::config::BarPosition::Right => Anchor::RIGHT | Anchor::TOP,
    };
    let gap = config.bar.panel_gap();
    let margin = (gap, gap, gap, gap);
    let panel_config = PanelConfig {
        size,
        anchor,
        margin,
        namespace: "calendar".to_string(),
    };
    ShellWindows::toggle_panel_on_display(cx, "calendar", panel_config, display_id, move |cx| {
        CalendarPanel::new(cx)
    });
}

pub fn toggle(cx: &mut App) {
    toggle_on_display(None, cx);
}
