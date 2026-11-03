//! Pointer and keyboard surfaces for colour coordinates.

use super::{color_picker::Controller, number_gauge::GaugeFocus};
use blinc_core::{Brush, Color, Gradient, Point, Rect, reactive::computed};
use blinc_layout::{
    canvas::canvas,
    div::{Div, div},
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Part {
    SaturationValue,
    Hue,
    Alpha,
}

impl Part {
    fn name(self) -> &'static str {
        match self {
            Self::SaturationValue => "sv",
            Self::Hue => "hue",
            Self::Alpha => "alpha",
        }
    }
    fn seek(self, c: &Controller, x: f32, y: f32, w: f32, h: f32) {
        let mut next = c.hsva.get();
        let fraction = |p: f32, size: f32| {
            if size > 0.0 {
                (p / size).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        match self {
            Self::SaturationValue => {
                next[1] = fraction(x, w);
                next[2] = 1.0 - fraction(y, h);
            }
            Self::Hue => next[0] = fraction(x, w) * 360.0,
            Self::Alpha => next[3] = fraction(x, w),
        }
        c.edit(next);
    }
    fn key(self, c: &Controller, code: u32, shift: bool) -> bool {
        let mut next = c.hsva.get();
        let amount = if shift { 0.1 } else { 0.01 };
        match self {
            Self::SaturationValue => match code {
                37 => next[1] -= amount,
                39 => next[1] += amount,
                38 => next[2] += amount,
                40 => next[2] -= amount,
                36 => next[1] = 0.0,
                35 => next[1] = 1.0,
                _ => return false,
            },
            Self::Hue | Self::Alpha => {
                let (index, max, step) = if self == Self::Hue {
                    (0, 360.0, if shift { 10.0 } else { 1.0 })
                } else {
                    (3, 1.0, amount)
                };
                match code {
                    37 | 40 => next[index] -= step,
                    39 | 38 => next[index] += step,
                    36 => next[index] = 0.0,
                    35 => next[index] = max,
                    33 => next[index] += max * 0.1,
                    34 => next[index] -= max * 0.1,
                    _ => return false,
                }
            }
        }
        c.edit(next);
        true
    }
}

pub(super) fn surface(c: &Controller, instance: &str, part: Part) -> Div {
    let key = format!("{instance}-{}", part.name());
    let id = format!("cn-color-picker-{key}");
    let focus = GaugeFocus::new(&key, &id);
    let bounds = Arc::new(Mutex::new((0.0, 0.0, 1.0, 1.0)));
    let mut area = div()
        .class(&format!("cn-color-picker-{}", part.name()))
        .id(&id)
        .attr("tabindex", "0")
        .attr(
            "role",
            if part == Part::SaturationValue {
                "group"
            } else {
                "slider"
            },
        )
        .attr(
            "aria-label",
            match part {
                Part::SaturationValue => "Saturation and brightness",
                Part::Hue => "Hue",
                Part::Alpha => "Opacity",
            },
        )
        .attr_when("disabled", "", c.disabled.clone())
        .attr_when("data-focus-visible", "", focus.visible())
        .relative()
        .w_full();
    let hsva = c.hsva;
    match part {
        Part::SaturationValue => {
            area = area
                .bg(computed(move |g| {
                    Color::from_hsva(g.get(hsva).unwrap_or_default()[0], 1.0, 1.0, 1.0)
                }))
                .child(
                    div()
                        .class("cn-color-picker-white")
                        .absolute()
                        .inset(0.0)
                        .pointer_events_none(),
                )
                .child(
                    div()
                        .class("cn-color-picker-black")
                        .absolute()
                        .inset(0.0)
                        .pointer_events_none(),
                );
        }
        Part::Hue => {
            area = area.flex_row();
            for sector in 0..6 {
                area = area.child(
                    div()
                        .class("cn-color-picker-hue-segment")
                        .attr("data-sector", sector.to_string())
                        .w(0.0)
                        .h_full()
                        .flex_grow()
                        .pointer_events_none(),
                );
            }
        }
        Part::Alpha => {
            area = area.child(
                div()
                    .absolute()
                    .inset(0.0)
                    .overflow_clip()
                    .pointer_events_none()
                    .child(checker())
                    .child(
                        canvas(move |ctx, b| {
                            let [h, s, v, _] = hsva.get();
                            let color = Color::from_hsva(h, s, v, 1.0);
                            ctx.fill_rect(
                                Rect::new(0.0, 0.0, b.width, b.height),
                                0.0.into(),
                                Brush::Gradient(Gradient::linear(
                                    Point::new(0.0, 0.0),
                                    Point::new(b.width, 0.0),
                                    color.with_alpha(0.0),
                                    color,
                                )),
                            );
                        })
                        .w_full()
                        .h_full()
                        .redraw_while(|| false),
                    ),
            );
        }
    }
    area = area.child(marker(c, part));
    area.on_focus(move |_| focus.press())
        .on_blur(move |_| focus.focused.set(false))
        .on_unmount(move |_| blinc_layout::click_outside::unregister_click_outside(&key))
        .on_mouse_down({
            let (c, bounds) = (c.clone(), bounds.clone());
            move |e| {
                if c.disabled.now().unwrap_or(false) {
                    return;
                }
                focus.press();
                *bounds.lock().unwrap() = (e.bounds_x, e.bounds_y, e.bounds_width, e.bounds_height);
                part.seek(&c, e.local_x, e.local_y, e.bounds_width, e.bounds_height);
            }
        })
        .on_drag({
            let c = c.clone();
            move |e| {
                let (x, y, w, h) = *bounds.lock().unwrap();
                part.seek(&c, e.mouse_x - x, e.mouse_y - y, w, h);
            }
        })
        .on_key_down({
            let c = c.clone();
            move |e| {
                let tabbed = blinc_layout::event_router::keyboard_focused_node();
                let focused = match tabbed {
                    Some(node) => node == e.node_id,
                    None => {
                        focus.focused.get()
                            && !blinc_layout::widgets::text_input::has_focused_text_input()
                            && blinc_layout::widgets::text_input::focused_editable_node_id()
                                .is_none()
                    }
                };
                if !focused {
                    if tabbed.is_some() && focus.focused.get() {
                        focus.focused.set(false);
                    }
                    return;
                }
                if !c.disabled.now().unwrap_or(false) && part.key(&c, e.key_code, e.shift) {
                    if tabbed.is_none() {
                        focus.key();
                    }
                }
            }
        })
}

pub(super) fn checker() -> Div {
    div().absolute().inset(0.0).pointer_events_none().child(
        canvas(move |ctx, b| {
            let sheet = blinc_layout::css_parser::active_stylesheet();
            let ink = |var, fallback| {
                sheet
                    .as_ref()
                    .and_then(|s| s.resolve_color(var))
                    .unwrap_or(fallback)
            };
            let light = ink("var(--cn-color-picker-checker-light)", Color::WHITE);
            let dark = ink("var(--cn-color-picker-checker-dark)", Color::GRAY);
            for row in 0..(b.height / 6.0).ceil() as i32 {
                for col in 0..(b.width / 6.0).ceil() as i32 {
                    let x = col as f32 * 6.0;
                    let y = row as f32 * 6.0;
                    ctx.fill_rect(
                        Rect::new(x, y, 6.0f32.min(b.width - x), 6.0f32.min(b.height - y)),
                        0.0.into(),
                        Brush::Solid(if (row + col) % 2 == 0 { light } else { dark }),
                    );
                }
            }
        })
        .w_full()
        .h_full()
        .redraw_while(|| false),
    )
}

fn marker(c: &Controller, part: Part) -> Div {
    let hsva = c.hsva;
    let x = move |g: &blinc_core::reactive::ReactiveGraph| {
        let v = g.get(hsva).unwrap_or_default();
        match part {
            Part::SaturationValue => v[1],
            Part::Hue => v[0] / 360.0,
            Part::Alpha => v[3],
        }
    };
    let y = move |g: &blinc_core::reactive::ReactiveGraph| {
        if part == Part::SaturationValue {
            1.0 - g.get(hsva).unwrap_or_default()[2]
        } else {
            0.5
        }
    };
    let spacer = || {
        div()
            .w(0.0)
            .h(0.0)
            .min_w(0.0)
            .min_h(0.0)
            .pointer_events_none()
    };
    div()
        .absolute()
        .inset(0.0)
        .flex_col()
        .pointer_events_none()
        .child(spacer().flex_grow_value(computed(y)))
        .child(
            div()
                .w_full()
                .h(0.0)
                .flex_row()
                .pointer_events_none()
                .child(spacer().flex_grow_value(computed(x)))
                .child(
                    spacer().flex_shrink_0().relative().child(
                        div()
                            .class("cn-color-picker-marker")
                            .absolute()
                            .pointer_events_none(),
                    ),
                )
                .child(spacer().flex_grow_value(computed(move |g| 1.0 - x(g)))),
        )
        .child(spacer().flex_grow_value(computed(move |g| 1.0 - y(g))))
}
