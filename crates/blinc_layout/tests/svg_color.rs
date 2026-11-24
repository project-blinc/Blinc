//! An SVG's colour can follow a signal: the change is written after build and
//! queues no rebuild.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use blinc_core::reactive::{ReactiveGraph, State};
use blinc_core::{Color, Computed};
use blinc_layout::binding::with_registry;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::svg::svg;

/// The pending-write queue is process-wide, and a private graph restarts its
/// signal ids at zero.
static LOCK: Mutex<()> = Mutex::new(());

const SOURCE: &str = "<svg viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>";

fn serial() -> std::sync::MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    with_registry(|_| {});
    guard
}

fn state(graph: &Arc<Mutex<ReactiveGraph>>, color: Color) -> State<Color> {
    let signal = graph.lock().unwrap().create_signal(color);
    State::new(signal, Arc::clone(graph), Arc::new(AtomicBool::new(false)))
}

fn tint(tree: &RenderTree) -> Option<[f32; 4]> {
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    tree.get_render_node(node).unwrap().props.svg_tint
}

fn drain(tree: &mut RenderTree) {
    assert!(
        blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
        "the change queued a subtree rebuild"
    );
    let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
    assert!(!updates.is_empty(), "the change queued no property write");
    tree.apply_partial_property_updates(updates);
}

#[test]
fn a_state_colour_follows_its_signal() {
    let _g = serial();
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let colour = state(&graph, Color::rgb(1.0, 0.0, 0.0));
    let mut tree = RenderTree::from_element(&div().child(svg(SOURCE).color(&colour)));
    assert_eq!(tint(&tree), None, "nothing is written before a change");

    colour.set(Color::rgb(0.0, 1.0, 0.0));
    drain(&mut tree);

    assert_eq!(tint(&tree), Some([0.0, 1.0, 0.0, 1.0]));
}

#[test]
fn a_computed_colour_follows_its_source() {
    let _g = serial();
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let flag = {
        let signal = graph.lock().unwrap().create_signal(false);
        State::new(signal, Arc::clone(&graph), Arc::new(AtomicBool::new(false)))
    };
    let signal = flag.signal();
    let derived = graph
        .lock()
        .unwrap()
        .create_derived(move |g: &ReactiveGraph| {
            if g.get(signal).unwrap_or_default() {
                Color::rgb(0.0, 0.0, 1.0)
            } else {
                Color::rgb(1.0, 0.0, 0.0)
            }
        });
    let colour = Computed::new(derived, Arc::clone(&graph));
    let mut tree = RenderTree::from_element(&div().child(svg(SOURCE).color(&colour)));
    // The first read records the dependency, as for any computed binding.
    let _ = colour.try_get();

    flag.set(true);
    drain(&mut tree);

    assert_eq!(tint(&tree), Some([0.0, 0.0, 1.0, 1.0]));
}

#[test]
fn a_constant_colour_is_the_built_tint_and_writes_nothing() {
    let _g = serial();
    let tree = RenderTree::from_element(&div().child(svg(SOURCE).color(Color::rgb(1.0, 0.0, 0.0))));
    assert_eq!(tint(&tree), None);
    assert!(blinc_layout::stateful::take_pending_partial_prop_updates().is_empty());
}
