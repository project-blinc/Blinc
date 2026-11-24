//! A bound layout property eases by the `transition` the node's rules give
//! it, as a change of rule does; without one it lands at once.

use std::sync::Arc;

use blinc_core::reactive::signal;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

const CSS: &str = ".eased { transition: flex-grow 200ms linear; }";

fn tree(grow: blinc_core::reactive::Signal<f32>, class: &str) -> RenderTree {
    let host = div()
        .w(200.0)
        .h(20.0)
        .flex_row()
        .child(div().class(class).w(0.0).h(20.0).flex_grow_value(grow));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet_arc(Arc::new(Stylesheet::parse(CSS).expect("css")));
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(200.0, 20.0);
    tree
}

/// Land the bound writes, run the transitions `ms` on, and lay out.
fn frame(tree: &mut RenderTree, ms: f32) -> f32 {
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.css_anim_store().lock().unwrap().tick(ms);
    tree.apply_all_css_transition_props();
    tree.apply_animated_layout_props();
    tree.compute_layout(200.0, 20.0);
    let bar = tree.layout_tree.children(tree.root().unwrap())[0];
    tree.layout_tree.get_bounds(bar, (0.0, 0.0)).unwrap().width
}

#[test]
fn a_bound_flex_grow_eases_by_its_rule() {
    let grow = signal(0.0f32);
    let mut tree = tree(grow, "eased");
    assert_eq!(frame(&mut tree, 0.0), 0.0);

    grow.set(1.0);
    let halfway = frame(&mut tree, 50.0);
    assert!(
        halfway > 10.0 && halfway < 190.0,
        "partway through the transition the bar is {halfway} wide"
    );
    for _ in 0..20 {
        frame(&mut tree, 16.0);
    }
    assert_eq!(frame(&mut tree, 16.0), 200.0, "it did not land");
}

#[test]
fn without_a_transition_it_lands_at_once() {
    let grow = signal(0.0f32);
    let mut tree = tree(grow, "plain");
    frame(&mut tree, 0.0);
    grow.set(1.0);
    assert_eq!(frame(&mut tree, 16.0), 200.0);
}
