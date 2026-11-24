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

    /// `frame`, with the pass a runner makes over the stylesheet's layout rules whenever
    /// layout is needed.
    fn frame_as_a_runner(&mut self) {
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.apply_stylesheet_layout_overrides();
        self.tree.compute_layout(400.0, 300.0);
    }

    /// Let the transitions run `ms`, as a runner does each frame.
    fn tick(&mut self, ms: f32) {
        self.tree.css_anim_store().lock().unwrap().tick(ms);
        self.tree.apply_all_css_transition_props();
        if self.tree.apply_animated_layout_props() {
            self.tree.compute_layout(400.0, 300.0);
        }
    }

    fn height(&self, node: LayoutNodeId) -> f32 {
        self.tree.get_absolute_bounds(node).unwrap().height
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
fn a_layout_pass_between_toggles_does_not_keep_the_rules_of_a_class() {
    let on = flag(false);
    let mut s = scene(Some(RULES), |d| d.class_when("on", &on));
    for round in 0..3 {
        on.set(true);
        s.frame_as_a_runner();
        assert_eq!(s.width(s.node), 150.0, "round {round}: the class applied");
        on.set(false);
        s.frame_as_a_runner();
        assert_eq!(s.width(s.node), 100.0, "round {round}: the class stayed");
    }
}

#[test]
fn a_layout_pass_leaves_a_class_that_holds_from_the_start_removable() {
    let on = flag(true);
    let mut s = scene(Some(RULES), |d| d.class_when("on", &on));
    s.frame_as_a_runner();
    assert_eq!(s.width(s.node), 150.0);
    on.set(false);
    s.frame_as_a_runner();
    assert_eq!(s.width(s.node), 100.0);
    on.set(true);
    s.frame_as_a_runner();
    assert_eq!(s.width(s.node), 150.0);
}

#[test]
fn a_layout_pass_keeps_the_rules_a_node_has_without_the_class() {
    let on = flag(false);
    let mut s = scene(Some(".t { height: 20px } .on { height: 80px }"), |d| {
        d.class_when("on", &on)
    });
    assert_eq!(s.height(s.node), 20.0);
    on.set(true);
    s.frame_as_a_runner();
    assert_eq!(s.height(s.node), 80.0);
    on.set(false);
    s.frame_as_a_runner();
    assert_eq!(
        s.height(s.node),
        20.0,
        "back to the rule the node always had"
    );
    assert_eq!(s.height(s.other), 20.0);
}

#[test]
fn a_transition_plays_each_time_a_class_comes_and_goes() {
    let on = flag(false);
    let mut s = scene(
        Some(".t { transition: width 300ms linear } .on { width: 200px }"),
        |d| d.class_when("on", &on),
    );
    for round in 0..3 {
        for (want, to) in [(true, 200.0), (false, 100.0)] {
            on.set(want);
            s.frame_as_a_runner();
            s.tick(100.0);
            let mid = s.width(s.node);
            assert!(
                mid > 101.0 && mid < 199.0,
                "round {round}, to {to}: the width was {mid} a third of the way"
            );
            for _ in 0..5 {
                s.tick(100.0);
            }
            assert_eq!(s.width(s.node), to, "round {round}: it did not arrive");
        }
    }
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

#[test]
fn a_checked_attribute_toggled_in_place_restyles_a_checked_rule() {
    let on = flag(false);
    let mut s = scene(
        Some(".t:checked { background: #ff0000; width: 150px }"),
        |d| d.attr_when("checked", "", &on),
    );
    assert_eq!(s.fill(s.node), BLUE);

    on.set(true);
    s.frame();
    assert_eq!(s.fill(s.node), RED);
    assert_eq!(s.width(s.node), 150.0);
    assert_eq!(s.fill(s.other), BLUE);

    on.set(false);
    s.frame();
    assert_eq!(s.fill(s.node), BLUE);
    assert_eq!(s.width(s.node), 100.0);
}

#[test]
fn a_disabled_attribute_toggled_in_place_restyles_a_disabled_rule() {
    let off = flag(true);
    let mut s = scene(Some(".t:disabled { opacity: 0.4 }"), |d| {
        d.attr_when("disabled", "", &off)
    });
    assert_eq!(s.opacity(s.node), 0.4);

    off.set(false);
    s.frame();
    assert_eq!(s.opacity(s.node), 1.0);
}

#[test]
fn a_later_rule_the_node_already_matches_stays_on_top_of_a_class() {
    let on = flag(false);
    let mut s = scene(
        Some(".t:checked { background: #ff0000 } .t:disabled { background: #00ff00 }"),
        |d| d.attr("disabled", "").attr_when("checked", "", &on),
    );
    assert_eq!(s.fill(s.node), GREEN);

    on.set(true);
    s.frame();
    assert_eq!(s.fill(s.node), GREEN, "the earlier :checked rule won");

    on.set(false);
    s.frame();
    assert_eq!(s.fill(s.node), GREEN);
}

#[test]
fn an_attribute_with_two_bound_values_reaches_a_child_rule_either_way() {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    with_registry(|_| {});
    let open = flag(false);
    let closed = flag(true);
    let ui = div().w(400.0).h(300.0).child(
        div()
            .class("t")
            .attr_when("data-state", "open", &open)
            .attr_when("data-state", "closed", &closed)
            .child(
                div()
                    .class("c")
                    .w(10.0)
                    .h(10.0)
                    .bg(Color::rgb(0.0, 0.0, 1.0)),
            ),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(
        Stylesheet::parse(".t[data-state=\"open\"] > .c { background: #ff0000 }").expect("css"),
    );
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 300.0);
    let t = tree.layout_tree.children(tree.root().unwrap())[0];
    let c = tree.layout_tree.children(t)[0];
    let fill = |tree: &RenderTree| match &tree.get_render_node(c).unwrap().props.background {
        Some(Brush::Solid(col)) => (col.r, col.g, col.b),
        other => panic!("{other:?}"),
    };
    let land = |tree: &mut RenderTree| {
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        tree.apply_partial_property_updates(updates);
    };
    assert_eq!(fill(&tree), BLUE);

    // The open value lands while the closed one is still listed.
    open.set(true);
    land(&mut tree);
    closed.set(false);
    land(&mut tree);
    assert_eq!(fill(&tree), RED, "opened");

    closed.set(true);
    land(&mut tree);
    open.set(false);
    land(&mut tree);
    assert_eq!(fill(&tree), BLUE, "closed again");
    for node in [tree.root().unwrap(), t, c] {
        blinc_layout::binding::unregister_node(node);
    }
}

/// With no stylesheet there is nothing to apply, but a bound attribute is
/// still the element's to query.
#[test]
fn a_bound_attribute_is_recorded_without_a_stylesheet() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let on = blinc_core::reactive::signal(false);
    let host = div().child(div().attr_when("data-state", "open", on));
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(100.0, 100.0);
    let node = tree.layout_tree.children(tree.root().unwrap())[0];
    assert_eq!(tree.element_registry().attribute(node, "data-state"), None);

    on.set(true);
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    assert_eq!(
        tree.element_registry()
            .attribute(node, "data-state")
            .as_deref(),
        Some("open")
    );
}

#[test]
fn a_bound_attribute_that_sets_display_none_takes_the_box_out_and_back() {
    let hidden = flag(false);
    let mut s = scene(Some(".t[data-hidden] { display: none }"), |d| {
        d.attr_when("data-hidden", "", &hidden)
    });
    let top = |s: &Scene, node| s.tree.get_absolute_bounds(node).unwrap().y;
    assert!(!s.tree.layout_tree.is_display_none(s.node));
    assert_eq!(top(&s, s.other), 50.0, "the second box is below the first");

    hidden.set(true);
    s.frame();
    assert!(
        s.tree.layout_tree.is_display_none(s.node),
        "the rule's display was not applied"
    );
    assert_eq!(top(&s, s.other), 0.0, "a hidden box takes no space");

    hidden.set(false);
    s.frame();
    assert!(!s.tree.layout_tree.is_display_none(s.node));
    assert_eq!(top(&s, s.other), 50.0, "the box did not come back");
}

#[test]
fn a_bound_class_that_sets_display_none_takes_the_box_out_and_back() {
    let hidden = flag(false);
    let mut s = scene(Some(".gone { display: none }"), |d| {
        d.class_when("gone", &hidden)
    });
    hidden.set(true);
    s.frame();
    assert!(s.tree.layout_tree.is_display_none(s.node));
    hidden.set(false);
    s.frame();
    assert!(!s.tree.layout_tree.is_display_none(s.node));
}
