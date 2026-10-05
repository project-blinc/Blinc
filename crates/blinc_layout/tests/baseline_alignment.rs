//! `align-items: baseline` across fonts and sizes.
//!
//! taffy never receives a first baseline for a text leaf:
//! `compute_leaf_layout` returns no baselines, so `compute/flexbox.rs`
//! falls back to `first_baselines.y.unwrap_or(size.height)` and
//! `align-items: baseline` aligns box bottoms.
//!
//! Blinc hides that for same-size text by giving single-line text a box
//! of `font_size * line_height` and drawing at `top + ascender`. Two
//! faces at different sizes have different ascenders, so their drawn
//! baselines diverge.
//!
//! `LayoutTree::align_baselines` corrects this after taffy runs: it
//! measures each baseline-aligned item's own baseline and shifts the
//! items of a line onto the deepest one. taffy is left alone, because
//! the dispatch that would carry a baseline lives on `TaffyView`, which
//! is `pub(crate)` and reaches into private tree state, so no wrapper
//! can override it.

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
/// its own top padding, plus half the line box's leading, plus the
/// ascender of the face it is drawn in. Mirrors the paint path rather
/// than restating CSS, because this is the position a reader sees.
fn drawn_baseline(
    tree: &mut LayoutTree,
    node: LayoutNodeId,
    font_size: f32,
    generic: GenericFont,
    padding_top: f32,
) -> f32 {
    let metrics = measure_text_with_options("x", font_size, &options(generic));
    let top = tree.get_absolute_bounds(node).expect("laid out").y;
    let line_box = metrics.height / metrics.line_count.max(1) as f32;
    let half_leading = (line_box - (metrics.ascender - metrics.descender)) / 2.0;
    top + padding_top + half_leading + metrics.ascender
}

/// The request: 16px sans beside 14px monospace with 1px padding, in a
/// row that asks for baseline alignment, must share a baseline.
#[test]
fn different_sizes_share_a_baseline() {
    let mut tree = LayoutTree::new();

    let root = tree.create_node(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: Some(AlignItems::BASELINE),
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
        align_items: Some(AlignItems::BASELINE),
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

/// The widest spread in the request's spirit: a heading beside small
/// text. Box bottoms used to coincide here, which put the big text's
/// baseline about `0.4 * (32 - 12)` px above the small one's.
#[test]
fn a_wide_size_spread_shares_a_baseline() {
    let mut tree = LayoutTree::new();

    let root = tree.create_node(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: Some(AlignItems::BASELINE),
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

    let a = drawn_baseline(&mut tree, tall, 32.0, GenericFont::SansSerif, 0.0);
    let b = drawn_baseline(&mut tree, short, 12.0, GenericFont::SansSerif, 0.0);
    assert!(
        (a - b).abs() <= 0.5,
        "baselines diverge by {:.2}px: 32px at {a:.2}, 12px at {b:.2}",
        (a - b).abs()
    );

    // And the boxes must NOT be bottom-aligned any more, which is what
    // the old fallback did.
    let tall_b = tree.get_absolute_bounds(tall).expect("laid out");
    let short_b = tree.get_absolute_bounds(short).expect("laid out");
    assert!(
        ((tall_b.y + tall_b.height) - (short_b.y + short_b.height)).abs() > 0.5,
        "boxes are still bottom-aligned, so nothing was shifted"
    );
}

/// A container takes its first child's baseline, which is what makes an
/// inline `code` chip (a padded box around text) line up with the prose
/// beside it. Without it the chip would align by its box instead.
#[test]
fn a_padded_container_aligns_by_its_text() {
    let mut tree = LayoutTree::new();

    let root = tree.create_node(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: Some(AlignItems::BASELINE),
        size: Size {
            width: length(400.0_f32),
            height: length(200.0_f32),
        },
        ..Default::default()
    });

    let prose = tree.create_text_node(
        Style::default(),
        context("paragraph", 16.0, GenericFont::SansSerif),
    );
    // The chip: a padded box whose only child is smaller mono text.
    let chip = tree.create_node(Style {
        display: Display::Flex,
        padding: Rect {
            left: length(4.0_f32),
            right: length(4.0_f32),
            top: length(3.0_f32),
            bottom: length(3.0_f32),
        },
        ..Default::default()
    });
    let chip_text = tree.create_text_node(
        Style::default(),
        context("code", 14.0, GenericFont::Monospace),
    );
    tree.add_child(chip, chip_text);
    tree.add_child(root, prose);
    tree.add_child(root, chip);

    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(400.0),
            height: AvailableSpace::Definite(200.0),
        },
    );

    let a = drawn_baseline(&mut tree, prose, 16.0, GenericFont::SansSerif, 0.0);
    // The chip's text carries no padding of its own; the chip's padding
    // already moved it down, and absolute bounds account for that.
    let b = drawn_baseline(&mut tree, chip_text, 14.0, GenericFont::Monospace, 0.0);

    assert!(
        (a - b).abs() <= 0.5,
        "chip text diverges by {:.2}px: prose at {a:.2}, code at {b:.2}",
        (a - b).abs()
    );
}

