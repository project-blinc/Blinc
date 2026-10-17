//! An OTP input is built once: a value written to its state from anywhere
//! shows across the slots in place, with no subtree rebuilt.

mod common;

use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use common::*;

fn frame(tree: &mut RenderTree) {
    assert!(
        !blinc_layout::stateful::has_pending_subtree_rebuilds(),
        "the OTP input queued a subtree rebuild"
    );
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(400.0, 200.0);
}

/// What each slot shows, in order.
fn slots(tree: &RenderTree) -> Vec<String> {
    let registry = tree.element_registry();
    let mut found = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if registry.has_class(node, "cn-input-otp-slot") {
            found.push(shown(tree, node));
            continue;
        }
        let mut children = tree.layout_tree.children(node);
        children.reverse();
        stack.extend(children);
    }
    found
}

fn shown(tree: &RenderTree, slot: LayoutNodeId) -> String {
    let mut text = String::new();
    let mut stack = vec![slot];
    while let Some(node) = stack.pop() {
        if let Some(ElementType::Text(t)) = tree.get_render_node(node).map(|r| &r.element_type) {
            text.push_str(&t.content);
        }
        let mut children = tree.layout_tree.children(node);
        children.reverse();
        stack.extend(children);
    }
    text
}

#[test]
fn a_value_written_from_outside_shows_across_the_slots() {
    guarded(|| {
        init();
        let value = State::new(signal(String::new()), global_graph(), global_dirty_flag());
        let otp = blinc_cn::input_otp(&value, 4).numeric_only(true);
        let host = div().w(400.0).h(200.0).child(otp);
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(400.0, 200.0);
        assert_eq!(slots(&tree), ["", "", "", ""]);

        value.set("12".to_string());
        frame(&mut tree);
        assert_eq!(
            slots(&tree),
            ["1", "2", "", ""],
            "the slots did not follow the value"
        );

        value.set(String::new());
        frame(&mut tree);
        assert_eq!(slots(&tree), ["", "", "", ""]);
    })
}
