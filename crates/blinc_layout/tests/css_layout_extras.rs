//! `margin: auto`, percent and `none` min and max sizes, `flex-basis` and
//! per-axis gaps reach the layout.

use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

fn laid_out(
    child_css: &str,
    child: blinc_layout::div::Div,
) -> (RenderTree, blinc_layout::LayoutNodeId) {
    let host = div().w(400.0).h(200.0).child(child.class("x"));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(Stylesheet::parse(child_css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 200.0);
    let node = tree.layout_tree.children(tree.root().expect("root"))[0];
    (tree, node)
}

#[test]
fn auto_side_margins_centre_a_fixed_width_child() {
    let (tree, node) = laid_out(".x { margin: 0 auto; }", div().w(100.0).h(40.0));
    let b = tree.get_absolute_bounds(node).expect("bounds");
    assert!((b.x - 150.0).abs() < 0.5, "x = {}", b.x);
}

#[test]
fn margin_top_auto_pushes_a_child_to_the_bottom() {
    let host = div()
        .w(400.0)
        .h(200.0)
        .flex_col()
        .child(div().class("x").w(100.0).h(40.0));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(Stylesheet::parse(".x { margin-top: auto; }").expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 200.0);
    let node = tree.layout_tree.children(tree.root().expect("root"))[0];
    let b = tree.get_absolute_bounds(node).expect("bounds");
    assert!((b.y - 160.0).abs() < 0.5, "y = {}", b.y);
}

#[test]
fn max_width_in_percent_limits_the_child() {
    let (tree, node) = laid_out(".x { max-width: 50%; }", div().w(380.0).h(40.0));
    let b = tree.get_absolute_bounds(node).expect("bounds");
    assert!((b.width - 200.0).abs() < 0.5, "w = {}", b.width);
}

#[test]
fn max_width_none_removes_the_limit() {
    let (tree, node) = laid_out(
        ".x { max-width: 50px; } .x { max-width: none; }",
        div().w(300.0).h(40.0),
    );
    let b = tree.get_absolute_bounds(node).expect("bounds");
    assert!((b.width - 300.0).abs() < 0.5, "w = {}", b.width);
}

#[test]
fn min_height_in_percent_grows_the_child() {
    let (tree, node) = laid_out(".x { min-height: 100%; }", div().w(100.0).h(10.0));
    let b = tree.get_absolute_bounds(node).expect("bounds");
    assert!((b.height - 200.0).abs() < 0.5, "h = {}", b.height);
}

#[test]
fn row_and_column_gaps_apply_per_axis() {
    // 20 + 10 + 20: no free space left for wrapped lines to stretch into.
    let host = div()
        .w(400.0)
        .h(50.0)
        .class("x")
        .flex_row()
        .flex_wrap()
        .child(div().w(150.0).h(20.0))
        .child(div().w(150.0).h(20.0))
        .child(div().w(150.0).h(20.0));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(Stylesheet::parse(".x { row-gap: 10px; column-gap: 30px; }").expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 200.0);
    let kids = tree.layout_tree.children(tree.root().expect("root"));
    let a = tree.get_absolute_bounds(kids[0]).expect("a");
    let b = tree.get_absolute_bounds(kids[1]).expect("b");
    let c = tree.get_absolute_bounds(kids[2]).expect("c");
    assert!((b.x - a.x - 180.0).abs() < 0.5, "column gap: {}", b.x - a.x);
    assert!((c.y - a.y - 30.0).abs() < 0.5, "row gap: {}", c.y - a.y);
}
