#![cfg(feature = "headless")]
//! Dragging across one line selects as the pointer moves: the selection's highlight follows
//! each frame, as it does across lines.

use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn selection_width(ui: &Headless) -> Option<f32> {
    let node = ui.find(".blinc-code-selection")?;
    ui.tree().get_absolute_bounds(node).map(|b| b.width)
}

#[test]
fn a_drag_along_one_line_highlights_each_frame() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(480, 160).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let state =
        code_editor_state("let a_rather_long_name = some_function(with, many);\nlet b = 2;\n");
    let editor = state.clone();
    ui.mount(move |_| {
        div().w(480.0).h(160.0).child(
            code_editor(&editor)
                .w(480.0)
                .h(160.0)
                .font_size(14.0)
                .line_numbers(true),
        )
    });
    ui.settle();

    let y = 20.0;
    ui.move_to(70.0, y).mouse_down().mouse_up();
    ui.frame(16);
    ui.move_to(70.0, y).mouse_down();
    let mut widths = Vec::new();
    for step in 1..=8 {
        ui.move_to(70.0 + step as f32 * 20.0, y);
        ui.frame(16);
        widths.push(selection_width(&ui));
    }
    ui.mouse_up();
    let shown: Vec<f32> = widths.iter().map(|w| w.unwrap_or(0.0)).collect();
    assert!(
        shown.windows(2).skip(1).all(|w| w[1] > w[0]),
        "the highlight did not follow the pointer frame by frame: {shown:?}"
    );
}
