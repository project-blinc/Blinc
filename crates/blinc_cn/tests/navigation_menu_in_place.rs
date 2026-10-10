//! The navigation bar is built once: hovering a trigger opens its menu and
//! marks that trigger active, and hovering a link brightens it, all in place.
//! Its labels stay on one line.

mod common;

use blinc_core::Color;
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::event_router::EventRouter;
use blinc_layout::overlay_state::overlay_stack;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_theme::{ColorToken, ThemeState};
use common::*;

/// Marks an item the active class is on, as the theme's rule would.
const RULE: &str = ".cn-nav-link--active { background: #123456 }";

struct Scene {
    tree: RenderTree,
    router: EventRouter,
}

fn scene() -> Scene {
    init();
    let nav = blinc_cn::navigation_menu()
        .item("Home", || {})
        .trigger("Products", || div().w(200.0).h(100.0))
        .trigger("Services", || div().w(200.0).h(100.0))
        .item("About", || {});
    let host = div().w(800.0).h(100.0).child(nav);
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(Stylesheet::parse(RULE).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(800.0, 100.0);
    Scene {
        tree,
        router: EventRouter::new(),
    }
}

impl Scene {
    /// The bar's items, in order.
    fn items(&self) -> Vec<LayoutNodeId> {
        let registry = self.tree.element_registry();
        let mut found = Vec::new();
        let mut stack = vec![self.tree.root().unwrap()];
        while let Some(node) = stack.pop() {
            if registry.has_class(node, "cn-nav-link") {
                found.push(node);
                continue;
            }
            let mut children = self.tree.layout_tree.children(node);
            children.reverse();
            stack.extend(children);
        }
        found
    }

    fn active(&self, item: LayoutNodeId) -> bool {
        match &self.tree.get_render_node(item).unwrap().props.background {
            Some(blinc_core::Brush::Solid(c)) => {
                (c.r - 0x12 as f32 / 255.0).abs() < 0.01 && (c.b - 0x56 as f32 / 255.0).abs() < 0.01
            }
            _ => false,
        }
    }

    fn label(&self, item: LayoutNodeId) -> LayoutNodeId {
        self.tree.layout_tree.children(item)[0]
    }

    /// The colour the label is drawn in: a bound colour, else its own.
    fn label_color(&self, item: LayoutNodeId) -> Color {
        let render = self.tree.get_render_node(self.label(item)).unwrap();
        let [r, g, b, a] = match &render.element_type {
            ElementType::Text(t) => render.props.text_color.unwrap_or(t.color),
            _ => panic!("the item's first child is not its label"),
        };
        Color::rgba(r, g, b, a)
    }

    fn centre(&self, node: LayoutNodeId) -> (f32, f32) {
        let b = self.tree.get_absolute_bounds(node).unwrap();
        (b.x + b.width / 2.0, b.y + b.height / 2.0)
    }

    fn move_to(&mut self, (x, y): (f32, f32)) {
        let events = self.router.on_mouse_move(&self.tree, x, y);
        for (node, event) in events {
            self.tree.dispatch_event(node, event, x, y);
        }
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "the pointer queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(800.0, 100.0);
    }
}

#[test]
fn hovering_a_trigger_opens_its_menu_and_marks_it_active() {
    guarded(|| {
        let mut s = scene();
        let items = s.items();
        assert_eq!(items.len(), 4);
        let (products, services) = (items[1], items[2]);
        assert!(items.iter().all(|&i| !s.active(i)));

        let before = overlay_stack().lock().unwrap().len();
        s.move_to(s.centre(products));
        assert!(s.active(products), "the open trigger is not marked active");
        assert!(!s.active(services));
        assert_eq!(
            overlay_stack().lock().unwrap().len(),
            before + 1,
            "no menu opened"
        );

        s.move_to(s.centre(services));
        assert!(s.active(services));
        assert!(!s.active(products), "two triggers are marked active");
    })
}

#[test]
fn hovering_a_link_brightens_it_in_place() {
    guarded(|| {
        let mut s = scene();
        let theme = ThemeState::get();
        let (rest, lit) = (
            theme.color(ColorToken::TextSecondary),
            theme.color(ColorToken::TextPrimary),
        );
        let home = s.items()[0];
        assert_eq!(s.label_color(home), rest);
        s.move_to(s.centre(home));
        assert_eq!(
            s.label_color(home),
            lit,
            "hovering did not brighten the link"
        );
        s.move_to((790.0, 90.0));
        assert_eq!(
            s.label_color(home),
            rest,
            "the link stayed lit after the pointer left"
        );
    })
}

#[test]
fn labels_stay_on_one_line() {
    guarded(|| {
        let s = scene();
        for item in s.items() {
            match &s.tree.get_render_node(s.label(item)).unwrap().element_type {
                ElementType::Text(t) => assert!(!t.wrap, "{:?} can wrap", t.content),
                _ => panic!("the item's first child is not its label"),
            }
        }
    })
}
