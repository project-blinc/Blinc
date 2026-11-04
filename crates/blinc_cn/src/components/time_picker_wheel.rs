//! Wheel selection with a draft committed by Save.

use super::time_picker::{Config, Segment, TimeFormat, TimeOfDay};
use blinc_core::reactive::{Signal, computed, effect, signal};
use blinc_layout::{
    binding::IntoReactive,
    div::{Div, div},
    selector::ScrollRef,
    text::Text,
    widgets::overlay_stack::OverlayHandle,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, Ordering},
};

fn close(active: Signal<Option<u64>>) {
    if let Some(raw) = active.get() {
        OverlayHandle::from_raw(raw).close();
    }
}

pub(super) fn picker(c: &Config, active: Signal<Option<u64>>) -> Div {
    let draft = c.draft();
    let value = draft.value;
    let source = c.value;
    effect(move |g| {
        if let Some(time) = g.get(source) {
            value.set(time);
        }
    });
    let focused = signal(None::<Segment>);
    let mut header = div()
        .class("cn-time-picker-wheel-header")
        .flex_col()
        .items_center()
        .child(Text::new("Select time").class("cn-time-picker-wheel-title"));
    if c.format == TimeFormat::TwelveHour {
        header = header.child(period(&draft, focused));
    }
    let mut wheels = div()
        .class("cn-time-picker-wheels")
        .relative()
        .flex_row()
        .child(
            div()
                .class("cn-time-picker-wheel-selection")
                .absolute()
                .pointer_events_none(),
        )
        .child(column(&draft, focused, Segment::Hour))
        .child(separator())
        .child(column(&draft, focused, Segment::Minute));
    if c.seconds {
        wheels = wheels
            .child(separator())
            .child(column(&draft, focused, Segment::Second));
    }
    let save = c.clone();
    div()
        .class("cn-time-picker-wheel")
        .attr(
            "data-format",
            if c.format == TimeFormat::TwelveHour {
                "12"
            } else {
                "24"
            },
        )
        .attr_when("data-seconds", "", c.seconds)
        .flex_col()
        .h_fit()
        .child(header)
        .child(wheels)
        .child(
            div()
                .class("cn-time-picker-wheel-footer")
                .flex_row()
                .justify_end()
                .child(
                    super::button::button("Cancel")
                        .variant(super::button::ButtonVariant::Ghost)
                        .size(super::button::ButtonSize::Small)
                        .on_click(move |_| close(active))
                        .build_component()
                        .class("cn-time-picker-wheel-cancel"),
                )
                .child(
                    super::button::button("Save")
                        .variant(super::button::ButtonVariant::Outline)
                        .size(super::button::ButtonSize::Small)
                        .on_click(move |_| {
                            save.edit(value.get());
                            close(active);
                        })
                        .build_component()
                        .class("cn-time-picker-wheel-save"),
                ),
        )
}

fn separator() -> Div {
    div()
        .class("cn-time-picker-wheel-separator")
        .flex_row()
        .items_center()
        .justify_center()
        .child(Text::new(":").no_cursor())
}

fn period(c: &Config, focused: Signal<Option<Segment>>) -> Div {
    let value = c.value;
    let mut group = div()
        .class("cn-time-picker-wheel-period")
        .attr("role", "group")
        .attr("aria-label", "Time period")
        .flex_row();
    for (half, label) in [(0, "AM"), (1, "PM")] {
        let edit = c.clone();
        group = group.child(
            div()
                .class("cn-time-picker-wheel-period-option")
                .class(format!("cn-time-picker-wheel-{}", label.to_lowercase()))
                .attr("role", "button")
                .attr("tabindex", "0")
                .attr_when(
                    "data-selected",
                    "",
                    computed(move |g| g.get(value).unwrap_or_default().hour() / 12 == half),
                )
                .flex_row()
                .items_center()
                .justify_center()
                .child(Text::new(label).no_cursor())
                .on_click(move |_| {
                    focused.set(None);
                    let time = value.get();
                    edit.edit(
                        TimeOfDay::new(time.hour() % 12 + half * 12, time.minute(), time.second())
                            .unwrap(),
                    );
                }),
        );
    }
    group
}

