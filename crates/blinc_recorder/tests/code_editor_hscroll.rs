#![cfg(feature = "headless")]
//! A code editor scrolls sideways: the wheel's sideways delta (or Shift with the wheel) moves
//! the code area while the gutter stays, and moving the caret past the edge brings it back into
//! view.

use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn code_area_x(ui: &Headless) -> f32 {
    let node = ui.find(".blinc-code-area").expect("the code area");
    ui.tree().get_absolute_bounds(node).unwrap().x
}

fn gutter_number_x(ui: &Headless) -> f32 {
    let node = ui.find(".blinc-code-line-number").expect("a line number");
    ui.tree().get_absolute_bounds(node).unwrap().x
}

fn mount(ui: &mut Headless, state: &SharedCodeEditorState) {
    let editor = state.clone();
    ui.mount(move |_| {
        div().w(320.0).h(120.0).child(
            code_editor(&editor)
                .w(320.0)
                .h(120.0)
                .font_size(14.0)
                .line_numbers(true),
        )
    });
    ui.settle();
}

const LONG: &str = "let a = 1;\nlet a_rather_long_name = some_function(with, many, arguments, that, run, past, the, edge);\n";

#[test]
fn the_wheel_scrolls_the_code_area_and_the_gutter_stays() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(320, 120).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let state = code_editor_state(LONG);
    mount(&mut ui, &state);
    // A click gives the editor its bounds and focus.
    ui.move_to(200.0, 20.0).mouse_down().mouse_up();
    ui.settle();
    let (area, gutter) = (code_area_x(&ui), gutter_number_x(&ui));

    ui.move_to(200.0, 40.0);
    ui.scroll(-60.0, 0.0);
    ui.scroll_end();
    ui.settle();
    assert!(
        state.lock().unwrap().scroll_x > 0.0,
        "the sideways wheel did not scroll"
    );
    assert!(code_area_x(&ui) < area, "the code area did not move left");
    assert_eq!(gutter_number_x(&ui), gutter, "the gutter moved");
}

#[test]
fn the_caret_at_the_end_of_a_long_line_is_scrolled_into_view() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(320, 120).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let state = code_editor_state(LONG);
    mount(&mut ui, &state);
    // Click on the long line, then End.
    ui.move_to(120.0, 45.0).mouse_down().mouse_up();
    ui.settle();
    ui.key(35);
    ui.settle();
    let d = state.lock().unwrap();
    assert_eq!(d.cursor.line, 1, "the click did not land on the long line");
    assert!(d.scroll_x > 0.0, "End left the caret past the edge");
}
