#![allow(deprecated, dead_code)]
//! A column of tiles that reorders itself and slides, in a window, for watching what a FLIP
//! slide costs.
//!
//! ```text
//! cargo run --release -p blinc_app_examples --example flip_window_check
//! BLINC_MOTION_TRACE=/tmp/flip.jsonl cargo run --release -p blinc_app_examples --example flip_window_check
//! BLINC_MOTION_PROMOTION=1 cargo run --release -p blinc_app_examples --example flip_window_check
//! ```
//!
//! - `TILES`: how many tiles (default 12).
//! - `PERIOD`: seconds between reorders (default 2).
//! - `SLIDE_MS`: length of the slide in milliseconds (default 600).
//! - `EXIT_AFTER`: seconds until the process exits; unset keeps the window open.
//!
//! Every reorder moves the first tile to the end, so one tile slides the length of the column
//! and the others one place. The title says how bound nodes are painted; `BLINC_MOTION_TRACE`
//! records for every frame whether it was a rebuild, whether it was drawn, and whether it took the
//! fast path.

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::Color;
use blinc_core::context_state::BlincContextState;
use blinc_core::reactive::State;
use std::sync::Once;

/// Rebuilds the container when its order changes; it has no events of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
struct PassiveFSM;

impl StateTransitions for PassiveFSM {
    fn on_event(&self, _event: u32) -> Option<Self> {
        None
    }
}

fn number(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn tiles() -> usize {
    number("TILES", 12.0).max(2.0) as usize
}

fn flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn mode() -> &'static str {
    if flag("BLINC_MOTION_PROMOTION") {
        "slides baked, painted in full"
    } else {
        "slides on rows"
    }
}

fn stylesheet() -> String {
    format!(
        r#"
    .tile-list {{
        flex-direction: column;
        gap: 6px;
        align-items: center;
    }}
    .tile {{
        width: 360px;
        height: 40px;
        border-radius: 10px;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        padding: 0px 14px 0px 14px;
        transition: transform {}ms ease;
    }}
"#,
        number("SLIDE_MS", 600.0) as u32
    )
}

fn tile(id: usize, of: usize) -> Div {
    let hue = id as f32 / of as f32;
    div()
        .id(format!("tile-{id}"))
        .class("tile")
        .bg(Color::rgb(0.2 + 0.5 * hue, 0.4, 0.9 - 0.5 * hue))
        .child(
            text(format!("Tile {id}"))
                .size(15.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE),
        )
        .child(
            text(format!("{:02}", of - id))
                .size(12.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.7)),
        )
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = WindowConfig {
        title: format!("FLIP check: {} tiles, {}", tiles(), mode()),
        width: 480,
        height: 40 + tiles() as u32 * 46 + 60,
        ..Default::default()
    };
    WindowedApp::run(config, build_ui)
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    if ctx.rebuild_count == 0 {
        ctx.add_css(&stylesheet());
    }
    let total = tiles();
    let order: State<Vec<usize>> =
        BlincContextState::get().use_state_keyed("flip_order", || (0..total).collect());

    // The order changes from a tick of the animation scheduler. The window ticks it only while it
    // draws frames, so the tick asks for the next one; it is added once and finds the same state
    // after a rebuild.
    static TICK: Once = Once::new();
    let reorders = order.clone();
    TICK.call_once(move || {
        let period = number("PERIOD", 2.0).max(0.1);
        let exit_after = std::env::var("EXIT_AFTER")
            .ok()
            .and_then(|v| v.parse::<f32>().ok());
        let mut since_reorder = 0.0f32;
        let mut elapsed = 0.0f32;
        blinc_animation::get_scheduler().register_tick_callback(move |dt| {
            blinc_layout::request_animation_tick();
            elapsed += dt;
            since_reorder += dt;
            if exit_after.is_some_and(|limit| elapsed >= limit) {
                std::process::exit(0);
            }
            if since_reorder >= period {
                since_reorder = 0.0;
                reorders.update(|mut order| {
                    order.rotate_left(1);
                    order
                });
            }
        });
    });

    let shown = order.clone();
    let list = stateful_with_key::<PassiveFSM>("flip-tiles")
        .deps([order.signal_id()])
        .on_state(move |_ctx| {
            let tiles: Vec<Div> = shown.get().iter().map(|&id| tile(id, total)).collect();
            div().children(tiles)
        })
        .class("tile-list");

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgb(0.06, 0.07, 0.09))
        .flex_col()
        .items_center()
        .gap(3.0)
        .p(3.0)
        .child(
            text(format!("{total} tiles, {}", mode()))
                .size(16.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        .child(list)
}
