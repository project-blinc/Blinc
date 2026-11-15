#![allow(deprecated, dead_code)]
//! The cn indeterminate progress bar, in a window over a tree the size of the cn demo's, for
//! watching what a time-only animation costs when the rest of the interface is large.
//!
//! ```text
//! cargo run --release -p blinc-bench --example cn_progress_window
//! BLINC_MOTION_TRACE=/tmp/cn.jsonl EXIT_AFTER=15 cargo run --release -p blinc-bench --example cn_progress_window
//! ```
//!
//! - `NODES`: how many filler cards follow the progress bars, each a handful of nodes (default 600).
//! - `ANIMATE`: `0` shows a static bar instead of the indeterminate one (default 1).
//! - `EXIT_AFTER`: seconds until the process exits; unset keeps the window open.

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_cn::prelude::*;
use blinc_core::Color;
use blinc_theme::{ColorScheme, HybridTheme};

fn number(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn flag(name: &str, default: bool) -> bool {
    std::env::var(name).map_or(default, |v| v == "1" || v.eq_ignore_ascii_case("true"))
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
        title: format!(
            "cn progress: {} nodes of filler, {}",
            number("NODES", 600.0),
            if flag("ANIMATE", true) {
                "indeterminate"
            } else {
                "static"
            }
        ),
        width: 900,
        height: 900,
        ..Default::default()
    };
    WindowedApp::run_with_theme(
        config,
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
        build_ui,
    )
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    let bar = if flag("ANIMATE", true) {
        cn::progress_indeterminate().w(300.0)
    } else {
        cn::progress(35.0).w(300.0)
    };
    let mut page = div()
        .w_full()
        .p(12.0)
        .flex_col()
        .gap(12.0)
        .child(cn::label("Progress"))
        .child(bar);
    for i in 0..number("NODES", 600.0) as usize {
        page = page.child(
            div()
                .w_full()
                .p(8.0)
                .gap(8.0)
                .flex_row()
                .items_center()
                .rounded(8.0)
                .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.15))
                .child(cn::label(format!("Row {i}")))
                .child(cn::button("Open"))
                .child(cn::progress(((i * 7) % 100) as f32).w(120.0)),
        );
    }
    div().w(ctx.width).h(ctx.height).flex_col().child(
        scroll()
            .w_full()
            .h(ctx.height)
            .viewport_cull(true)
            .child(page),
    )
}
