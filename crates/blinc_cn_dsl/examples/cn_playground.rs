//! `cargo run -p blinc_cn_dsl --example cn_playground`
//!
//! Playground for the DSL widget surface: every exposed cn.* widget and
//! its variants, plus view-body control flow and all three reactive
//! mechanisms. The static gallery is split into one module per widget
//! group under `examples/playground/`, imported by the entry file, so
//! it stays small as widgets land.
//!
//! Reactive behaviour, shown side by side so a regression in one is
//! visible against the others:
//!
//! * **Grow** steps the numeric context. The progress bar fills, the
//!   skeleton's width / height / radius grow, and the separator fades
//!   — all in-place property writes, no rebuild. The two derived bars
//!   track the same signals through `computed { }`, one of them
//!   combining two of them.
//! * **Busy** flips the button's label and disabled state together.
//!   Those take the `deps()` rebuild path: text content has no
//!   property writer, and `disabled` gates the whole style branch.
//! * **Reset** returns every signal to its start value.
//!
//! The f64 props also cover the numeric-width path — the DSL types
//! every number as f64 while layout stores f32, and the narrowing
//! happens inside the binding layer rather than at the call site.

#![cfg(not(target_arch = "wasm32"))]

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedApp;
use blinc_cn::cn_styles::CN_STYLES;
use blinc_dsl_core::BlincDsl;
use blinc_theme::themes::universal::hybrid;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "warn,blinc_runtime::fsm=debug,blinc_dsl_core=debug,blinc_cn_dsl=debug",
                )
            }),
        )
        .init();

    // `compile_project`, not `compile_source`: the gallery modules are
    // only resolved by walking the import graph from the entry file.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = std::sync::Arc::new(BlincDsl::new().expect("BlincDsl::new"));
    blinc_cn_dsl::register_all(&dsl).expect("register cn.* widgets");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile");

    // Edit any `.blinc` file under `examples/playground` and the window
    // follows, with signal values intact -- the registry is keyed by
    // name and a declared default only applies on first mint.
    //
    // `cargo run -p blinc_cn_dsl --example cn_playground --features hot-reload`
    //
    // The watcher thread only raises a flag. `BlincDsl` owns JIT
    // function pointers and is not `Send`, so the recompile itself has
    // to happen where the view is built: on the main thread, at the top
    // of the UI builder, before anything reads the program.
    #[cfg(feature = "hot-reload")]
    static SOURCES_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    #[cfg(feature = "hot-reload")]
    let _watch = blinc_app::hot_reload::watch_dir_with(&root, |paths| {
        if paths
            .iter()
            .any(|p| p.extension().is_some_and(|e| e == "blinc"))
        {
            SOURCES_DIRTY.store(true, std::sync::atomic::Ordering::Release);
        }
    });

    WindowedApp::run_with_theme(
        WindowConfig {
            title: "Blinc DSL — reactive cn.* props".to_string(),
            width: 720,
            height: 820,
            resizable: true,
            ..Default::default()
        },
        hybrid::HybridTheme::bundle().with_css(CN_STYLES),
        blinc_theme::ColorScheme::Dark,
        move |ctx| {
            #[cfg(feature = "hot-reload")]
            if SOURCES_DIRTY.swap(false, std::sync::atomic::Ordering::AcqRel) {
                match dsl.recompile_project(&root.join("main.blinc"), &root) {
                    Ok(_) => tracing::info!("hot-reload: recompiled"),
                    // Keep the previous program: source is unparseable
                    // for most of the time it is being typed.
                    Err(e) => {
                        tracing::warn!(error = %e, "hot-reload: keeping the previous program")
                    }
                }
            }
            div()
                .w(ctx.width)
                .h(ctx.height)
                .bg(ThemeState::get().colors().background)
                .child_box(dsl.view_widget())
        },
    )
}
