//! Active window widget displaying the title of the currently focused window.

mod config;
pub use config::ActiveWindowConfig;

use gpui::{AnyElement, Context, Window, div, prelude::*};
use services::CompositorState;
use ui::{IconName, LabelCommon};

use super::{BarCtx, BarWidget, BarWidgetDescriptor, BarWidgetMeta, BarWidgetShell, boxed, style};
use crate::config::ActiveConfig;
use crate::state::AppState;
use crate::state::watch;

pub const fn descriptor() -> BarWidgetDescriptor {
    BarWidgetDescriptor::new(
        BarWidgetMeta {
            name: "ActiveWindow",
            aliases: &["WindowTitle"],
            label: "Active window",
            description: "Title of the focused window.",
            icon: IconName::Layout,
        },
        |cx| boxed(cx, ActiveWindow::new),
    )
}

/// Widget that displays the currently focused window's title.
pub struct ActiveWindow {
    _compositor: services::CompositorSubscriber,
    state: CompositorState,
}

impl ActiveWindow {
    const VERTICAL_LINE_WIDTH: usize = 3;
    const VERTICAL_MAX_LINES: usize = 5;
    /// Characters a vertical stack keeps before it marks the overflow.
    const VERTICAL_MAX_CHARS: usize = 15;

    /// Create a new active window widget.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let compositor = AppState::compositor(cx).clone();
        let state = compositor.get();

        watch(cx, compositor.subscribe(), |this, new_state, cx| {
            this.state = new_state;
            cx.notify();
        });

