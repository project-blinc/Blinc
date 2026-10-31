//! A bound colour with a transition eases to its new value, a tick at a
//! time, instead of jumping.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use blinc_animation::Easing;
use blinc_core::reactive::{ReactiveGraph, State};
use blinc_core::{Brush, Color};
use blinc_layout::LayoutNodeId;
use blinc_layout::binding::with_registry;
use blinc_layout::div::div;
use blinc_layout::property::PropertyId;
use blinc_layout::renderer::RenderTree;
use blinc_layout::transition::{tick, with_transitions_enabled};

static LOCK: Mutex<()> = Mutex::new(());

fn colour(initial: Color) -> State<Color> {
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let signal = graph.lock().unwrap().create_signal(initial);
    State::new(signal, graph, Arc::new(AtomicBool::new(false)))
}

struct Scene {
    tree: RenderTree,
    node: LayoutNodeId,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Scene {
    fn new(fill: &State<Color>, eased: bool) -> Self {
        let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
        with_registry(|_| {});
        let mut child = div().w(50.0).h(50.0).bg(fill);
        if eased {
            child = child.transition(PropertyId::Background, 100.0, Easing::Linear);
        }
        let ui = div().w(200.0).h(200.0).child(child);
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(200.0, 200.0);
        let node = tree.layout_tree.children(tree.root().unwrap())[0];
        Self {
            tree,
            node,
            _guard: guard,
        }
    }

    fn frame(&mut self) {
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
    }

    fn red(&self) -> f32 {
        match &self
            .tree
            .get_render_node(self.node)
            .unwrap()
            .props
            .background
        {
            Some(Brush::Solid(c)) => c.r,
            other => panic!("not a solid fill: {other:?}"),
        }
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        let root = self.tree.root().unwrap();
        for node in std::iter::once(root).chain(self.tree.layout_tree.children(root)) {
            blinc_layout::binding::unregister_node(node);
        }
    }
}

const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
const RED: Color = Color::rgb(1.0, 0.0, 0.0);

#[test]
fn a_change_eases_over_the_duration() {
    with_transitions_enabled(|| {
        let fill = colour(BLACK);
        let mut s = Scene::new(&fill, true);
        assert_eq!(s.red(), 0.0);

        fill.set(RED);
        s.frame();
        assert_eq!(s.red(), 0.0, "the change must not land at once");

        tick(50.0);
        s.frame();
        assert!((s.red() - 0.5).abs() < 0.02, "halfway: {}", s.red());

        tick(30.0);
        s.frame();
        assert!((s.red() - 0.8).abs() < 0.02, "{}", s.red());

        tick(30.0);
        s.frame();
        assert_eq!(s.red(), 1.0, "ends exactly on the target");
    });
}

#[test]
fn a_new_target_mid_flight_starts_from_where_it_is() {
    with_transitions_enabled(|| {
        let fill = colour(BLACK);
        let mut s = Scene::new(&fill, true);
        fill.set(RED);
        s.frame();
        tick(50.0);
        s.frame();
        let mid = s.red();

        fill.set(BLACK);
        s.frame();
        tick(1.0);
        s.frame();
        assert!(s.red() <= mid && s.red() > 0.4, "{} from {mid}", s.red());
    });
}

#[test]
fn without_a_transition_the_change_lands_at_once() {
    with_transitions_enabled(|| {
        let fill = colour(BLACK);
        let mut s = Scene::new(&fill, false);
        fill.set(RED);
        s.frame();
        assert_eq!(s.red(), 1.0);
    });
}

#[test]
fn with_transitions_off_the_change_lands_at_once() {
    let fill = colour(BLACK);
    let mut s = Scene::new(&fill, true);
    fill.set(RED);
    s.frame();
    assert_eq!(s.red(), 1.0);
}
