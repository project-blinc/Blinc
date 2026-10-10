//! A dropdown's trigger is built once: hovering it changes its fill, and
//! opening the menu turns the chevron or swaps a custom trigger for its open
//! form, all in place.

mod common;

use blinc_core::Color;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::{Div, div};
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::overlay_state::overlay_stack;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::text::text;
use common::*;

struct Scene {
    tree: RenderTree,
    router: EventRouter,
}

fn scene(menu: impl blinc_layout::div::ElementBuilder + 'static) -> Scene {
    init();
    let host = div().w(400.0).h(200.0).child(menu);
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    Scene {
        tree,
        router: EventRouter::new(),
    }
}

impl Scene {
    /// The trigger: the element under the host.
    fn trigger(&self) -> LayoutNodeId {
        let root = self.tree.root().unwrap();
        self.tree.layout_tree.children(root)[0]
    }

    /// The default trigger's face: the bordered row inside it.
    fn face(&self) -> LayoutNodeId {
        self.tree.layout_tree.children(self.trigger())[0]
    }

    fn fill(&self, node: LayoutNodeId) -> Option<Color> {
        match &self.tree.get_render_node(node).unwrap().props.background {
            Some(blinc_core::Brush::Solid(c)) => Some(*c),
            _ => None,
        }
    }

    fn texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![self.trigger()];
        while let Some(node) = stack.pop() {
            if let Some(ElementType::Text(t)) =
                self.tree.get_render_node(node).map(|r| &r.element_type)
            {
                out.push(t.content.clone());
            }
            stack.extend(self.tree.layout_tree.children(node));
        }
        out
    }

    fn centre(&self) -> (f32, f32) {
        let b = self.tree.get_absolute_bounds(self.trigger()).unwrap();
        (b.x + b.width / 2.0, b.y + b.height / 2.0)
    }

    fn deliver(&mut self, events: Vec<(LayoutNodeId, u32)>, x: f32, y: f32) {
        for (node, event) in events {
            self.tree.dispatch_event(node, event, x, y);
        }
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "using the trigger queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 200.0);
    }

    fn move_to(&mut self, (x, y): (f32, f32)) {
        let events = self.router.on_mouse_move(&self.tree, x, y);
        self.deliver(events, x, y);
    }

    fn click(&mut self) {
        let (x, y) = self.centre();
        self.move_to((x, y));
        let down = self
            .router
            .on_mouse_down(&self.tree, x, y, MouseButton::Left);
        self.deliver(down, x, y);
        let up = self.router.on_mouse_up(&self.tree, x, y, MouseButton::Left);
        self.deliver(up, x, y);
    }
}

fn menu() -> blinc_cn::DropdownMenuBuilder {
    blinc_cn::dropdown_menu("Options")
        .item("Edit", || {})
        .item("Delete", || {})
}

#[test]
fn hovering_the_trigger_changes_its_fill_in_place() {
    guarded(|| {
        let mut s = scene(menu());
        let rest = s.fill(s.face());
        s.move_to(s.centre());
        let hovered = s.fill(s.face());
        assert_ne!(rest, hovered, "hovering did not change the trigger's fill");
        s.move_to((390.0, 190.0));
        assert_eq!(
            s.fill(s.face()),
            rest,
            "the fill stayed after the pointer left"
        );
    })
}

#[test]
fn opening_the_menu_turns_the_chevron() {
    guarded(|| {
        let mut s = scene(menu());
        let chevron = s.tree.layout_tree.children(s.face())[1];
        let rotation = |s: &Scene| {
            format!(
                "{:?}",
                s.tree.get_render_node(chevron).unwrap().props.transform
            )
        };
        let closed = rotation(&s);

        let before = overlay_stack().lock().unwrap().len();
        s.click();
        assert_eq!(
            overlay_stack().lock().unwrap().len(),
            before + 1,
            "no menu opened"
        );
        assert_ne!(rotation(&s), closed, "the chevron did not turn");
    })
}

#[test]
fn a_custom_trigger_is_built_for_the_open_menu() {
    guarded(|| {
        let custom = blinc_cn::dropdown_menu_custom(|open: bool| -> Div {
            div()
                .w(120.0)
                .h(32.0)
                .child(text(if open { "open" } else { "closed" }))
        })
        .item("Edit", || {});
        let mut s = scene(custom);
        assert_eq!(s.texts(), vec!["closed".to_string()]);
        s.click();
        assert_eq!(
            s.texts(),
            vec!["open".to_string()],
            "the trigger kept its closed form"
        );
    })
}
