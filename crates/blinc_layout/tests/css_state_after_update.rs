//! A node updated in place keeps its new props and layout through the state
//! pass that a hover, press or focus change runs.

use blinc_core::{Brush, Color};
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::{Div, div};
use blinc_layout::event_router::EventRouter;
use blinc_layout::renderer::RenderTree;

/// A sheet with a state rule that matches nothing in the tree.
const SHEET: &str = ".elsewhere:hover { opacity: 0.5; }";

fn scene(ui: &Div) -> RenderTree {
    let mut tree = RenderTree::from_element(ui);
    tree.set_stylesheet(Stylesheet::parse(SHEET).expect("css"));
    tree.apply_all_stylesheet_styles();
    tree.compute_layout(400.0, 200.0);
    tree
}

/// Updates to `ui` and runs the passes a frame runs after it, then a state
/// pass as a pointer move would.
fn update(tree: &mut RenderTree, ui: &Div, router: &EventRouter) {
    tree.incremental_update(ui);
    tree.restyle_replaced_nodes(Some(router));
    tree.compute_layout(400.0, 200.0);
    tree.apply_stylesheet_state_styles(router);
    tree.compute_layout(400.0, 200.0);
}

fn width_and_color(tree: &RenderTree, id: &str) -> (f32, Option<Color>) {
    let node = tree.query_by_id(id).expect("node");
    let color = match tree.get_render_node(node).unwrap().props.background {
        Some(Brush::Solid(color)) => Some(color),
        _ => None,
    };
    (tree.get_bounds(node).unwrap().width, color)
}

fn ui(width: f32, color: Color) -> Div {
    div()
        .w(400.0)
        .h(200.0)
        .child(div().id("panel").w(width).h(30.0).bg(color))
}

#[test]
fn an_updated_node_with_an_id_keeps_its_new_size_and_color() {
    let router = EventRouter::new();
    let mut tree = scene(&ui(100.0, Color::RED));
    tree.apply_stylesheet_state_styles(&router);
    assert_eq!(width_and_color(&tree, "panel"), (100.0, Some(Color::RED)));

    update(&mut tree, &ui(200.0, Color::BLUE), &router);
    assert_eq!(width_and_color(&tree, "panel"), (200.0, Some(Color::BLUE)));
}

#[test]
fn a_change_of_size_alone_holds() {
    let router = EventRouter::new();
    let mut tree = scene(&ui(100.0, Color::RED));
    tree.apply_stylesheet_state_styles(&router);

    update(&mut tree, &ui(200.0, Color::RED), &router);
    assert_eq!(width_and_color(&tree, "panel"), (200.0, Some(Color::RED)));
}

#[test]
fn a_change_of_color_alone_holds() {
    let router = EventRouter::new();
    let mut tree = scene(&ui(100.0, Color::RED));
    tree.apply_stylesheet_state_styles(&router);

    update(&mut tree, &ui(100.0, Color::BLUE), &router);
    assert_eq!(width_and_color(&tree, "panel"), (100.0, Some(Color::BLUE)));
}
