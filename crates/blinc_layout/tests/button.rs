//! The core button restyles from the pointer and the disabled state without
//! a rebuild, runs its click only while enabled, and hands its content the
//! colour it resolves.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_core::{Brush, Color};
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::{Stylesheet, set_active_stylesheet};
use blinc_layout::div::{ElementBuilder, div};
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::text::text;
use blinc_layout::widgets::{button, button_with};

static LOCK: Mutex<()> = Mutex::new(());

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const GREY: Color = Color::rgb(0.5, 0.5, 0.5);

fn init() -> std::sync::MutexGuard<'static, ()> {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(AtomicBool::new(false)),
            );
        }
    });
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    set_active_stylesheet(Arc::new(Stylesheet::parse("").expect("css")));
    guard
}

fn flag(initial: bool) -> State<bool> {
    State::new(signal::<bool>(initial), global_graph(), global_dirty_flag())
}

struct Scene {
    tree: RenderTree,
    router: EventRouter,
}

impl Scene {
    fn new(widget: impl ElementBuilder + 'static) -> Self {
        let host = div().w(400.0).h(200.0).child(widget);
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(400.0, 200.0);
        Self {
            tree,
            router: EventRouter::new(),
        }
    }

    fn button(&self) -> LayoutNodeId {
        self.tree.layout_tree.children(self.tree.root().unwrap())[0]
    }

    fn frame(&mut self) {
        assert!(
            !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "an interaction queued a subtree rebuild"
        );
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.compute_layout(400.0, 200.0);
    }

    fn deliver(&mut self, events: Vec<(LayoutNodeId, u32)>, at: (f32, f32)) {
        for (node, event) in events {
            self.tree.dispatch_event(node, event, at.0, at.1);
        }
        self.frame();
    }

    fn centre(&self) -> (f32, f32) {
        let b = self.tree.get_absolute_bounds(self.button()).unwrap();
        (b.x + b.width / 2.0, b.y + b.height / 2.0)
    }

    fn hover(&mut self) {
        let at = self.centre();
        let events = self.router.on_mouse_move(&self.tree, at.0, at.1);
        self.deliver(events, at);
    }

    fn press(&mut self) {
        let at = self.centre();
        let events = self
            .router
            .on_mouse_down(&self.tree, at.0, at.1, MouseButton::Left);
        self.deliver(events, at);
    }

    fn release(&mut self) {
        let at = self.centre();
        let events = self
            .router
            .on_mouse_up(&self.tree, at.0, at.1, MouseButton::Left);
        self.deliver(events, at);
    }

    fn leave(&mut self) {
        let at = (390.0, 190.0);
        let events = self.router.on_mouse_move(&self.tree, at.0, at.1);
        self.deliver(events, at);
    }

    fn fill(&self) -> Color {
        match &self
            .tree
            .get_render_node(self.button())
            .unwrap()
            .props
            .background
        {
            Some(Brush::Solid(c)) => *c,
            other => panic!("not a solid fill: {other:?}"),
        }
    }

    /// The colour the first text under the button is drawn in.
    fn label_colour(&self) -> [f32; 4] {
        let mut stack = vec![self.button()];
        while let Some(id) = stack.pop() {
            let node = self.tree.get_render_node(id).unwrap();
            if let ElementType::Text(t) = &node.element_type {
                return node.props.text_color.unwrap_or(t.color);
            }
            stack.extend(self.tree.layout_tree.children(id).into_iter().rev());
        }
        panic!("no text under the button");
    }
}

fn colours() -> blinc_layout::widgets::Button {
    button("Go")
        .bg_color(RED)
        .hover_color(GREEN)
        .pressed_color(BLUE)
        .disabled_color(GREY)
}

#[test]
fn the_fill_follows_hover_and_press() {
    let _g = init();
    let mut s = Scene::new(colours());
    assert_eq!(s.fill(), RED);
    s.hover();
    assert_eq!(s.fill(), GREEN);
    s.press();
    assert_eq!(s.fill(), BLUE);
    s.release();
    assert_eq!(s.fill(), GREEN, "an up over the button leaves it hovered");
    s.leave();
    assert_eq!(s.fill(), RED);
}

