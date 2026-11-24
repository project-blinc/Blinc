//! A border width and a whole shadow stack can follow a signal, in place.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use blinc_core::reactive::{ReactiveGraph, State};
use blinc_core::{Color, Shadow};
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

fn state<T: Clone + Send + 'static>(value: T) -> State<T> {
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let signal = graph.lock().unwrap().create_signal(value);
    State::new(signal, graph, Arc::new(AtomicBool::new(false)))
}

fn frame(tree: &mut RenderTree) {
    assert!(
        blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
        "a change queued a subtree rebuild"
    );
    let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
    assert!(!updates.is_empty(), "a change queued no property write");
    tree.apply_partial_property_updates(updates);
}

fn shadow(blur: f32) -> Shadow {
    Shadow::new(0.0, 2.0, blur, Color::BLACK)
}

#[test]
fn a_border_width_follows_its_signal() {
    let _g = serial();
    let width = state(1.0_f32);
    let mut tree = RenderTree::from_element(&div().child(div().border_width(&width)));
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    assert_eq!(tree.get_render_node(node).unwrap().props.border_width, 1.0);

    width.set(3.0);
    frame(&mut tree);

    assert_eq!(tree.get_render_node(node).unwrap().props.border_width, 3.0);
}

#[test]
fn a_constant_border_width_still_works() {
    let _g = serial();
    let tree = RenderTree::from_element(&div().child(div().border_width(2.0)));
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    assert_eq!(tree.get_render_node(node).unwrap().props.border_width, 2.0);
}

#[test]
fn a_shadow_stack_follows_its_signal_and_can_be_taken_away() {
    let _g = serial();
    let stack = state(vec![shadow(4.0), shadow(8.0)]);
    let mut tree = RenderTree::from_element(&div().child(div().shadows(&stack)));
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    assert_eq!(tree.get_render_node(node).unwrap().props.shadow.len(), 2);

    stack.set(Vec::new());
    frame(&mut tree);
    assert!(tree.get_render_node(node).unwrap().props.shadow.is_empty());

    stack.set(vec![shadow(6.0)]);
    frame(&mut tree);
    let shown = &tree.get_render_node(node).unwrap().props.shadow;
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].blur, 6.0);
}
