#![cfg(feature = "headless")]
//! A code block's lines scroll sideways in their own scroll container, the gutter beside them
//! staying put, inside a vertical scroll that still takes the vertical wheel.

use blinc_core::reactive::signal;
use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn x_of(ui: &Headless, selector: &str) -> f32 {
    let node = ui.find(selector).expect(selector);
    ui.tree().get_absolute_bounds(node).unwrap().x
}

fn y_of(ui: &Headless, selector: &str) -> f32 {
    let node = ui.find(selector).expect(selector);
    ui.tree().get_absolute_bounds(node).unwrap().y
}

fn source() -> String {
    let mut text = String::from(
        "let a_rather_long_name = some_function(with, many, arguments, that, run, past, the, edge);\n",
    );
    for i in 0..30 {
        text.push_str(&format!("let v{i} = {i};\n"));
    }
    text
}

fn check(bound: bool) {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(320, 120).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let text = signal(source());
    ui.mount(move |_| {
        let code = if bound {
            Code::bound(text)
        } else {
            Code::new(source())
        };
        div().w(320.0).h(120.0).child(
            scroll()
                .w_full()
                .h_full()
                .direction(ScrollDirection::Vertical)
                .child(code.font_size(14.0).line_numbers(true).w_full()),
        )
    });
    ui.settle();
    let (gutter, area) = (
        x_of(&ui, ".blinc-code-line-number"),
        x_of(&ui, ".blinc-code-area"),
    );

    ui.move_to(200.0, 40.0);
    ui.scroll(-60.0, 0.0);
    ui.scroll_end();
    ui.advance(300).settle();
    assert!(
        x_of(&ui, ".blinc-code-area") < area,
        "the sideways wheel did not scroll the lines"
    );
    assert_eq!(
        x_of(&ui, ".blinc-code-line-number"),
        gutter,
        "the gutter moved"
    );

    let top = y_of(&ui, ".blinc-code-area");
    ui.move_to(200.0, 40.0);
    ui.scroll(0.0, -60.0);
    ui.scroll_end();
    ui.advance(300).settle();
    assert!(
        y_of(&ui, ".blinc-code-area") < top,
        "the vertical wheel over the lines did not scroll the block"
    );
}

#[test]
fn a_block_scrolls_sideways_and_its_gutter_stays() {
    check(false);
}

#[test]
fn a_bound_block_scrolls_sideways_and_its_gutter_stays() {
    check(true);
}