#[test]
fn a_disabled_button_keeps_its_disabled_fill() {
    let _g = init();
    let mut s = Scene::new(colours().disabled(true));
    assert_eq!(s.fill(), GREY);
    s.hover();
    s.press();
    assert_eq!(s.fill(), GREY);
}

#[test]
fn a_signal_disables_and_enables_in_place() {
    let _g = init();
    let disabled = flag(false);
    let mut s = Scene::new(colours().disabled(&disabled));
    s.hover();
    assert_eq!(s.fill(), GREEN);

    disabled.set(true);
    s.frame();
    assert_eq!(s.fill(), GREY);

    disabled.set(false);
    s.frame();
    assert_eq!(s.fill(), GREEN, "still hovered once enabled again");
}

#[test]
fn a_click_runs_only_while_enabled() {
    let _g = init();
    let clicks = Arc::new(AtomicUsize::new(0));
    let disabled = flag(false);
    let counter = Arc::clone(&clicks);
    let mut s = Scene::new(colours().disabled(&disabled).on_click(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    }));

    s.hover();
    s.press();
    s.release();
    assert_eq!(clicks.load(Ordering::SeqCst), 1);

    disabled.set(true);
    s.frame();
    s.press();
    s.release();
    assert_eq!(clicks.load(Ordering::SeqCst), 1, "clicked while disabled");

    disabled.set(false);
    s.frame();
    s.press();
    s.release();
    assert_eq!(clicks.load(Ordering::SeqCst), 2);
}

#[test]
fn content_gets_the_colour_the_button_resolves_for_its_state() {
    let _g = init();
    set_active_stylesheet(Arc::new(
        Stylesheet::parse(".b:hover { color: #00ff00 } .b:active { color: #0000ff }").unwrap(),
    ));
    let mut s =
        Scene::new(button_with(|look| div().child(text("Go").color(look.text_color()))).class("b"));
    let idle = s.label_colour();
    s.hover();
    assert_eq!(s.label_colour(), [0.0, 1.0, 0.0, 1.0]);
    s.press();
    assert_eq!(s.label_colour(), [0.0, 0.0, 1.0, 1.0]);
    s.leave();
    assert_eq!(
        s.label_colour(),
        idle,
        "a hover colour must not outlast the hover"
    );
}

/// Two buttons side by side, the first hovered; their fills afterwards.
fn hover_the_first_of_two(first_key: &str, second_key: &str) -> (Color, Color) {
    let make = |key: &str| colours().key(key).w(100.0).h(40.0);
    let host = div()
        .w(400.0)
        .h(200.0)
        .flex_row()
        .child(make(first_key))
        .child(make(second_key));
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    let kids = tree.layout_tree.children(tree.root().unwrap());
    let mut router = EventRouter::new();
    let b = tree.get_absolute_bounds(kids[0]).unwrap();
    let at = (b.x + 10.0, b.y + 10.0);
    let events = router.on_mouse_move(&tree, at.0, at.1);
    for (n, e) in events {
        tree.dispatch_event(n, e, at.0, at.1);
    }
    let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);

    let fill = |n: LayoutNodeId| match &tree.get_render_node(n).unwrap().props.background {
        Some(Brush::Solid(c)) => *c,
        other => panic!("{other:?}"),
    };
    (fill(kids[0]), fill(kids[1]))
}

#[test]
fn buttons_with_different_keys_keep_separate_pointer_state() {
    let _g = init();
    assert_eq!(
        hover_the_first_of_two("sep-a", "sep-b"),
        (GREEN, RED),
        "hovering one button restyled the other"
    );
}

#[test]
fn buttons_with_one_key_share_pointer_state() {
    let _g = init();
    assert_eq!(
        hover_the_first_of_two("same", "same"),
        (GREEN, GREEN),
        "the key does not name the pointer state"
    );
}
