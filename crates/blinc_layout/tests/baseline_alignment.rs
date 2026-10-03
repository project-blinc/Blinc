//! `align-items: baseline` across fonts and sizes.
//!
//! taffy 0.6 never receives a first baseline for a text leaf:
//! `compute/leaf.rs` returns `first_baselines: Point::NONE`, and a
//! measure function can only return a size. `compute/flexbox.rs` then
//! falls back to `first_baselines.y.unwrap_or(size.height)`, so
//! `align-items: baseline` aligns box bottoms.
//!
//! Blinc hides that for same-size text by giving single-line text a box
//! of `font_size * line_height` and drawing at `top + ascender`. Two
//! faces at different sizes have different ascenders, so their drawn
//! baselines diverge.
//!
//! These tests state the property that has to hold. They are ignored
//! because satisfying them needs a first baseline to reach taffy, which
//! taffy 0.6 has no extension point for: the dispatch that would carry
//! it lives on `TaffyView`, which is `pub(crate)` and reaches into
//! private tree state, so no wrapper can override it.

use blinc_layout::div::GenericFont;
use blinc_layout::text_measure::{TextLayoutOptions, measure_text_with_options};
use blinc_layout::{LayoutNodeId, LayoutTree, TextMeasureContext};
use taffy::prelude::*;

fn options(generic: GenericFont) -> TextLayoutOptions {
    let mut o = TextLayoutOptions::new();
    o.generic_font = generic;
    o
}

fn context(content: &str, font_size: f32, generic: GenericFont) -> TextMeasureContext {
    TextMeasureContext {
        content: content.into(),
        font_size,
        line_height: 1.2,
        letter_spacing: 0.0,
        wrap: false,
        font_name: None,
        generic_font: generic,
        font_weight: 400,
        italic: false,
    }
}

/// Where Blinc draws the baseline of a text node: the node's top, plus
/// its own top padding, plus the ascender of the face it is drawn in.
/// Mirrors the paint path rather than restating CSS, because this is
/// the position a reader actually sees.
fn drawn_baseline(
    tree: &mut LayoutTree,
    node: LayoutNodeId,
    font_size: f32,
    generic: GenericFont,
    padding_top: f32,
) -> f32 {
    let metrics = measure_text_with_options("x", font_size, &options(generic));
    let top = tree.get_absolute_bounds(node).expect("laid out").y;
    top + padding_top + metrics.ascender
}

/// The request: 16px sans beside 14px monospace with 1px padding, in a
/// row that asks for baseline alignment, must share a baseline.
#[test]
#[ignore = "taffy 0.6 gives a text leaf no first baseline; needs the decision recorded in git-bug"]
fn different_sizes_share_a_baseline() {
    let mut tree = LayoutTree::new();

    let root = tree.create_node(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: Some(AlignItems::Baseline),
        size: Size {
            width: length(400.0_f32),
            height: length(200.0_f32),
        },
        ..Default::default()
    });

    let sans = tree.create_text_node(
        Style::default(),
        context("paragraph", 16.0, GenericFont::SansSerif),
    );
    let mono = tree.create_text_node(
        Style {
            padding: Rect {
                left: length(1.0_f32),
                right: length(1.0_f32),
                top: length(1.0_f32),
                bottom: length(1.0_f32),
            },
            ..Default::default()
        },
        context("code", 14.0, GenericFont::Monospace),
    );
    tree.add_child(root, sans);
    tree.add_child(root, mono);

    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(400.0),
            height: AvailableSpace::Definite(200.0),
        },
    );

    let a = drawn_baseline(&mut tree, sans, 16.0, GenericFont::SansSerif, 0.0);
    let b = drawn_baseline(&mut tree, mono, 14.0, GenericFont::Monospace, 1.0);

    assert!(
        (a - b).abs() <= 0.5,
        "baselines diverge by {:.2}px: sans at {a:.2}, mono at {b:.2}",
        (a - b).abs()
    );
}

/// Same size and face must already align, whatever the baseline support
/// is. This one is NOT ignored: it guards the case Blinc's box estimate
/// does handle, so a baseline change cannot regress it.
#[test]
fn the_same_face_at_the_same_size_already_aligns() {
    let mut tree = LayoutTree::new();

    let root = tree.create_node(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: Some(AlignItems::Baseline),
        size: Size {
            width: length(400.0_f32),
            height: length(200.0_f32),
        },
        ..Default::default()
    });

    let left = tree.create_text_node(
        Style::default(),
        context("left", 16.0, GenericFont::SansSerif),
    );
    let right = tree.create_text_node(
        Style::default(),
        context("right", 16.0, GenericFont::SansSerif),
    );
    tree.add_child(root, left);
    tree.add_child(root, right);

    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(400.0),
            height: AvailableSpace::Definite(200.0),
        },
    );

    let a = drawn_baseline(&mut tree, left, 16.0, GenericFont::SansSerif, 0.0);
    let b = drawn_baseline(&mut tree, right, 16.0, GenericFont::SansSerif, 0.0);
    assert!((a - b).abs() <= 0.5, "{a} vs {b}");
}

/// Documents what taffy does today, so the gap is visible in the suite
/// rather than only in a report. If this ever fails, taffy has started
/// giving leaves a baseline and the ignored test above should be tried.
#[test]
fn today_baseline_alignment_aligns_box_bottoms() {
    let mut tree = LayoutTree::new();

    let root = tree.create_node(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: Some(AlignItems::Baseline),
        size: Size {
            width: length(400.0_f32),
            height: length(200.0_f32),
        },
        ..Default::default()
    });

    let tall = tree.create_text_node(
        Style::default(),
        context("tall", 32.0, GenericFont::SansSerif),
    );
    let short = tree.create_text_node(
        Style::default(),
        context("short", 12.0, GenericFont::SansSerif),
    );
    tree.add_child(root, tall);
    tree.add_child(root, short);

    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(400.0),
            height: AvailableSpace::Definite(200.0),
        },
    );

    let tall_b = tree.get_absolute_bounds(tall).expect("laid out");
    let short_b = tree.get_absolute_bounds(short).expect("laid out");

    assert!(
        ((tall_b.y + tall_b.height) - (short_b.y + short_b.height)).abs() <= 0.5,
        "expected bottoms to coincide, which is the fallback taffy uses: \
         {} vs {}",
        tall_b.y + tall_b.height,
        short_b.y + short_b.height
    );
}
