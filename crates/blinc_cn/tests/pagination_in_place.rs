//! Pagination is used in place: going to a page moves the active mark, keeps
//! the buttons of pages still in view, and disables the arrows at the ends,
//! with no subtree rebuilt.

mod common;

use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::{ElementType, RenderTree};
use common::*;

/// Marks the active and disabled buttons, as the theme's rules would.
const RULES: &str = ".cn-pagination-btn--active { background: #123456 } \
                     .cn-pagination-btn--disabled { background: #654321 }";

struct Scene {
    tree: RenderTree,
    router: EventRouter,
    page: State<usize>,
}

fn scene() -> Scene {
    init();
    let page = State::new(signal(1usize), global_graph(), global_dirty_flag());
    let pager = blinc_cn::pagination(20, page.clone()).visible_pages(5);
    let host = div().w(600.0).h(100.0).child(pager);
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(Stylesheet::parse(RULES).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(600.0, 100.0);
    Scene {
        tree,
        router: EventRouter::new(),
        page,
    }
}

impl Scene {
    fn row(&self) -> LayoutNodeId {
        let root = self.tree.root().unwrap();
        self.tree.layout_tree.children(root)[0]
    }

    fn buttons(&self) -> Vec<LayoutNodeId> {
        self.tree.layout_tree.children(self.row())
    }

    fn label(&self, button: LayoutNodeId) -> Option<String> {
        let child = *self.tree.layout_tree.children(button).first()?;
        match &self.tree.get_render_node(child)?.element_type {
            ElementType::Text(t) => Some(t.content.clone()),
            _ => None,
        }
    }

    fn page_button(&self, n: usize) -> LayoutNodeId {
        let label = n.to_string();
        self.buttons()
            .into_iter()
            .find(|&b| self.label(b).as_deref() == Some(&label))
            .unwrap_or_else(|| panic!("no button for page {n}"))
    }

    fn marked(&self, node: LayoutNodeId, red: u8) -> bool {
        match &self.tree.get_render_node(node).unwrap().props.background {
            Some(blinc_core::Brush::Solid(c)) => (c.r - red as f32 / 255.0).abs() < 0.01,
            _ => false,
        }
    }

    fn active(&self, node: LayoutNodeId) -> bool {
        self.marked(node, 0x12)
    }

    fn disabled(&self, node: LayoutNodeId) -> bool {
        self.marked(node, 0x65)
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

    fn frame(&mut self) {
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "using the pagination queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(600.0, 100.0);
    }
}

#[test]
fn going_to_a_page_moves_the_active_mark_in_place() {
    guarded(|| {
        let mut s = scene();
        assert!(s.active(s.page_button(1)));
        let three = s.page_button(3);

        s.click(three);
        assert_eq!(s.page.get(), 3);
        assert!(
            s.active(s.page_button(3)),
            "the clicked page is not marked active"
        );
        assert!(
            !s.active(s.page_button(1)),
            "the old page is still marked active"
        );
        assert_eq!(
            s.page_button(3),
            three,
            "a page still in view lost its button"
        );
    })
}

#[test]
fn the_arrows_are_disabled_at_the_ends() {
    guarded(|| {
        let mut s = scene();
        let (prev, next) = {
            let b = s.buttons();
            (b[0], *b.last().unwrap())
        };
        assert!(s.disabled(prev), "previous is usable on the first page");
        assert!(!s.disabled(next));

        s.click(next);
        assert_eq!(s.page.get(), 2);
        assert!(
            !s.disabled(prev),
            "previous stayed disabled after leaving the first page"
        );

        s.page.set(20);
        s.frame();
        s.click(next);
        assert_eq!(s.page.get(), 20, "next went past the last page");
        assert!(s.disabled(next));
    })
}

#[test]
fn the_page_window_follows_the_current_page() {
    guarded(|| {
        let mut s = scene();
        s.page.set(10);
        s.frame();
        let labels: Vec<String> = s.buttons().into_iter().filter_map(|b| s.label(b)).collect();
        assert_eq!(labels, ["8", "9", "10", "11", "12"]);
    })
}
