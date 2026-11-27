//! `Div::visible` is followed by an incremental update, as a constant and as a
//! signal, and an update does not show a hidden element again.

use blinc_layout::div::{Div, div};
use blinc_layout::renderer::{RenderTree, UpdateResult};

fn ui(width: f32, visible: impl blinc_layout::binding::IntoReactive<bool>) -> Div {
    div()
        .w(200.0)
        .h(100.0)
        .child(div().id("v").w(width).h(30.0).visible(visible))
}

fn built(ui: &Div) -> RenderTree {
    let mut tree = RenderTree::from_element(ui);
    tree.compute_layout(200.0, 100.0);
    tree
}

fn update(tree: &mut RenderTree, ui: &Div) -> UpdateResult {
    let result = tree.incremental_update(ui);
    tree.compute_layout(200.0, 100.0);
    result
}

/// The size of the element, which is nothing while it is out of layout.
fn size(tree: &RenderTree) -> (f32, f32) {
    let node = tree.query_by_id("v").expect("node");
    let bounds = tree.get_bounds(node).expect("bounds");
    (bounds.width, bounds.height)
}

#[test]
fn a_constant_hides_and_shows_through_an_update() {
    let mut tree = built(&ui(80.0, true));
    assert_eq!(size(&tree), (80.0, 30.0));

    assert_ne!(update(&mut tree, &ui(80.0, false)), UpdateResult::NoChanges);
    assert_eq!(size(&tree), (0.0, 0.0));

    assert_ne!(update(&mut tree, &ui(80.0, true)), UpdateResult::NoChanges);
    assert_eq!(size(&tree), (80.0, 30.0));
}

#[test]
fn an_update_of_a_hidden_element_leaves_it_hidden() {
    let mut tree = built(&ui(80.0, false));
    update(&mut tree, &ui(90.0, false));
    assert_eq!(size(&tree), (0.0, 0.0));
    update(&mut tree, &ui(90.0, true));
    assert_eq!(size(&tree), (90.0, 30.0));
}

#[test]
fn an_update_does_not_show_what_a_signal_hides() {
    let shown = blinc_core::reactive::signal(false);
    let mut tree = built(&ui(80.0, shown));
    assert_eq!(size(&tree), (0.0, 0.0));
    update(&mut tree, &ui(90.0, shown));
    assert_eq!(size(&tree), (0.0, 0.0));
}
