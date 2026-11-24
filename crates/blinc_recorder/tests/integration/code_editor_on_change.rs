#![cfg(feature = "headless")]
//! A code editor's `on_change` runs once the editor is unlocked, so it can read the editor's
//! state, as a handler that recompiles the text does.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

#[test]
fn on_change_can_read_the_editor() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    // A deadlock would hang the run; end it with a message instead.
    let finished = Arc::new(AtomicBool::new(false));
    let watching = Arc::clone(&finished);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(30));
        if !watching.load(Ordering::SeqCst) {
            eprintln!("on_change deadlocked against the editor's lock");
            std::process::exit(1);
        }
    });

    let mut ui = Headless::new(400, 120).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let state = code_editor_state("let a = 1;\n");
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let (editor, reader, log) = (state.clone(), state.clone(), Arc::clone(&seen));
    ui.mount(move |_| {
        let reader = reader.clone();
        let log = Arc::clone(&log);
        div().w(400.0).h(120.0).child(
            code_editor(&editor)
                .w(400.0)
                .h(120.0)
                .on_change(move |_| log.lock().unwrap().push(reader.lock().unwrap().value())),
        )
    });
    ui.settle();
    ui.move_to(200.0, 20.0).mouse_down().mouse_up();
    ui.settle();
    ui.type_text("x");
    ui.key(8);
    ui.settle();
    finished.store(true, Ordering::SeqCst);

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "one change per key: {seen:?}");
    assert!(seen[0].contains('x'), "the typed character: {seen:?}");
    assert_eq!(seen[1], state.lock().unwrap().value());
}
