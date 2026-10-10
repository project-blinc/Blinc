//! A select is used in place: opening it builds the list, choosing an item
//! sets the value, closes the list and shows the new label, and the trigger's
//! fill and border follow the pointer and the open state, with no subtree
//! rebuilt.

mod common;

use blinc_core::Color;
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::{ElementType, RenderTree};
use common::*;

struct Scene {
    tree: RenderTree,
    router: EventRouter,
    value: State<String>,
}

fn scene() -> Scene {
    init();
    let value = State::new(signal(String::new()), global_graph(), global_dirty_flag());
    let select = blinc_cn::select(&value)
        .placeholder("Pick a fruit")
        .option("apple", "Apple")
        .option("pear", "Pear")
        .w(200.0);
    let host = div().w(400.0).h(300.0).child(select);
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 300.0);
    Scene {
        tree,
        router: EventRouter::new(),
        value,
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

    fn trigger(&self) -> LayoutNodeId {
        self.find_class("cn-select-trigger")[0]
    }

    fn is_open(&self) -> bool {
        !self.find_class("cn-select-content").is_empty()
    }

    fn shown_text(&self) -> String {
        let mut stack = vec![self.trigger()];
        while let Some(node) = stack.pop() {
            if let Some(ElementType::Text(t)) =
                self.tree.get_render_node(node).map(|r| &r.element_type)
            {
                return t.content.clone();
            }
            stack.extend(self.tree.layout_tree.children(node));
        }
        panic!("the trigger shows no text");
    }

    fn fill(&self) -> Option<Color> {
        match &self
            .tree
            .get_render_node(self.trigger())
            .unwrap()
            .props
            .background
        {
            Some(blinc_core::Brush::Solid(c)) => Some(*c),
            _ => None,
        }
    }

    fn border(&self) -> Option<Color> {
        self.tree
            .get_render_node(self.trigger())
            .unwrap()
            .props
            .border_color
    }

    fn centre(&self, node: LayoutNodeId) -> (f32, f32) {
        let b = self.tree.get_absolute_bounds(node).unwrap();
        (b.x + b.width / 2.0, b.y + b.height / 2.0)
    }

    fn frame(&mut self) {
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "using the select queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 300.0);
    }

    fn deliver(&mut self, events: Vec<(LayoutNodeId, u32)>, x: f32, y: f32) {
        for (node, event) in events {
            self.tree.dispatch_event(node, event, x, y);
        }
        self.frame();
    }

    fn move_to(&mut self, (x, y): (f32, f32)) {
        let events = self.router.on_mouse_move(&self.tree, x, y);
        self.deliver(events, x, y);
    }

    fn click(&mut self, node: LayoutNodeId) {
        let (x, y) = self.centre(node);
        self.move_to((x, y));
        let down = self
            .router
            .on_mouse_down(&self.tree, x, y, MouseButton::Left);
        self.deliver(down, x, y);
        let up = self.router.on_mouse_up(&self.tree, x, y, MouseButton::Left);
        self.deliver(up, x, y);
    }
}

#[test]
fn choosing_an_item_sets_the_value_and_closes_the_list() {
    guarded(|| {
        let mut s = scene();
        assert_eq!(s.shown_text(), "Pick a fruit");
        assert!(!s.is_open());

        s.click(s.trigger());
        assert!(s.is_open(), "clicking the trigger did not open the list");
        let items = s.find_class("cn-select-item");
        assert_eq!(items.len(), 2);

        s.click(items[1]);
        assert_eq!(s.value.get(), "pear");
        assert!(!s.is_open(), "choosing an item did not close the list");
        assert_eq!(s.shown_text(), "Pear");
    })
}

#[test]
fn a_click_outside_closes_the_list() {
    guarded(|| {
        let mut s = scene();
        s.click(s.trigger());
        assert!(s.is_open());
        blinc_layout::click_outside::fire_click_outside(&[]);
        s.frame();
        assert!(!s.is_open(), "a click outside left the list open");
    })
}

#[test]
fn the_trigger_follows_the_pointer_and_the_open_state() {
    guarded(|| {
        let mut s = scene();
        let (rest_fill, rest_border) = (s.fill(), s.border());
        s.move_to(s.centre(s.trigger()));
        assert_ne!(s.fill(), rest_fill, "hovering did not change the fill");
        s.click(s.trigger());
        assert_ne!(s.border(), rest_border, "opening did not change the border");
    })
}
