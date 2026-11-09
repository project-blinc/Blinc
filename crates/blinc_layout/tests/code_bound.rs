//! `Code::bound`: a read-only code block that follows its text in place. A changed line is
//! built again, an unchanged one keeps its node, and the gutter follows the line count.

use std::sync::Mutex;

use blinc_core::reactive::signal;
use blinc_layout::LayoutNodeId;
use blinc_layout::binding::with_registry;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::widgets::code::Code;

/// The pending-write queue and the reactive graph are process-wide.
static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    blinc_theme::ThemeState::init_default();
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
    tree.compute_layout(600.0, 400.0);
}

/// The gutter's region and the lines' column, as the block lays them out.
fn parts(tree: &RenderTree) -> (LayoutNodeId, LayoutNodeId) {
    let root = tree.root().unwrap();
    let block = tree.layout_tree.children(root)[0];
    let kids = tree.layout_tree.children(block);
    (kids[0], kids[1])
}

fn height(tree: &RenderTree, node: LayoutNodeId) -> f32 {
    tree.get_absolute_bounds(node).unwrap().height
}

#[test]
fn a_changed_line_is_built_again_and_the_rest_keep_their_nodes() {
    let _g = serial();
    let text = signal("fn a() {}\nlet b = 1;".to_string());
    let ui = div().w(600.0).child(Code::bound(text).line_numbers(true));
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(600.0, 400.0);
    let (gutter, lines) = parts(&tree);
    let before = tree.layout_tree.children(lines);
    assert_eq!(before.len(), 2);
    assert_eq!(height(&tree, gutter), height(&tree, lines));

    text.set("fn a() {}\nlet b = 2;\nlet c = 3;".to_string());
    frame(&mut tree);
    let (gutter, lines) = parts(&tree);
    let after = tree.layout_tree.children(lines);
    assert_eq!(after.len(), 3);
    assert_eq!(after[0], before[0], "an unchanged line was built again");
    assert_ne!(after[1], before[1], "a changed line kept its old row");
    assert_eq!(
        height(&tree, gutter),
        height(&tree, lines),
        "the gutter did not follow the line count"
    );
}
