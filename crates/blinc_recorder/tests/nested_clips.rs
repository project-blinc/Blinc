#![cfg(feature = "headless")]
//! Rounded clips inside rounded clips, and overflow fades over icons.

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

/// An icon of one solid square, `at` from the left of a 100 px lane with a 24 px fade on its right
/// edge, the lane inside a scroll. The icon's own box clips too, as a button's does.
fn icon_at(at: f32) -> Option<Headless> {
    const SQUARE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><rect width="20" height="20" fill="currentColor"/></svg>"#;
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(200, 100).ok()?;
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    ui.mount(move |_| {
        blinc_layout::widgets::scroll::scroll()
            .w(200.0)
            .h(100.0)
            .child(
                div().w(200.0).h(100.0).child(
                    div()
                        .w(100.0)
                        .h(40.0)
                        .relative()
                        .overflow_clip()
                        .overflow_fade_edges(0.0, 24.0, 0.0, 0.0)
                        .child(
                            div()
                                .absolute()
                                .left(at)
                                .top(10.0)
                                .w(20.0)
                                .h(20.0)
                                .overflow_clip()
                                .child(
                                    blinc_layout::svg::svg(SQUARE)
                                        .size(20.0, 20.0)
                                        .color(Color::WHITE),
                                ),
                        ),
                ),
            )
    });
    ui.leave().settle();
    Some(ui)
}

fn brightness(ui: &mut Headless, x: u32) -> u32 {
    let p = ui.snapshot().unwrap().get_pixel(x, 20).unwrap();
    p[0] as u32 + p[1] as u32 + p[2] as u32
}

/// An icon at a fading edge fades toward it, and one away from it does not.
#[test]
fn an_icon_fades_at_an_overflow_fade() {
    let (Some(mut at_edge), Some(mut away)) = (icon_at(78.0), icon_at(20.0)) else {
        return;
    };
    let (near, far) = (brightness(&mut at_edge, 96), brightness(&mut at_edge, 80));
    assert!(
        near + 150 < far,
        "no fade toward the edge: {far} then {near}"
    );
    let (left, right) = (brightness(&mut away, 22), brightness(&mut away, 38));
    assert!(
        left > 700 && right > 700,
        "an icon away from the edge faded: {left} and {right}"
    );
}
