//! A class that follows a signal: the stylesheet's rules for it apply when it
//! is added and stop applying when it is removed, in place.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use blinc_core::reactive::{ReactiveGraph, State};
use blinc_core::{Brush, Color, Computed};
use blinc_layout::LayoutNodeId;
use blinc_layout::binding::with_registry;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::event_router::EventRouter;
use blinc_layout::renderer::RenderTree;

/// The pending-write queue is process-wide, and a private graph restarts its
/// signal ids at zero.
static LOCK: Mutex<()> = Mutex::new(());

const BLUE: (f32, f32, f32) = (0.0, 0.0, 1.0);
const RED: (f32, f32, f32) = (1.0, 0.0, 0.0);
const GREEN: (f32, f32, f32) = (0.0, 1.0, 0.0);

struct Scene {
    tree: RenderTree,
    node: LayoutNodeId,
    other: LayoutNodeId,
    _guard: std::sync::MutexGuard<'static, ()>,
}

fn flag(initial: bool) -> State<bool> {
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let signal = graph.lock().unwrap().create_signal(initial);
    State::new(signal, graph, Arc::new(AtomicBool::new(false)))
}

/// Two 100x50 blue boxes in a column, both classed "t"; the first also has
/// the class "on" while `condition` holds. `css` is the stylesheet, or none.
fn scene(
    css: Option<&str>,
    class: impl FnOnce(blinc_layout::div::Div) -> blinc_layout::div::Div,
) -> Scene {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    with_registry(|_| {});
    let boxed = || {
        div()
            .class("t")
            .w(100.0)
            .h(50.0)
            .bg(Color::rgb(0.0, 0.0, 1.0))
    };
    let ui = div()
        .w(400.0)
        .h(300.0)
        .flex_col()
        .child(class(boxed()))
        .child(boxed());
    let mut tree = RenderTree::from_element(&ui);
    if let Some(css) = css {
        tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
        tree.apply_stylesheet_layout_overrides();
        tree.apply_stylesheet_base_styles();
    }
    tree.compute_layout(400.0, 300.0);
    let kids = tree.layout_tree.children(tree.root().unwrap());
    Scene {
        tree,
        node: kids[0],
        other: kids[1],
        _guard: guard,
    }
}

/// A tree's bindings are keyed by node id, and the next test's tree reuses the
/// ids, so a scene takes its own away.
impl Drop for Scene {
    fn drop(&mut self) {
        let root = self.tree.root().unwrap();
        for node in std::iter::once(root).chain(self.tree.layout_tree.children(root)) {
            blinc_layout::binding::unregister_node(node);
        }
    }
}

impl Scene {
    /// Apply what the signals queued and lay out, as a runner does.
    fn frame(&mut self) {
        assert!(
            blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
            "a change queued a subtree rebuild"
        );
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 300.0);
    }

    fn fill(&self, node: LayoutNodeId) -> (f32, f32, f32) {
        match &self.tree.get_render_node(node).unwrap().props.background {
            Some(Brush::Solid(c)) => (c.r, c.g, c.b),
            other => panic!("not a solid fill: {other:?}"),
        }
    }

    fn opacity(&self, node: LayoutNodeId) -> f32 {
        self.tree.get_render_node(node).unwrap().props.opacity
    }

    fn width(&self, node: LayoutNodeId) -> f32 {
        self.tree.get_absolute_bounds(node).unwrap().width
    }

    fn has_class(&self, node: LayoutNodeId, class: &str) -> bool {
        self.tree.element_registry().has_class(node, class)
    }
}

const RULES: &str = ".on { background: #ff0000; width: 150px; opacity: 0.5 }";

#[test]
fn a_class_added_applies_its_rules_and_one_removed_takes_them_away() {
    let on = flag(false);
    let mut s = scene(Some(RULES), |d| d.class_when("on", &on));
    assert_eq!(s.fill(s.node), BLUE);
    assert_eq!(s.width(s.node), 100.0);
    assert!(!s.has_class(s.node, "on"));

    on.set(true);
    s.frame();
    assert_eq!(s.fill(s.node), RED);
    assert_eq!(s.width(s.node), 150.0, "a layout rule applied");
    assert_eq!(s.opacity(s.node), 0.5);
    assert!(s.has_class(s.node, "on"));
    assert_eq!(s.fill(s.other), BLUE, "another element with the base class");

    on.set(false);
    s.frame();
    assert_eq!(s.fill(s.node), BLUE, "the node went back to what it was");
    assert_eq!(s.width(s.node), 100.0);
    assert_eq!(s.opacity(s.node), 1.0);
    assert!(!s.has_class(s.node, "on"));
}

#[test]
fn a_class_that_holds_from_the_start_can_be_taken_away() {
    let on = flag(true);
    let mut s = scene(Some(RULES), |d| d.class_when("on", &on));
    assert_eq!(s.fill(s.node), RED, "applied at the start");
    assert_eq!(s.width(s.node), 150.0);

    on.set(false);
    s.frame();
    assert_eq!(s.fill(s.node), BLUE);
    assert_eq!(s.width(s.node), 100.0);
}

#[test]
fn a_computed_condition_works_the_same() {
    let source = flag(false);
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    // A computed over its own graph's signal, so the source is a signal there.
    let signal = graph.lock().unwrap().create_signal(false);
    let derived = graph
        .lock()
        .unwrap()
        .create_derived(move |g: &ReactiveGraph| g.get(signal).unwrap_or_default());
    let on = Computed::new(derived, Arc::clone(&graph));
    let _ = source;
    let mut s = scene(Some(RULES), |d| d.class_when("on", &on));
    let _ = on.try_get();
    assert_eq!(s.fill(s.node), BLUE);

    let state = State::new(signal, graph, Arc::new(AtomicBool::new(false)));
    state.set(true);
    s.frame();
    assert_eq!(s.fill(s.node), RED);
}

