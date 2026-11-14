//! Benchmark scenes for a phone: an iOS static library that runs the scene `BLINC_BENCH` names
//! (`cards_plain` when it is unset). It reuses the Swift host of `mobile/example`, which links
//! `libexample.a`, so the library is built and copied in under that name:
//!
//! ```text
//! cd tools/blinc-bench/mobile
//! cargo build --lib --release --target aarch64-apple-ios
//! cp target/aarch64-apple-ios/release/libblinc_bench_mobile.a \
//!    ../../../mobile/example/platforms/ios/libs/device/libexample.a
//! ```
//!
//! then build and install the app from `mobile/example/platforms/ios` as usual. Rebuild the
//! example's own library to get the example back. The environment is passed at launch, with
//! `xcrun devicectl device process launch --environment-variables '{"BLINC_BENCH":"cards"}'`:
//!
//! - `BLINC_BENCH`: the scene, see [`scenes`].
//! - `BLINC_BENCH_STATS`: print frame statistics once per window of frames.
//! - `BLINC_BENCH_SCROLL`: feed a scripted drag and fling as touches (for `list`).
//!
//! The statistics and the scripted touches are the runner's, in `blinc_app`.
#![cfg(target_os = "ios")]

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_cn::cn_bundle;
use blinc_core::Color;
use blinc_theme::{ThemeState, detect_system_color_scheme};
use std::sync::Once;

pub mod scenes;

/// iOS initialization, called from Swift at launch before the context is created.
#[unsafe(no_mangle)]
pub extern "C" fn ios_app_init() {
    // Warnings only: debug lines per event would change the frames that are measured.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .with_writer(std::io::stderr)
        .try_init();

    // The theme goes in before the first frame so its stylesheet is drained with the first build.
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        ThemeState::init(
            cn_bundle().with_css(scenes::CSS),
            detect_system_color_scheme(),
        )
    });
    blinc_app::ios::register_rust_ui_builder(|ctx| ui(ctx));
}

fn ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    let name = std::env::var("BLINC_BENCH").unwrap_or_else(|_| "cards_plain".into());
    scenes::scene(&name, ctx).unwrap_or_else(|| {
        div()
            .w(ctx.width)
            .h(ctx.height)
            .pt(ctx.safe_top())
            .child(text(format!("BLINC_BENCH={name}: no such scene")).color(Color::WHITE))
    })
}
