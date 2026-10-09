//! A tracked element's hovered, pressed and focused signals follow the
//! pointer and focus events the router emits, and a property bound to them
//! restyles the node in place.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use blinc_core::reactive::{ReactiveGraph, State};
use blinc_core::{Color, Computed};
use blinc_layout::binding::with_registry;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::RenderTree;
use blinc_layout::{Interaction, LayoutNodeId};

/// The pending-write queue is process-wide, and a private graph restarts its
/// signal ids at zero.
static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    guard
}

fn flag(graph: &Arc<Mutex<ReactiveGraph>>) -> State<bool> {
    let signal = graph.lock().unwrap().create_signal(false);
    State::new(signal, Arc::clone(graph), Arc::new(AtomicBool::new(false)))
}

fn tracked() -> (Interaction, Arc<Mutex<ReactiveGraph>>) {
    with_registry(|_| {});
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let interaction = Interaction::new(flag(&graph), flag(&graph), flag(&graph));
    (interaction, graph)
}

/// A 100x100 tracked square at the origin of a 400x300 root.
fn scene(interaction: &Interaction) -> RenderTree {
    let ui = div()
        .w(400.0)
        .h(300.0)
        .child(div().w(100.0).h(100.0).track(interaction));
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(400.0, 300.0);
    tree
}

fn deliver(tree: &mut RenderTree, events: Vec<(LayoutNodeId, u32)>, x: f32, y: f32) {
    for (node, event) in events {
        tree.dispatch_event(node, event, x, y);
    }
}

#[test]
fn hover_follows_the_pointer_in_and_out() {
    let _g = serial();
    let (i, _graph) = tracked();
    let mut tree = scene(&i);
    let mut router = EventRouter::new();

    let events = router.on_mouse_move(&tree, 50.0, 50.0);
    deliver(&mut tree, events, 50.0, 50.0);
    assert!(i.hovered().get());
    assert!(!i.pressed().get());

    let events = router.on_mouse_move(&tree, 300.0, 250.0);
    deliver(&mut tree, events, 300.0, 250.0);
    assert!(!i.hovered().get());
}

#[test]
fn press_is_set_by_down_and_cleared_by_up() {
    let _g = serial();
    let (i, _graph) = tracked();
    let mut tree = scene(&i);
    let mut router = EventRouter::new();

    let events = router.on_mouse_move(&tree, 50.0, 50.0);
    deliver(&mut tree, events, 50.0, 50.0);
    let events = router.on_mouse_down(&tree, 50.0, 50.0, MouseButton::Left);
    deliver(&mut tree, events, 50.0, 50.0);
    assert!(i.pressed().get());
    assert!(i.hovered().get());

    let events = router.on_mouse_up(&tree, 50.0, 50.0, MouseButton::Left);
    deliver(&mut tree, events, 50.0, 50.0);
    assert!(!i.pressed().get());
    assert!(i.hovered().get(), "an up over the node leaves it hovered");
}

#[test]
fn a_press_with_no_hover_first_counts_as_hovered() {
    let _g = serial();
    let (i, _graph) = tracked();
    let mut tree = scene(&i);
    let mut router = EventRouter::new();

    let events = router.on_mouse_down(&tree, 50.0, 50.0, MouseButton::Left);
    deliver(&mut tree, events, 50.0, 50.0);

    assert!(i.pressed().get());
    assert!(i.hovered().get());
}

#[test]
fn leaving_while_pressed_clears_both() {
    let _g = serial();
    let (i, _graph) = tracked();
    let mut tree = scene(&i);
    let mut router = EventRouter::new();

    let events = router.on_mouse_move(&tree, 50.0, 50.0);
    deliver(&mut tree, events, 50.0, 50.0);
    let events = router.on_mouse_down(&tree, 50.0, 50.0, MouseButton::Left);
    deliver(&mut tree, events, 50.0, 50.0);
    let events = router.on_mouse_move(&tree, 300.0, 250.0);
    deliver(&mut tree, events, 300.0, 250.0);

    assert!(!i.hovered().get());
    assert!(!i.pressed().get());
}

#[test]
fn focus_follows_the_router() {
    let _g = serial();
    let (i, _graph) = tracked();
    let mut tree = scene(&i);
    let node = tree.layout_tree.children(tree.root().unwrap())[0];

    let seen: Rc<RefCell<Vec<(LayoutNodeId, u32)>>> = Rc::default();
    let mut router = EventRouter::new();
    let sink = Rc::clone(&seen);
    router.set_event_callback(move |n, e| sink.borrow_mut().push((n, e)));

    router.set_focus(Some(node));
    deliver(&mut tree, seen.take(), 0.0, 0.0);
    assert!(i.focused().get());

    router.set_focus(None);
    deliver(&mut tree, seen.take(), 0.0, 0.0);
    assert!(!i.focused().get());
}

#[test]
fn a_property_bound_to_hover_restyles_in_place_and_queues_no_rebuild() {
    let _g = serial();
    let (i, graph) = tracked();
    let hovered = i.hovered();
    let signal = hovered.signal();
    let derived = graph
        .lock()
        .unwrap()
        .create_derived(move |g: &ReactiveGraph| {
            if g.get(signal).unwrap_or_default() {
                Color::rgb(1.0, 0.0, 0.0)
            } else {
                Color::rgb(0.0, 0.0, 1.0)
            }
        });
    let bg = Computed::new(derived, Arc::clone(&graph));

    let ui = div()
        .w(400.0)
        .h(300.0)
        .child(div().w(100.0).h(100.0).bg(&bg).track(&i));
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(400.0, 300.0);
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    let colour =
        |tree: &RenderTree| format!("{:?}", tree.get_render_node(node).unwrap().props.background);
    let before = colour(&tree);

    let mut router = EventRouter::new();
    let events = router.on_mouse_move(&tree, 50.0, 50.0);
    deliver(&mut tree, events, 50.0, 50.0);

    assert!(
        blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
        "hover queued a subtree rebuild"
    );
    let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
    assert!(!updates.is_empty(), "hover queued no property write");
    tree.apply_partial_property_updates(updates);
    assert_ne!(colour(&tree), before, "the background did not follow hover");
}

#[test]
fn a_repeated_event_wakes_no_dependent() {
    let _g = serial();
    let (i, graph) = tracked();
    let hovered = i.hovered();
    let signal = hovered.signal();
    let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&runs);
    let derived = graph
        .lock()
        .unwrap()
        .create_derived(move |g: &ReactiveGraph| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            g.get(signal).unwrap_or_default()
        });
    let seen = Computed::<bool>::new(derived, Arc::clone(&graph));
    let _ = seen.try_get();
    let after_first = runs.load(std::sync::atomic::Ordering::SeqCst);

    let mut tree = scene(&i);
    let mut router = EventRouter::new();
    let events = router.on_mouse_move(&tree, 50.0, 50.0);
    deliver(&mut tree, events, 50.0, 50.0);
    // The same enter again, as a second move inside the node would not send,
    // but a rebuilt router might.
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    tree.dispatch_event(
        node,
        blinc_core::events::event_types::POINTER_ENTER,
        50.0,
        50.0,
    );
    let _ = seen.try_get();
    let after_two = runs.load(std::sync::atomic::Ordering::SeqCst);
    tree.dispatch_event(
        node,
        blinc_core::events::event_types::POINTER_ENTER,
        50.0,
        50.0,
    );
    let _ = seen.try_get();

    assert_eq!(
        after_two,
        after_first + 1,
        "the first enter recomputes once"
    );
    assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), after_two);
}
