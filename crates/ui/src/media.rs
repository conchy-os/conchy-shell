//! Helpers shared by the media surfaces: cover loading and track progress.

use std::path::PathBuf;

use gpui::{App, ImageSource, IntoElement, div, prelude::*, px, relative};

use crate::{ActiveTheme, Color, Label, LabelCommon, Radius, Spacing, TextSize, h_flex};

/// `http(s)` sources are fetched through the app's HTTP client; anything
/// else is a path, with `~/` expanded.
pub fn image_source(source: &str) -> ImageSource {
    if source.starts_with("http://") || source.starts_with("https://") {
        return source.to_string().into();
    }
    match (source.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest).into(),
        _ => PathBuf::from(source).into(),
    }
}

/// `1:05`, or `1:02:05` past an hour.
pub fn format_track_time(us: i64) -> String {
    let total = (us / 1_000_000).max(0);
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Elapsed time, a thin track and the total, as media cards show under a
/// title.
pub fn track_progress(position_us: i64, duration_us: i64, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let fraction = (position_us as f32 / duration_us.max(1) as f32).clamp(0.0, 1.0);
    h_flex()
        .w_full()
        .items_center()
        .gap(Spacing::Small.pixels())
        .child(
            Label::new(format_track_time(position_us))
                .size(TextSize::XSmall)
                .color(Color::Muted),
        )
        .child(
            div()
                .flex_1()
                .h(px(3.))
                .rounded(Radius::Full.pixels())
                .bg(colors.element_background)
                .overflow_hidden()
                .child(
                    div()
                        .h_full()
                        .rounded(Radius::Full.pixels())
                        .bg(colors.accent)
                        .w(relative(fraction)),
                ),
        )
        .child(
            Label::new(format_track_time(duration_us))
                .size(TextSize::XSmall)
                .color(Color::Muted),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_times_show_hours_only_when_needed() {
        assert_eq!(format_track_time(65_000_000), "1:05");
        assert_eq!(format_track_time(3_725_000_000), "1:02:05");
        assert_eq!(format_track_time(-5), "0:00");
    }
}