#[test]
fn a_more_specific_rule_for_the_class_wins() {
    let on = flag(false);
    let mut s = scene(
        Some(".on { background: #ff0000 } .t.on { background: #00ff00 }"),
        |d| d.class_when("on", &on),
    );
    on.set(true);
    s.frame();
    assert_eq!(s.fill(s.node), GREEN);

    on.set(false);
    s.frame();
    assert_eq!(s.fill(s.node), BLUE);
}

#[test]
fn a_value_bound_while_the_class_holds_is_what_the_node_returns_to() {
    let on = flag(false);
    let shade = flag(false);
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let signal = graph.lock().unwrap().create_signal(false);
    let derived = graph
        .lock()
        .unwrap()
        .create_derived(move |g: &ReactiveGraph| {
            if g.get(signal).unwrap_or_default() {
                Color::rgb(0.0, 1.0, 1.0)
            } else {
                Color::rgb(0.0, 0.0, 1.0)
            }
        });
    let bg = Computed::new(derived, Arc::clone(&graph));
    let bound = State::new(signal, graph, Arc::new(AtomicBool::new(false)));
    let _ = shade;
    let mut s = scene(Some(RULES), |d| d.bg(&bg).class_when("on", &on));
    let _ = bg.try_get();

    on.set(true);
    s.frame();
    assert_eq!(s.fill(s.node), RED);

    bound.set(true);
    s.frame();
    assert_eq!(s.fill(s.node), RED, "the class's rule wins while it holds");

    on.set(false);
    s.frame();
    assert_eq!(
        s.fill(s.node),
        (0.0, 1.0, 1.0),
        "the bound value shows again"
    );
}

#[test]
fn a_hover_rule_and_a_class_rule_layer_over_each_other() {
    let on = flag(true);
    let mut s = scene(
        Some(".on { opacity: 0.5; background: #ff0000 } .t:hover { opacity: 0.2 }"),
        |d| d.class_when("on", &on),
    );
    let mut router = EventRouter::new();
    let hover = |s: &mut Scene, router: &mut EventRouter, x: f32, y: f32| {
        let _ = router.on_mouse_move(&s.tree, x, y);
        s.tree.apply_stylesheet_state_styles(router);
    };

    hover(&mut s, &mut router, 20.0, 20.0);
    assert_eq!(
        s.opacity(s.node),
        0.2,
        "the hover rule applies over the class's"
    );
    assert_eq!(
        s.fill(s.node),
        RED,
        "the class's other rule survives the hover"
    );

    hover(&mut s, &mut router, 350.0, 250.0);
    assert_eq!(s.opacity(s.node), 0.5, "back to the class's value");
    assert_eq!(s.fill(s.node), RED);

    on.set(false);
    s.frame();
    assert_eq!(s.opacity(s.node), 1.0);
    assert_eq!(s.fill(s.node), BLUE);
}

#[test]
fn a_hover_rule_on_an_id_keeps_the_class_rules_too() {
    let on = flag(true);
    let mut s = scene(
        Some(".on { opacity: 0.5; background: #ff0000 } #n:hover { opacity: 0.2 }"),
        |d| d.id("n").class_when("on", &on),
    );
    let mut router = EventRouter::new();
    let _ = router.on_mouse_move(&s.tree, 20.0, 20.0);
    s.tree.apply_stylesheet_state_styles(&router);
    assert_eq!(s.opacity(s.node), 0.2);
    assert_eq!(
        s.fill(s.node),
        RED,
        "the class's rule was lost to the hover"
    );

    let _ = router.on_mouse_move(&s.tree, 350.0, 250.0);
    s.tree.apply_stylesheet_state_styles(&router);
    assert_eq!(s.opacity(s.node), 0.5);
    assert_eq!(s.fill(s.node), RED);
}

#[test]
fn without_a_stylesheet_nothing_happens_until_there_is_one() {
    let on = flag(false);
    let mut s = scene(None, |d| d.class_when("on", &on));
    on.set(true);
    s.frame();
    assert_eq!(s.fill(s.node), BLUE);

    s.tree
        .set_stylesheet(Stylesheet::parse(RULES).expect("css"));
    s.tree.apply_stylesheet_layout_overrides();
    s.tree.apply_stylesheet_base_styles();
    s.tree.compute_layout(400.0, 300.0);
    assert_eq!(
        s.fill(s.node),
        RED,
        "the class held, so its rules apply now"
    );
}

#[test]
fn a_constant_condition_is_an_ordinary_class() {
    {
        let s = scene(Some(RULES), |d| d.class_when("on", true));
        assert_eq!(s.fill(s.node), RED);
        assert!(s.has_class(s.node, "on"));
    }
    let s = scene(Some(RULES), |d| d.class_when("on", false));
    assert_eq!(s.fill(s.node), BLUE);
    assert!(!s.has_class(s.node, "on"));
}

#[test]
fn a_class_that_takes_an_element_out_of_flow_and_back() {
    let on = flag(false);
    let mut s = scene(Some(".on { position: absolute; top: 0; left: 0 }"), |d| {
        d.class_when("on", &on)
    });
    let top = |s: &Scene, node| s.tree.get_absolute_bounds(node).unwrap().y;
    assert_eq!(top(&s, s.other), 50.0, "the second box is below the first");

    on.set(true);
    s.frame();
    assert_eq!(top(&s, s.other), 0.0, "the first box is still in flow");

    on.set(false);
    s.frame();
    assert_eq!(top(&s, s.other), 50.0, "the first box did not come back");
}
