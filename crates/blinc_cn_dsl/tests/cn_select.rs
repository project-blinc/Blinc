//! `cn.Select` — a bound signal and `cn.Option` children.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::{Arc, Mutex};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
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

fn compile(src: &str, name: &str) -> BlincDsl {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

/// The trigger shows the picked option's LABEL, not its value: an
/// author writes `value = "pro"` and expects to read "Pro".
#[test]
fn the_trigger_shows_the_picked_label() {
    let dsl = compile(
        r#"signal sel_plan: string = "pro"

           view {
             cn.Select(value = sel_plan, label = "Plan") {
               cn.Option(value = "free", label = "Free")
               cn.Option(value = "pro", label = "Pro")
             }
           }"#,
        "select_label.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(h.shows("Plan"), "the select's own label: {:?}", h.texts());
    assert!(h.shows("Pro"), "the picked option's label: {:?}", h.texts());
}

/// Nothing picked yet, so the placeholder stands in.
#[test]
fn a_placeholder_shows_while_nothing_is_picked() {
    let dsl = compile(
        r#"signal sel_empty: string = ""

           view {
             cn.Select(value = sel_empty, placeholder = "Pick a plan") {
               cn.Option(value = "free", label = "Free")
             }
           }"#,
        "select_placeholder.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(h.shows("Pick a plan"), "{:?}", h.texts());
}

/// Writing the signal moves the selection, which is the half of the
/// binding a click cannot prove.
#[test]
fn writing_the_signal_reselects() {
    let dsl = compile(
        r#"signal sel_bound: string = "one"

           view {
             cn.Select(value = sel_bound) {
               cn.Option(value = "one", label = "One")
               cn.Option(value = "two", label = "Two")
             }
           }"#,
        "select_bound.blinc",
    );

    let _ = Harness::new(&dsl);
    blinc_runtime::signal::set_str("sel_bound", "two");
    assert_eq!(
        blinc_runtime::signal::get_str("sel_bound").as_deref(),
        Some("two"),
    );
    let h = Harness::new(&dsl);
    assert!(
        h.shows("Two"),
        "the trigger follows the signal: {:?}",
        h.texts()
    );
}

/// A bare value labels itself, and a child that is not a `cn.Option` is
/// dropped rather than drawn somewhere arbitrary.
#[test]
fn a_bare_value_labels_itself_and_a_loose_child_is_dropped() {
    let dsl = compile(
        r#"signal sel_loose: string = "solo"

           view {
             cn.Select(value = sel_loose) {
               cn.Label("not an option")
               cn.Option(value = "solo")
             }
           }"#,
        "select_loose.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(h.shows("solo"), "the value stands in: {:?}", h.texts());
    assert!(
        !h.shows("not an option"),
        "loose child dropped: {:?}",
        h.texts()
    );
}

/// `cn.Option` is shared, not per-parent, so it renders on its own too
/// rather than vanishing outside a select.
#[test]
fn an_option_outside_a_select_still_renders() {
    let dsl = compile(
        r#"view { Div { cn.Option(value = "lonely", label = "Lonely") } }"#,
        "select_lone_option.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(h.shows("Lonely"), "{:?}", h.texts());
}
