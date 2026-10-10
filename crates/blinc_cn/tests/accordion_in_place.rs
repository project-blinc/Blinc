//! An accordion is built once: opening a section turns its chevron and gives
//! its content its height back in place, the height animating there, and in
//! single mode the section that was open closes.

mod common;

use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::RenderTree;
use common::*;

struct Scene {
    tree: RenderTree,
    router: EventRouter,
}

fn scene() -> Scene {
    init();
    let accordion = blinc_cn::accordion()
        .item("one", "First", || div().w_full().h(60.0))
        .item("two", "Second", || div().w_full().h(60.0));
    let host = div().w(400.0).h(400.0).child(accordion);
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 400.0);
    Scene {
        tree,
        router: EventRouter::new(),
    }
}

impl Scene {
    fn find_class(&self, class: &str) -> Vec<LayoutNodeId> {
        let registry = self.tree.element_registry();
        let mut found = Vec::new();
        let mut stack = vec![self.tree.root().unwrap()];
        while let Some(node) = stack.pop() {
            if registry.has_class(node, class) {
                found.push(node);
            }
            let mut children = self.tree.layout_tree.children(node);
            children.reverse();
            stack.extend(children);
        }
        found
    }

    fn triggers(&self) -> Vec<LayoutNodeId> {
        self.find_class("cn-accordion-trigger")
    }

    fn contents(&self) -> Vec<LayoutNodeId> {
        self.find_class("cn-accordion-content")
    }

    /// The height layout gives a section's content.
    fn height(&self, content: LayoutNodeId) -> f32 {
        self.tree.get_absolute_bounds(content).unwrap().height
    }

    /// The height the content is drawn at, mid-animation or not.
    fn drawn_height(&self, content: LayoutNodeId) -> f32 {
        self.tree
            .get_visual_render_bounds(content)
            .map(|b| b.height)
            .unwrap_or_else(|| self.height(content))
    }

    fn chevron_turn(&self, trigger: LayoutNodeId) -> String {
        let chevron = self.tree.layout_tree.children(trigger)[1];
        format!(
            "{:?}",
            self.tree.get_render_node(chevron).unwrap().props.transform
        )
    }

    fn frame(&mut self) {
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "using the accordion queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 400.0);
    }

    fn click(&mut self, node: LayoutNodeId) {
        let b = self.tree.get_absolute_bounds(node).unwrap();
        let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
        let mut events = self.router.on_mouse_move(&self.tree, x, y);
        events.extend(
            self.router
                .on_mouse_down(&self.tree, x, y, MouseButton::Left),
        );
        events.extend(self.router.on_mouse_up(&self.tree, x, y, MouseButton::Left));
        for (node, event) in events {
            self.tree.dispatch_event(node, event, x, y);
        }
        self.frame();
    }

    /// Let the height animations run to the end.
    fn settle(&mut self) {
        for _ in 0..120 {
            tick_animations(1.0 / 60.0);
            self.tree.compute_layout(400.0, 400.0);
        }
    }
}

#[test]
fn opening_a_section_expands_it_in_place() {
    guarded(|| {
        let mut s = scene();
        let (trigger, content) = (s.triggers()[0], s.contents()[0]);
        assert!(s.height(content) < 2.0, "a closed section has height");
        let closed_turn = s.chevron_turn(trigger);

        s.click(trigger);
        assert!(
            s.height(content) >= 60.0,
            "the open section is {} tall",
            s.height(content)
        );
        assert_ne!(
            s.chevron_turn(trigger),
            closed_turn,
            "the chevron did not turn"
        );

        s.settle();
        s.click(trigger);
        assert!(s.height(content) < 2.0, "the section did not close");
    })
}

#[test]
fn the_height_animates_open() {
    guarded(|| {
        let mut s = scene();
        let (trigger, content) = (s.triggers()[0], s.contents()[0]);
        s.click(trigger);
        let full = s.height(content);
        tick_animations(1.0 / 60.0);
        s.tree.compute_layout(400.0, 400.0);
        let early = s.drawn_height(content);
        assert!(
            early > 0.0 && early < full - 5.0,
            "the section jumped open: drawn at {early} of {full} on the first frame"
        );
        s.settle();
        assert!(
            (s.drawn_height(content) - full).abs() < 1.0,
            "the section settled at {} of {full}",
            s.drawn_height(content)
        );
    })
}

#[test]
fn opening_one_section_closes_the_other() {
    guarded(|| {
        let mut s = scene();
        let (triggers, contents) = (s.triggers(), s.contents());
        s.click(triggers[0]);
        s.settle();
        // The second trigger has moved down below the open first section.
        let second = s.triggers()[1];
        s.click(second);
        assert!(
            s.height(contents[1]) >= 60.0,
            "the second section did not open"
        );
        assert!(s.height(contents[0]) < 2.0, "the first section stayed open");
    })
}
