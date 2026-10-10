//! `Div::collapsed_when`: an element collapses to no height and opens to
//! its content's height again, in place, with what follows it moving up and
//! back down.

use blinc_core::reactive::{computed, signal};
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

fn bounds(tree: &RenderTree, node: LayoutNodeId) -> (f32, f32) {
    let b = tree.get_absolute_bounds(node).unwrap();
    (b.y, b.height)
}

/// A column: a section holding 50px of content, then a 10px footer.
fn column(section: blinc_layout::div::Div) -> (RenderTree, LayoutNodeId, LayoutNodeId) {
    let ui = div()
        .w(200.0)
        .flex_col()
        .child(section.overflow_clip().child(div().w_full().h(50.0)))
        .child(div().w_full().h(10.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(200.0, 400.0);
    let root = tree.root().unwrap();
    let kids = tree.layout_tree.children(root);
    (tree, kids[0], kids[1])
}

#[test]
fn a_section_collapses_and_opens_in_place() {
    let _g = serial();
    let collapsed = signal(true);
    let (mut tree, section, footer) = column(div().collapsed_when(collapsed));
    assert_eq!(bounds(&tree, section).1, 0.0);
    assert_eq!(
        bounds(&tree, footer).0,
        0.0,
        "the footer sits below a collapsed section"
    );

    collapsed.set(false);
    frame(&mut tree);
    assert_eq!(bounds(&tree, section).1, 50.0, "the section did not open");
    assert_eq!(bounds(&tree, footer).0, 50.0);

    collapsed.set(true);
    frame(&mut tree);
    assert_eq!(
        bounds(&tree, section).1,
        0.0,
        "the section did not collapse again"
    );
    assert_eq!(bounds(&tree, footer).0, 0.0);
}

#[test]
fn a_computed_condition_is_followed_too() {
    let _g = serial();
    let open = signal(false);
    let closed = computed(move |g| !g.get(open).unwrap_or(false));
    let (mut tree, section, _) = column(div().collapsed_when(&closed));
    assert_eq!(bounds(&tree, section).1, 0.0);
    open.set(true);
    frame(&mut tree);
    assert_eq!(bounds(&tree, section).1, 50.0);
}

#[test]
fn a_constant_is_decided_once() {
    let _g = serial();
    let (tree, section, _) = column(div().collapsed_when(true));
    assert_eq!(bounds(&tree, section).1, 0.0);
    let (tree, section, _) = column(div().collapsed_when(false));
    assert_eq!(bounds(&tree, section).1, 50.0);
}
