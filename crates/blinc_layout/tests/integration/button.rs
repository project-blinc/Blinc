//! The core button is restyled by `:hover`, `:active` and `:disabled` rules
//! without a rebuild, runs its click only while enabled, and hands its
//! content the colour its state gives it.

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
const GREY: Color = Color::rgb(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0);

fn init() -> std::sync::MutexGuard<'static, ()> {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init(
            blinc_theme::HybridTheme::bundle(),
            blinc_theme::ColorScheme::Dark,
        );
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
        Self::styled("", widget)
    }

    fn styled(css: &str, widget: impl ElementBuilder + 'static) -> Self {
        let host = div().w(400.0).h(200.0).child(widget);
        let mut tree = RenderTree::from_element(&host);
        let vars = blinc_theme::ThemeState::get().to_css_variable_map();
        tree.set_stylesheet(Stylesheet::parse_with_variables(css, &vars).expect("css"));
        tree.apply_stylesheet_layout_overrides();
        tree.apply_stylesheet_base_styles();
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
        self.tree.apply_stylesheet_state_styles(&self.router);
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
                return self.tree.resolved_text_color(id).unwrap_or(t.color);
            }
            stack.extend(self.tree.layout_tree.children(id).into_iter().rev());
        }
        panic!("no text under the button");
    }
}

const LOOK: &str = r#"
.b { background: #ff0000; }
.b:hover { background: #00ff00; }
.b:active { background: #0000ff; }
.b:disabled, .b:disabled:hover, .b:disabled:active { background: #808080; }
"#;

fn looks() -> blinc_layout::widgets::Button {
    button("Go").class("b")
}

#[test]
fn the_fill_follows_hover_and_press() {
    let _g = init();
    let mut s = Scene::styled(LOOK, looks());
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
    let mut s = Scene::styled(LOOK, looks().disabled(true));
    assert_eq!(s.fill(), GREY);
    s.hover();
    s.press();
    assert_eq!(s.fill(), GREY);
}

#[test]
fn a_signal_disables_and_enables_in_place() {
    let _g = init();
    let disabled = flag(false);
    let mut s = Scene::styled(LOOK, looks().disabled(&disabled));
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
    let mut s = Scene::new(button("Go").disabled(&disabled).on_click(move |_| {
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
fn content_takes_the_colour_the_button_gives_it_in_each_state() {
    let _g = init();
    let mut s = Scene::styled(
        ".b { color: #ff0000 } .b:hover { color: #00ff00 } .b:active { color: #0000ff }",
        button_with(|_| div().child(text("Go"))).class("b"),
    );
    assert_eq!(s.label_colour(), [1.0, 0.0, 0.0, 1.0]);
    s.hover();
    assert_eq!(s.label_colour(), [0.0, 1.0, 0.0, 1.0]);
    s.press();
    assert_eq!(s.label_colour(), [0.0, 0.0, 1.0, 1.0]);
    s.release();
    s.leave();
    assert_eq!(
        s.label_colour(),
        [1.0, 0.0, 0.0, 1.0],
        "a hover colour must not outlast the hover"
    );
}

/// Two buttons side by side, the first hovered; their fills afterwards.
#[test]
fn hovering_one_button_leaves_the_other_alone() {
    let _g = init();
    let make = |key: &str| looks().key(key).w(100.0).h(40.0);
    let host = div()
        .w(400.0)
        .h(200.0)
        .flex_row()
        .child(make("sep-a"))
        .child(make("sep-b"));
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(Stylesheet::parse(LOOK).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 200.0);
    let kids = tree.layout_tree.children(tree.root().unwrap());
    let mut router = EventRouter::new();
    let b = tree.get_absolute_bounds(kids[0]).unwrap();
    let _ = router.on_mouse_move(&tree, b.x + 10.0, b.y + 10.0);
    tree.apply_stylesheet_state_styles(&router);

    let fill = |n: LayoutNodeId| match &tree.get_render_node(n).unwrap().props.background {
        Some(Brush::Solid(c)) => *c,
        other => panic!("{other:?}"),
    };
    assert_eq!((fill(kids[0]), fill(kids[1])), (GREEN, RED));
}

/// A disabled button matches `:disabled` rules, and stops when enabled.
#[test]
fn a_disabled_button_matches_a_disabled_rule_and_stops_when_enabled() {
    let _g = init();
    let off = flag(true);
    let mut s = Scene::styled(
        ".b:disabled { opacity: 0.4; }",
        button("Go").class("b").disabled(&off),
    );
    let opacity = |s: &Scene| s.tree.get_render_node(s.button()).unwrap().props.opacity;
    assert_eq!(opacity(&s), 0.4);

    off.set(false);
    s.frame();
    assert_eq!(opacity(&s), 1.0);
}

/// The default look: the theme's primary fill, its hover and disabled fills.
#[test]
fn the_default_look_comes_from_the_ua_sheet() {
    use blinc_theme::{ColorToken, ThemeState};
    let _g = init();
    let theme = ThemeState::get();
    let near = |got: Color, want: Color| {
        let d = (got.r - want.r).abs() + (got.g - want.g).abs() + (got.b - want.b).abs();
        assert!(d < 0.02, "{got:?} is not {want:?}");
    };
    let mut s = Scene::styled(blinc_layout::ua_styles::UA_STYLES, button("Go"));
    near(s.fill(), theme.color(ColorToken::Primary));
    s.hover();
    near(s.fill(), theme.color(ColorToken::PrimaryHover));

    let mut off = Scene::styled(
        blinc_layout::ua_styles::UA_STYLES,
        button("Go").disabled(true),
    );
    off.hover();
    near(off.fill(), theme.color(ColorToken::InputBgDisabled));
}

/// A fluent fill is not overridden by the default look.
#[test]
fn a_fluent_fill_wins_over_the_default_look() {
    let _g = init();
    let s = Scene::styled(
        blinc_layout::ua_styles::UA_STYLES,
        button("Go").bg_color(BLUE),
    );
    assert_eq!(s.fill(), BLUE);
}
