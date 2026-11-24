//! `line-height` counts against the font size, as CSS specifies.
//!
//! Blinc's layout engine used to multiply it by the FACE's natural line
//! height (ascent + descent + gap). Helvetica's is exactly 1em, so the
//! default family hid the difference; monospace lines came out 13%
//! taller than asked for. Every other part of Blinc — the estimator,
//! the hit tester, the code editor, the rich-text renderer — already
//! counted against the font size, so the layout engine disagreed with
//! them and with its own fallback.

#![cfg(feature = "text_measurer")]

use blinc_layout::div::GenericFont;
use blinc_layout::text_measure::{TextLayoutOptions, measure_text_with_options};

fn height(generic: GenericFont, font_size: f32, line_height: f32) -> f32 {
    let mut o = TextLayoutOptions::new();
    o.generic_font = generic;
    o.line_height = line_height;
    measure_text_with_options("Hxq", font_size, &o).height
}

/// One line is font_size * line_height, whatever the face's own metrics
/// are. Monospace is the case that used to differ.
#[test]
fn a_line_is_the_font_size_times_the_multiplier() {
    blinc_layout::text_measurer::init_text_measurer();

    for generic in [
        GenericFont::System,
        GenericFont::SansSerif,
        GenericFont::Monospace,
    ] {
        for (size, lh) in [(100.0, 1.0), (16.0, 1.5), (32.0, 1.2)] {
            let got = height(generic, size, lh);
            let want = size * lh;
            assert!(
                (got - want).abs() < 0.5,
                "{generic:?} at {size}px x {lh}: measured {got}, CSS says {want}"
            );
        }
    }
}

/// Two faces with different natural metrics must agree at the same
/// size and multiplier. They did not before: monospace was 1.13x.
#[test]
fn faces_with_different_metrics_agree() {
    blinc_layout::text_measurer::init_text_measurer();

    let sans = height(GenericFont::SansSerif, 100.0, 1.0);
    let mono = height(GenericFont::Monospace, 100.0, 1.0);
    assert!(
        (sans - mono).abs() < 0.5,
        "sans {sans} vs mono {mono}: line height still depends on the face"
    );
}
