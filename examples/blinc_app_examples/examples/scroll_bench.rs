#![allow(deprecated, dead_code)]
//! A long list in a window, for watching what scrolling costs.
//!
//! ```text
//! cargo run --release -p blinc_app_examples --example scroll_bench
//! ROWS=250000 SPEED=3000 cargo run --release -p blinc_app_examples --example scroll_bench
//! BLINC_NODE_TABLE=1 cargo run --release -p blinc_app_examples --example scroll_bench
//! BLINC_MOTION_PROMOTION=1 cargo run --release -p blinc_app_examples --example scroll_bench
//! ```
//!
//! - `ROWS`: how many rows (default 100000).
//! - `LIST`: `virtual` (default) builds only the rows in view, so the window is up at once however
//!   long the list is; `flat` builds a child for every row before the first frame, which at this
//!   size takes seconds with nothing on screen.
//! - `SPEED`: pixels a second the list scrolls by itself, up and down the whole list (default
//!   2000); `0` leaves it to the wheel.
//! - `SCROLLBAR`: `auto` (default), `always`, `hover` or `never`.
//!
//! The window title says how the list is painted: rows on the CPU (the default), rows on the GPU
//! with `BLINC_NODE_TABLE=1`, or the old way, a full paint for every scrolled frame, with
//! `BLINC_MOTION_PROMOTION=1`. `RUST_LOG=blinc_app::frame_timing=trace` prints what each frame
//! cost. The header does not scroll.

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_core::Color;
use blinc_layout::selector::ScrollRef;
use blinc_layout::widgets::ScrollbarVisibility;
use blinc_layout::widgets::virtual_list::virtual_list;
use std::sync::OnceLock;

/// A row's height, in layout units.
const ROW_HEIGHT: f32 = 30.0;

fn number(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn rows() -> usize {
    number("ROWS", 100_000.0).max(1.0) as usize
}

fn speed() -> f32 {
    number("SPEED", 2000.0).max(0.0)
}

fn scrollbar() -> ScrollbarVisibility {
    match std::env::var("SCROLLBAR").as_deref() {
        Ok("always") => ScrollbarVisibility::Always,
        Ok("hover") => ScrollbarVisibility::Hover,
        Ok("never") => ScrollbarVisibility::Never,
        _ => ScrollbarVisibility::Auto,
    }
}

fn flat() -> bool {
    std::env::var("LIST").is_ok_and(|v| v == "flat")
}

fn flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn mode() -> &'static str {
    if flag("BLINC_NODE_TABLE") {
        "rows on the GPU"
    } else if flag("BLINC_MOTION_PROMOTION") {
        "a full paint every frame"
    } else {
        "rows on the CPU"
    }
}

/// The list's scroll reference, shared with the tick that scrolls it.
fn reference() -> &'static ScrollRef {
    static REFERENCE: OnceLock<ScrollRef> = OnceLock::new();
    REFERENCE.get_or_init(ScrollRef::new)
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    // `RUST_LOG=blinc_app::frame_timing=trace` prints what each frame cost.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = WindowConfig {
        title: format!("Scroll bench: {} rows, {}", rows(), mode()),
        width: 480,
        height: 800,
        ..Default::default()
    };
    blinc_app::windowed::WindowedApp::run(config, build_ui)
}

fn row(i: usize) -> Div {
    let hue = (i % 11) as f32 / 11.0;
    div()
        .w_full()
        .h(ROW_HEIGHT - 4.0)
        .mb(1.0)
        .px(3.0)
        .rounded(8.0)
        .bg(Color::rgb(0.18 + 0.3 * hue, 0.4, 0.88 - 0.35 * hue))
        .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.18))
        .flex_row()
        .items_center()
        .justify_between()
        .child(
            text(format!("Row {i}"))
                .size(14.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE),
        )
        .child(
            text(format!("{:.1}%", (i % 1000) as f32 / 10.0))
                .size(12.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.7)),
        )
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    let total = rows();
    let list: Box<dyn ElementBuilder> = if flat() {
        let mut rows = div().flex_col().w_full().p(2.0);
        for i in 0..total {
            rows = rows.child(row(i));
        }
        Box::new(
            scroll()
                .bind(reference())
                .scrollbar_visibility(scrollbar())
                .w_full()
                .h_full()
                .child(rows),
        )
    } else {
        Box::new(
            virtual_list(total, row)
                .estimated_item_height(ROW_HEIGHT)
                .bind(reference())
                .w_full()
                .h_full()
                .into_div(),
        )
    };

    // The list scrolls itself, up and down the whole of it, from a tick of the animation
    // scheduler: it runs a step a frame and keeps frames coming, where a thread of the example's own
    // would set an offset the window does not wake to apply. It is added once, and finds the same
    // reference after a rebuild.
    static AUTO: std::sync::Once = std::sync::Once::new();
    let pixels_a_second = speed();
    if pixels_a_second > 0.0 {
        AUTO.call_once(move || {
            let length = total as f32 * ROW_HEIGHT;
            let mut travelled = 0.0f32;
            blinc_animation::get_scheduler().register_tick_callback(move |dt| {
                travelled = (travelled + dt * pixels_a_second) % (2.0 * length);
                // Triangle wave over the whole list.
                let y = if travelled < length {
                    travelled
                } else {
                    2.0 * length - travelled
                };
                reference().set_scroll_offset(0.0, y);
            });
        });
    }

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgb(0.06, 0.07, 0.09))
        .flex_col()
        .child(
            div()
                .w_full()
                .px(4.0)
                .py(2.0)
                .bg(Color::rgb(0.12, 0.14, 0.2))
                .flex_col()
                .child(
                    text(format!("{total} rows"))
                        .size(18.0)
                        .weight(FontWeight::Bold)
                        .color(Color::WHITE),
                )
                .child(
                    text(format!(
                        "{}; {}",
                        mode(),
                        if pixels_a_second > 0.0 {
                            format!("scrolling itself at {pixels_a_second:.0} px a second")
                        } else {
                            "scroll with the wheel".to_string()
                        }
                    ))
                    .size(11.0)
                    .color(Color::rgb(0.6, 0.65, 0.75)),
                ),
        )
        .child(div().w_full().flex_grow().child_box(list))
}

#[cfg(target_arch = "wasm32")]
fn main() {}
