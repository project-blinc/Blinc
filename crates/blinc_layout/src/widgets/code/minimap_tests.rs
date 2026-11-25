//! Where the minimap puts its bars and slider, and where a press on it scrolls to.

use blinc_core::Color;

use super::{Bar, bars, geometry, offset_for};

const LINE_H: f32 = 19.5;
const PAD: f32 = 8.0;

fn content(lines: usize) -> f32 {
    lines as f32 * LINE_H + 2.0 * PAD
}

#[test]
fn the_slider_is_at_the_top_unscrolled_and_at_the_bottom_scrolled_to_the_end() {
    let (lines, viewport) = (200, 300.0);
    let top = geometry(lines, viewport, viewport, content(lines), 0.0);
    assert_eq!(top.slider_top, 4.0);
    let end = content(lines) - viewport;
    let bottom = geometry(lines, viewport, viewport, content(lines), end);
    assert!(
        (bottom.slider_top + bottom.slider_h - (4.0 + bottom.lines_h)).abs() < 1e-3,
        "{bottom:?}"
    );
    // 200 lines at 2 px do not fit in 300 px less padding, so they share the height.
    assert!(bottom.line_h < 2.0 && (bottom.lines_h - (viewport - 8.0)).abs() < 1e-3);
}

#[test]
fn the_slider_covers_the_share_of_the_text_in_view() {
    let (lines, viewport) = (40, 300.0);
    let g = geometry(lines, viewport, viewport, content(lines), 0.0);
    assert_eq!(g.line_h, 2.0);
    let share = viewport / content(lines);
    assert!((g.slider_h - g.lines_h * share).abs() < 1e-3, "{g:?}");
}

#[test]
fn text_that_fits_the_editor_has_no_slider() {
    let g = geometry(5, 300.0, 300.0, content(5), 50.0);
    assert_eq!(g.slider_top, 4.0);
    assert_eq!(g.slider_h, 0.0);
}

#[test]
fn a_press_scrolls_its_line_to_the_middle_within_the_text() {
    let (lines, viewport) = (200, 300.0);
    let max = content(lines) - viewport;
    let at = |y| offset_for(y, lines, viewport, viewport, content(lines), LINE_H, PAD);
    assert_eq!(at(0.0), 0.0);
    assert_eq!(at(viewport), max);
    let middle = at(viewport / 2.0);
    assert!(middle > 0.0 && middle < max, "{middle}");
    let g = geometry(lines, viewport, viewport, content(lines), 0.0);
    let line = (viewport / 2.0 - 4.0) / g.line_h;
    let expected = line * LINE_H + PAD + LINE_H / 2.0 - viewport / 2.0;
    assert!((middle - expected).abs() < 1e-2, "{middle} {expected}");
}

#[test]
fn lines_thinner_than_a_pixel_share_one_bar_as_wide_as_the_widest() {
    let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
    let blue = Color::rgba(0.0, 0.0, 1.0, 1.0);
    let lines = [(0.2, red), (0.9, blue), (0.5, blue), (0.1, red)];
    let out = bars(&lines, 0.5);
    assert_eq!(
        out,
        vec![
            Bar {
                top: 4.0,
                h: 1.0,
                width: 0.9,
                color: red
            },
            Bar {
                top: 5.0,
                h: 1.0,
                width: 0.5,
                color: blue
            },
        ]
    );
    let whole = bars(&lines, 2.0);
    assert_eq!(whole.len(), 4);
    assert_eq!(whole[3].top, 10.0);
}