        Self {
            _compositor: compositor,
            state,
        }
    }

    fn window_text(&self) -> &str {
        let Some(window) = self.state.active_window.as_ref() else {
            return "";
        };

        let title = window.title.trim();
        if !title.is_empty() {
            return title;
        }

        window.class.trim()
    }

    /// The title cut at a word boundary: at `max_length` characters from the
    /// config, or at what the bar's title width holds when that is 0.
    fn display_title(&self, max_length: usize, fit: usize) -> String {
        let limit = if max_length == 0 {
            fit
        } else {
            max_length.min(fit)
        };
        style::fit_words(self.window_text(), limit)
    }

    /// Substring to icon, in match order.
    ///
    /// Brand marks first, then the "what it *is*" fallbacks - an editor we
    /// don't have a logo for is still a `<>`, a terminal is still a prompt.
    /// Order carries real weight here: the haystack is class *and* title, so
    /// a generic needle like "code" would otherwise swallow any window whose
    /// title merely mentions it.
    const WINDOW_ICON_HINTS: &[(&str, IconName)] = &[
        ("firefox", IconName::Firefox),
        // "chromium" does not contain "chrome", so both spellings are needed.
        ("chrome", IconName::Chrome),
        ("chromium", IconName::Chrome),
        ("telegram", IconName::Telegram),
        ("discord", IconName::Discord),
        ("vesktop", IconName::Discord),
        ("slack", IconName::Slack),
        ("spotify", IconName::Spotify),
        ("thunderbird", IconName::Thunderbird),
        ("alacritty", IconName::Alacritty),
        ("wezterm", IconName::Wezterm),
        ("neovim", IconName::Neovim),
        ("nvim", IconName::Neovim),
        // Zed's app id, not a bare "zed": that is a substring of "optimized"
        // and "analyzed", which any editor could have in a window title.
        ("dev.zed", IconName::Zed),
        ("code", IconName::VisualStudioCode),
        ("kitty", IconName::Terminal),
        ("terminal", IconName::Terminal),
        ("nautilus", IconName::Folder),
        ("thunar", IconName::Folder),
        ("dolphin", IconName::Folder),
        ("files", IconName::Folder),
    ];

    /// Icon for the focused window, keyed off its class/title.
    ///
    /// Anything unrecognised falls back to a generic window.
    fn window_icon(&self) -> Option<IconName> {
        let window = self.state.active_window.as_ref()?;
        let haystack = format!(
            "{} {}",
            window.class.to_lowercase(),
            window.title.to_lowercase()
        );

        Some(
            Self::WINDOW_ICON_HINTS
                .iter()
                .find(|(needle, _)| haystack.contains(needle))
                .map_or(IconName::Layout, |&(_, icon)| icon),
        )
    }

    fn vertical_lines(&self, max_length: usize) -> Vec<String> {
        let source = self.window_text();
        if source.is_empty() {
            return Vec::new();
        }
        let max_length = if max_length == 0 {
            usize::MAX
        } else {
            max_length
        };
        let max_lines = Self::VERTICAL_MAX_LINES;
        let line_width = Self::VERTICAL_LINE_WIDTH;

        let filtered: String = source
            .chars()
            .filter(|ch| *ch != '\n' && *ch != '\r')
            .take(max_length)
            .collect();
        let was_truncated = source
            .chars()
            .filter(|ch| *ch != '\n' && *ch != '\r')
            .count()
            > filtered.chars().count();

        let tokens: Vec<String> = filtered
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|segment| !segment.is_empty())
            .map(|segment| Self::vertical_segment(segment, line_width))
            .filter(|segment| !segment.is_empty())
            .collect();

        let mut lines = if tokens.len() > 1 {
            tokens
        } else {
            let condensed: String = filtered.chars().filter(|ch| ch.is_alphanumeric()).collect();
            Self::chunk_vertical_text(&condensed, line_width)
        };

        if lines.is_empty() {
            lines = Self::chunk_vertical_text(&filtered, line_width);
        }

        let clipped = lines.len() > max_lines;
        lines.truncate(max_lines);

        if was_truncated || clipped {
            Self::mark_vertical_overflow(&mut lines);
        }

        lines
    }

    fn vertical_segment(segment: &str, width: usize) -> String {
        segment
            .chars()
            .take(width)
            .collect::<String>()
            .to_uppercase()
    }

    fn chunk_vertical_text(source: &str, width: usize) -> Vec<String> {
        let chars: Vec<char> = source.chars().collect();
        chars
            .chunks(width)
            .map(|chunk| chunk.iter().collect::<String>().to_uppercase())
            .collect()
    }

    fn mark_vertical_overflow(lines: &mut [String]) {
        let Some(last) = lines.last_mut() else {
            return;
        };

        let mut chars: Vec<char> = last.chars().collect();
        if chars.is_empty() {
            *last = "…".to_string();
            return;
        }

        if chars.len() >= 3 {
            chars.truncate(chars.len() - 1);
        } else {
            chars.truncate(1);
        }

        chars.push('…');
        *last = chars.into_iter().collect();
    }

    fn has_window_content(&self) -> bool {
        !self.window_text().is_empty()
    }
}

impl BarWidget for ActiveWindow {
    fn shell(&self) -> BarWidgetShell {
        if self.has_window_content() {
            BarWidgetShell::Standard
        } else {
            BarWidgetShell::None
        }
    }

    fn render_vertical(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.has_window_content() {
            return div().id("active-window").into_any_element();
        }

        style::stack(ctx)
            .id("active-window")
            .gap(ctx.metrics.line_gap)
            .children(
                self.vertical_lines(Self::VERTICAL_MAX_CHARS)
                    .into_iter()
                    .map(|line| style::text_line(ctx, style::label(ctx, line))),
            )
            .into_any_element()
    }

    fn render_horizontal(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.has_window_content() {
            return div().id("active-window").into_any_element();
        }

        let config = &cx.config().bar.modules.active_window;
        let title = self.display_title(config.max_length, ctx.metrics.title_chars);
        let icon = config.show_app_icon.then(|| self.window_icon()).flatten();

        style::stack(ctx)
            .id("active-window")
            .justify_center()
            .gap(ctx.metrics.gap)
            .max_w(ctx.metrics.title_width)
            .overflow_hidden()
            .children(icon.map(|icon| style::icon(ctx, icon)))
            .child(style::label(ctx, title).truncate())
            .into_any_element()
    }
}
