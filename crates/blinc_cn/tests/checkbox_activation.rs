mod common;

use blinc_core::reactive::{Signal, signal};
use blinc_layout::{
    LayoutNodeId,
    accessibility::AccessibilityAction,
    div::div,
    event_router::{EventRouter, MouseButton},
    renderer::RenderTree,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn click(tree: &mut RenderTree, router: &mut EventRouter, node: LayoutNodeId) {
    let bounds = tree.get_absolute_bounds(node).unwrap();
    let (x, y) = (
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    let mut events = router.on_mouse_move(tree, x, y);
    events.extend(router.on_mouse_down(tree, x, y, MouseButton::Left));
    events.extend(router.on_mouse_up(tree, x, y, MouseButton::Left));
    for (node, event) in events {
        tree.dispatch_event(node, event, x, y);
    }
}

fn check(tree: &mut RenderTree, checked: Signal<bool>, calls: &AtomicUsize, expected: usize) {
    tree.apply_partial_property_updates(blinc_layout::take_pending_partial_prop_updates());
    assert_eq!(calls.load(Ordering::SeqCst), expected);
    assert_eq!(checked.get(), expected % 2 == 1);
    assert_eq!(tree.accessibility_nodes()[0].checked, Some(checked.get()));
}

#[test]
fn box_label_keyboard_and_assistive_actions_each_toggle_once() {
    common::guarded(|| {
        common::init();
        let checked = signal(false);
        let disabled = signal(false);
        let calls = Arc::new(AtomicUsize::new(0));
        let callback = calls.clone();
        let mut tree = RenderTree::from_element(
            &div().w(300.0).h(100.0).child(
                blinc_cn::checkbox(checked)
                    .label("Notifications")
                    .disabled(disabled)
                    .on_change(move |_| {
                        callback.fetch_add(1, Ordering::SeqCst);
                    }),
            ),
        );
        tree.compute_layout(300.0, 100.0);
        let row = tree.layout_tree.children(tree.root().unwrap())[0];
        let children = tree.layout_tree.children(row);
        let (box_node, label) = (children[0], children[1]);
        assert_eq!(tree.tab_stops(), [box_node]);
        let id = tree.accessibility_nodes()[0].id;
        let mut router = EventRouter::new();

        click(&mut tree, &mut router, box_node);
        check(&mut tree, checked, &calls, 1);
        click(&mut tree, &mut router, label);
        check(&mut tree, checked, &calls, 2);
        assert!(tree.handle_navigation_key(&mut router, 9, false));
        assert!(tree.handle_navigation_key(&mut router, 32, false));
        check(&mut tree, checked, &calls, 3);
        assert!(tree.perform_accessibility_action(id, AccessibilityAction::Activate));
        check(&mut tree, checked, &calls, 4);
        click(&mut tree, &mut router, label);
        check(&mut tree, checked, &calls, 5);

        disabled.set(true);
        check(&mut tree, checked, &calls, 5);
        click(&mut tree, &mut router, box_node);
        click(&mut tree, &mut router, label);
        assert!(!tree.perform_accessibility_action(id, AccessibilityAction::Activate));
        check(&mut tree, checked, &calls, 5);
    });
}
