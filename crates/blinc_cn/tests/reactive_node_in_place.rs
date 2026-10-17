//! A node that follows a bound source swaps its element in place when the
//! source changes, with no subtree rebuilt.

mod common;

use blinc_layout::binding::Reactive;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use common::*;

fn texts(tree: &RenderTree) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if let Some(ElementType::Text(t)) = tree.get_render_node(node).map(|r| &r.element_type) {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(node));
    }
    out
}

#[test]
fn a_bound_source_swaps_the_element_in_place() {
    guarded(|| {
        init();
        let source = string_state("first");
        let node = blinc_cn::reactive_props::reactive_node(&Reactive::Bound(source.clone()), |s| {
            Box::new(blinc_layout::text::text(format!("<{s}>")))
        });
        let host = div().w(200.0).h(60.0).child_box(node);
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(200.0, 60.0);
        assert_eq!(texts(&tree), ["<first>"]);

        source.set("second".to_string());
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "the node queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        tree.apply_partial_property_updates(updates);
        tree.compute_layout(200.0, 60.0);
        assert_eq!(
            texts(&tree),
            ["<second>"],
            "the element did not follow the source"
        );
    })
}
