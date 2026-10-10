//! The content area of a tabs widget follows the selection in place: the
//! panel for the new tab is built where it goes and the old one is taken
//! away, with no subtree rebuild. With a transition the old panel stays
//! mounted, out of flow, until its exit animation is done.

mod common;

use blinc_cn::cn_styles::CN_STYLES;
use blinc_cn::{TabsTransition, tabs};
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::{Stylesheet, set_active_stylesheet};
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use blinc_layout::text::text;
use common::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scene {
    tree: RenderTree,
    selected: blinc_core::reactive::State<String>,
    builds: Arc<AtomicUsize>,
}

fn scene(transition: TabsTransition) -> Scene {
    let css = Arc::new(Stylesheet::parse(CN_STYLES).expect("cn styles"));
    set_active_stylesheet(Arc::clone(&css));
    let selected = string_state("a");
    let builds = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&builds);
    let host = div().w(400.0).h(300.0).child(
        tabs(&selected)
            .transition(transition)
            .tab("a", "Alpha", move || {
                counted.fetch_add(1, Ordering::SeqCst);
                div().h(40.0).child(text("alpha"))
            })
            .tab("b", "Beta", || div().h(40.0).child(text("beta")))
            .tab("c", "Gamma", || div().h(40.0).child(text("gamma"))),
    );
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet_arc(css);
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 300.0);
    Scene {
        tree,
        selected,
        builds,
    }
}

impl Scene {
    fn content(&self) -> LayoutNodeId {
        let widget = self.tree.layout_tree.children(self.tree.root().unwrap())[0];
        self.tree.layout_tree.children(widget)[1]
    }

    fn panels(&self) -> Vec<LayoutNodeId> {
        self.tree.layout_tree.children(self.content())
    }

    /// What the frame loop does: no subtree rebuild may be asked for.
    fn frame(&mut self) {
        assert!(
            blinc_layout::stateful::take_pending_subtree_rebuilds().is_empty(),
            "the selection queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 300.0);
    }

    fn y(&self, node: LayoutNodeId) -> f32 {
        self.tree.get_absolute_bounds(node).unwrap().y
    }

    fn absolute(&self, node: LayoutNodeId) -> bool {
        self.tree.layout_tree.get_style(node).unwrap().position == taffy::Position::Absolute
    }
}

#[test]
fn with_no_transition_the_panel_is_swapped_at_once() {
    init();
    guarded(|| {
        let mut s = scene(TabsTransition::None);
        let before = s.panels();
        assert_eq!(before.len(), 1);

        s.selected.set("b".to_string());
        s.frame();

        let after = s.panels();
        assert_eq!(after.len(), 1, "the old panel stayed");
        assert_ne!(after[0], before[0]);
        assert_eq!(s.builds.load(Ordering::SeqCst), 1, "tab a was built again");

        s.selected.set("a".to_string());
        s.frame();
        assert_eq!(s.builds.load(Ordering::SeqCst), 2);
    });
}

#[test]
fn a_panel_is_built_once_while_it_is_selected() {
    init();
    guarded(|| {
        let mut s = scene(TabsTransition::None);
        let panel = s.panels()[0];
        for _ in 0..5 {
            s.frame();
        }
        assert_eq!(s.panels(), vec![panel]);
        assert_eq!(s.builds.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn with_a_transition_the_old_panel_leaves_flow_and_goes_when_done() {
    init();
    guarded(|| {
        let mut s = scene(TabsTransition::Fade);
        let old = s.panels()[0];
        let top = s.y(old);
        assert!(!s.absolute(old));

        s.selected.set("b".to_string());
        s.frame();

        let both = s.panels();
        assert_eq!(both.len(), 2, "the old panel did not stay for its exit");
        assert_eq!(both[0], old, "the old panel moved");
        assert!(s.absolute(old), "the old panel is still in flow");
        let new = both[1];
        assert!(!s.absolute(new));
        assert_eq!(s.y(new), top, "the new panel is pushed down by the old");

        // The exit animation runs in real time; the old panel goes after it.
        let mut left = false;
        for _ in 0..200 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            s.frame();
            if s.panels().len() == 1 {
                left = true;
                break;
            }
        }
        assert!(left, "the old panel never left");
        assert_eq!(s.panels(), vec![new], "the new panel was rebuilt");
    });
}
