//! A thumbnail screen with the frame around its work area: the bar's and a
//! taskbar dock's edges as deep as they are, the rest as the band.

use gpui::{AnyElement, App, div, prelude::*};
use ui::ActiveTheme;

use super::preview::{frame, scale, screen};
use super::schema::ConfigPath;
use crate::bar::config::BarPosition;
use crate::config::{Config, FrameConfig};

pub fn frame_preview(path: &ConfigPath<FrameConfig>, cx: &App) -> AnyElement {
    let edited = path.read(cx).clone();
    // The bar and dock come from the live config: the frame form only edits its own keys.
    let config = Config::global(cx);
    let theme = cx.theme();
    let colors = theme.colors();

    let caption = if edited.enabled {
        format!(
            "{:.0} px band · {:.0} px corners",
            edited.thickness,
            f32::from(edited.radius())
        )
    } else {
        "Off".to_string()
    };
    if !edited.enabled {
        return frame(screen(cx), caption);
    }

    let color = colors.accent.opacity(
        theme
            .surface_opacity(edited.opacity.or(config.bar.opacity))
            .max(0.4),
    );
    let band = scale(edited.thickness, 2., 10.);
    let bar_depth = scale(config.bar.size, 4., 18.);
    let dock_depth = scale(crate::dock::thickness(&config.dock), 4., 18.);
    let depth = |edge: BarPosition| {
        if config.bar_is_frame_side() && config.bar.position == edge {
            bar_depth
        } else if config.dock_is_frame_side() && config.dock.position == edge {
            dock_depth
        } else {
            band
        }
    };
    let hole = div()
        .absolute()
        .rounded(scale(f32::from(edited.radius()), 0., 12.))
        .bg(colors.surface_background)
        .top(depth(BarPosition::Top))
        .right(depth(BarPosition::Right))
        .bottom(depth(BarPosition::Bottom))
        .left(depth(BarPosition::Left));

    frame(screen(cx).bg(color).child(hole), caption)
}
