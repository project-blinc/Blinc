#![cfg(feature = "headless")]
//! Rounded clips inside rounded clips.

use blinc_core::Color;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);

/// A red panel with corners of 12, at `at` in a 300 px frame with corners of 40, both clipping.
fn frame_at(at: f32) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(300, 300).ok()?;
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    ui.mount(move |_| {
        div()
            .w(300.0)
            .h(300.0)
            .relative()
            .rounded(40.0)
            .overflow_clip()
            .child(
                div()
                    .absolute()
                    .left(at)
                    .top(at)
                    .w(100.0)
                    .h(100.0)
                    .rounded(12.0)
                    .overflow_clip()
                    .child(div().w(100.0).h(100.0).bg(RED)),
            )
    });
    ui.leave().settle();
    Some(ui)
}

fn red(ui: &mut Headless, x: u32, y: u32) -> bool {
    let p = ui.snapshot().unwrap().get_pixel(x, y).unwrap();
    p[0] > 128 && p[1] < 64
}

/// A panel away from the frame's corners keeps its own, though the frame's are larger.
#[test]
fn a_panel_keeps_its_corners_inside_a_rounder_frame() {
    let Some(mut ui) = frame_at(100.0) else {
        return;
    };
    for (x, y) in [(101, 101), (198, 101), (198, 198), (101, 198)] {
        assert!(!red(&mut ui, x, y), "the corner at {x}, {y} is square");
    }
    assert!(red(&mut ui, 150, 101), "the panel's edge is missing");
}

/// A panel in the frame's corner is cut to the frame's larger radius there, and keeps its own on
/// the corners the frame does not reach.
#[test]
fn a_panel_in_the_frames_corner_takes_the_larger_radius() {
    let Some(mut ui) = frame_at(0.0) else {
        return;
    };
    // Inside the panel's own 12 px corner, outside the frame's 40 px one.
    assert!(!red(&mut ui, 8, 8), "the frame does not cut the panel");
    assert!(!red(&mut ui, 98, 98), "the far corner is square");
    assert!(red(&mut ui, 50, 50));
}
