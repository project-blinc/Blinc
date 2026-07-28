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
    // The watcher only raises a flag once the save has settled.
    // `BlincDsl` owns JIT function pointers and is not `Send`, so the
    // recompile happens where the view is built: on the main thread, at
    // the top of the UI builder, before anything reads the program.
    #[cfg(feature = "hot-reload")]
    let _watch = blinc_app::hot_reload::watch_sources(
        &root,
        &["blinc"],
        std::time::Duration::from_millis(120),
    );

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
            if blinc_app::hot_reload::take_sources_dirty() {
                match build(&root) {
                    Ok(fresh) => {
                        *dsl.borrow_mut() = fresh;
                        tracing::info!("hot-reload: reloaded");
                    }
                    // Source is unparseable for most of the time it is
                    // being typed. The running program has to survive
                    // that, so a failed compile is discarded and the
                    // window keeps rendering what it has.
                    Err(e) => {
                        tracing::warn!(error = %e, "hot-reload: keeping the running program")
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
