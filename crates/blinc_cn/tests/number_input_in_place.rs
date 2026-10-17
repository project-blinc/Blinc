//! A number input is built once: stepping it, or setting its state from
//! anywhere, shows the new value in the field in place, and a stepper dims
//! where a step would change nothing, with no subtree rebuilt.

mod common;

use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::{ElementType, RenderTree};
use common::*;

struct Scene {
    tree: RenderTree,
    router: EventRouter,
    value: State<f64>,
}

fn scene(initial: f64) -> Scene {
    init();
    let value = State::new(signal(initial), global_graph(), global_dirty_flag());
    let input = blinc_cn::number_input(&value)
        .min(0.0)
        .max(3.0)
        .step(1.0)
        .precision(0);
    let host = div().w(400.0).h(200.0).child(input);
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    Scene {
        tree,
        router: EventRouter::new(),
        value,
    }
}

impl Scene {
    fn nodes(&self) -> Vec<LayoutNodeId> {
        let mut found = Vec::new();
        let mut stack = vec![self.tree.root().unwrap()];
        while let Some(node) = stack.pop() {
            found.push(node);
            let mut children = self.tree.layout_tree.children(node);
            children.reverse();
            stack.extend(children);
        }
        found
    }

    fn find_class(&self, class: &str) -> LayoutNodeId {
        let registry = self.tree.element_registry();
        self.nodes()
            .into_iter()
            .find(|&n| registry.has_class(n, class))
            .unwrap_or_else(|| panic!("no .{class}"))
    }

    /// What the field shows.
    fn shown(&self) -> String {
        let field = self.find_class("cn-number-input-field");
        let mut text = String::new();
        let mut stack = vec![field];
        while let Some(node) = stack.pop() {
            if let Some(ElementType::Text(t)) =
                self.tree.get_render_node(node).map(|r| &r.element_type)
            {
                text.push_str(&t.content);
            }
            let mut children = self.tree.layout_tree.children(node);
            children.reverse();
            stack.extend(children);
        }
        text
    }

    /// The tint of a stepper's icon.
    fn icon_tint(&self, class: &str) -> Option<[f32; 4]> {
        let stepper = self.find_class(class);
        let icon = self.tree.layout_tree.children(stepper)[0];
        self.tree.get_render_node(icon).unwrap().props.svg_tint
    }

    fn frame(&mut self) {
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "the number input queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 200.0);
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
}

#[test]
fn stepping_shows_the_new_value_in_place() {
    guarded(|| {
        let mut s = scene(1.0);
        assert_eq!(s.shown(), "1");
        let inc = s.find_class("cn-number-input-step--inc");
        s.click(inc);
        assert_eq!(s.value.get(), 2.0);
        assert_eq!(s.shown(), "2", "the field did not show the step");

        let dec = s.find_class("cn-number-input-step--dec");
        s.click(dec);
        assert_eq!(s.shown(), "1");
    })
}

#[test]
fn a_value_set_from_outside_shows_in_the_field() {
    guarded(|| {
        let mut s = scene(1.0);
        s.value.set(3.0);
        s.frame();
        assert_eq!(s.shown(), "3");
    })
}

#[test]
fn a_stepper_dims_at_its_bound() {
    guarded(|| {
        let mut s = scene(2.0);
        let open = s.icon_tint("cn-number-input-step--inc");
        let inc = s.find_class("cn-number-input-step--inc");
        s.click(inc);
        assert_eq!(s.value.get(), 3.0);
        assert_ne!(
            s.icon_tint("cn-number-input-step--inc"),
            open,
            "the + did not dim at the maximum"
        );
        assert_ne!(
            s.icon_tint("cn-number-input-step--dec"),
            s.icon_tint("cn-number-input-step--inc"),
            "the - dimmed too"
        );
    })
}
