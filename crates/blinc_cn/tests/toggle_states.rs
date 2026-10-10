//! What a toggle looks like in each of its states, recorded as text.
//!
//! A scenario builds a toggle, then walks it through hover, press, click and
//! leave. After each step the queued rebuilds and property writes are applied,
//! as a runner does, and every node of the toggle is written out: fill,
//! border, opacity, radius and the colour a text or an icon is drawn with. The
//! record is compared with `toggle_states.golden`.
//!
//! Set `BLESS_GOLDEN=1` to write the record instead of comparing.

use blinc_core::events::event_types;
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::css_parser::{Stylesheet, set_active_stylesheet};
use blinc_layout::div::{ElementBuilder, div};
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::{ElementType, RenderTree};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

/// The pending queues and the active stylesheet are process-wide.
static LOCK: Mutex<()> = Mutex::new(());

const ICON: &str = "<svg viewBox=\"0 0 24 24\"><path d=\"M4 4h16v16H4z\"/></svg>";

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        // The records are made with one theme, not the system's, so they
        // read the same on every machine.
        blinc_theme::ThemeState::init(
            blinc_theme::HybridTheme::bundle(),
            blinc_theme::ColorScheme::Dark,
        );
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

fn bool_state(initial: bool) -> State<bool> {
    State::new(signal::<bool>(initial), global_graph(), global_dirty_flag())
}

fn rgba(c: impl Into<[f32; 4]>) -> String {
    let c = c.into();
    format!("{:.3},{:.3},{:.3},{:.3}", c[0], c[1], c[2], c[3])
}

/// Every node under `root`, one line each, in tree order.
fn describe(tree: &RenderTree, root: LayoutNodeId, out: &mut String, depth: usize) {
    let node = tree.get_render_node(root).expect("render node");
    let p = &node.props;
    let fill = match &p.background {
        Some(blinc_core::Brush::Solid(c)) => rgba([c.r, c.g, c.b, c.a]),
        Some(_) => "gradient".into(),
        None => "-".into(),
    };
    let border = match p.border_color {
        Some(c) => format!("{} w{:.1}", rgba([c.r, c.g, c.b, c.a]), p.border_width),
        None => "-".into(),
    };
    let kind = match &node.element_type {
        ElementType::Text(t) => {
            let drawn = p
                .text_color
                .unwrap_or([t.color[0], t.color[1], t.color[2], t.color[3]]);
            format!(
                "text {:?} color {} size {:.1}",
                t.content,
                rgba(drawn),
                p.font_size.unwrap_or(t.font_size)
            )
        }
        ElementType::Svg(s) => {
            let drawn = p.svg_tint.or_else(|| s.tint.map(|c| [c.r, c.g, c.b, c.a]));
            format!("svg tint {}", drawn.map(rgba).unwrap_or_else(|| "-".into()))
        }
        _ => "div".into(),
    };
    let b = tree.get_absolute_bounds(root).expect("bounds");
    let _ = writeln!(
        out,
        "{}{kind} | fill {fill} | border {border} | opacity {:.2} | radius {:.1} | {:.1}x{:.1}",
        "  ".repeat(depth),
        p.opacity,
        p.border_radius.top_left,
        b.width,
        b.height,
    );
    for child in tree.layout_tree.children(root) {
        describe(tree, child, out, depth + 1);
    }
}

struct Harness {
    tree: RenderTree,
    router: EventRouter,
    on: State<bool>,
    out: String,
}

impl Harness {
    fn new(widget: impl ElementBuilder + 'static, on: State<bool>) -> Self {
        let host = div().w(400.0).h(200.0).child(widget);
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(400.0, 200.0);
        Self {
            tree,
            router: EventRouter::new(),
            on,
            out: String::new(),
        }
    }

    /// What the frame loop does after an event.
    fn frame(&mut self) {
        blinc_layout::stateful::check_stateful_deps(&[self.on.signal_id()]);
        for _ in 0..3 {
            self.tree.process_pending_subtree_rebuilds();
            let updates = blinc_layout::take_pending_partial_prop_updates();
            self.tree.apply_partial_property_updates(updates);
        }
        self.tree.compute_layout(400.0, 200.0);
    }

    fn deliver(&mut self, events: Vec<(LayoutNodeId, u32)>, x: f32, y: f32) {
        for (node, event) in events {
            self.tree.dispatch_event(node, event, x, y);
        }
        assert!(
            std::env::var_os("BLESS_GOLDEN").is_some()
                || !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "an interaction queued a subtree rebuild"
        );
        self.frame();
    }

