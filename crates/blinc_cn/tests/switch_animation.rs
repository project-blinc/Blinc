//! The switch thumb must travel, not teleport.
//!
//! A switch is an element with a `checked` attribute while it is on, and
//! `CN_STYLES` slides its thumb with a `transition`. A toggle, from the
//! switch or from anything else writing its signal, changes the attribute in
//! place: the thumb eases to the other end, and a switch that mounts on starts
//! there.
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::Arc;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                global_graph(),
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

struct Scene {
    tree: RenderTree,
    thumb: LayoutNodeId,
}

fn build(checked: &State<bool>) -> Scene {
    let vars = blinc_theme::ThemeState::get().to_css_variable_map();
    let cn = Stylesheet::parse_with_variables(blinc_cn::cn_styles::CN_STYLES, &vars).expect("cn");
    let host = div()
        .w(200.0)
        .h(60.0)
        .child(blinc_cn::switch(checked).label("busy"));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(cn);
    tree.apply_all_stylesheet_styles();
    tree.compute_layout(200.0, 60.0);
    let thumb = tree
        .element_registry()
        .class_to_nodes_index()
        .remove("cn-switch-thumb")
        .and_then(|nodes| nodes.first().copied())
        .expect("a thumb");
    Scene { tree, thumb }
}

impl Scene {
    /// Land what a signal write queued, as a frame does.
    fn frame(&mut self) {
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.apply_all_css_transition_props();
        self.tree.compute_layout(200.0, 60.0);
    }

    /// Where the thumb is drawn: the transition's value while one plays.
    fn thumb_x(&self) -> f32 {
        if let Some(stable) = self.tree.stable_id(self.thumb) {
            let store = self.tree.css_anim_store();
            let store = store.lock().unwrap();
            if let Some(t) = store.transitions.get(&stable).filter(|t| t.is_playing) {
                if let Some(x) = t.current_properties.translate_x {
                    return x;
                }
            }
        }
        match &self
            .tree
            .get_render_node(self.thumb)
            .unwrap()
            .props
            .transform
        {
            Some(blinc_core::Transform::Affine2D(a)) => a.elements[4],
            _ => 0.0,
        }
    }

    fn transition_playing(&self) -> bool {
        self.tree.css_has_active()
    }
}

#[test]
fn a_switch_that_mounts_on_does_not_animate_into_place() {
    init();
    let checked = State::new(signal::<bool>(true), global_graph(), global_dirty_flag());
    let s = build(&checked);
    assert_eq!(
        s.thumb_x(),
        20.0,
        "a switch built `on` starts at the far end"
    );
    assert!(!s.transition_playing(), "it slid in on mount");
}

#[test]
fn an_externally_toggled_switch_animates_rather_than_jumping() {
    init();
    let checked = State::new(signal::<bool>(false), global_graph(), global_dirty_flag());
    let mut s = build(&checked);
    assert_eq!(s.thumb_x(), 0.0, "starts off, at the near end");

    // Something else flips it: an FSM transition, another control bound to
    // the same signal.
    checked.set(true);
    s.frame();
    assert!(s.transition_playing(), "no transition started");
    assert!(
        s.thumb_x() < 1.0,
        "the thumb must still be near the start, travelling: {}",
        s.thumb_x()
    );

    for _ in 0..10 {
        s.tree.css_anim_store().lock().unwrap().tick(50.0);
        s.tree.apply_all_css_transition_props();
    }
    assert_eq!(s.thumb_x(), 20.0, "it did not arrive");
}

/// The switch follows its signal in place, with no rebuild to restart the
/// travel from wherever a new one was built.
#[test]
fn a_toggle_does_not_rebuild_the_switch() {
    init();
    let checked = State::new(signal::<bool>(false), global_graph(), global_dirty_flag());
    let mut s = build(&checked);
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    checked.set(true);
    assert!(
        !blinc_layout::stateful::has_pending_subtree_rebuilds(),
        "a toggle queued a rebuild"
    );
    s.frame();
}
