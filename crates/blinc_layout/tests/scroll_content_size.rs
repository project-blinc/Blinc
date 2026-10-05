//! The content size of a container, which scrolling reads as the extent it
//! can scroll over.
//!
//! It is the far edge of the content measured from the container's own
//! origin: padding counts at both ends, and an overflowing absolute child
//! counts up to its own far edge.

use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

/// The root's content size after laying `host` out in a 600x600 viewport.
fn content_of(host: blinc_layout::div::Div) -> (f32, f32) {
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 600.0);
    let root = tree.root().expect("root");
    tree.layout_tree.get_content_size(root).expect("laid out")
}

#[test]
fn a_tall_child_sets_the_height_and_the_width_stays_the_containers() {
    // The child shrinks to the container's width: only the height overflows.
    let c = content_of(
        div()
            .w(200.0)
            .h(100.0)
            .overflow_scroll()
            .child(div().w(300.0).h(400.0)),
    );
    assert_eq!(c, (200.0, 400.0));
}

#[test]
fn padding_is_counted_in_the_content_extent() {
    let c = content_of(
        div()
            .w(200.0)
            .h(100.0)
            .p(2.5)
            .overflow_scroll()
            .child(div().w(300.0).h(400.0)),
    );
    assert_eq!(c, (200.0, 420.0));
}

#[test]
fn content_that_fits_reports_its_own_size_plus_padding() {
    let c = content_of(
        div()
            .w(200.0)
            .h(100.0)
            .p(5.0)
            .overflow_scroll()
            .child(div().w(50.0).h(40.0)),
    );
    assert_eq!(c, (90.0, 80.0));
}

#[test]
fn an_absolute_child_extends_the_width_to_its_far_edge() {
    let c = content_of(
        div()
            .w(200.0)
            .h(100.0)
            .overflow_scroll()
            .child(div().absolute().top(10.0).left(250.0).w(80.0).h(30.0)),
    );
    assert_eq!(c, (330.0, 40.0));
}

#[test]
fn a_column_of_fixed_rows_with_gaps_sums_them() {
    let c = content_of(
        div()
            .w(200.0)
            .h(100.0)
            .flex_col()
            .gap(1.0)
            .overflow_y_scroll()
            .children((0..10).map(|_| div().w_full().h(30.0).flex_shrink_0())),
    );
    // Ten 30px rows and nine 4px gaps.
    assert_eq!(c, (200.0, 336.0));
}
