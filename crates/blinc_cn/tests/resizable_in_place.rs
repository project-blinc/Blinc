//! Dragging a resizable group's handle resizes its panels in place: no
//! subtree is rebuilt, the panel's size follows the pointer within its
//! bounds, and the handle shows the primary colour only while dragged.

mod common;

use blinc_cn::{ResizeDirection, resizable_group, resizable_panel};
use blinc_core::Color;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::RenderTree;
use blinc_theme::{ColorToken, ThemeState};
use common::*;

struct Scene {
    tree: RenderTree,
    router: EventRouter,
}

fn scene(key: &str) -> Scene {
    init();
    let group = resizable_group()
        .key(key)
        .direction(ResizeDirection::Horizontal)
        .panel(
            resizable_panel()
                .default_size(200.0)
                .min_size(100.0)
                .max_size(300.0)
                .child(div().w_full().h_full()),
        )
        .panel(resizable_panel().flex_grow().child(div().w_full().h_full()));
    let host = div().w(600.0).h(200.0).child(group);
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 200.0);
    Scene {
        tree,
        router: EventRouter::new(),
    }
}

impl Scene {
    fn group(&self) -> LayoutNodeId {
        let root = self.tree.root().unwrap();
        self.tree.layout_tree.children(root)[0]
    }

    fn first_panel_width(&self) -> f32 {
        let panel = self.tree.layout_tree.children(self.group())[0];
        self.tree.get_absolute_bounds(panel).unwrap().width
    }

    /// The handle's hit area and the thin bar inside it.
    fn handle(&self) -> (LayoutNodeId, LayoutNodeId) {
        let registry = self.tree.element_registry();
        let mut stack = vec![self.group()];
        while let Some(node) = stack.pop() {
            if registry.has_class(node, "cn-resizable-handle") {
                let bar = self.tree.layout_tree.children(node)[0];
                return (node, bar);
            }
            stack.extend(self.tree.layout_tree.children(node));
        }
        panic!("no resize handle");
    }

    fn handle_centre(&self) -> (f32, f32) {
        let b = self.tree.get_absolute_bounds(self.handle().0).unwrap();
        (b.x + b.width / 2.0, b.y + b.height / 2.0)
    }

    fn bar_color(&self) -> Color {
        match &self
            .tree
            .get_render_node(self.handle().1)
            .unwrap()
            .props
            .background
        {
            Some(blinc_core::Brush::Solid(c)) => *c,
            other => panic!("the bar has no solid fill: {other:?}"),
        }
    }

    fn deliver(&mut self, events: Vec<(LayoutNodeId, u32)>, x: f32, y: f32) {
        for (node, event) in events {
            self.tree.dispatch_event(node, event, x, y);
        }
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "using the handle queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(600.0, 200.0);
    }

    fn move_to(&mut self, x: f32, y: f32) {
        let events = self.router.on_mouse_move(&self.tree, x, y);
        self.deliver(events, x, y);
    }

    fn press(&mut self, x: f32, y: f32) {
        let events = self
            .router
            .on_mouse_down(&self.tree, x, y, MouseButton::Left);
        self.deliver(events, x, y);
    }

    fn release(&mut self, x: f32, y: f32) {
        let events = self.router.on_mouse_up(&self.tree, x, y, MouseButton::Left);
        self.deliver(events, x, y);
    }
}

#[test]
fn dragging_the_handle_resizes_the_panel_in_place() {
    guarded(|| {
        let mut s = scene("rz_drag");
        assert_eq!(s.first_panel_width(), 200.0);
        let (x, y) = s.handle_centre();

        s.move_to(x, y);
        s.press(x, y);
        s.move_to(x + 30.0, y);
        s.move_to(x + 50.0, y);
        assert_eq!(s.first_panel_width(), 250.0);

        s.move_to(x - 40.0, y);
        assert_eq!(s.first_panel_width(), 160.0);
        s.release(x - 40.0, y);
        assert_eq!(s.first_panel_width(), 160.0, "releasing moved the panel");
    })
}

#[test]
fn a_dragged_panel_stays_within_its_bounds() {
    guarded(|| {
        let mut s = scene("rz_bounds");
        let (x, y) = s.handle_centre();
        s.move_to(x, y);
        s.press(x, y);
        s.move_to(x + 250.0, y);
        assert_eq!(s.first_panel_width(), 300.0, "past its maximum");
        s.move_to(x - 250.0, y);
        assert_eq!(s.first_panel_width(), 100.0, "past its minimum");
        s.release(x - 250.0, y);
    })
}

