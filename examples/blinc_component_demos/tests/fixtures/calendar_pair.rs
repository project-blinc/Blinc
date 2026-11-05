use blinc_app::windowed::WindowedContext;
use blinc_cn::{CalendarDay, calendar};
use blinc_layout::prelude::*;

pub const CSS: &str = include_str!("calendar_pair.css");

pub fn build(ctx: &mut WindowedContext) -> Div {
    let left = ctx.use_signal_keyed("calendar-pair-left", || None);
    let right = ctx.use_signal_keyed("calendar-pair-right", || None);
    let today = CalendarDay::new(2026, 11, 4);
    div()
        .class("calendar-pair")
        .child(
            div()
                .class("calendar-pair-item")
                .child(text("First calendar"))
                .child(calendar(left).today(today).week_start(1)),
        )
        .child(
            div()
                .class("calendar-pair-item")
                .child(text("Second calendar"))
                .child(calendar(right).today(today).week_start(1)),
        )
}