/// An absolutely positioned child of a flex container is not a flex
/// item: CSS positions it against the container's padding box and it
/// takes no part in alignment. Shifting one would move a box whose
/// position its author already computed, which is what an inline-flow
/// implementation does when it places each piece itself.
#[test]
fn an_absolute_child_is_not_shifted() {
    let mut tree = LayoutTree::new();

    let root = tree.create_node(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: Some(AlignItems::BASELINE),
        size: Size {
            width: length(400.0_f32),
            height: length(200.0_f32),
        },
        ..Default::default()
    });

    // An in-flow item big enough that baseline alignment would move
    // anything aligned against it.
    let prose = tree.create_text_node(
        Style::default(),
        context("paragraph", 32.0, GenericFont::SansSerif),
    );
    // Placed by its author, at an explicit offset.
    let placed = tree.create_text_node(
        Style {
            position: Position::Absolute,
            inset: Rect {
                left: length(10.0_f32),
                // Shallower than the 32px item beside it, so baseline
                // alignment would pull it DOWN if it took part. Placing
                // it lower would make it the deepest baseline and it
                // would never be the one moved.
                top: length(5.0_f32),
                right: auto(),
                bottom: auto(),
            },
            ..Default::default()
        },
        context("inline", 12.0, GenericFont::SansSerif),
    );
    tree.add_child(root, prose);
    tree.add_child(root, placed);

    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(400.0),
            height: AvailableSpace::Definite(200.0),
        },
    );

    let b = tree.get_absolute_bounds(placed).expect("laid out");
    assert_eq!(
        b.y, 5.0,
        "an absolutely positioned child was moved off the top its author set"
    );
}

/// Nor does an out-of-flow child define its container's first baseline:
/// a chip whose only in-flow content is text must align by that text,
/// not by something floating over it.
#[test]
fn an_absolute_child_does_not_define_a_container_baseline() {
    let mut tree = LayoutTree::new();

    let root = tree.create_node(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: Some(AlignItems::BASELINE),
        size: Size {
            width: length(400.0_f32),
            height: length(200.0_f32),
        },
        ..Default::default()
    });

    let prose = tree.create_text_node(
        Style::default(),
        context("paragraph", 16.0, GenericFont::SansSerif),
    );

    let chip = tree.create_node(Style {
        display: Display::Flex,
        ..Default::default()
    });
    // Out of flow, and deliberately first in order.
    let floating = tree.create_text_node(
        Style {
            position: Position::Absolute,
            inset: Rect {
                left: length(0.0_f32),
                top: length(30.0_f32),
                right: auto(),
                bottom: auto(),
            },
            ..Default::default()
        },
        context("x", 40.0, GenericFont::SansSerif),
    );
    let chip_text = tree.create_text_node(
        Style::default(),
        context("code", 14.0, GenericFont::Monospace),
    );
    tree.add_child(chip, floating);
    tree.add_child(chip, chip_text);
    tree.add_child(root, prose);
    tree.add_child(root, chip);

    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(400.0),
            height: AvailableSpace::Definite(200.0),
        },
    );

    let a = drawn_baseline(&mut tree, prose, 16.0, GenericFont::SansSerif, 0.0);
    let b = drawn_baseline(&mut tree, chip_text, 14.0, GenericFont::Monospace, 0.0);
    assert!(
        (a - b).abs() <= 0.5,
        "chip aligned by its floating child instead of its text: {a:.2} vs {b:.2}"
    );
}

/// A wrapping row aligns each LINE on its own baseline.
///
/// The pass groups items into lines by whether their boxes overlap
/// vertically, because taffy exposes no line assignment. That is untested
/// reasoning until a row actually wraps: two lines, mixed sizes on each,
/// and the items of one line must not be pulled onto the other's baseline.
#[test]
fn a_wrapping_row_aligns_each_line_on_its_own_baseline() {
    let mut tree = LayoutTree::new();

    // Wide enough for two items per line, not three: the estimator gives
    // roughly 0.55em per character, so "paragraph" is about 158px at 32px,
    // 59px at 12px and 99px at 20px.
    let root = tree.create_node(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        align_items: Some(AlignItems::BASELINE),
        size: Size {
            width: length(260.0_f32),
            height: length(300.0_f32),
        },
        ..Default::default()
    });

    let specs = [
        ("paragraph", 32.0_f32, GenericFont::SansSerif),
        ("paragraph", 12.0, GenericFont::SansSerif),
        ("paragraph", 20.0, GenericFont::SansSerif),
        ("code", 14.0, GenericFont::Monospace),
    ];
    let items: Vec<_> = specs
        .iter()
        .map(|&(t, size, g)| {
            let n = tree.create_text_node(Style::default(), context(t, size, g));
            tree.add_child(root, n);
            n
        })
        .collect();

    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(260.0),
            height: AvailableSpace::Definite(300.0),
        },
    );

    let tops: Vec<f32> = items
        .iter()
        .map(|&n| tree.get_absolute_bounds(n).expect("laid out").y)
        .collect();
    let base: Vec<f32> = items
        .iter()
        .zip(specs.iter())
        .map(|(&n, &(_, size, g))| drawn_baseline(&mut tree, n, size, g, 0.0))
        .collect();

    // The row really did wrap into two lines: the third item starts a new
    // line below the first two. Without this the test proves nothing.
    assert!(
        tops[2] > tops[0].max(tops[1]) + 1.0,
        "the row did not wrap; item tops {tops:?}"
    );

    // Each line shares a baseline within it.
    assert!(
        (base[0] - base[1]).abs() <= 0.5,
        "line 1 diverges by {:.2}px: {:.2} vs {:.2}",
        (base[0] - base[1]).abs(),
        base[0],
        base[1]
    );
    assert!(
        (base[2] - base[3]).abs() <= 0.5,
        "line 2 diverges by {:.2}px: {:.2} vs {:.2}",
        (base[2] - base[3]).abs(),
        base[2],
        base[3]
    );

    // And the two lines stay apart: line 2 was not dragged up to line 1.
    assert!(
        base[2] > base[0] + 1.0,
        "lines were aligned to each other: line 1 at {:.2}, line 2 at {:.2}",
        base[0],
        base[2]
    );
}
