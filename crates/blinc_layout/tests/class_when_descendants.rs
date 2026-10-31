//! A class or attribute that follows a signal restyles the node's
//! descendants too, and eases them through their `transition`.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use blinc_core::reactive::{ReactiveGraph, State};
use blinc_core::{Brush, Color};
use blinc_layout::LayoutNodeId;
use blinc_layout::binding::with_registry;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

static LOCK: Mutex<()> = Mutex::new(());

fn flag(initial: bool) -> State<bool> {
    let graph = Arc::new(Mutex::new(ReactiveGraph::new()));
    let signal = graph.lock().unwrap().create_signal(initial);
    State::new(signal, graph, Arc::new(AtomicBool::new(false)))
}

struct Scene {
    tree: RenderTree,
    parent: LayoutNodeId,
    child: LayoutNodeId,
    _guard: std::sync::MutexGuard<'static, ()>,
}

/// A `.switch` holding a `.thumb`; the switch is `checked` while `on`.
fn scene(css: &str, on: &State<bool>) -> Scene {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    with_registry(|_| {});
    let ui = div().w(200.0).h(100.0).child(
        div()
            .class("switch")
            .w(80.0)
            .h(40.0)
            .attr_when("checked", "", on)
            .child(
                div()
                    .class("thumb")
                    .w(20.0)
                    .h(20.0)
                    .bg(Color::rgb(0.0, 0.0, 1.0)),
            ),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(200.0, 100.0);
    let parent = tree.layout_tree.children(tree.root().unwrap())[0];
    let child = tree.layout_tree.children(parent)[0];
    Scene {
        tree,
        parent,
        child,
        _guard: guard,
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        let root = self.tree.root().unwrap();
        for node in [root, self.parent, self.child] {
            blinc_layout::binding::unregister_node(node);
        }
    }
}

impl Scene {
    fn frame(&mut self) {
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.apply_all_css_transition_props();
        self.tree.compute_layout(200.0, 100.0);
    }

    fn tick(&mut self, ms: f32) {
        let mut left = ms;
        while left > 0.0 {
            let step = left.min(50.0);
            self.tree.css_anim_store().lock().unwrap().tick(step);
            self.tree.apply_all_css_transition_props();
            left -= step;
        }
    }

    fn red(&self, node: LayoutNodeId) -> f32 {
        match &self.tree.get_render_node(node).unwrap().props.background {
            Some(Brush::Solid(c)) => c.r,
            other => panic!("not a solid fill: {other:?}"),
        }
    }
}

const CHECKED_THUMB: &str = ".switch:checked > .thumb { background: #ff0000 }";

#[test]
fn a_rule_about_a_child_follows_its_parents_attribute_both_ways() {
    let on = flag(false);
    let mut s = scene(CHECKED_THUMB, &on);
    assert_eq!(s.red(s.child), 0.0);

    on.set(true);
    s.frame();
    assert_eq!(s.red(s.child), 1.0, "the child did not follow its parent");

    on.set(false);
    s.frame();
    assert_eq!(s.red(s.child), 0.0, "the child kept the rule");
}

#[test]
fn a_child_rule_that_holds_from_the_start_can_be_taken_away() {
    let on = flag(true);
    let mut s = scene(CHECKED_THUMB, &on);
    assert_eq!(s.red(s.child), 1.0);
    on.set(false);
    s.frame();
    assert_eq!(s.red(s.child), 0.0);
}

#[test]
fn a_child_eases_to_what_a_toggle_gives_it() {
    let on = flag(false);
    let mut s = scene(
        ".thumb { transition: background-color 200ms linear }\n\
         .switch:checked > .thumb { background: #ff0000 }",
        &on,
    );
    on.set(true);
    s.frame();
    assert_eq!(s.red(s.child), 0.0, "the fill jumped");
    s.tick(100.0);
    let mid = s.red(s.child);
    assert!((0.4..0.6).contains(&mid), "half way: {mid}");
    s.tick(150.0);
    assert_eq!(s.red(s.child), 1.0);
}

#[test]
fn the_toggled_node_eases_too() {
    let on = flag(false);
    let mut s = scene(
        ".switch { background: #0000ff; transition: background-color 200ms linear }\n\
         .switch:checked { background: #ff0000 }",
        &on,
    );
    on.set(true);
    s.frame();
    assert_eq!(s.red(s.parent), 0.0, "the fill jumped");
    s.tick(100.0);
    let mid = s.red(s.parent);
    assert!((0.4..0.6).contains(&mid), "half way: {mid}");
}

/// The runners build a tree with `apply_all_stylesheet_styles`: a class that
/// holds from the start must apply there, as in `apply_stylesheet_base_styles`.
#[test]
fn the_full_style_pass_applies_a_class_that_holds_at_mount() {
    let on = flag(true);
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    with_registry(|_| {});
    let ui = div().w(200.0).h(100.0).child(
        div()
            .class("switch")
            .w(80.0)
            .h(40.0)
            .attr_when("checked", "", &on)
            .child(
                div()
                    .class("thumb")
                    .w(20.0)
                    .h(20.0)
                    .bg(Color::rgb(0.0, 0.0, 1.0)),
            ),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(
        Stylesheet::parse(
            ".switch:checked { background: #00ff00 }\n.switch:checked > .thumb { background: #ff0000 }",
        )
        .expect("css"),
    );
    tree.apply_all_stylesheet_styles();
    let parent = tree.layout_tree.children(tree.root().unwrap())[0];
    let child = tree.layout_tree.children(parent)[0];
    let fill = |n| match &tree.get_render_node(n).unwrap().props.background {
        Some(Brush::Solid(c)) => (c.r, c.g),
        other => panic!("{other:?}"),
    };
    assert_eq!(fill(parent), (0.0, 1.0), "the switch did not take :checked");
    assert_eq!(fill(child), (1.0, 0.0), "the thumb did not take :checked");
    for node in [tree.root().unwrap(), parent, child] {
        blinc_layout::binding::unregister_node(node);
    }
    drop(guard);
}

/// The translation a node is painted with: the transition's value while one
/// plays (a promoted layer is moved by it), else the node's own transform.
fn translate_x(s: &Scene, node: LayoutNodeId) -> f32 {
    if let Some(stable) = s.tree.stable_id(node) {
        let store = s.tree.css_anim_store();
        let store = store.lock().unwrap();
        if let Some(t) = store.transitions.get(&stable).filter(|t| t.is_playing) {
            if let Some(x) = t.current_properties.translate_x {
                return x;
            }
        }
    }
    match &s.tree.get_render_node(node).unwrap().props.transform {
        Some(blinc_core::Transform::Affine2D(a)) => a.elements[4],
        None => 0.0,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_child_slides_to_what_a_toggle_gives_it() {
    let on = flag(false);
    let mut s = scene(
        ".thumb { transition: transform 200ms linear }\n\
         .switch:checked > .thumb { transform: translateX(20px) }",
        &on,
    );
    assert_eq!(translate_x(&s, s.child), 0.0);
    on.set(true);
    s.frame();
    assert_eq!(translate_x(&s, s.child), 0.0, "the child jumped");
    s.tick(100.0);
    let mid = translate_x(&s, s.child);
    assert!((9.0..11.0).contains(&mid), "half way: {mid}");
    s.tick(150.0);
    assert_eq!(translate_x(&s, s.child), 20.0);
}

/// What a node is built with is not a change a viewer sees: no easing.
#[test]
fn a_class_that_holds_at_mount_does_not_ease_in() {
    let on = flag(true);
    let s = scene(
        ".thumb { transition: background-color 200ms linear }\n\
         .switch:checked > .thumb { background: #ff0000 }",
        &on,
    );
    assert_eq!(s.red(s.child), 1.0);
    assert!(!s.tree.css_has_active(), "the mount started a transition");
}
