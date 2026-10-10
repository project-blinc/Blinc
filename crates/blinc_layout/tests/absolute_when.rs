//! `Div::absolute_when`: an element leaves flow and comes back in place.

use blinc_core::reactive::signal;
use blinc_layout::LayoutNodeId;
use blinc_layout::binding::with_registry;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::Mutex;

/// The pending-write queue and the reactive graph are process-wide.
static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    with_registry(|_| {});
    guard
}

fn frame(tree: &mut RenderTree) {
    assert!(
        blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
        "a change queued a subtree rebuild"
    );
    let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(200.0, 400.0);
}

fn y(tree: &RenderTree, node: LayoutNodeId) -> f32 {
    tree.get_absolute_bounds(node).unwrap().y
}

#[test]
fn an_element_leaves_flow_while_the_condition_holds_and_comes_back() {
    let _g = serial();
    let out = signal(false);
    let ui = div()
        .w(200.0)
        .flex_col()
        .child(div().h(50.0).absolute_when(out))
        .child(div().h(30.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(200.0, 400.0);
    let kids = tree.layout_tree.children(tree.root().unwrap());
    assert_eq!(y(&tree, kids[1]), 50.0);

    out.set(true);
    frame(&mut tree);
    assert_eq!(y(&tree, kids[1]), 0.0, "the first element is still in flow");

    out.set(false);
    frame(&mut tree);
    assert_eq!(y(&tree, kids[1]), 50.0, "it did not come back");
}

#[test]
fn a_condition_that_holds_at_the_start_takes_it_out_of_flow_from_the_start() {
    let _g = serial();
    let out = signal(true);
    let ui = div()
        .w(200.0)
        .flex_col()
        .child(div().h(50.0).absolute_when(out))
        .child(div().h(30.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(200.0, 400.0);
    let kids = tree.layout_tree.children(tree.root().unwrap());
    assert_eq!(y(&tree, kids[1]), 0.0);

    out.set(false);
    frame(&mut tree);
    assert_eq!(y(&tree, kids[1]), 50.0);
}

#[test]
fn a_constant_is_absolute_or_nothing() {
    let _g = serial();
    let ui = div()
        .w(200.0)
        .flex_col()
        .child(div().h(50.0).absolute_when(true))
        .child(div().h(30.0).absolute_when(false));
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(200.0, 400.0);
    let kids = tree.layout_tree.children(tree.root().unwrap());
    assert_eq!(
        y(&tree, kids[1]),
        0.0,
        "the second sits where the first was"
    );
}