#[test]
fn the_handle_shows_the_primary_colour_only_while_dragged() {
    guarded(|| {
        let mut s = scene("rz_colour");
        let theme = ThemeState::get();
        let (rest, active) = (
            theme.color(ColorToken::Border),
            theme.color(ColorToken::Primary),
        );
        assert_eq!(s.bar_color(), rest);

        let (x, y) = s.handle_centre();
        s.move_to(x, y);
        s.press(x, y);
        s.move_to(x + 20.0, y);
        assert_eq!(s.bar_color(), active, "not highlighted while dragged");
        s.release(x + 20.0, y);
        assert_eq!(s.bar_color(), rest, "still highlighted after the drag");
    })
}

fn panels(group: blinc_cn::ResizableGroupBuilder) -> blinc_cn::ResizableGroupBuilder {
    group
        .panel(
            resizable_panel()
                .default_size(200.0)
                .min_size(100.0)
                .child(div().w_full().h_full()),
        )
        .panel(resizable_panel().flex_grow().child(div().w_full().h_full()))
}

#[test]
fn groups_made_in_different_places_keep_their_own_sizes() {
    guarded(|| {
        init();
        // Neither has a key: each is named by where it was made.
        let first = panels(resizable_group());
        let second = panels(resizable_group());
        let host = div()
            .w(600.0)
            .h(400.0)
            .flex_col()
            .child(div().w_full().h(200.0).child(first))
            .child(div().w_full().h(200.0).child(second));
        let mut s = Scene {
            tree: RenderTree::from_element(&host),
            router: EventRouter::new(),
        };
        s.tree.compute_layout(600.0, 400.0);
        let width = |s: &Scene, row: usize| {
            let root = s.tree.root().unwrap();
            let slot = s.tree.layout_tree.children(root)[row];
            let group = s.tree.layout_tree.children(slot)[0];
            let panel = s.tree.layout_tree.children(group)[0];
            s.tree.get_absolute_bounds(panel).unwrap().width
        };

        let (x, y) = s.handle_centre();
        s.move_to(x, y);
        s.press(x, y);
        s.move_to(x + 40.0, y);
        s.release(x + 40.0, y);
        assert_eq!(width(&s, 0), 240.0);
        assert_eq!(width(&s, 1), 200.0, "the other group's panel moved too");
    })
}

#[test]
fn on_resize_reports_the_new_sizes() {
    guarded(|| {
        init();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Vec<f32>>::new()));
        let log = seen.clone();
        let group = panels(resizable_group().key("rz_report"))
            .on_resize(move |sizes| log.lock().unwrap().push(sizes.to_vec()));
        let host = div().w(600.0).h(200.0).child(group);
        let mut s = Scene {
            tree: RenderTree::from_element(&host),
            router: EventRouter::new(),
        };
        s.tree.compute_layout(600.0, 200.0);

        let (x, y) = s.handle_centre();
        s.move_to(x, y);
        s.press(x, y);
        s.move_to(x + 30.0, y);
        s.release(x + 30.0, y);
        assert_eq!(
            seen.lock().unwrap().last().map(|v| v[0]),
            Some(230.0),
            "on_resize did not report the drag"
        );
    })
}

#[test]
fn a_vertical_group_resizes_heights() {
    guarded(|| {
        init();
        let group = resizable_group()
            .key("rz_vertical")
            .vertical()
            .panel(resizable_panel().flex_grow().child(div().w_full().h_full()))
            .panel(
                resizable_panel()
                    .default_size(120.0)
                    .min_size(80.0)
                    .child(div().w_full().h_full()),
            );
        let host = div().w(400.0).h(400.0).child(group);
        let mut s = Scene {
            tree: RenderTree::from_element(&host),
            router: EventRouter::new(),
        };
        s.tree.compute_layout(400.0, 400.0);
        let height = |s: &Scene| {
            let panel = s.tree.layout_tree.children(s.group())[2];
            s.tree.get_absolute_bounds(panel).unwrap().height
        };
        assert_eq!(height(&s), 120.0);

        let (x, y) = s.handle_centre();
        s.move_to(x, y);
        s.press(x, y);
        // Dragging the handle up grows the panel below it.
        s.move_to(x, y - 30.0);
        s.release(x, y - 30.0);
        assert_eq!(height(&s), 150.0);
    })
}
