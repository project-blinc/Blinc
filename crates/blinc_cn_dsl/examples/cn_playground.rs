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
    let dsl = BlincDsl::new().expect("BlincDsl::new");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.* widgets");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile");

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
            div()
                .w(ctx.width)
                .h(ctx.height)
                .bg(Color::rgb(0.04, 0.06, 0.1))
                // Not `justify_center`: centring a child taller than the
                // viewport pins it around the midpoint, so the top and
                // bottom are unreachable and the wheel has nowhere to go.
                // Content flows from the top and this root is the single
                // scroll container.
                .justify_start()
                .overflow_y_scroll()
                .child_box(dsl.view_widget())
        },
    )
}
