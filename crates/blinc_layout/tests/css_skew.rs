//! A skew from a stylesheet is applied once, whichever way it is written.

use blinc_core::{Affine2D, Transform};
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

/// The transform of a box with classes `x y` under `css`.
fn transform(css: &str) -> [f32; 6] {
    let ui = div()
        .w(100.0)
        .h(100.0)
        .child(div().id("box").class("x").class("y").w(40.0).h(40.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_all_stylesheet_styles();
    let node = tree.query_by_id("box").expect("the box");
    match tree.get_render_node(node).unwrap().props.transform {
        Some(Transform::Affine2D(a)) => a.elements,
        ref other => panic!("{css}: {other:?}"),
    }
}

fn assert_is(css: &str, expected: Affine2D) {
    let got = transform(css);
    assert!(
        got.iter()
            .zip(expected.elements)
            .all(|(a, b)| (a - b).abs() < 1e-5),
        "{css}: {got:?}, expected {:?}",
        expected.elements
    );
}

#[test]
fn a_skew_function_in_transform_is_applied_once() {
    let x = Affine2D::skew_x(10f32.to_radians());
    let y = Affine2D::skew_y(5f32.to_radians());
    assert_is(".x { transform: skewX(10deg) }", x);
    assert_is(".x { transform: skewY(5deg) }", y);
    assert_is(".x { transform: skew(10deg, 5deg) }", x.then(&y));
}

#[test]
fn a_skew_property_goes_on_top_of_the_transform() {
    let skew = Affine2D::skew_x(10f32.to_radians());
    let rotation = Affine2D::rotation(30f32.to_radians());
    assert_is(".x { skew-x: 10deg }", skew);
    assert_is(
        ".x { transform: rotate(30deg) } .x.y { skew-x: 10deg }",
        rotation.then(&skew),
    );
}
