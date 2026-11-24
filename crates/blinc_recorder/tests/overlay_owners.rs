#![cfg(feature = "headless")]
//! What an overlay's content creates goes with it: opening and closing a dialog again and
//! again leaves only the open one's effects running.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use blinc_cn::cn;
use blinc_core::reactive::{effect, signal};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

#[test]
fn closed_dialogs_leave_no_effects_running() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(480, 360).unwrap();
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.mount(|_| div().w(480.0).h(360.0));
    ui.leave().settle();

    let source = signal(0_i32);
    let runs = Arc::new(AtomicUsize::new(0));
    let live = |ui: &mut Headless| {
        let before = runs.load(Ordering::SeqCst);
        source.update(|v| v + 1);
        ui.settle();
        runs.load(Ordering::SeqCst) - before
    };
    let open = |ui: &mut Headless| {
        let counted = Arc::clone(&runs);
        let handle = cn::dialog()
            .title("Counting")
            .content(move || {
                let counted = Arc::clone(&counted);
                effect(move |g| {
                    let _ = g.get(source);
                    counted.fetch_add(1, Ordering::SeqCst);
                });
                div()
            })
            .show();
        ui.settle();
        handle
    };

    for _ in 0..5 {
        let handle = open(&mut ui);
        handle.close();
        ui.settle();
    }
    let handle = open(&mut ui);
    let running = live(&mut ui);
    assert!(
        running >= 1 && running <= 2,
        "{running} effects run for one open dialog after five opened and closed"
    );
    handle.close();
    ui.settle();
    assert_eq!(live(&mut ui), 0, "the closed dialog's effects still run");
}
