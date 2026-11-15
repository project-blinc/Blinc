#![allow(deprecated, dead_code)]
//! A grid of cards under a CSS keyframe animation of transform, or of plain blocks fading, in a
//! window, for watching what a frame costs.
//!
//! ```text
//! cargo run --release -p blinc-bench --example css_window_check
//! BLINC_MOTION_TRACE=/tmp/css.jsonl cargo run --release -p blinc-bench --example css_window_check
//! BLINC_MOTION_PROMOTION=1 cargo run --release -p blinc-bench --example css_window_check
//! ```
//!
//! - `CARDS`: how many cards (default 140).
//! - `FADE`: `1` animates opacity as well, which keeps the animation off rows.
//! - `BLOCKS`: `1` makes each card a plain filled rectangle (no border, no label) that fades and
//!   does not move, which rides rows.
//! - `TEXT`: `0` leaves the label off the cards (default 1).
//! - `ANIMATE`: `0` leaves the animation off the cards, for an idle baseline (default 1).
//! - `STATIC`: how many labelled cards that never animate to add after the animated ones, as a
//!   static interface behind a few animated nodes (default 0).
//! - `FPS_CAP`: frames a second the animation is held to (default: the display's).
//! - `EXIT_AFTER`: seconds until the process exits; unset keeps the window open.
//!
//! The title says how animated nodes are painted. `BLINC_MOTION_TRACE` records for every frame
//! whether it was drawn and whether it took the fast path.

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::Color;

fn number(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn flag(name: &str, default: bool) -> bool {
    std::env::var(name).map_or(default, |v| v == "1" || v.eq_ignore_ascii_case("true"))
}

fn mode() -> &'static str {
    if flag("BLINC_MOTION_PROMOTION", false) {
        "animations baked"
    } else {
        "animations on rows"
    }
}

fn stylesheet() -> String {
    if flag("BLOCKS", false) {
        return "@keyframes pulse {
            from { opacity: 1; }
            to { opacity: 0.35; }
        }
        .pulse { animation: pulse 500ms ease-in-out infinite alternate; }"
            .into();
    }
    let fade = if flag("FADE", false) {
        " opacity: 0.6;"
    } else {
        ""
    };
    format!(
        "@keyframes pulse {{
            from {{ transform: translateY(0px) scale(1) rotate(0deg); }}
            to {{ transform: translateY(-6px) scale(1.15) rotate(6deg);{fade} }}
        }}
        .pulse {{ animation: pulse 500ms ease-in-out infinite alternate; }}"
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
    if let Some(limit) = std::env::var("EXIT_AFTER")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(limit));
            std::process::exit(0);
        });
    }
    let config = WindowConfig {
        title: format!("CSS check: {} cards, {}", number("CARDS", 140.0), mode()),
        width: 720,
        height: 480,
        animation_fps_cap: std::env::var("FPS_CAP").ok().and_then(|v| v.parse().ok()),
        ..Default::default()
    };
    WindowedApp::run(config, build_ui)
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    if ctx.rebuild_count == 0 {
        ctx.add_css(&stylesheet());
    }
    let blocks = flag("BLOCKS", false);
    let label = flag("TEXT", true) && !blocks;
    let mut grid = div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgb(0.06, 0.07, 0.09))
        .flex_row()
        .flex_wrap()
        .gap(6.0)
        .p(8.0);
    for i in 0..number("CARDS", 140.0) as usize {
        let mut card = div()
            .w(48.0)
            .h(28.0)
            .rounded(6.0)
            .bg(Color::rgb(0.2, 0.45, 0.9))
            .flex_row()
            .items_center()
            .justify_center();
        if flag("ANIMATE", true) {
            card = card.class("pulse");
        }
        if !blocks {
            card = card.border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.9));
        }
        if label {
            card = card.child(text(format!("{i}")).size(12.0).color(Color::WHITE));
        }
        grid = grid.child(div().w(52.0).h(32.0).child(card));
    }
    for i in 0..number("STATIC", 0.0) as usize {
        let card = div()
            .w(48.0)
            .h(28.0)
            .rounded(6.0)
            .bg(Color::rgb(0.3, 0.3, 0.35))
            .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.5))
            .flex_row()
            .items_center()
            .justify_center()
            .child(text(format!("s{i}")).size(12.0).color(Color::WHITE));
        grid = grid.child(div().w(52.0).h(32.0).child(card));
    }
    grid
}
