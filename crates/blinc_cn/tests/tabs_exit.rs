//! A tab panel's exit plays: the old panel stays mounted, out of flow, for its
//! CSS exit animation and goes when it is done.
//!
//! The frame does what the windowed runner does, in its order: drain the
//! property updates and lay out, start the CSS animations rules give, tick
//! them and write their values.

mod common;

use blinc_cn::cn_styles::CN_STYLES;
use blinc_cn::{TabsTransition, tabs};
use blinc_core::BlincContextState;
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::{Stylesheet, set_active_stylesheet};
use blinc_layout::div::div;
use blinc_layout::render_state::{RenderState, create_shared_motion_states};
use blinc_layout::renderer::RenderTree;
use blinc_layout::text::text;
use common::*;
use std::sync::{Arc, Mutex};

struct Runner {
    tree: RenderTree,
    rs: RenderState,
    now_ms: u64,
}

fn runner(selected: &blinc_core::reactive::State<String>) -> Runner {
    runner_with(selected, TabsTransition::Fade)
}

fn runner_with(
    selected: &blinc_core::reactive::State<String>,
    transition: TabsTransition,
) -> Runner {
    let vars = blinc_theme::ThemeState::get().to_css_variable_map();
    let css = Arc::new(Stylesheet::parse_with_variables(CN_STYLES, &vars).expect("cn styles"));
    set_active_stylesheet(Arc::clone(&css));

    let shared = create_shared_motion_states();
    let for_callback = Arc::clone(&shared);
    BlincContextState::get().set_motion_state_callback(Arc::new(move |key: &str| {
        for_callback
            .read()
            .ok()
            .and_then(|states| states.get(key).copied())
            .unwrap_or(blinc_core::MotionAnimationState::NotFound)
    }));
    let mut rs = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    rs.set_shared_motion_states(shared);

    let host = div().w(400.0).h(300.0).child(
        tabs(selected.signal())
            .transition(transition)
            .tab("a", "Alpha", || div().w(100.0).h(40.0).child(text("alpha")))
            .tab("b", "Beta", || div().w(100.0).h(40.0).child(text("beta")))
            .tab("c", "Gamma", || div().w(100.0).h(40.0).child(text("gamma"))),
    );
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet_arc(css);
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 300.0);
    let mut r = Runner {
        tree,
        rs,
        now_ms: 0,
    };
    for _ in 0..30 {
        r.frame();
    }
    r
}

impl Runner {
    fn frame(&mut self) {
        self.now_ms += 16;
        self.rs.process_global_motion_exit_cancels();
        self.rs.process_global_motion_exit_starts();
        self.rs.process_global_motion_starts();
        self.rs.sync_shared_motion_states();

        self.tree.process_pending_subtree_rebuilds();
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 300.0);

        self.rs.begin_stable_motion_frame();
        self.tree.initialize_motion_animations(&mut self.rs);
        self.rs.end_stable_motion_frame();
        self.rs.process_global_motion_replays();

        self.rs.process_global_motion_exit_cancels();
        self.rs.process_global_motion_exit_starts();
        self.rs.process_global_motion_starts();
        self.rs.tick(self.now_ms);
        self.rs.sync_shared_motion_states();

        self.tree.start_all_css_animations();
        let store = self.tree.css_anim_store();
        store.lock().unwrap().tick(16.0);
        self.tree.apply_all_css_animation_props();
    }

    fn panels(&self) -> Vec<LayoutNodeId> {
        let widget = self.tree.layout_tree.children(self.tree.root().unwrap())[0];
        let content = self.tree.layout_tree.children(widget)[1];
        self.tree.layout_tree.children(content)
    }

    /// The `@keyframes` `node` plays now.
    fn playing(&self, node: LayoutNodeId) -> Option<String> {
        let id = self.tree.stable_id(node)?;
        let store = self.tree.css_anim_store();
        let store = store.lock().unwrap();
        store.animations.get(&id)?.name.clone()
    }
}

/// A panel slides the way the active tab moved: to a tab further along the
/// new one comes from the right and the old leaves to the left, and back
/// the other way.
#[test]
fn the_panels_slide_the_way_the_tab_moved() {
    init();
    guarded(|| {
        let selected = string_state("a");
        let mut r = runner_with(&selected, TabsTransition::Slide);
        let switch = |r: &mut Runner, to: &str| {
            let old = r.panels()[0];
            selected.set(to.to_string());
            r.frame();
            r.frame();
            let new = *r.panels().last().unwrap();
            assert_ne!(old, new);
            let played = (r.playing(new), r.playing(old));
            for _ in 0..60 {
                r.frame();
            }
            played
        };
        let (enter, exit) = switch(&mut r, "c");
        assert_eq!(enter.as_deref(), Some("cn-tab-from-right"), "on to c");
        assert_eq!(exit.as_deref(), Some("cn-tab-to-left"), "on to c");
        let (enter, exit) = switch(&mut r, "b");
        assert_eq!(enter.as_deref(), Some("cn-tab-from-left"), "back to b");
        assert_eq!(exit.as_deref(), Some("cn-tab-to-right"), "back to b");
    });
}

#[test]
fn the_old_panel_plays_its_exit_before_it_goes() {
    init();
    guarded(|| {
        let selected = string_state("a");
        let mut r = runner(&selected);
        assert_eq!(r.panels().len(), 1);
        let old = r.panels()[0];

        selected.set("b".to_string());
        r.frame();
        assert_eq!(
            r.panels().len(),
            2,
            "the old panel did not stay for its exit"
        );
        assert_eq!(r.panels()[0], old);
        let registry = r.tree.element_registry();
        assert_eq!(
            registry.attribute(old, "data-motion").as_deref(),
            Some("exit"),
            "the old panel is leaving"
        );
        let id = registry.get_id(old).expect("the panel's id");

        let start = r.now_ms;
        let mut left = None;
        let mut played = false;
        while r.now_ms - start <= 2000 {
            r.frame();
            played |= blinc_layout::css_animation_playing(&id);
            if r.panels().len() == 1 {
                left = Some(r.now_ms - start);
                break;
            }
        }
        let took = left.expect("the old panel never left");
        assert!(played, "its exit animation never played");
        assert!(
            took >= 100,
            "the old panel left after {took}ms, before its exit could play"
        );
        assert_ne!(r.panels()[0], old);
    });
}