    fn record(&mut self, step: &str) {
        let _ = writeln!(self.out, "== {step}");
        let root = self.tree.root().unwrap();
        let toggle = self.tree.layout_tree.children(root)[0];
        describe(&self.tree, toggle, &mut self.out, 0);
    }

    fn walk(&mut self) {
        self.frame();
        self.record("idle");
        let events = self.router.on_mouse_move(&self.tree, 20.0, 15.0);
        self.deliver(events, 20.0, 15.0);
        self.record("hover");
        let events = self
            .router
            .on_mouse_down(&self.tree, 20.0, 15.0, MouseButton::Left);
        self.deliver(events, 20.0, 15.0);
        self.record("pressed");
        let events = self
            .router
            .on_mouse_up(&self.tree, 20.0, 15.0, MouseButton::Left);
        self.deliver(events, 20.0, 15.0);
        self.record("released, flipped");
        let events = self.router.on_mouse_move(&self.tree, 300.0, 180.0);
        self.deliver(events, 300.0, 180.0);
        self.record("left");
        let events = self.router.on_mouse_move(&self.tree, 20.0, 15.0);
        self.deliver(events, 20.0, 15.0);
        self.record("hover again");
        let events = self
            .router
            .on_mouse_down(&self.tree, 20.0, 15.0, MouseButton::Left);
        self.deliver(events, 20.0, 15.0);
        self.record("pressed again");
        let events = self
            .router
            .on_mouse_up(&self.tree, 20.0, 15.0, MouseButton::Left);
        self.deliver(events, 20.0, 15.0);
        self.record("released, flipped back");
        let _ = event_types::POINTER_UP;
    }
}

fn scenario(
    name: &str,
    css: &str,
    build: impl FnOnce(&State<bool>) -> Box<dyn ElementBuilder>,
) -> String {
    set_active_stylesheet(Arc::new(Stylesheet::parse(css).expect("css")));
    let on = bool_state(false);
    let widget = build(&on);
    let mut h = Harness::new(BoxedElement(widget), on);
    let mut out = format!("######## {name}\n");
    h.walk();
    out.push_str(&h.out);
    out
}

struct BoxedElement(Box<dyn ElementBuilder>);

impl ElementBuilder for BoxedElement {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> LayoutNodeId {
        self.0.build(tree)
    }
    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.0.render_props()
    }
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.0.children_builders()
    }
    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.0.element_type_id()
    }
    fn semantic_type_name(&self) -> Option<&'static str> {
        self.0.semantic_type_name()
    }
    fn layout_style(&self) -> Option<&taffy::Style> {
        self.0.layout_style()
    }
    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.0.event_handlers()
    }
    fn element_id(&self) -> Option<&str> {
        self.0.element_id()
    }
    fn element_classes(&self) -> &[Arc<str>] {
        self.0.element_classes()
    }
}

fn record() -> String {
    let mut out = String::new();
    out += &scenario("default with a label", "", |on| {
        Box::new(blinc_cn::toggle(on).label("Bold"))
    });
    out += &scenario("outline with an icon and a label", "", |on| {
        Box::new(
            blinc_cn::toggle(on)
                .variant(blinc_cn::ToggleVariant::Outline)
                .icon(ICON)
                .label("Bold"),
        )
    });
    out += &scenario("disabled", "", |on| {
        Box::new(blinc_cn::toggle(on).label("Bold").disabled(true))
    });
    out += &scenario(
        "hover and checked rules in a stylesheet",
        ".cn-toggle:hover { background: #336699; color: #ffeedd } .cn-toggle { border-radius: 4px }",
        |on| Box::new(blinc_cn::toggle(on).icon(ICON).label("Bold")),
    );
    out
}

#[test]
fn a_toggle_looks_the_same_in_every_state() {
    init();
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let _ = blinc_layout::take_pending_partial_prop_updates();

    let got = record();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/toggle_states.golden");
    if std::env::var_os("BLESS_GOLDEN").is_some() {
        std::fs::write(&path, &got).expect("write golden");
        return;
    }
    // A checkout may have turned the file's line feeds into CRLF.
    let want = std::fs::read_to_string(&path)
        .expect("tests/toggle_states.golden")
        .replace("\r\n", "\n");
    assert_eq!(got, want, "the toggle no longer looks as recorded");
}
