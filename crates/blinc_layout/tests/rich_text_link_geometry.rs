//! Link geometry must follow the container width.
//!
//! The same markup wrapped into 900px and into 200px puts its link in
//! different places. Geometry computed once at build time cannot know
//! either width, so this is the property that decides where the
//! computation belongs.
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::rich_text;

const MARKUP: &str = "Press <b>Enter</b> to accept, <i>Escape</i> to cancel, or read \
                      the <a href='https://example.com'>manual</a> for the full list \
                      of shortcuts and their meanings in every mode.";

/// Every cursor region on the rich-text node, in element-local space.
fn link_regions(width: f32) -> Vec<(f32, f32)> {
    let host = div().w(width).h(600.0).child(rich_text(MARKUP));
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(width, 600.0);

    let root = tree.root().expect("root");
    let node = tree.layout_tree.children(root)[0];
    // The node's OWN laid-out width, not the container's: that is what the
    // cursor path passes, so a node that sized itself wider than its parent
    // shows up here rather than being papered over.
    let node_width = tree
        .get_absolute_bounds(node)
        .map(|b| b.width)
        .unwrap_or(0.0);

    tree.get_render_node(node)
        .and_then(|n| n.props.text_hit_spans.clone())
        .map(|hit| {
            hit.rects(node_width)
                .iter()
                .map(|r| (r.x0, r.x1))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// A link that fits on one line at 900px must move when the text wraps
/// at 200px — and a link split across two lines needs two rects.
///
/// Fails until link geometry is derived where the width is known.
#[test]
fn link_geometry_follows_the_container_width() {
    let wide = link_regions(900.0);
    let narrow = link_regions(200.0);

    assert!(!wide.is_empty(), "the link publishes a region at 900px");
    assert!(!narrow.is_empty(), "and at 200px");

    assert_ne!(
        wide, narrow,
        "geometry must differ between widths; identical rects mean it \
         was computed before any width was known",
    );

    // At 200px every rect must fit inside the container. A build-time
    // rect measured as one continuous line runs far past it.
    for (start, end) in &narrow {
        assert!(
            *end <= 200.0,
            "rect {start}..{end} escapes the 200px container",
        );
    }
}
