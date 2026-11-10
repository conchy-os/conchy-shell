//! Battery widget that displays battery status using the UPower service.

mod config;
pub use config::BatteryConfig;

use gpui::{AnyElement, Context, Window, prelude::*};
use services::UPowerData;
use ui::IconName;

use super::{BarCtx, BarWidget, BarWidgetDescriptor, BarWidgetMeta, boxed, style};
use crate::config::ActiveConfig;
use crate::state::AppState;
use crate::state::watch;

pub const fn descriptor() -> BarWidgetDescriptor {
    BarWidgetDescriptor::new(
        BarWidgetMeta {
            name: "Battery",
            aliases: &[],
            label: "Battery",
            description: "Charge level and charging state.",
            icon: IconName::Battery,
        },
        |cx| boxed(cx, Battery::new),
    )
}

/// A battery widget that displays the current battery percentage and status.
pub struct Battery {
    data: UPowerData,
}

impl Battery {
    /// Create a new battery widget.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscriber = AppState::upower(cx).clone();
        let initial_data = subscriber.get();

        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });

        Battery { data: initial_data }
    }

    fn render(&self, ctx: &BarCtx, config: &BatteryConfig) -> AnyElement {
        let (icon_color, text_color) = style::battery_ink(ctx, self.data.battery.as_ref());
        let text = match &self.data.battery {
            Some(battery) if config.show_percentage => {
                style::compact_percent(ctx, battery.percentage.into())
            }
            _ => String::new(),
        };

        style::stack(ctx)
            .id("battery-widget")
            .gap(ctx.metrics.gap)
            .when(config.show_icon, |el| {
                let icon = crate::icons::battery_data_icon(self.data.battery.as_ref());
                el.child(style::icon_in(ctx, icon, icon_color))
            })
            .when(!text.is_empty(), |el| {
                el.child(style::value(ctx, style::label_in(ctx, text, text_color)))
            })
            .into_any_element()
    }
}

impl BarWidget for Battery {
    fn render_vertical(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, &cx.config().bar.modules.battery)
    }

    fn render_horizontal(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, &cx.config().bar.modules.battery)
    }
}
