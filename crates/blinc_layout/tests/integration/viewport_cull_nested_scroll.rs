//! A culling scroll paints what is in view inside a nested scroll after it
//! has itself been scrolled, and counts it as on screen: the nested
//! scroll's children are placed on screen by both scrolls.

use blinc_layout::canvas::canvas;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::widgets::scroll::scroll;

/// Further than the cull's overscan, so a child tested without the outer
/// scroll's offset falls outside it.
const SPACER: f32 = 600.0;

fn paint(outer_scroll: f32) -> (RenderTree, blinc_layout::LayoutNodeId) {
    let ui = scroll()
        .id("outer")
        .w(200.0)
        .h(100.0)
        .viewport_cull(true)
        .child(
            div()
                .w_full()
                .flex_col()
                .child(div().w_full().h(SPACER))
                .child(
                    scroll().w_full().h(80.0).child(
                        div()
                            .id("mark")
                            .w(2.0)
                            .h(15.0)
                            .child(canvas(|_, _| {}).w(2.0).h(15.0)),
                    ),
                ),
        );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(200.0, 100.0);
    let outer = tree.query_by_id("outer").unwrap();
    tree.dispatch_scroll_chain(outer, &[outer], 50.0, 50.0, 0.0, outer_scroll);
    let animations = std::sync::Arc::new(std::sync::Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    ));
    let mut rs = blinc_layout::render_state::RenderState::new(animations);
    rs.set_viewport(0.0, 0.0, 200.0, 100.0);
    let mut ctx = blinc_core::RecordingContext::new(blinc_core::Size::new(200.0, 100.0));
    // As the compositor paints: canvases are recorded for its overlay pass.
    tree.set_skip_canvas_drawing(true);
    tree.render_with_motion(&mut ctx, &rs);
    let mark = tree.query_by_id("mark").unwrap();
    let canvas = tree.layout_tree.children(mark)[0];
    (tree, canvas)
}

#[test]
fn a_nested_scroll_scrolled_into_view_is_painted() {
    let (tree, canvas) = paint(-SPACER);
    assert!(
        tree.canvas_paint_records().contains_key(&canvas),
        "the canvas in the nested scroll was culled"
    );
}

#[test]
fn a_nested_scroll_out_of_view_is_culled() {
    let (tree, canvas) = paint(0.0);
    assert!(
        !tree.canvas_paint_records().contains_key(&canvas),
        "a canvas far below the outer scroll's viewport was painted"
    );
}

#[test]
fn a_nested_scroll_scrolled_into_view_counts_as_on_screen() {
    let (tree, canvas) = paint(-SPACER);
    assert!(
        tree.painted_node_ids().contains(&canvas),
        "the canvas in view was counted as off screen"
    );
    let (tree, canvas) = paint(0.0);
    assert!(!tree.painted_node_ids().contains(&canvas));
}
