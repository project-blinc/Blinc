//! `Div::when` with a signal: the children it adds are built either way and
//! shown while the condition holds, in place, as the element's own children.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use blinc_core::Computed;
use blinc_core::reactive::{ReactiveGraph, State};
use blinc_layout::LayoutNodeId;
use blinc_layout::binding::with_registry;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

/// The pending-write queue is process-wide, and a private graph restarts its
/// signal ids at zero.
static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    with_registry(|_| {});
    guard
}

fn flag(graph: &Arc<Mutex<ReactiveGraph>>, initial: bool) -> State<bool> {
    let signal = graph.lock().unwrap().create_signal(initial);
    State::new(signal, Arc::clone(graph), Arc::new(AtomicBool::new(false)))
}

/// Apply what the signals queued and lay out, as a runner does.
fn frame(tree: &mut RenderTree) {
    assert!(
        blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
        "a change queued a subtree rebuild"
    );
    let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(200.0, 400.0);
}

/// A column of `a` (10 high), whatever `when` adds, and `c` (30 high).
/// Returns the tree and the root's children in order.
fn column(
    build: impl FnOnce(blinc_layout::div::Div) -> blinc_layout::div::Div,
) -> (RenderTree, Vec<LayoutNodeId>) {
    let ui = build(div().w(200.0).flex_col().child(div().h(10.0))).child(div().h(30.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(200.0, 400.0);
    let kids = tree.layout_tree.children(tree.root().unwrap());
    (tree, kids)
}

fn y(tree: &RenderTree, node: LayoutNodeId) -> f32 {
    tree.get_absolute_bounds(node).unwrap().y
}

fn hidden(tree: &RenderTree, node: LayoutNodeId) -> bool {
    tree.layout_tree.is_display_none(node)
}

#[test]
fn a_constant_condition_applies_the_closure_or_not() {
    let _g = serial();
    let (_, kids) = column(|d| d.when(true, |d| d.child(div().h(20.0))));
    assert_eq!(kids.len(), 3);

    let (_, kids) = column(|d| d.when(false, |d| d.child(div().h(20.0))));
    assert_eq!(kids.len(), 2);
}

#[test]
fn the_added_child_follows_the_signal_in_place() {
    let _g = serial();
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let show = flag(&graph, true);
    let (mut tree, kids) = column(|d| d.when(&show, |d| d.child(div().h(20.0))));
    let (b, c) = (kids[1], kids[2]);
    assert!(!hidden(&tree, b));
    assert_eq!(y(&tree, c), 30.0, "c sits below a and b");

    show.set(false);
    frame(&mut tree);
    assert!(hidden(&tree, b));
    assert_eq!(y(&tree, c), 10.0, "c moved up into b's place");

    show.set(true);
    frame(&mut tree);
    assert!(!hidden(&tree, b));
    assert_eq!(y(&tree, c), 30.0);
    assert_eq!(
        tree.get_absolute_bounds(b).unwrap().height,
        20.0,
        "b comes back as it was built"
    );
}

#[test]
fn a_condition_that_starts_false_builds_the_child_hidden() {
    let _g = serial();
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let show = flag(&graph, false);
    let (mut tree, kids) = column(|d| d.when(&show, |d| d.child(div().h(20.0))));
    let (b, c) = (kids[1], kids[2]);
    assert!(hidden(&tree, b));
    assert_eq!(y(&tree, c), 10.0);

    show.set(true);
    frame(&mut tree);
    assert!(!hidden(&tree, b));
    assert_eq!(y(&tree, c), 30.0);
}

#[test]
fn a_computed_condition_works_the_same() {
    let _g = serial();
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let source = flag(&graph, false);
    let signal = source.signal();
    let derived = graph
        .lock()
        .unwrap()
        .create_derived(move |g: &ReactiveGraph| g.get(signal).unwrap_or_default());
    let shown = Computed::new(derived, Arc::clone(&graph));
    let (mut tree, kids) = column(|d| d.when(&shown, |d| d.child(div().h(20.0))));
    let (b, c) = (kids[1], kids[2]);
    // The first read records the dependency, as for any computed binding.
    let _ = shown.try_get();
    assert!(hidden(&tree, b));

    source.set(true);
    frame(&mut tree);
    assert!(!hidden(&tree, b));
    assert_eq!(y(&tree, c), 30.0);
}

#[test]
fn every_child_the_closure_adds_follows_the_signal_and_no_other() {
    let _g = serial();
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let show = flag(&graph, false);
    let (mut tree, kids) =
        column(|d| d.when(&show, |d| d.child(div().h(20.0)).child(div().h(5.0))));
    assert_eq!(kids.len(), 4);
    let (a, c) = (kids[0], kids[3]);
    assert!(!hidden(&tree, a) && !hidden(&tree, c));
    assert!(hidden(&tree, kids[1]) && hidden(&tree, kids[2]));

    show.set(true);
    frame(&mut tree);
    assert!(!hidden(&tree, kids[1]) && !hidden(&tree, kids[2]));
    assert_eq!(y(&tree, c), 35.0);
}

#[test]
fn a_child_under_two_conditions_is_shown_while_both_hold() {
    let _g = serial();
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let (outer, inner) = (flag(&graph, true), flag(&graph, true));
    let (mut tree, kids) =
        column(|d| d.when(&outer, |d| d.when(&inner, |d| d.child(div().h(20.0)))));
    let b = kids[1];
    assert!(!hidden(&tree, b));

    inner.set(false);
    frame(&mut tree);
    assert!(hidden(&tree, b));

    outer.set(false);
    frame(&mut tree);
    assert!(hidden(&tree, b));

    inner.set(true);
    frame(&mut tree);
    assert!(hidden(&tree, b), "still hidden: the outer condition fails");

    outer.set(true);
    frame(&mut tree);
    assert!(!hidden(&tree, b));
}
