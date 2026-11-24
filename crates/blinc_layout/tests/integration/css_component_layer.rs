//! A rule in the component layer is a default: it fills what the element did
//! not set, and gives way to a fluent value. A rule outside the layer is the
//! application's and still wins.

use blinc_core::{Brush, Color};
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

fn fill(css: &str, element: blinc_layout::div::Div) -> Option<Color> {
    let ui = div()
        .w(200.0)
        .h(200.0)
        .child(element.class("x").w(50.0).h(50.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    match tree.get_render_node(node).unwrap().props.background.clone() {
        Some(Brush::Solid(c)) => Some(c),
        _ => None,
    }
}

#[test]
fn a_component_rule_fills_what_the_element_did_not_set() {
    let css = "@layer components { .x { background: #ff0000 } }";
    assert_eq!(fill(css, div()), Some(RED));
}

#[test]
fn a_component_rule_gives_way_to_a_fluent_value() {
    let css = "@layer components { .x { background: #ff0000 } }";
    assert_eq!(fill(css, div().bg(BLUE)), Some(BLUE));
}

#[test]
fn an_application_rule_still_wins() {
    assert_eq!(
        fill(".x { background: #ff0000 }", div().bg(BLUE)),
        Some(RED)
    );
}

#[test]
fn an_important_component_colour_wins_over_normal_app_and_fluent_colours() {
    assert_eq!(
        fill(
            "@layer components { .x { background: red !important; } } .x { background: blue; }",
            div().bg(BLUE),
        ),
        Some(RED)
    );
}

#[test]
fn important_component_colours_outrank_unlayered_important_colours() {
    assert_eq!(
        fill(
            "@layer components { .x { background: red !important; } } #sample { background: blue !important; }",
            div().id("sample"),
        ),
        Some(RED)
    );
}

#[test]
fn app_importance_does_not_promote_other_properties_in_the_rule() {
    assert_eq!(
        fill(
            ".x { background: red; color: white !important; } #sample { background: blue; }",
            div().id("sample"),
        ),
        Some(BLUE)
    );
}

#[test]
fn a_component_layer_variant_can_restyle_protected_selection_colours() {
    assert_eq!(
        fill(
            "@layer components { .x { background: red !important; } } @layer components { .x.variant { background: blue !important; } }",
            div().class("variant"),
        ),
        Some(BLUE)
    );
}
