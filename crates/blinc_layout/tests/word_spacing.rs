//! `word_spacing` widens each word separator, in what a text measures and in what is drawn.

#![cfg(feature = "text_measurer")]

use blinc_layout::div::GenericFont;
use blinc_layout::renderer::RenderTree;
use blinc_layout::text::text;
use blinc_layout::text_measure::{TextLayoutOptions, measure_text_with_options};
use blinc_text::TextRenderer;
use blinc_text::layout::LayoutOptions;
use blinc_text::registry::GenericFont as TextGeneric;

const SIZE: f32 = 16.0;
const SPACING: f32 = 10.0;
const TEXT: &str = "one two\u{a0}three";

fn width(spacing: f32, max_width: Option<f32>) -> (f32, u32) {
    let mut o = TextLayoutOptions::new();
    o.generic_font = GenericFont::System;
    o.word_spacing = spacing;
    o.max_width = max_width;
    let m = measure_text_with_options(TEXT, SIZE, &o);
    (m.width, m.line_count)
}

#[test]
fn each_separator_adds_the_spacing_to_what_is_measured_and_drawn() {
    blinc_layout::text_measurer::init_text_measurer();
    let added = width(SPACING, None).0 - width(0.0, None).0;
    assert!((added - 2.0 * SPACING).abs() < 0.01, "measured {added}");

    let mut renderer = TextRenderer::new();
    let mut last_x = |word_spacing: f32| {
        let options = LayoutOptions {
            word_spacing,
            ..LayoutOptions::default()
        };
        renderer
            .prepare_text_with_style(
                TEXT,
                SIZE,
                [1.0; 4],
                &options,
                None,
                TextGeneric::System,
                400,
                false,
            )
            .ok()
            .and_then(|p| p.glyphs.last().map(|g| g.bounds[0]))
    };
    let (Some(plain), Some(spaced)) = (last_x(0.0), last_x(SPACING)) else {
        eprintln!("SKIP: renderer unavailable");
        return;
    };
    // Glyphs sit on whole or half pixels.
    assert!(
        (spaced - plain - 2.0 * SPACING).abs() < 1.0,
        "the last glyph moved {}",
        spaced - plain
    );
}

#[test]
fn spaced_words_wrap_where_they_no_longer_fit() {
    blinc_layout::text_measurer::init_text_measurer();
    let fits = width(0.0, None).0 + 1.0;
    assert_eq!(width(0.0, Some(fits)).1, 1);
    assert!(width(SPACING, Some(fits)).1 > 1);
}

#[test]
fn a_text_box_that_does_not_wrap_holds_its_spacing() {
    blinc_layout::text_measurer::init_text_measurer();
    let box_width = |word: f32, letter: f32| {
        let element = text(TEXT)
            .size(SIZE)
            .no_wrap()
            .word_spacing(word)
            .letter_spacing(letter);
        let mut tree = RenderTree::from_element(&element);
        tree.compute_layout(800.0, 100.0);
        tree.get_absolute_bounds(tree.root().unwrap())
            .unwrap()
            .width
    };
    let plain = box_width(0.0, 0.0);
    let words = box_width(SPACING, 0.0) - plain;
    assert!((words - 2.0 * SPACING).abs() <= 1.0, "words added {words}");
    let letters = box_width(0.0, 2.0) - plain;
    let expected = 2.0 * TEXT.chars().count() as f32;
    assert!((letters - expected).abs() <= 1.0, "letters added {letters}");
}

mod estimator {
    use super::*;

    #[test]
    fn the_estimator_counts_each_separator() {
        let added = width(SPACING, None).0 - width(0.0, None).0;
        assert!(
            (added - 2.0 * SPACING).abs() < 0.01,
            "estimator added {added}"
        );
    }
}
