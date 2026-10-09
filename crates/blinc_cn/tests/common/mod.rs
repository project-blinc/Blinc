//! A harness that walks a widget through pointer states and writes down what
//! it looks like after each step. Shared by the `*_states` tests.
#![allow(dead_code)]

use blinc_core::reactive::{SignalId, State, global_dirty_flag, global_graph, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::{ElementBuilder, div};
use blinc_layout::event_router::{EventRouter, MouseButton};
use blinc_layout::renderer::{ElementType, RenderTree};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

pub fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
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

pub fn bool_state(initial: bool) -> State<bool> {
    State::new(signal::<bool>(initial), global_graph(), global_dirty_flag())
}

pub fn string_state(initial: &str) -> State<String> {
    State::new(
        signal::<String>(initial.to_string()),
        global_graph(),
        global_dirty_flag(),
    )
}

pub fn rgba(c: [f32; 4]) -> String {
    format!("{:.3},{:.3},{:.3},{:.3}", c[0], c[1], c[2], c[3])
}

/// The fill of a node, `-` when there is none or it cannot be seen.
pub fn fill_of(tree: &RenderTree, node: LayoutNodeId) -> String {
    match &tree
        .get_render_node(node)
        .expect("render node")
        .props
        .background
    {
        Some(blinc_core::Brush::Solid(c)) if c.a > 0.0 => rgba([c.r, c.g, c.b, c.a]),
        Some(blinc_core::Brush::Solid(_)) | None => "-".into(),
        Some(_) => "gradient".into(),
    }
}

/// One line for a node: what it draws and where it lies.
pub fn node_line(tree: &RenderTree, node: LayoutNodeId) -> String {
    let render = tree.get_render_node(node).expect("render node");
    let p = &render.props;
    let border = match p.border_color {
        Some(c) => format!("{} w{:.1}", rgba([c.r, c.g, c.b, c.a]), p.border_width),
        None => "-".into(),
    };
    let kind = match &render.element_type {
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
    let b = tree.get_absolute_bounds(node).expect("bounds");
    format!(
        "{kind} | fill {} | border {border} | opacity {:.2} | radius {:.1} | scale {} | {:.1}x{:.1}",
        fill_of(tree, node),
        p.opacity,
        p.border_radius.top_left,
        match &p.transform {
            Some(t) => format!("{t:?}"),
            None => "-".into(),
        },
        b.width,
        b.height,
    )
}

/// Every node under `root`, one line each, in tree order.
pub fn describe_tree(tree: &RenderTree, root: LayoutNodeId, out: &mut String, depth: usize) {
    let _ = writeln!(out, "{}{}", "  ".repeat(depth), node_line(tree, root));
    for child in tree.layout_tree.children(root) {
        describe_tree(tree, child, out, depth + 1);
    }
}

pub type Describe = fn(&RenderTree, LayoutNodeId, &mut String);

pub struct BoxedElement(pub Box<dyn ElementBuilder>);

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

pub struct Harness {
    pub tree: RenderTree,
    router: EventRouter,
    deps: Vec<SignalId>,
    describe: Describe,
    /// Whether an interaction must be absorbed without a subtree rebuild.
    pub in_place: bool,
    pub out: String,
}

impl Harness {
    pub fn new(widget: Box<dyn ElementBuilder>, deps: Vec<SignalId>, describe: Describe) -> Self {
        let host = div().w(400.0).h(200.0).child(BoxedElement(widget));
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(400.0, 200.0);
        Self {
            tree,
            router: EventRouter::new(),
            deps,
            describe,
            in_place: false,
            out: String::new(),
        }
    }

    /// What the frame loop does after an event.
    pub fn frame(&mut self) {
        blinc_layout::stateful::check_stateful_deps(&self.deps);
        for _ in 0..3 {
            self.tree.process_pending_subtree_rebuilds();
            let updates = blinc_layout::take_pending_partial_prop_updates();
            self.tree.apply_partial_property_updates(updates);
        }
        self.tree.compute_layout(400.0, 200.0);
    }

    fn deliver(&mut self, events: Vec<(LayoutNodeId, u32)>, at: (f32, f32)) {
        for (node, event) in events {
            self.tree.dispatch_event(node, event, at.0, at.1);
        }
        assert!(
            !self.in_place || !blinc_layout::stateful::has_pending_subtree_rebuilds(),
            "an interaction queued a subtree rebuild"
        );
        self.frame();
    }

    pub fn record(&mut self, step: &str) {
        let _ = writeln!(self.out, "== {step}");
        let root = self.tree.root().unwrap();
        let widget = self.tree.layout_tree.children(root)[0];
        (self.describe)(&self.tree, widget, &mut self.out);
    }

    pub fn move_to(&mut self, at: (f32, f32)) {
        let events = self.router.on_mouse_move(&self.tree, at.0, at.1);
        self.deliver(events, at);
    }

    pub fn press(&mut self, at: (f32, f32)) {
        let events = self
            .router
            .on_mouse_down(&self.tree, at.0, at.1, MouseButton::Left);
        self.deliver(events, at);
    }

    pub fn release(&mut self, at: (f32, f32)) {
        let events = self
            .router
            .on_mouse_up(&self.tree, at.0, at.1, MouseButton::Left);
        self.deliver(events, at);
    }

    /// Hover, press and click on `at`, then leave to `away` and come back,
    /// recording after each step.
    pub fn walk(&mut self, at: (f32, f32), away: (f32, f32)) {
        self.frame();
        self.record("idle");
        self.move_to(at);
        self.record("hover");
        self.press(at);
        self.record("pressed");
        self.release(at);
        self.record("released");
        self.move_to(away);
        self.record("left");
        self.move_to(at);
        self.record("hover again");
        self.press(at);
        self.record("pressed again");
        self.release(at);
        self.record("released again");
    }
}

/// The centre of the node reached from the widget root by child indices.
impl Harness {
    pub fn center(&self, path: &[usize]) -> (f32, f32) {
        let root = self.tree.root().unwrap();
        let mut node = self.tree.layout_tree.children(root)[0];
        for i in path {
            node = self.tree.layout_tree.children(node)[*i];
        }
        let b = self.tree.get_absolute_bounds(node).expect("bounds");
        (b.x + b.width / 2.0, b.y + b.height / 2.0)
    }
}

/// The tint of the first svg under `node` that is not hidden, if any.
pub fn visible_svg(tree: &RenderTree, node: LayoutNodeId) -> Option<String> {
    if tree.layout_tree.is_display_none(node) {
        return None;
    }
    let render = tree.get_render_node(node)?;
    if let ElementType::Svg(s) = &render.element_type {
        let drawn = render
            .props
            .svg_tint
            .or_else(|| s.tint.map(|c| [c.r, c.g, c.b, c.a]));
        return Some(drawn.map(rgba).unwrap_or_else(|| "-".into()));
    }
    tree.layout_tree
        .children(node)
        .into_iter()
        .find_map(|c| visible_svg(tree, c))
}

/// Run `scenario` on a fresh state under `css`, under the process-wide lock.
pub fn guarded<T>(f: impl FnOnce() -> T) -> T {
    static LOCK: Mutex<()> = Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    let _ = blinc_layout::take_pending_partial_prop_updates();
    f()
}

/// Compare `got` with the recorded file, or write it when `BLESS_GOLDEN` is set.
pub fn check_golden(name: &str, got: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name);
    if std::env::var_os("BLESS_GOLDEN").is_some() {
        std::fs::write(&path, got).expect("write golden");
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("tests/{name}"));
    assert_eq!(
        got, want,
        "tests/{name}: the widget no longer looks as recorded"
    );
}
