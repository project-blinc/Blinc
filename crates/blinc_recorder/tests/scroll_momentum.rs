#![cfg(feature = "headless")]
//! The momentum after a trackpad scroll stays with the container the fingers scrolled, wherever
//! the pointer goes while it glides.

use blinc_core::Color;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

/// A 400x400 outer scroll whose top half holds a 200 px tall inner scroll on its left.
fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(400, 400).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    ui.mount(|_| {
        div().w(400.0).h(400.0).child(
            div()
                .id("outer")
                .w(400.0)
                .h(400.0)
                .overflow_y_scroll()
                .child(
                    div().w(400.0).h(2000.0).child(
                        div()
                            .id("inner")
                            .w(200.0)
                            .h(200.0)
                            .overflow_y_scroll()
                            .child(div().w(200.0).h(2000.0).bg(Color::BLUE)),
                    ),
                ),
        )
    });
    ui.settle();
    Some(ui)
}

fn offset(ui: &Headless, id: &str) -> f32 {
    let node = ui.find(id).unwrap();
    -ui.tree().get_scroll_offset(node).1
}

#[test]
fn momentum_stays_with_the_inner_scroll_when_the_pointer_leaves_it() {
    let Some(mut ui) = session() else {
        return;
    };
    ui.move_to(100.0, 100.0);
    ui.scroll_start();
    for _ in 0..3 {
        ui.scroll(0.0, -20.0);
    }
    ui.scroll_end();
    let (inner, outer) = (offset(&ui, "#inner"), offset(&ui, "#outer"));
    assert!(
        inner > 0.0 && outer == 0.0,
        "the fingers scrolled the inner: {inner} {outer}"
    );

    // The pointer moves over the outer scroll while the momentum glides on.
    ui.move_to(300.0, 300.0);
    for _ in 0..5 {
        ui.scroll(0.0, -15.0);
    }
    assert!(
        offset(&ui, "#inner") > inner,
        "the momentum moved the inner"
    );
    assert_eq!(offset(&ui, "#outer"), 0.0, "and not the outer");
    ui.scroll_momentum_end();

    // A new gesture over the outer scroll scrolls it.
    ui.scroll_start();
    ui.scroll(0.0, -20.0);
    ui.scroll_end();
    assert!(
        offset(&ui, "#outer") > 0.0,
        "a new gesture scrolls the outer"
    );
}