pub(super) fn row_height() -> f32 {
    blinc_layout::css_parser::active_stylesheet()
        .and_then(|sheet| {
            sheet
                .get_variable("cn-time-picker-wheel-row-height")
                .map(str::to_owned)
        })
        .and_then(|value| value.trim().trim_end_matches("px").parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(32.0)
}

fn column(c: &Config, focused: Signal<Option<Segment>>, part: Segment) -> Div {
    let name = match part {
        Segment::Hour => "hour",
        Segment::Minute => "minute",
        Segment::Second => "second",
    };
    let (min, max) = part.bounds(c);
    let height = row_height();
    let reveal = ScrollRef::new();
    let initial = max - part.get(c, c.value.get());
    let position = reveal.watch_y(initial as f32 * height);
    let observed = Arc::new(AtomicU8::new(initial));
    reveal.set_scroll_offset(0.0, initial as f32 * height);
    effect({
        let config = c.clone();
        let reveal = reveal.clone();
        let observed = observed.clone();
        move |g| {
            let selected = part.get(&config, g.get(config.value).unwrap_or_default());
            let index = max - selected;
            if observed.swap(index, Ordering::Relaxed) != index {
                reveal.set_scroll_offset(0.0, index as f32 * height);
            }
        }
    });
    effect({
        let config = c.clone();
        move |g| {
            let offset = g.get(position).unwrap_or(initial as f32 * height);
            let index = (offset / height).round().clamp(0.0, (max - min) as f32) as u8;
            if observed.swap(index, Ordering::Relaxed) != index {
                let time = g.get_untracked(config.value).unwrap_or_default();
                config.value.set(part.replace(&config, time, max - index));
            }
        }
    });
    let mut rows = div()
        .class("cn-time-picker-wheel-list")
        .w_full()
        .h_fit()
        .flex_col();
    for value in (min..=max).rev() {
        rows = rows.child(
            div()
                .class("cn-time-picker-wheel-option")
                .flex_shrink_0()
                .flex_row()
                .items_center()
                .justify_center()
                .pointer_events_none()
                .child(Text::new(format!("{value:02}")).no_cursor()),
        );
    }
    let pressed = Arc::new(Mutex::new(None::<(f32, f32, bool)>));
    div()
        .class("cn-time-picker-wheel-column")
        .class(format!("cn-time-picker-wheel-{name}"))
        .attr("tabindex", "0")
        .attr("role", "spinbutton")
        .attr("aria-label", format!("Select {name}"))
        .overflow_y_scroll()
        .bind_scroll(&reveal)
        .child(rows)
        .on_mouse_down({
            let pressed = pressed.clone();
            let reveal = reveal.clone();
            move |e| {
                focused.set(Some(part));
                *pressed.lock().unwrap() = Some((e.mouse_y, reveal.scroll_y(), false));
            }
        })
        .on_drag({
            let pressed = pressed.clone();
            let reveal = reveal.clone();
            move |e| {
                let mut pressed = pressed.lock().unwrap();
                if let Some((start_y, start_offset, dragged)) = pressed.as_mut() {
                    *dragged = true;
                    reveal.set_scroll_offset(
                        0.0,
                        (*start_offset - (e.mouse_y - *start_y))
                            .clamp(0.0, (max - min) as f32 * height),
                    );
                }
            }
        })
        .on_drag_end({
            let pressed = pressed.clone();
            move |_| {
                if let Some((_, _, dragged)) = pressed.lock().unwrap().as_mut() {
                    *dragged = true;
                }
            }
        })
        .on_mouse_up({
            let config = c.clone();
            let reveal = reveal.clone();
            move |e| {
                let pressed = pressed.lock().unwrap().take();
                let index = if matches!(pressed, Some((_, _, false))) {
                    ((reveal.scroll_y() + e.local_y - 2.5 * height) / height).round()
                } else {
                    (reveal.scroll_y() / height).round()
                }
                .clamp(0.0, (max - min) as f32) as u8;
                part.set(&config, max - index);
                reveal.set_scroll_offset(0.0, index as f32 * height);
            }
        })
        .on_hover_leave({
            let config = c.clone();
            let reveal = reveal.clone();
            move |_| {
                reveal.set_scroll_offset(
                    0.0,
                    (max - part.get(&config, config.value.get())) as f32 * height,
                )
            }
        })
        .on_key_down({
            let config = c.clone();
            move |e| {
                let owns_focus = blinc_layout::event_router::keyboard_focused_node()
                    .map_or_else(|| focused.get() == Some(part), |node| node == e.node_id);
                if !owns_focus || config.disabled.now().unwrap_or(false) {
                    return;
                }
                match e.key_code {
                    38 => part.step(&config, 1),
                    40 => part.step(&config, -1),
                    33 => part.step(&config, 5),
                    34 => part.step(&config, -5),
                    36 => part.set(&config, min),
                    35 => part.set(&config, max),
                    _ => {}
                }
            }
        })
}
