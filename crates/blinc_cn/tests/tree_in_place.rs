//! A tree view is built once: clicking a node selects it and opens or shuts
//! its children in place, the height animating, with no subtree rebuilt.

mod common;

use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::RenderTree;
use common::*;

/// Marks the selected node, as the theme's rule would.
const RULE: &str = ".cn-tree-node--selected { background: #123456 }";

struct Scene {
    tree: RenderTree,
    router: EventRouter,
}

fn scene() -> Scene {
    init();
    let view = blinc_cn::tree_view()
        .node("src", "src", |n| {
            n.child("main", "main.rs", |c| c)
                .child("lib", "lib.rs", |c| c)
        })
        .node("docs", "docs", |n| n.child("readme", "README.md", |c| c));
    let host = div().w(300.0).h(400.0).child(view);
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(Stylesheet::parse(RULE).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(300.0, 400.0);
    Scene {
        tree,
        router: EventRouter::new(),
    }
}

impl Scene {
    /// The node rows, in order.
    fn rows(&self) -> Vec<LayoutNodeId> {
        let registry = self.tree.element_registry();
        let mut found = Vec::new();
        let mut stack = vec![self.tree.root().unwrap()];
        while let Some(node) = stack.pop() {
            if registry.has_class(node, "cn-tree-node") {
                found.push(node);
            }
            let mut children = self.tree.layout_tree.children(node);
            children.reverse();
            stack.extend(children);
        }
        found
    }

    /// The container holding a node row's children: the row's next sibling.
    fn children_of(&self, row: LayoutNodeId) -> LayoutNodeId {
        let mut stack = vec![self.tree.root().unwrap()];
        while let Some(node) = stack.pop() {
            let kids = self.tree.layout_tree.children(node);
            if kids.first() == Some(&row) && kids.len() > 1 {
                return kids[1];
            }
            stack.extend(kids);
        }
        panic!("the row has no children container");
    }

    fn height(&self, node: LayoutNodeId) -> f32 {
        self.tree.get_absolute_bounds(node).unwrap().height
    }

    fn selected(&self, row: LayoutNodeId) -> bool {
        match &self.tree.get_render_node(row).unwrap().props.background {
            Some(blinc_core::Brush::Solid(c)) => (c.r - 0x12 as f32 / 255.0).abs() < 0.01,
            _ => false,
        }
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
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "using the tree queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(300.0, 400.0);
    }
}

#[test]
fn clicking_a_node_opens_it_and_selects_it_in_place() {
    guarded(|| {
        let mut s = scene();
        let src = s.rows()[0];
        let children = s.children_of(src);
        assert_eq!(s.height(children), 0.0, "a shut node shows its children");

        s.click(src);
        assert!(s.selected(src), "the clicked node is not selected");
        assert!(s.height(children) >= 56.0, "the node did not open");

        // The children are rows now in view; select one.
        let lib = s.rows()[2];
        s.click(lib);
        assert!(s.selected(lib));
        assert!(!s.selected(src), "the old selection is still marked");
    })
}

#[test]
fn opening_a_node_animates_its_height() {
    guarded(|| {
        let mut s = scene();
        let src = s.rows()[0];
        let children = s.children_of(src);
        s.click(src);
        let full = s.height(children);
        tick_animations(1.0 / 60.0);
        s.tree.compute_layout(300.0, 400.0);
        let drawn = s
            .tree
            .get_visual_render_bounds(children)
            .map(|b| b.height)
            .unwrap_or(full);
        assert!(
            drawn > 0.0 && drawn < full - 5.0,
            "the node jumped open: drawn {drawn} of {full}"
        );
    })
}
