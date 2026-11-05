use crate::demos::{DemoState, bound_text};
use blinc_cn as cn;
use blinc_core::reactive::computed;
use blinc_layout::prelude::*;

pub fn checkbox_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let checked = values.confirmed;
    div()
        .class("demo-stack")
        .child(
            cn::checkbox(checked)
                .label("Keep me signed in")
                .disabled(values.disabled),
        )
        .child(
            bound_text(computed(move |g| {
                if g.get(checked).unwrap_or(false) {
                    "Remembered"
                } else {
                    "Sign in each time"
                }
                .into()
            }))
            .class("demo-caption"),
        )
}

pub fn switch_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let checked = values.confirmed;
    div()
        .class("demo-stack")
        .child(
            cn::switch(checked)
                .label("Email notifications")
                .disabled(values.disabled),
        )
        .child(
            bound_text(computed(move |g| {
                if g.get(checked).unwrap_or(false) {
                    "Notifications on"
                } else {
                    "Notifications off"
                }
                .into()
            }))
            .class("demo-caption"),
        )
}

pub fn radio_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let choice = values.choice;
    div()
        .class("demo-stack")
        .child(
            cn::radio_group(choice)
                .label("Language")
                .option("typescript", "TypeScript")
                .option("rust", "Rust")
                .option("python", "Python")
                .disabled(values.disabled),
        )
        .child(
            bound_text(computed(move |g| {
                format!("Selected: {}", g.get(choice).unwrap_or_default())
            }))
            .class("demo-caption"),
        )
}

pub fn toggle_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let checked = values.confirmed;
    div()
        .class("demo-stack")
        .class("demo-stack--center")
        .child(cn::toggle(checked).label("Bold").disabled(values.disabled))
        .child(
            bound_text(computed(move |g| {
                if g.get(checked).unwrap_or(false) {
                    "Bold text enabled"
                } else {
                    "Regular text"
                }
                .into()
            }))
            .class("demo-caption"),
        )
}

pub fn toggle_group_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let choice = values.choice;
    let icon = |paths: &str| blinc_icons::to_svg_with_stroke(paths, 18.0, 2.0);
    div()
        .class("demo-wide")
        .child(
            div().class("demo-row").child(
                cn::toggle_group(choice)
                    .item(
                        cn::toggle_item("left")
                            .icon(icon(blinc_icons::icons::TEXT_ALIGN_START))
                            .aria_label("Align left"),
                    )
                    .item(
                        cn::toggle_item("center")
                            .icon(icon(blinc_icons::icons::TEXT_ALIGN_CENTER))
                            .aria_label("Align center"),
                    )
                    .item(
                        cn::toggle_item("right")
                            .icon(icon(blinc_icons::icons::TEXT_ALIGN_END))
                            .aria_label("Align right"),
                    )
                    .disabled(values.disabled),
            ),
        )
        .child(
            div()
                .class("demo-alignment-copy")
                .attr_when(
                    "data-align",
                    "left",
                    computed(move |g| g.get(choice).as_deref() == Some("left")),
                )
                .attr_when(
                    "data-align",
                    "right",
                    computed(move |g| g.get(choice).as_deref() == Some("right")),
                )
                .child(text("Find the right balance").class("demo-label"))
                .child(text("Align the content to suit your layout.").class("demo-caption")),
        )
}

pub fn number_input_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let number = values.number;
    div()
        .class("demo-stack")
        .child(cn::label("Quantity"))
        .child(
            cn::number_input(number)
                .min(0.0)
                .max(99.0)
                .step(1.0)
                .disabled(values.disabled),
        )
        .child(
            bound_text(computed(move |g| {
                format!("{} items", g.get(number).unwrap_or(0.0) as u32)
            }))
            .class("demo-caption"),
        )
}

pub fn number_gauge_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let number = values.number;
    div()
        .class("demo-stack")
        .child(cn::label("Volume"))
        .child(
            cn::number_gauge(number)
                .range(0.0..=100.0)
                .step(1.0)
                .w(256.0)
                .disabled(values.disabled),
        )
        .child(
            bound_text(computed(move |g| {
                format!("{:.0}%", g.get(number).unwrap_or(0.0))
            }))
            .class("demo-caption"),
        )
}

pub fn number_range_gauge_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let range = values.range;
    div()
        .class("demo-stack")
        .child(cn::label("Price range"))
        .child(
            cn::number_range_gauge(range)
                .range(0.0..=100.0)
                .step(1.0)
                .w(256.0)
                .disabled(values.disabled),
        )
        .child(
            bound_text(computed(move |g| {
                let range = g.get(range).unwrap_or(0.0..=0.0);
                format!("${:.0} – ${:.0}", range.start(), range.end())
            }))
            .class("demo-caption"),
        )
}

