//! A slider is used in place: hovering, pressing, dragging and releasing
//! queue no subtree rebuild. The halo follows the pointer through a spring,
//! and the thumb and value follow the drag.

mod common;

use blinc_cn::SliderBuilder;
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::div::div;
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::RenderTree;
use common::*;

const WIDTH: f32 = 300.0;
/// Medium thumb; the travel is `WIDTH - THUMB`.
const THUMB: f32 = 18.0;
const TRAVEL: f32 = WIDTH - THUMB;

struct Scene {
    tree: RenderTree,
    router: EventRouter,
    value: State<f32>,
    key: String,
}

fn scene(key: &str, disabled: bool) -> Scene {
    let value = State::new(signal::<f32>(0.5), global_graph(), global_dirty_flag());
    let slider = SliderBuilder::with_key(key, &value)
        .min(0.0)
        .max(1.0)
        .w(WIDTH)
        .disabled(disabled);
    let host = div().w(400.0).h(200.0).child(slider);
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    Scene {
        tree,
        router: EventRouter::new(),
        value,
        key: key.to_string(),
    }
}

impl Scene {
    /// Where the thumb's centre is for the current value.
    fn thumb_x(&self) -> f32 {
        self.value.get() * TRAVEL + THUMB / 2.0
    }

    fn deliver(&mut self, events: Vec<(blinc_layout::LayoutNodeId, u32)>, x: f32, y: f32) {
        for (node, event) in events {
            self.tree.dispatch_event(node, event, x, y);
        }
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "using the slider queued a subtree rebuild"
        );
        let updates = blinc_layout::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 200.0);
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

    fn spring(&self, part: &str) -> blinc_layout::motion::SharedAnimatedValue {
        let key = if part == "thumb" {
            format!("cn-slider:{}:thumb", self.value.signal_id().to_raw())
        } else {
            format!("cn-slider:{}:{part}", self.key)
        };
        blinc_layout::stateful::try_persisted_animated_value(&key)
            .unwrap_or_else(|| panic!("the slider has no {part} spring"))
    }

    fn halo(&self) -> f32 {
        self.spring("halo").lock().unwrap().target()
    }

    fn thumb_offset(&self) -> f32 {
        self.spring("thumb").lock().unwrap().target()
    }
}

#[test]
fn the_halo_follows_the_pointer_and_stays_for_a_drag() {
    init();
    guarded(|| {
        let mut s = scene("slider-halo", false);
        let (x, y) = (s.thumb_x(), THUMB / 2.0);
        assert_eq!(s.halo(), 0.0, "lit before the pointer came");

        s.move_to(x, y);
        assert_eq!(s.halo(), 1.0, "hovering did not light the halo");
        s.move_to(390.0, 150.0);
        assert_eq!(s.halo(), 0.0, "leaving did not put it out");

        // A drag that wanders off the slider keeps it lit until the release.
        s.move_to(x, y);
        s.press(x, y);
        s.move_to(x + 12.0, y);
        s.move_to(x + 24.0, 150.0);
        assert_eq!(s.halo(), 1.0, "the halo went out during a drag");
        s.release(x + 24.0, 150.0);
        assert_eq!(
            s.halo(),
            0.0,
            "the halo stayed after a drag ended off the slider"
        );
    });
}

#[test]
fn a_drag_moves_the_thumb_and_the_value_with_the_pointer() {
    init();
    guarded(|| {
        let mut s = scene("slider-drag", false);
        let (x0, y) = (s.thumb_x(), THUMB / 2.0);
        let start = 0.5 * TRAVEL;
        s.move_to(x0, y);
        s.press(x0, y);
        for step in 1..=8 {
            let x = x0 + step as f32 * 6.0;
            s.move_to(x, y);
            let offset = start + step as f32 * 6.0;
            assert!(
                (s.thumb_offset() - offset).abs() < 1e-3,
                "step {step}: the thumb is at {} not {offset}",
                s.thumb_offset()
            );
            assert!((s.value.get() - offset / TRAVEL).abs() < 1e-4);
        }
        s.release(x0 + 48.0, y);
        // The click that follows a drag does not seek.
        assert!((s.value.get() - (start + 48.0) / TRAVEL).abs() < 1e-4);
    });
}

#[test]
fn a_disabled_slider_neither_moves_nor_lights() {
    init();
    guarded(|| {
        let mut s = scene("slider-disabled", true);
        let (x, y) = (s.thumb_x(), THUMB / 2.0);
        s.move_to(x, y);
        s.press(x, y);
        s.move_to(x + 30.0, y);
        s.release(x + 30.0, y);
        assert_eq!(s.value.get(), 0.5);
        assert_eq!(s.halo(), 0.0);
    });
}
