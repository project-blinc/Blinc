//! A stylesheet state rule is a layer over the node's own properties: a
//! signal-bound write made under it, or before it first applies, is what the
//! node returns to, and a rule that sets the same property keeps winning
//! while it applies.

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

const RED: (f32, f32, f32) = (1.0, 0.0, 0.0);
const BLUE: (f32, f32, f32) = (0.0, 0.0, 1.0);
const GREEN: (f32, f32, f32) = (0.0, 1.0, 0.0);

struct Scene {
    tree: RenderTree,
    router: EventRouter,
    node: LayoutNodeId,
    on: State<bool>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

/// A 100x100 node, class and id "t", whose background is blue and turns red
/// when `on` is set, under `css`.
fn scene(css: &str) -> Scene {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    with_registry(|_| {});
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let signal = graph.lock().unwrap().create_signal(false);
    let on = State::new(signal, Arc::clone(&graph), Arc::new(AtomicBool::new(false)));
    let derived = graph
        .lock()
        .unwrap()
        .create_derived(move |g: &ReactiveGraph| {
            let (r, gr, b) = if g.get(signal).unwrap_or_default() {
                RED
            } else {
                BLUE
            };
            Color::rgb(r, gr, b)
        });
    let bg = Computed::new(derived, Arc::clone(&graph));
    let ui = div()
        .w(400.0)
        .h(300.0)
        .child(div().id("t").class("t").w(100.0).h(100.0).bg(&bg));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 300.0);
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    Scene {
        tree,
        router: EventRouter::new(),
        node,
        on,
        _guard: guard,
    }
}

impl Scene {
    fn pointer(&mut self, x: f32, y: f32) {
        let _ = self.router.on_mouse_move(&self.tree, x, y);
        self.tree.apply_stylesheet_state_styles(&self.router);
    }

    fn hover(&mut self) {
        self.pointer(50.0, 50.0);
    }

    fn leave(&mut self) {
        self.pointer(300.0, 250.0);
    }

    /// Set the signal and apply what it queued, as a runner does.
    fn set(&mut self, on: bool) {
        self.on.set(on);
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
    }

    fn bg(&self) -> (f32, f32, f32) {
        match &self
            .tree
            .get_render_node(self.node)
            .unwrap()
            .props
            .background
        {
            Some(Brush::Solid(c)) => (c.r, c.g, c.b),
            other => panic!("not a solid fill: {other:?}"),
        }
    }

    fn opacity(&self) -> f32 {
        self.tree.get_render_node(self.node).unwrap().props.opacity
    }
}

const CLASS_HOVER_OPACITY: &str = ".t:hover { opacity: 0.5 }";
const ID_HOVER_OPACITY: &str = "#t:hover { opacity: 0.5 }";
const CLASS_HOVER_BG: &str = ".t:hover { background: #00ff00 }";
const ID_HOVER_BG: &str = "#t:hover { background: #00ff00 }";

fn a_change_made_while_hovered_survives_the_hover(css: &str) {
    let mut s = scene(css);
    s.hover();
    assert_eq!(s.opacity(), 0.5, "the hover rule did not apply");

    s.set(true);
    s.leave();

    assert_eq!(
        s.bg(),
        RED,
        "the bound change was lost when the hover ended"
    );
    assert_eq!(s.opacity(), 1.0);
}

fn a_change_made_before_the_first_hover_survives_it(css: &str) {
    let mut s = scene(css);
    s.set(true);
    s.hover();
    s.leave();

    assert_eq!(
        s.bg(),
        RED,
        "the bound change was lost to an older snapshot"
    );
}

fn a_change_made_between_two_hovers_survives_the_second(css: &str) {
    let mut s = scene(css);
    s.hover();
    s.leave();
    s.set(true);
    s.hover();
    s.leave();

    assert_eq!(s.bg(), RED);
}

fn a_rule_that_sets_the_property_keeps_winning_while_it_applies(css: &str) {
    let mut s = scene(css);
    assert_eq!(s.bg(), BLUE);
    s.hover();
    assert_eq!(s.bg(), GREEN, "the hover rule did not apply");

    s.set(true);
    assert_eq!(s.bg(), GREEN, "the bound write beat the hover rule");

    s.leave();
    assert_eq!(s.bg(), RED, "the node did not return to the bound value");
}

#[test]
fn class_rule_a_change_made_while_hovered_survives_the_hover() {
    a_change_made_while_hovered_survives_the_hover(CLASS_HOVER_OPACITY);
}

