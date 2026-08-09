//! `cn.Tabs` — a strip of `cn.Tab` children, one panel showing.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::{Arc, Mutex};

/// The pending-subtree-rebuild queue is process-global, and every test
/// here builds its own tree and drains it. Run in parallel they take
/// each other's entries, and a recycled `LayoutNodeId` lands a stolen
/// entry on an unrelated node. Taking this first makes them take turns.
///
/// Poison is ignored on purpose: one failing assertion must not cascade
/// into every other test in the file reporting a lock error instead of
/// its own result.
static SUBTREE_QUEUE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SUBTREE_QUEUE.lock().unwrap_or_else(|e| e.into_inner())
}

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        // The app installs this; without it a signal write reaches
        // property bindings but never the statefuls that declared deps
        // on it, so a subscribed widget looks unsubscribed.
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// One tree, rebuilt in place — what a running app does. A fresh tree
/// per check would re-run every callback whether or not anything
/// subscribed, so a widget that could never react would still pass.
struct Harness {
    tree: RenderTree,
}

impl Harness {
    fn new(dsl: &BlincDsl) -> Self {
        let host = div().w(600.0).h(400.0).child_box(dsl.view_widget());
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(600.0, 400.0);
        Self { tree }
    }

    /// Apply whatever a signal write queued, as a frame would.
    fn frame(&mut self) {
        self.tree.process_pending_subtree_rebuilds();
        self.tree.compute_layout(600.0, 400.0);
    }

    fn texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        let Some(root) = self.tree.root() else {
            return out;
        };
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if let Some(node) = self.tree.get_render_node(id)
                && let ElementType::Text(t) = &node.element_type
            {
                out.push(t.content.clone());
            }
            stack.extend(self.tree.layout_tree.children(id).iter().copied());
        }
        out
    }

    fn shows(&self, needle: &str) -> bool {
        self.texts().iter().any(|t| t.contains(needle))
    }
}

/// The whole contract in one test: tabs are process-global by signal
/// name, so a second program declaring `signal section` would read this
/// one's value.
#[test]
fn tabs_draw_their_strip_and_follow_the_bound_signal() {
    let _serial = serial();
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal tab_probe: string = "account"

           view {
             cn.Tabs(value = tab_probe) {
               cn.Tab(value = "account", label = "Account") {
                 Div { Text("who you are") }
               }
               cn.Tab(value = "alerts", label = "Alerts") {
                 Div { Text("what we sent you") }
               }
             }
           }"#,
        "tabs.blinc",
    )
    .expect("compile");

    let mut h = Harness::new(&dsl);

    // Every tab appears in the strip, whether or not it is showing.
    assert!(h.shows("Account"), "strip: {:?}", h.texts());
    assert!(h.shows("Alerts"), "strip: {:?}", h.texts());

    // Only the selected panel is drawn.
    assert!(h.shows("who you are"), "the open panel: {:?}", h.texts());
    assert!(
        !h.shows("what we sent you"),
        "and not the other one: {:?}",
        h.texts()
    );

    // A write from outside moves the strip. Nothing was clicked.
    dsl.set_signal_string("tab_probe", "alerts");
    h.frame();

    assert!(h.shows("what we sent you"), "swapped: {:?}", h.texts());
    assert!(
        !h.shows("who you are"),
        "and the first panel went: {:?}",
        h.texts()
    );
}

/// A child that is not a `cn.Tab` has nowhere to go, so it is dropped
/// rather than drawn somewhere arbitrary.
#[test]
fn a_loose_child_is_dropped_not_drawn() {
    let _serial = serial();
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal loose_probe: string = "one"

           view {
             cn.Tabs(value = loose_probe) {
               cn.Label("not a tab")
               cn.Tab(value = "one", label = "One") {
                 Div { Text("first panel") }
               }
             }
           }"#,
        "tabs_loose.blinc",
    )
    .expect("compile");

    let h = Harness::new(&dsl);
    assert!(
        h.shows("One"),
        "the real tab is in the strip: {:?}",
        h.texts()
    );
    assert!(
        !h.shows("not a tab"),
        "the loose child is not drawn: {:?}",
        h.texts()
    );
}

/// Inherited text colour has to survive the panel swap.
///
/// The colour comes from a `color:` on an ancestor, which the full
/// stylesheet pass propagates down. A tab swap rebuilds only the panel,
/// so if the subtree pass does not re-propagate, the new text falls back
/// to the layout default — black, and invisible on a dark scheme.
#[test]
fn panel_text_keeps_its_inherited_colour_across_a_swap() {
    let _serial = serial();
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal swap_probe: string = "one"
           view {
             Div(class = "page") {
               cn.Tabs(value = swap_probe) {
                 cn.Tab(value = "one", label = "One") { Div { Text("first body") } }
                 cn.Tab(value = "two", label = "Two") { Div { Text("second body") } }
               }
             }
           }"#,
        "swap_probe.blinc",
    )
    .expect("compile");

    let host = div().w(600.0).h(400.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(
        blinc_layout::css_parser::Stylesheet::parse(".page { color: #ff0000 }").expect("css"),
    );
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(600.0, 400.0);

    let painted = |tree: &RenderTree, needle: &str| -> Option<[f32; 4]> {
        let mut stack = vec![tree.root()?];
        while let Some(id) = stack.pop() {
            if let Some(node) = tree.get_render_node(id)
                && let ElementType::Text(t) = &node.element_type
                && t.content.contains(needle)
            {
                return Some(node.props.text_color.unwrap_or(t.color));
            }
            stack.extend(tree.layout_tree.children(id).iter().copied());
        }
        None
    };

    let red = [1.0, 0.0, 0.0, 1.0];
    assert_eq!(
        painted(&tree, "first body"),
        Some(red),
        "inherited at build"
    );

    dsl.set_signal_string("swap_probe", "two");
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(600.0, 400.0);

    assert_eq!(
        painted(&tree, "second body"),
        Some(red),
        "and still inherited after the swap"
    );
}
