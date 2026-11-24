//! `border-top` and friends reach the node's per-side borders, and a side
//! with no declaration keeps the uniform border.

use blinc_core::Color;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

fn sides(css: &str) -> blinc_layout::element::BorderSides {
    let host = div()
        .w(200.0)
        .h(100.0)
        .child(div().class("x").w(50.0).h(50.0));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    let root = tree.root().expect("root");
    let node = tree.layout_tree.children(root)[0];
    tree.get_render_node(node).expect("node").props.border_sides
}

#[test]
fn one_side_alone_draws_only_that_side() {
    let s = sides(".x { border-top: 3px solid #0000ff; }");
    let top = s.top.expect("top");
    assert_eq!(top.width, 3.0);
    assert_eq!(top.color, Color::rgb(0.0, 0.0, 1.0));
    assert!(s.right.is_none() && s.bottom.is_none() && s.left.is_none());
}

#[test]
fn an_undeclared_side_keeps_the_uniform_border() {
    let s = sides(".x { border: 1px solid #ff0000; border-bottom-width: 4px; }");
    assert_eq!(s.bottom.expect("bottom").width, 4.0);
    assert_eq!(s.top.expect("top").width, 1.0);
    assert_eq!(s.left.expect("left").color, Color::rgb(1.0, 0.0, 0.0));
}

#[test]
fn none_removes_a_side() {
    let s = sides(".x { border: 1px solid #ff0000; border-left: none; }");
    assert!(s.left.is_none());
    assert!(s.right.is_some());
}

#[test]
fn important_uniform_colour_protects_sides_without_changing_their_widths() {
    let s = sides(
        "@layer components { .x { border: 1px solid red; border-color: red !important; } } .x { border-top: 3px solid blue; }",
    );
    assert_eq!(s.top.unwrap().width, 3.0);
    assert_eq!(s.top.unwrap().color, Color::from_hex(0xff0000));
    assert_eq!(s.bottom.unwrap().width, 1.0);
}
