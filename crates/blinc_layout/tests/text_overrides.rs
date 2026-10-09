//! A font property a stylesheet or a binding writes after build reaches the
//! box the text is measured in, not only the glyphs drawn.
//!
//! Paint draws with the overrides on `RenderProps`. The layout box, the
//! baseline and the width paint compares are all made at build, so an
//! override used to leave a box sized for one text and glyphs from another.

#![cfg(feature = "text_measurer")]

use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::text::text;
use blinc_layout::{ElementBuilder, LayoutNodeId};

/// Texts measure through the measurer only once it is installed, and a text
/// is measured when it is built, so every test starts here.
fn init() {
    blinc_layout::text_measurer::init_text_measurer();
}

fn layout(host: &impl ElementBuilder, css: &str) -> RenderTree {
    let mut tree = RenderTree::from_element(host);
    if !css.is_empty() {
        tree.set_stylesheet(blinc_layout::css_parser::Stylesheet::parse(css).expect("css"));
        tree.apply_stylesheet_layout_overrides();
        tree.apply_stylesheet_base_styles();
    }
    tree.compute_layout(600.0, 300.0);
    tree
}

/// The `n`th child of the root.
fn child(tree: &RenderTree, n: usize) -> LayoutNodeId {
    tree.layout_tree.children(tree.root().expect("root"))[n]
}

fn size(tree: &RenderTree, node: LayoutNodeId) -> (f32, f32) {
    let b = tree.get_absolute_bounds(node).expect("laid out");
    (b.width, b.height)
}

fn text_data(tree: &RenderTree, node: LayoutNodeId) -> blinc_layout::renderer::TextData {
    match &tree
        .get_render_node(node)
        .expect("render node")
        .element_type
    {
        ElementType::Text(t) => t.clone(),
        _ => panic!("not a text node"),
    }
}

/// Where paint puts the first baseline: the node's top plus half-leading and
/// the ascender it carries.
fn drawn_baseline(tree: &RenderTree, node: LayoutNodeId) -> f32 {
    let t = text_data(tree, node);
    tree.get_absolute_bounds(node).expect("laid out").y + t.half_leading + t.ascender
}

fn host(items: Vec<Box<dyn ElementBuilder>>) -> impl ElementBuilder {
    div().w(600.0).flex_row().items_baseline().children(items)
}

#[test]
fn a_css_font_size_sizes_a_non_wrapping_box() {
    init();
    let plain = layout(&div().w(600.0).child(text("Hello").no_wrap()), "");
    let sized = layout(
        &div().w(600.0).child(text("Hello").no_wrap().class("big")),
        ".big { font-size: 40px }",
    );

    let (pw, ph) = size(&plain, child(&plain, 0));
    let (sw, sh) = size(&sized, child(&sized, 0));
    assert!(
        sw > pw * 2.4,
        "the box stayed {pw} wide for text drawn {sw} wide"
    );
    assert!(
        (sh - 40.0 * 1.2).abs() < 1.0,
        "height {sh} for 40px at 1.2 (was {ph})"
    );
}

#[test]
fn a_css_font_size_sizes_a_wrapping_box() {
    init();
    let plain = layout(&div().child(text("Hello")), "");
    let sized = layout(
        &div().child(text("Hello").class("big")),
        ".big { font-size: 40px }",
    );

    let (pw, _) = size(&plain, child(&plain, 0));
    let (sw, sh) = size(&sized, child(&sized, 0));
    assert!(sw > pw * 2.4, "{pw} -> {sw}");
    assert!(sh >= 40.0, "a 40px line is at least 40 tall, got {sh}");
}

#[test]
fn a_css_font_weight_is_measured_at_that_weight() {
    init();
    let plain = layout(&div().w(600.0).child(text("Weighted text").no_wrap()), "");
    let bold = layout(
        &div()
            .w(600.0)
            .child(text("Weighted text").no_wrap().class("bold")),
        ".bold { font-weight: 700 }",
    );
    let (pw, _) = size(&plain, child(&plain, 0));
    let (bw, _) = size(&bold, child(&bold, 0));
    assert!(bw > pw, "bold measured {bw}, no wider than regular {pw}");
}

#[test]
fn css_letter_spacing_widens_a_fixed_size_box() {
    init();
    let plain = layout(&div().w(600.0).child(text("abcd").no_wrap()), "");
    let spaced = layout(
        &div().w(600.0).child(text("abcd").no_wrap().class("s")),
        ".s { letter-spacing: 4px }",
    );
    let (pw, _) = size(&plain, child(&plain, 0));
    let (sw, _) = size(&spaced, child(&spaced, 0));
    assert!(sw > pw + 12.0, "four letters at 4px spacing: {pw} -> {sw}");
}

#[test]
fn the_measured_width_paint_compares_follows_the_override() {
    init();
    let tree = layout(
        &div().w(600.0).child(text("Hello").no_wrap().class("big")),
        ".big { font-size: 40px }",
    );
    let node = child(&tree, 0);
    let t = text_data(&tree, node);
    let (w, _) = size(&tree, node);
    assert_eq!(t.font_size, 40.0);
    assert!(
        (t.measured_width - w).abs() < 1.5,
        "measured {} vs box {w}",
        t.measured_width
    );
}

/// Half-leading plus ascender for `size`, from the face's own metrics: where
/// a baseline belongs below the top of a line box that tall.
fn baseline_offset_at(size: f32) -> f32 {
    let options = blinc_layout::text_measure::TextLayoutOptions::new();
    let m = blinc_layout::text_measure::measure_text_with_options("x", size, &options);
    let line_box = m.height / m.line_count.max(1) as f32;
    (line_box - (m.ascender - m.descender)) / 2.0 + m.ascender
}

#[test]
fn an_overridden_size_carries_the_baseline_of_that_size() {
    init();
    let tree = layout(
        &host(vec![
            Box::new(text("plain").no_wrap()),
            Box::new(text("large").no_wrap().class("big")),
        ]),
        ".big { font-size: 32px }",
    );

    // The big text's baseline offset is the 32px one, not the 14px it was
    // built with, and the neighbouring baselines line up.
    let big = text_data(&tree, child(&tree, 1));
    let want = baseline_offset_at(32.0);
    assert!(
        (big.half_leading + big.ascender - want).abs() < 1.0,
        "baseline offset {:.2} for 32px text, the face says {want:.2}",
        big.half_leading + big.ascender
    );

    let a = drawn_baseline(&tree, child(&tree, 0));
    let b = drawn_baseline(&tree, child(&tree, 1));
    assert!(
        (a - b).abs() <= 1.0,
        "baselines diverge by {:.2}px: {a:.2} vs {b:.2}",
        (a - b).abs()
    );
}

#[test]
fn text_without_an_override_is_left_as_built() {
    init();
    let tree = layout(&div().w(600.0).child(text("Hello").no_wrap()), "");
    let t = text_data(&tree, child(&tree, 0));
    assert_eq!(t.font_size, 14.0);
    assert_eq!(t.line_height, 1.2);
    // The baseline the builder reported is not disturbed either.
    let built = text("Hello").no_wrap();
    let info = built.text_render_info().expect("info");
    assert_eq!(t.ascender, info.ascender);
    assert_eq!(t.half_leading, info.half_leading);
}
