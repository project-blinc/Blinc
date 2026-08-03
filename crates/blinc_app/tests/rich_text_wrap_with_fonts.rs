//! Wrapping and link geometry under the REAL font measurer.
//!
//! `blinc_layout`'s own tests run on `EstimatedTextMeasurer`, whose
//! 0.55-em guess is not what ships. These pin the same properties against
//! `FontTextMeasurer`, which is what the window uses.
use blinc_app::init_text_measurer;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::rich_text;
use std::sync::Once;

static FONTS: Once = Once::new();

fn with_fonts() {
    FONTS.call_once(init_text_measurer);
}

const MARKUP: &str = "Press <b>Enter</b> to accept, <i>Escape</i> to cancel, or read \
                      the <a href='https://example.com'>manual</a> for the full list \
                      of shortcuts and their meanings in every mode.";

fn node_at(width: f32) -> (f32, f32, Vec<(f32, f32, f32)>) {
    with_fonts();
    let host = div()
        .w(width)
        .h(600.0)
        .flex_col()
        .items_start()
        .child(rich_text(MARKUP));
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(width, 600.0);

    let root = tree.root().expect("root");
    let node = tree.layout_tree.children(root)[0];
    let bounds = tree.get_absolute_bounds(node).expect("bounds");
    let rects = tree
        .get_render_node(node)
        .and_then(|n| n.props.text_hit_spans.clone())
        .map(|hit| {
            hit.rects(bounds.width)
                .iter()
                .map(|r| (r.x0, r.x1, r.y0))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    (bounds.width, bounds.height, rects)
}

#[test]
fn real_fonts_wrap_the_paragraph() {
    let (wide_w, wide_h, _) = node_at(1200.0);
    let (narrow_w, narrow_h, _) = node_at(240.0);

    assert!(wide_w > 500.0, "the sentence is long: {wide_w}");
    assert!(narrow_w <= 240.0, "and fits its container: {narrow_w}");
    assert!(
        narrow_h > wide_h,
        "narrow allocates more lines: {narrow_h} vs {wide_h}",
    );
}

#[test]
fn the_link_moves_to_a_later_line_when_narrowed() {
    let (_, _, wide) = node_at(1200.0);
    let (narrow_w, _, narrow) = node_at(240.0);

    assert!(!wide.is_empty() && !narrow.is_empty(), "link places rects");
    assert_ne!(wide, narrow, "geometry follows the width");

    // One line at 1200px, so nothing is pushed down.
    assert!(wide.iter().all(|(_, _, y)| *y == 0.0), "{wide:?}");
    // Deep in the sentence, so at 240px it cannot still be on line one.
    assert!(narrow.iter().any(|(_, _, y)| *y > 0.0), "{narrow:?}");

    for (x0, x1, _) in &narrow {
        assert!(*x1 <= narrow_w, "rect {x0}..{x1} escapes {narrow_w}px");
    }
}
