//! Benchmark scenes for a phone: a library that runs the scene `BLINC_BENCH` names
//! (`cards_plain` when it is unset). It reuses the native host of `mobile/example`, which loads
//! `libexample`, so the library is built and copied in under that name.
//!
//! iOS (a static library for the Swift host):
//!
//! ```text
//! cd tools/blinc-bench/mobile
//! cargo build --lib --release --target aarch64-apple-ios
//! cp target/aarch64-apple-ios/release/libblinc_bench_mobile.a \
//!    ../../../mobile/example/platforms/ios/libs/device/libexample.a
//! ```
//!
//! then build and install the app from `mobile/example/platforms/ios` as usual. The environment
//! is passed at launch, with
//! `xcrun devicectl device process launch --environment-variables '{"BLINC_BENCH":"cards"}'`.
//!
//! Android (a shared library for the NativeActivity host). The cdylib is built with `rustc`
//! so the iOS build does not try to link one:
//!
//! ```text
//! cd tools/blinc-bench/mobile
//! cargo ndk -t arm64-v8a rustc --lib --release --crate-type cdylib
//! cp target/aarch64-linux-android/release/libblinc_bench_mobile.so \
//!    ../../../mobile/example/platforms/android/app/src/main/jniLibs/arm64-v8a/libexample.so
//! ```
//!
//! then assemble and install the app with `gradlew assembleDebug` as the example does. An
//! Android launch carries no environment, so a parameter is read from the system property
//! `debug.` plus its lowercase name (`adb shell setprop debug.blinc_bench cards`).
//! `android_ab.py` installs one or more APKs in turn, sets the properties and prints fps, CPU
//! and memory for each without capturing the screen.
//!
//! - `BLINC_BENCH`: the scene, see [`scenes`].
//! - `BLINC_BENCH_CARDS`: how many cards the card scenes lay out.
//! - `BLINC_BENCH_LOG`: `all` keeps the Android runner's own, unfiltered log (warnings only
//!   otherwise).
//! - `BLINC_BENCH_STATS`: print frame statistics once per window of frames (iOS runner).
//! - `BLINC_BENCH_SCROLL`: feed a scripted drag and fling as touches (iOS runner, for `list`).
//!
//! Rebuild the example's own library to get the example back.
#![cfg(any(target_os = "ios", target_os = "android"))]

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_cn::cn_bundle;
use blinc_core::Color;
use blinc_theme::{ThemeState, detect_system_color_scheme};
use std::sync::Once;

pub mod scenes;

/// The value of a bench parameter: the environment variable, or on Android the system property
/// `debug.` plus its lowercase name.
pub fn param(key: &str) -> Option<String> {
    if let Ok(v) = std::env::var(key) {
        return Some(v);
    }
    #[cfg(target_os = "android")]
    {
        use std::ffi::{CStr, CString, c_char, c_int};
        unsafe extern "C" {
            fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
        }
        let name = CString::new(format!("debug.{}", key.to_lowercase())).ok()?;
        // PROP_VALUE_MAX is 92 including the terminator.
        let mut buf = [0 as c_char; 92];
        let len = unsafe { __system_property_get(name.as_ptr(), buf.as_mut_ptr()) };
        if len > 0 {
            let v = unsafe { CStr::from_ptr(buf.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            return Some(v);
        }
    }
    None
}

/// The cn theme with the bench stylesheet, set before the first frame so it is drained with the
/// first build.
fn install_theme() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        ThemeState::init(
            cn_bundle().with_css(scenes::CSS),
            detect_system_color_scheme(),
        )
    });
}

/// iOS initialization, called from Swift at launch before the context is created.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn ios_app_init() {
    // Warnings only: debug lines per event would change the frames that are measured.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("warn"))
        .with_writer(std::io::stderr)
        .try_init();

    install_theme();
    blinc_app::ios::register_rust_ui_builder(|ctx| ui(ctx));
}

/// NativeActivity entry. The Kotlin activity has already registered the bridge defaults.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: android_activity::AndroidApp) {
    // Warnings only, installed before the runner's own unfiltered subscriber: a line per frame
    // would change the frames that are measured. `BLINC_BENCH_LOG=all` leaves the runner's own.
    if param("BLINC_BENCH_LOG").as_deref() != Some("all") {
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::filter::LevelFilter::WARN)
            .with(tracing_android::layer("Blinc").expect("logcat layer"));
        let _ = tracing::subscriber::set_global_default(subscriber);
    }
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Warn)
            .with_tag("bench"),
    );
    if let Err(e) = blinc_platform_android::init_android_native_bridge(&app) {
        log::warn!("native bridge: {e}");
    }
    install_theme();
    blinc_app::AndroidApp::run(app, |ctx| ui(ctx)).expect("run");
}

fn ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    let name = param("BLINC_BENCH").unwrap_or_else(|| "cards_plain".into());
    #[cfg(target_os = "android")]
    tracing::warn!(
        "bench scene {name}, cards {:?}",
        param("BLINC_BENCH_CARDS").unwrap_or_default()
    );
    scenes::scene(&name, ctx).unwrap_or_else(|| {
        div()
            .w(ctx.width)
            .h(ctx.height)
            .pt(ctx.safe_top())
            .child(text(format!("BLINC_BENCH={name}: no such scene")).color(Color::WHITE))
    })
}
