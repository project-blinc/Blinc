//! Measured width must count letter-spacing the way the renderer draws
//! it.
//!
//! `blinc_text`'s layout adds `letter_spacing` to EVERY glyph's advance,
//! the last one included, which is what CSS does and why letter-spaced
//! text looks shifted when centred. A measurer that counts the gaps
//! between characters instead, `(n - 1)`, makes every box one spacing
//! narrower than the line drawn in it, which puts centred text half a
//! spacing off.

#![cfg(feature = "text_measurer")]

use blinc_layout::text_measure::{TextLayoutOptions, measure_text_with_options};

const SPACING: f32 = 10.0;

fn width(text: &str, spacing: f32) -> f32 {
    let mut o = TextLayoutOptions::new();
    o.letter_spacing = spacing;
    measure_text_with_options(text, 16.0, &o).width
}

/// With a real face installed, measurement goes through the same layout
/// engine that draws, so it already counts every character. This pins
/// that, because it is what the estimator has to agree with.
#[test]
fn the_font_measurer_counts_every_character() {
    blinc_layout::text_measurer::init_text_measurer();

    for text in ["A", "AB", "ABCD"] {
        let n = text.chars().count() as f32;
        let delta = width(text, SPACING) - width(text, 0.0);
        let counted = delta / SPACING;
        assert!(
            (counted - n).abs() < 0.01,
            "{text:?}: counted {counted} spacings, expected {n} (n-1 would be {})",
            n - 1.0
        );
    }
}

/// A single character still gets its trailing spacing. This is the case
/// that showed up as a visibly off-centre one-glyph toggle.
#[test]
fn one_character_still_gets_its_spacing() {
    blinc_layout::text_measurer::init_text_measurer();

    let delta = width("B", SPACING) - width("B", 0.0);
    assert!(
        (delta - SPACING).abs() < 0.01,
        "one character measured {delta} of spacing, expected {SPACING}"
    );
}

/// The estimator, used when no face is available, has to count the same
/// way. These tests deliberately do NOT install a measurer.
///
/// They rely on nextest giving each test its own process: under
/// `cargo test` a sibling that calls `init_text_measurer` would install
/// the global first and these would silently measure through the real
/// path instead.
mod estimator {
    use super::*;

    #[test]
    fn the_estimator_counts_every_character() {
        for text in ["A", "AB", "ABCD"] {
            let n = text.chars().count() as f32;
            let delta = width(text, SPACING) - width(text, 0.0);
            let counted = delta / SPACING;
            assert!(
                (counted - n).abs() < 0.01,
                "{text:?}: estimator counted {counted} spacings, expected {n}"
            );
        }
    }

    /// Empty text has no character to carry a spacing.
    #[test]
    fn empty_text_gets_no_spacing() {
        assert_eq!(width("", SPACING), width("", 0.0));
    }
}
