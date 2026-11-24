//! What is hovered follows the content when it moves under a pointer that does not: a list that
//! scrolls, or a layout that changes, as well as a pointer that moves.

use blinc_core::Color;
use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_layout::selector::ScrollRef;
use blinc_recorder::{Headless, MotionPaint};
use blinc_theme::{ColorScheme, HybridTheme};

const ROW: f32 = 30.0;

fn list(rows: usize, reference: &ScrollRef) -> Option<Headless> {
    if !Headless::available() {
        return None;
    }
    let mut ui = Headless::with_motion_paint(300, 200, MotionPaint::Vector).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(".row:hover { background: #ffffff55; }"),
        ColorScheme::Dark,
    );
    ui.gate_state_styles();
    let bound = reference.clone();
    ui.mount(move |_| {
        let mut list = div().flex_col().w(300.0);
        for i in 0..rows {
            list = list.child(
                div()
                    .id(format!("row-{i}"))
                    .class("row")
                    .w_full()
                    .h(ROW)
                    .bg(Color::rgb(0.2, 0.3, 0.6)),
            );
        }
        div()
            .w(300.0)
            .h(200.0)
            .child(scroll().bind(&bound).w(300.0).h(200.0).child(list))
    });
    ui.settle();
    Some(ui)
}

#[test]
fn a_list_scrolled_under_the_pointer_hovers_the_row_that_arrives() {
    let reference = ScrollRef::new();
    let Some(mut ui) = list(300, &reference) else {
        return;
    };
    ui.move_to(60.0, 15.0);
    ui.frame(16);
    assert!(ui.is_hovered("#row-0"), "the first row is not hovered");

    // Three rows of scroll, the pointer where it was.
    reference.set_scroll_offset(0.0, ROW * 3.0);
    ui.frame(16);
    assert!(
        ui.is_hovered("#row-3"),
        "the row that came under the pointer is not hovered"
    );
    assert!(
        !ui.is_hovered("#row-0"),
        "the row that left is still hovered"
    );

    // And again, a long way: a rows container, a layout epoch that has not moved.
    reference.set_scroll_offset(0.0, ROW * 200.0);
    ui.frame(16);
    assert!(ui.is_hovered("#row-200"));
    assert!(!ui.is_hovered("#row-3"));
}

#[test]
fn a_pointer_outside_the_window_hovers_nothing_as_content_moves() {
    let reference = ScrollRef::new();
    let Some(mut ui) = list(300, &reference) else {
        return;
    };
    ui.move_to(60.0, 15.0);
    ui.frame(16);
    ui.leave();
    ui.frame(16);
    reference.set_scroll_offset(0.0, ROW * 3.0);
    ui.frame(16);
    assert!(
        !ui.is_hovered("#row-3") && !ui.is_hovered("#row-0"),
        "something is hovered with the pointer away"
    );
}