pub fn slider_demo(values: &DemoState, viewport: (f32, f32)) -> Div {
    let number = values.value;
    div().class("demo-wide").child(
        cn::slider(number)
            .label("Volume")
            .min(0.0)
            .max(100.0)
            .step(1.0)
            .show_value()
            .w((viewport.0 - 64.0).clamp(1.0, 360.0))
            .disabled(values.disabled),
    )
}

pub fn textarea_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let name = values.name;
    div()
        .class("demo-wide")
        .child(
            cn::textarea(&values.multiline)
                .label("Your message")
                .placeholder("Tell us what you are building…")
                .rows(4)
                .disabled(values.disabled)
                .on_change(move |text| name.set(text.to_string())),
        )
        .child(
            bound_text(computed(move |g| {
                format!(
                    "{} characters",
                    g.get(name).unwrap_or_default().chars().count()
                )
            }))
            .class("demo-caption"),
        )
}

pub fn input_otp_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let code = values.name;
    div()
        .class("demo-wide")
        .child(cn::label("Verification code"))
        .child(
            cn::input_otp(code, 6)
                .numeric_only(true)
                .group_every(3)
                .disabled(values.disabled),
        )
        .child(
            bound_text(computed(move |g| {
                format!("{} of 6 digits", g.get(code).unwrap_or_default().len())
            }))
            .class("demo-caption"),
        )
}

pub fn combobox_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let choice = values.choice;
    div()
        .class("demo-stack")
        .child(
            cn::combobox(choice)
                .label("Framework language")
                .option("rust", "Rust")
                .option("typescript", "TypeScript")
                .option("python", "Python")
                .disabled(values.disabled),
        )
        .child(
            bound_text(computed(move |g| {
                format!("Selected: {}", g.get(choice).unwrap_or_default())
            }))
            .class("demo-caption"),
        )
}

pub fn calendar_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let date = values.date;
    div()
        .class("demo-calendar")
        .child(
            cn::calendar(date)
                .today(cn::CalendarDay::new(2026, 11, 4))
                .week_start(1)
                .events(vec![cn::CalendarEvent::new(
                    cn::CalendarDay::new(2026, 11, 12),
                    "Design review",
                )]),
        )
        .child(
            bound_text(computed(move |g| {
                g.get(date)
                    .flatten()
                    .map(|day| format!("Selected: {:04}-{:02}-{:02}", day.year, day.month, day.day))
                    .unwrap_or_else(|| "Choose a date".into())
            }))
            .class("demo-caption"),
        )
}

pub fn date_picker_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let date = values.date;
    div()
        .class("demo-stack")
        .child(cn::label("Appointment date"))
        .child(
            cn::date_picker(date)
                .today(cn::CalendarDay::new(2026, 11, 4))
                .disabled(values.disabled),
        )
        .child(
            bound_text(computed(move |g| {
                g.get(date)
                    .flatten()
                    .map(|day| format!("Selected: {:04}-{:02}-{:02}", day.year, day.month, day.day))
                    .unwrap_or_else(|| "No date selected".into())
            }))
            .class("demo-caption"),
        )
}

pub fn time_picker_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let time = values.time;
    div()
        .class("demo-stack")
        .child(cn::label("Meeting time"))
        .child(
            cn::time_picker(time)
                .format(cn::TimeFormat::TwelveHour)
                .minute_step(5)
                .disabled(values.disabled),
        )
        .child(
            bound_text(computed(move |g| {
                format!("Selected: {}", g.get(time).unwrap_or_default())
            }))
            .class("demo-caption"),
        )
}

pub fn color_picker_demo(values: &DemoState, _: (f32, f32)) -> Div {
    div()
        .class("demo-stack")
        .child(cn::label("Accent colour"))
        .child(
            cn::color_picker(values.color)
                .alpha(true)
                .disabled(values.disabled)
                .swatches(vec![
                    blinc_core::Color::from_hex(0x7da8ff),
                    blinc_core::Color::from_hex(0x2dd4bf),
                    blinc_core::Color::from_hex(0xfbbf24),
                ]),
        )
}

pub fn label_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div()
        .class("demo-stack")
        .child(cn::label("Email address").required())
        .child(text("Labels give a control its context.").class("demo-caption"))
}
