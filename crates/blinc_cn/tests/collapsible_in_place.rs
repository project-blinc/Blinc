//! A collapsible is built once: its section opens and shuts in place when
//! the bound state changes, from anywhere, and its trigger's chevron turns
//! and fill follows the pointer, with no subtree rebuilt.

mod common;

use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::RenderTree;
use common::*;

fn open_state(initial: bool) -> State<bool> {
    State::new(signal(initial), global_graph(), global_dirty_flag())
}

fn frame(tree: &mut RenderTree) {
    assert!(
        !blinc_layout::stateful::has_pending_subtree_rebuilds(),
        "the collapsible queued a subtree rebuild"
    );
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(400.0, 400.0);
}

fn first_child(tree: &RenderTree, node: LayoutNodeId) -> LayoutNodeId {
    tree.layout_tree.children(node)[0]
}

#[test]
fn the_section_follows_its_state_in_place() {
    guarded(|| {
        init();
        let open = open_state(false);
        let section = blinc_cn::collapsible(&open).content(|| div().w_full().h(80.0));
        let host = div().w(400.0).h(400.0).flex_col().child(section);
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(400.0, 400.0);
        // host > collapsible wrapper > folding body
        let body = first_child(&tree, first_child(&tree, tree.root().unwrap()));
        let height = |tree: &RenderTree| tree.get_absolute_bounds(body).unwrap().height;
        assert_eq!(height(&tree), 0.0, "a closed section has height");

        open.set(true);
        frame(&mut tree);
        assert_eq!(height(&tree), 80.0, "the section did not open");

        open.set(false);
        frame(&mut tree);
        assert_eq!(height(&tree), 0.0, "the section did not shut");
    })
}

#[test]
fn the_trigger_turns_its_chevron_and_follows_the_pointer() {
    guarded(|| {
        init();
        let open = open_state(false);
        let section = blinc_cn::collapsible_section("Details", &open, || div().w_full().h(40.0));
        let host = div().w(400.0).h(400.0).child(section);
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(400.0, 400.0);
        let trigger = first_child(&tree, first_child(&tree, tree.root().unwrap()));
        let chevron = tree.layout_tree.children(trigger)[1];
        let fill = |tree: &RenderTree| {
            format!(
                "{:?}",
                tree.get_render_node(trigger).unwrap().props.background
            )
        };
        let turn = |tree: &RenderTree| {
            format!(
                "{:?}",
                tree.get_render_node(chevron).unwrap().props.transform
            )
        };
        let (rest_fill, closed_turn) = (fill(&tree), turn(&tree));

        let b = tree.get_absolute_bounds(trigger).unwrap();
        let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
        let mut router = EventRouter::new();
        for (node, event) in router.on_mouse_move(&tree, x, y) {
            tree.dispatch_event(node, event, x, y);
        }
        frame(&mut tree);
        assert_ne!(fill(&tree), rest_fill, "hovering did not change the fill");

        let mut events = router.on_mouse_down(&tree, x, y, MouseButton::Left);
        events.extend(router.on_mouse_up(&tree, x, y, MouseButton::Left));
        for (node, event) in events {
            tree.dispatch_event(node, event, x, y);
        }
        frame(&mut tree);
        assert!(open.get(), "clicking the trigger did not open the section");
        assert_ne!(turn(&tree), closed_turn, "the chevron did not turn");
    })
}
