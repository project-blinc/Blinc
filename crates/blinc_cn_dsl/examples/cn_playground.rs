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

    // Everything a fresh instance needs: the extern widgets it cannot
    // know about, then the sources. A reload runs exactly this again.
    let build = |root: &std::path::Path| -> blinc_dsl_core::BlincDslResult<BlincDsl> {
        BlincDsl::reload_project(&root.join("main.blinc"), root, |dsl| {
            blinc_cn_dsl::register_all(dsl)
        })
    };
    let dsl = std::cell::RefCell::new(build(&root).expect("compile"));

    // Edit any `.blinc` file under `examples/playground` and the window
    // follows, with signal values intact -- the registry is keyed by
    // name and a declared default only applies on first mint.
    //
    // `cargo run -p blinc_cn_dsl --example cn_playground --features hot-reload`
    //
    // The watcher thread only raises a flag. `BlincDsl` owns JIT
    // function pointers and is not `Send`, so the rebuild happens where
    // the view is built: on the main thread, at the top of the UI
    // builder, before anything reads the program.
    #[cfg(feature = "hot-reload")]
    static SOURCES_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    /// Failed reload attempts since the last good one, so a half-written
    /// file gets another look while broken source does not spin.
    #[cfg(feature = "hot-reload")]
    static RETRIES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    /// Wall-clock millis of the most recent file event, for coalescing.
    #[cfg(feature = "hot-reload")]
    static LAST_EVENT_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    #[cfg(feature = "hot-reload")]
    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
    /// How long the file events have to go quiet before compiling.
    ///
    /// One save is several events -- editors write, truncate, rename,
    /// touch metadata -- and each one that compiles is a chance to read
    /// the file mid-rewrite. That matters more than it looks: the
    /// component and signal registries are process-global, so a compile
    /// of a half-written file can leave them holding its remains even
    /// though the failed reload is discarded and the running instance
    /// kept. The symptom is a correct frame that reverts a moment later.
    #[cfg(feature = "hot-reload")]
    const SETTLE_MS: u64 = 120;
    #[cfg(feature = "hot-reload")]
    let _watch = blinc_app::hot_reload::watch_dir_with(&root, |paths| {
        if paths
            .iter()
            .any(|p| p.extension().is_some_and(|e| e == "blinc"))
        {
            SOURCES_DIRTY.store(true, std::sync::atomic::Ordering::Release);
            LAST_EVENT_MS.store(now_ms(), std::sync::atomic::Ordering::Release);
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
            if SOURCES_DIRTY.load(std::sync::atomic::Ordering::Acquire) {
                let quiet_for = now_ms()
                    .saturating_sub(LAST_EVENT_MS.load(std::sync::atomic::Ordering::Acquire));
                if quiet_for < SETTLE_MS {
                    // Still mid-burst. Come back next frame rather than
                    // compiling a file the editor is still writing.
                    blinc_app::hot_reload::request_rebuild();
                } else {
                    SOURCES_DIRTY.store(false, std::sync::atomic::Ordering::Release);
                    match build(&root) {
                        Ok(fresh) => {
                            *dsl.borrow_mut() = fresh;
                            RETRIES.store(0, std::sync::atomic::Ordering::Release);
                            tracing::info!("hot-reload: reloaded");
                        }
                        Err(e) => {
                            // Two different failures look identical here.
                            // Source is unparseable for most of the time it
                            // is being typed, and the running program has to
                            // survive that. But `notify` also fires while an
                            // editor is still writing, so the same error can
                            // mean "read a half-written file" -- and giving
                            // up on that one left the window showing the old
                            // text until the file was saved a second time.
                            // A few retries cover the partial write without
                            // spinning on genuinely broken source.
                            let n = RETRIES.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                            if n < 3 {
                                SOURCES_DIRTY.store(true, std::sync::atomic::Ordering::Release);
                                blinc_app::hot_reload::request_rebuild();
                            } else {
                                tracing::warn!(error = %e, "hot-reload: keeping the running program");
                            }
                        }
                    }
                }
            }
            div()
                .w(ctx.width)
                .h(ctx.height)
                .bg(ThemeState::get().colors().background)
                .child_box(dsl.borrow().view_widget())
        },
    )
}
