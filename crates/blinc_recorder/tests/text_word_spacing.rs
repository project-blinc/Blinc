#![cfg(feature = "headless")]
//! `Text::word_spacing` moves the words apart on screen.

use blinc_core::Color;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{BlincTheme, ColorScheme};

const W: u32 = 320;
const H: u32 = 60;
const SPACING: f32 = 20.0;

/// The column of the rightmost dark pixel of "a b c d" drawn with `spacing`.
fn right_edge(spacing: f32) -> Option<u32> {
    let mut ui = Headless::new(W, H).ok()?;
    ui.theme(BlincTheme::bundle(), ColorScheme::Light);
    ui.mount(move |_| {
        div().w(W as f32).h(H as f32).p(2.0).bg(Color::WHITE).child(
            text("a b c d")
                .size(20.0)
                .color(Color::BLACK)
                .no_wrap()
                .word_spacing(spacing),
        )
    });
    ui.settle();
    let frame = ui.snapshot().unwrap();
    (0..W)
        .rev()
        .find(|&x| (0..H).any(|y| frame.get_pixel(x, y).unwrap()[0] < 128))
}

#[test]
fn the_words_are_drawn_apart() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let plain = right_edge(0.0).expect("the text did not draw");
    let spaced = right_edge(SPACING).expect("the spaced text did not draw");
    let moved = spaced as f32 - plain as f32;
    assert!(
        (moved - 3.0 * SPACING).abs() <= 1.5,
        "the last word moved {moved}"
    );
}