#[test]
fn id_rule_a_change_made_while_hovered_survives_the_hover() {
    a_change_made_while_hovered_survives_the_hover(ID_HOVER_OPACITY);
}

#[test]
fn class_rule_a_change_made_before_the_first_hover_survives_it() {
    a_change_made_before_the_first_hover_survives_it(CLASS_HOVER_OPACITY);
}

#[test]
fn id_rule_a_change_made_before_the_first_hover_survives_it() {
    a_change_made_before_the_first_hover_survives_it(ID_HOVER_OPACITY);
}

#[test]
fn class_rule_a_change_made_between_two_hovers_survives_the_second() {
    a_change_made_between_two_hovers_survives_the_second(CLASS_HOVER_OPACITY);
}

#[test]
fn id_rule_a_change_made_between_two_hovers_survives_the_second() {
    a_change_made_between_two_hovers_survives_the_second(ID_HOVER_OPACITY);
}

#[test]
fn class_rule_that_sets_the_property_keeps_winning_while_it_applies() {
    a_rule_that_sets_the_property_keeps_winning_while_it_applies(CLASS_HOVER_BG);
}

#[test]
fn id_rule_that_sets_the_property_keeps_winning_while_it_applies() {
    a_rule_that_sets_the_property_keeps_winning_while_it_applies(ID_HOVER_BG);
}

#[test]
fn a_state_rule_for_another_element_leaves_the_write_in_place() {
    let mut s = scene(".other:hover { opacity: 0.5 }");
    s.hover();
    s.set(true);
    assert_eq!(s.bg(), RED);
    s.leave();
    assert_eq!(s.bg(), RED);
}

#[test]
fn a_sheet_with_no_state_rules_takes_the_write_directly() {
    let mut s = scene(".t { opacity: 0.9 }");
    s.hover();
    s.set(true);
    assert_eq!(s.bg(), RED);
    s.leave();
    assert_eq!(s.bg(), RED);
    assert_eq!(s.opacity(), 0.9);
}

/// The same, for a bound layout property.
struct WidthScene {
    tree: RenderTree,
    router: EventRouter,
    node: LayoutNodeId,
    width: State<f32>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

/// A node, class and id "t", 50 high and 100 wide until `width` is set, under
/// `css`.
fn width_scene(css: &str) -> WidthScene {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    with_registry(|_| {});
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let signal = graph.lock().unwrap().create_signal(100.0_f32);
    let width = State::new(signal, graph, Arc::new(AtomicBool::new(false)));
    let ui = div()
        .w(400.0)
        .h(300.0)
        .child(div().id("t").class("t").w(&width).h(50.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 300.0);
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    WidthScene {
        tree,
        router: EventRouter::new(),
        node,
        width,
        _guard: guard,
    }
}

impl WidthScene {
    fn pointer(&mut self, x: f32, y: f32) {
        let _ = self.router.on_mouse_move(&self.tree, x, y);
        self.tree.apply_stylesheet_state_styles(&self.router);
        self.tree.compute_layout(400.0, 300.0);
    }

    fn hover(&mut self) {
        self.pointer(20.0, 20.0);
    }

    fn leave(&mut self) {
        self.pointer(390.0, 290.0);
    }

    fn set(&mut self, width: f32) {
        self.width.set(width);
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 300.0);
    }

    fn shown(&self) -> f32 {
        self.tree.get_absolute_bounds(self.node).unwrap().width
    }
}

#[test]
fn class_rule_a_width_changed_while_hovered_survives_the_hover() {
    let mut s = width_scene(CLASS_HOVER_OPACITY);
    s.hover();
    s.set(180.0);
    assert_eq!(s.shown(), 180.0);
    s.leave();
    assert_eq!(
        s.shown(),
        180.0,
        "the bound width was lost when the hover ended"
    );
}

#[test]
fn id_rule_a_width_changed_while_hovered_survives_the_hover() {
    let mut s = width_scene(ID_HOVER_OPACITY);
    s.hover();
    s.set(180.0);
    s.leave();
    assert_eq!(
        s.shown(),
        180.0,
        "the bound width was lost when the hover ended"
    );
}

#[test]
fn a_rule_that_sets_the_width_keeps_winning_while_it_applies() {
    let mut s = width_scene(".t:hover { width: 150px }");
    assert_eq!(s.shown(), 100.0);
    s.hover();
    assert_eq!(s.shown(), 150.0, "the hover rule did not apply");

    s.set(180.0);
    assert_eq!(s.shown(), 150.0, "the bound write beat the hover rule");

    s.leave();
    assert_eq!(
        s.shown(),
        180.0,
        "the node did not return to the bound width"
    );
}
