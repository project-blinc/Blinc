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

#[test]
fn a_repeat_track_list_lays_items_on_a_grid() {
    // 10 + 6 + 10: no free height for the rows to stretch into.
    let host = div()
        .w(400.0)
        .h(26.0)
        .class("g")
        .child(div().h(10.0))
        .child(div().h(10.0))
        .child(div().h(10.0))
        .child(div().h(10.0));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(
        Stylesheet::parse(
            ".g { display: grid; grid-template-columns: repeat(3, 40px); row-gap: 6px; column-gap: 5px; align-items: start; }",
        )
        .expect("css"),
    );
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 200.0);
    let kids = tree.layout_tree.children(tree.root().expect("root"));
    let at = |i: usize| tree.get_absolute_bounds(kids[i]).expect("bounds");
    assert!(
        (at(1).x - at(0).x - 45.0).abs() < 0.5,
        "column step {}",
        at(1).x - at(0).x
    );
    assert!((at(0).width - 40.0).abs() < 0.5);
    assert!(
        (at(3).x - at(0).x).abs() < 0.5,
        "fourth item wraps to the first column"
    );
    assert!(
        (at(3).y - at(0).y - 16.0).abs() < 0.5,
        "row step {}",
        at(3).y - at(0).y
    );
}

#[test]
fn grid_placement_and_minmax_tracks_follow_the_alert_shape() {
    let host = div()
        .w(400.0)
        .h(200.0)
        .class("g")
        .child(div().class("icon").w(16.0).h(16.0))
        .child(div().class("title").h(20.0))
        .child(div().class("desc").h(20.0));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(
        Stylesheet::parse(
            r#".g { display: grid; grid-template-columns: 16px minmax(0, 1fr); column-gap: 12px; align-items: start; }
               .icon { grid-row: 1 / span 2; }
               .title, .desc { grid-column: 2; }"#,
        )
        .expect("css"),
    );
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 200.0);
    let kids = tree.layout_tree.children(tree.root().expect("root"));
    let icon = tree.get_absolute_bounds(kids[0]).expect("icon");
    let title = tree.get_absolute_bounds(kids[1]).expect("title");
    let desc = tree.get_absolute_bounds(kids[2]).expect("desc");
    assert!(
        (title.x - icon.x - 28.0).abs() < 0.5,
        "title x offset {}",
        title.x - icon.x
    );
    assert!(
        (title.width - (400.0 - 28.0)).abs() < 0.5,
        "title fills the fr track: {}",
        title.width
    );
    assert!(desc.y > title.y, "description sits below the title");
    assert!((desc.x - title.x).abs() < 0.5);
}
