//! Compile guard for the widget playground example.
//!
//! The playground is the one place the whole exposed cn.* surface is
//! exercised together: every widget and its variants across the
//! imported group modules, plus view-body control flow and all three
//! reactive mechanisms in the entry module. A regression in the macro,
//! a lowering pass, a cn builder signature, or module import
//! resolution shows up here.
//!
//! Compiled with `compile_project`, not `compile_source`: imports are
//! only resolved when the whole module graph is walked from an entry
//! path, and that name-mangling step is part of what this covers.

use blinc_dsl_core::BlincDsl;
use std::path::Path;

#[test]
fn cn_playground_compiles() {
    let _ = tracing_subscriber::fmt::try_init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.* widgets");
    let names = dsl
        .compile_project(&root.join("main.blinc"), &root)
        .expect("playground must compile");
    // Each imported module contributes a mangled view fn; the entry
    // contributes `render_view`. Assert the imports actually resolved
    // rather than silently compiling to an empty graph.
    for expected in [
        "forms$FormWidgets$view",
        "feedback$FeedbackWidgets$view",
        "media$MediaWidgets$view",
        "render_view",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing {expected} in compiled names: {names:?}"
        );
    }
}
