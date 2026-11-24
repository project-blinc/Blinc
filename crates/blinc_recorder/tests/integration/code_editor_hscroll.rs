#![cfg(feature = "headless")]
//! A code editor's code area scrolls sideways in its own scroll container: the sideways wheel
//! moves it while the gutter stays, the vertical wheel still scrolls the lines, and moving the
//! caret past the edge brings it back into view.

use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

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

fn source() -> String {
    let mut text = String::from(
        "let a = 1;\nlet a_rather_long_name = some_function(with, many, arguments, that, run, past, the, edge);\n",
    );
    for i in 0..30 {
        text.push_str(&format!("let v{i} = {i};\n"));
    }
    text
}

fn offsets(state: &SharedCodeEditorState) -> (f32, f32) {
    let d = state.lock().unwrap();
    let x = d.h_scroll.lock().unwrap().offset_x;
    let y = d.scroll_physics.lock().unwrap().offset_y;
    (x, y)
}

#[test]
fn the_wheel_scrolls_each_axis_and_the_gutter_stays() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(320, 120).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let state = code_editor_state(source());
    mount(&mut ui, &state);
    let gutter = gutter_number_x(&ui);

    ui.move_to(200.0, 40.0);
    ui.scroll(-60.0, 0.0);
    ui.scroll_end();
    ui.advance(300).settle();
    let (x, y) = offsets(&state);
    assert!(
        x < 0.0,
        "the sideways wheel did not scroll the code area: {x}"
    );
    assert_eq!(y, 0.0, "the sideways wheel scrolled the lines");
    assert_eq!(gutter_number_x(&ui), gutter, "the gutter moved");

    ui.move_to(200.0, 40.0);
    ui.scroll(0.0, -60.0);
    ui.scroll_end();
    ui.advance(300).settle();
    let (_, y) = offsets(&state);
    assert!(
        y < 0.0,
        "the vertical wheel over the code did not scroll the lines: {y}"
    );
}

#[test]
fn the_caret_at_the_end_of_a_long_line_is_scrolled_into_view() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(320, 120).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let state = code_editor_state(source());
    mount(&mut ui, &state);
    // Click on the long line, then End.
    ui.move_to(120.0, 45.0).mouse_down().mouse_up();
    ui.settle();
    ui.key(35);
    ui.settle();
    assert_eq!(
        state.lock().unwrap().cursor.line,
        1,
        "the click missed the long line"
    );
    let (x, _) = offsets(&state);
    assert!(x < 0.0, "End left the caret past the edge: {x}");
}
