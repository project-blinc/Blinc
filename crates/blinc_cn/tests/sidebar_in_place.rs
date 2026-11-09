//! A sidebar's rail is built once: collapsing narrows it in place by rule,
//! and choosing an item moves the active mark, with no subtree rebuilt.

mod common;

use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::RenderTree;
use common::*;

/// Marks the active item, as the theme's rule would.
const RULE: &str =
    ".cn-sidebar-item[data-active], .cn-sidebar-item[data-active]:hover { background: #123456 }";
const ICON: &str = r#"<svg viewBox="0 0 24 24"><rect width="24" height="24"/></svg>"#;

struct Scene {
    tree: RenderTree,
    router: EventRouter,
    collapsed: State<bool>,
    /// The flag the "Inspector" toggle item follows.
    inspector: State<bool>,
}

fn scene() -> Scene {
    init();
    let collapsed = State::new(signal(false), global_graph(), global_dirty_flag());
    let inspector = State::new(signal(false), global_graph(), global_dirty_flag());
    let flag = inspector.clone();
    let bar = blinc_cn::sidebar(collapsed.signal())
        .show_toggle(true)
        .section("Mail")
        .item_active("Inbox", ICON, || {})
        .item("Drafts", ICON, || {})
        .item("Archive", ICON, || {})
        .toggle_item("Inspector", ICON, inspector.signal(), move || {
            flag.update(|on| !on)
        });
    let host = div().w(600.0).h(400.0).child(bar);
    let mut tree = RenderTree::from_element(&host);
    // cn's rules size the rail; the test's rule marks the active item.
    let vars = blinc_theme::ThemeState::get().to_css_variable_map();
    let css = format!("{}\n{RULE}", blinc_cn::cn_styles::CN_STYLES);
    tree.set_stylesheet(Stylesheet::parse_with_variables(&css, &vars).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(600.0, 400.0);
    Scene {
        tree,
        router: EventRouter::new(),
        collapsed,
        inspector,
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

    fn rail_width(&self) -> f32 {
        let rail = self.find_class("cn-sidebar")[0];
        self.tree.get_absolute_bounds(rail).unwrap().width
    }

    fn active(&self, item: LayoutNodeId) -> bool {
        match &self.tree.get_render_node(item).unwrap().props.background {
            Some(blinc_core::Brush::Solid(c)) => (c.r - 0x12 as f32 / 255.0).abs() < 0.01,
            _ => false,
        }
    }

    fn frame(&mut self) {
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "using the sidebar queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.apply_stylesheet_layout_overrides();
        self.tree.compute_layout(600.0, 400.0);
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
fn collapsing_narrows_the_rail_in_place() {
    guarded(|| {
        let mut s = scene();
        let open = s.rail_width();
        s.collapsed.set(true);
        s.frame();
        let shut = s.rail_width();
        assert!(
            shut < open - 20.0,
            "the rail stayed {shut} wide, from {open}"
        );
        s.collapsed.set(false);
        s.frame();
        assert_eq!(s.rail_width(), open, "the rail did not open again");
    })
}

#[test]
fn choosing_an_item_moves_the_active_mark() {
    guarded(|| {
        let mut s = scene();
        let items = s.find_class("cn-sidebar-item");
        assert_eq!(items.len(), 4);
        assert!(s.active(items[0]), "the item marked active is not");
        assert!(!s.active(items[1]));

        s.click(items[1]);
        assert!(s.active(items[1]), "the chosen item is not marked active");
        assert!(!s.active(items[0]), "the old item is still marked active");
    })
}

#[test]
fn a_toggle_item_follows_its_flag_and_leaves_the_selection() {
    guarded(|| {
        let mut s = scene();
        let items = s.find_class("cn-sidebar-item");
        let toggle = items[3];
        assert!(
            !s.active(toggle),
            "the toggle is active while its flag is off"
        );

        s.click(toggle);
        assert!(
            s.inspector.get(),
            "pressing the toggle did not run its action"
        );
        assert!(
            s.active(toggle),
            "the toggle is not active while its flag is on"
        );
        assert!(s.active(items[0]), "pressing the toggle took the selection");

        s.click(toggle);
        assert!(
            !s.active(toggle),
            "the toggle stayed active after its flag went off"
        );

        s.inspector.set(true);
        s.frame();
        assert!(s.active(toggle), "the toggle did not follow its flag");
    })
}
