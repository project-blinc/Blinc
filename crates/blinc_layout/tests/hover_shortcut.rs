//! The pointer may skip the hit test while it stays inside the element it last
//! hit, but not where something drawn above that element takes the pointer.

use blinc_layout::div::{Div, div};
use blinc_layout::event_router::EventRouter;
use blinc_layout::renderer::RenderTree;

fn hovering(ui: &Div, x: f32, y: f32) -> EventRouter {
    let mut tree = RenderTree::from_element(ui);
    tree.compute_layout(200.0, 200.0);
    let mut router = EventRouter::new();
    router.on_mouse_move(&tree, x, y);
    router
}

fn square(x: f32, y: f32, side: f32) -> Div {
    div().absolute().left(x).top(y).w(side).h(side)
}

/// Whether the shortcut holds at `inside` after a move that hit at `(5, 5)`.
fn holds(ui: &Div, inside: (f32, f32)) -> bool {
    hovering(ui, 5.0, 5.0).cursor_inside_last_leaf(inside.0, inside.1)
}

fn page() -> Div {
    div().w(200.0).h(200.0)
}

#[test]
fn a_later_sibling_over_the_leaf_ends_the_shortcut() {
    let ui = page()
        .child(square(0.0, 0.0, 100.0))
        .child(square(20.0, 20.0, 40.0));
    assert!(holds(&ui, (10.0, 10.0)));
    assert!(!holds(&ui, (30.0, 30.0)));
}

#[test]
fn a_later_sibling_of_an_ancestor_ends_it_too() {
    let ui = page()
        .child(square(0.0, 0.0, 150.0).child(square(0.0, 0.0, 100.0)))
        .child(square(20.0, 20.0, 40.0));
    assert!(holds(&ui, (10.0, 10.0)));
    assert!(!holds(&ui, (30.0, 30.0)));
}

#[test]
fn what_reaches_past_a_sibling_that_does_not_clip_counts() {
    // The sibling has no size of its own; its child lies over the leaf.
    let ui = page()
        .child(square(0.0, 0.0, 100.0))
        .child(square(0.0, 0.0, 0.0).child(square(20.0, 20.0, 40.0)));
    assert!(!holds(&ui, (30.0, 30.0)));
}

#[test]
fn what_is_below_the_leaf_or_lets_the_pointer_through_does_not() {
    let below = page()
        .child(square(20.0, 20.0, 40.0))
        .child(square(0.0, 0.0, 100.0));
    assert!(holds(&below, (30.0, 30.0)));

    let decoration = page()
        .child(square(0.0, 0.0, 100.0))
        .child(square(20.0, 20.0, 40.0).pointer_events_none());
    assert!(holds(&decoration, (30.0, 30.0)));
}
