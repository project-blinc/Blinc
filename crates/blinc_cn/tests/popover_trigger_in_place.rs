//! A popover's trigger is built once: opening the popover swaps the trigger
//! for the open form its builder makes, in place.

mod common;

use blinc_layout::LayoutNodeId;
use blinc_layout::div::{Div, div};
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::overlay_state::overlay_stack;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::text::text;
use common::*;

fn texts(tree: &RenderTree, from: LayoutNodeId) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![from];
    while let Some(node) = stack.pop() {
        if let Some(ElementType::Text(t)) = tree.get_render_node(node).map(|r| &r.element_type) {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(node));
    }
    out
}

#[test]
fn opening_the_popover_swaps_the_trigger_in_place() {
    guarded(|| {
        init();
        let popover = blinc_cn::popover(|open: bool| -> Div {
            div()
                .w(120.0)
                .h(32.0)
                .child(text(if open { "open" } else { "closed" }))
        })
        .content(|| div().w(200.0).h(100.0));
        let host = div().w(400.0).h(300.0).child(popover);
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(400.0, 300.0);
        let trigger = tree.layout_tree.children(tree.root().unwrap())[0];
        assert_eq!(texts(&tree, trigger), vec!["closed".to_string()]);

        let b = tree.get_absolute_bounds(trigger).unwrap();
        let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
        let before = overlay_stack().lock().unwrap().len();
        let mut router = EventRouter::new();
        let mut events = router.on_mouse_move(&tree, x, y);
        events.extend(router.on_mouse_down(&tree, x, y, MouseButton::Left));
        events.extend(router.on_mouse_up(&tree, x, y, MouseButton::Left));
        for (node, event) in events {
            tree.dispatch_event(node, event, x, y);
        }
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "opening the popover queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        tree.apply_partial_property_updates(updates);
        tree.compute_layout(400.0, 300.0);

        assert_eq!(
            overlay_stack().lock().unwrap().len(),
            before + 1,
            "no popover opened"
        );
        assert_eq!(
            texts(&tree, trigger),
            vec!["open".to_string()],
            "the trigger kept its closed form"
        );
    })
}
