#![cfg(feature = "headless")]
//! What a widget creates while the UI is built goes when a fresh tree replaces the one it built:
//! a window resized twenty times does not keep twenty effects for each bound widget.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use blinc_core::reactive::{effect, signal};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

#[test]
fn a_fresh_tree_disposes_the_effects_the_builds_before_it_made() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(100, 100).expect("headless");
    ui.theme(HybridTheme::bundle(), ColorScheme::Light);
    let source = signal(0_i32);
    let runs = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&runs);
    // A widget that makes an effect while it builds, as number_input and the OTP input do.
    ui.mount(move |_| {
        let counted = Arc::clone(&counted);
        effect(move |g| {
            let _ = g.get(source);
            counted.fetch_add(1, Ordering::SeqCst);
        });
        div().w(100.0).h(100.0)
    });
    for _ in 0..5 {
        ui.rebuild_fresh();
    }

    let before = runs.load(Ordering::SeqCst);
    source.set(1);
    assert_eq!(
        runs.load(Ordering::SeqCst) - before,
        1,
        "one effect, the last build's, and not one per build"
    );
}
