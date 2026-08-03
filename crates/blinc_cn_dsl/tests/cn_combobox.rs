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

/// The field shows the picked option's LABEL, not its value.
#[test]
fn the_field_shows_the_picked_label() {
    let dsl = compile(
        r#"signal cb_plan: string = "pro"

           view {
             cn.Combobox(value = cb_plan, label = "Plan") {
               cn.Option(value = "free", label = "Free")
               cn.Option(value = "pro", label = "Pro")
             }
           }"#,
        "combo_label.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(h.shows("Plan"), "the combobox's own label: {:?}", h.texts());
    assert!(h.shows("Pro"), "the picked option's label: {:?}", h.texts());
}

/// Nothing picked, so the placeholder stands in.
#[test]
fn a_placeholder_shows_while_nothing_is_picked() {
    let dsl = compile(
        r#"signal cb_empty: string = ""

           view {
             cn.Combobox(value = cb_empty, placeholder = "Search plans") {
               cn.Option(value = "free", label = "Free")
             }
           }"#,
        "combo_placeholder.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(h.shows("Search plans"), "{:?}", h.texts());
}

/// Writing the signal moves the selection.
#[test]
fn writing_the_signal_reselects() {
    let dsl = compile(
        r#"signal cb_bound: string = "one"

           view {
             cn.Combobox(value = cb_bound) {
               cn.Option(value = "one", label = "One")
               cn.Option(value = "two", label = "Two")
             }
           }"#,
        "combo_bound.blinc",
    );

    let _ = Harness::new(&dsl);
    blinc_runtime::signal::set_str("cb_bound", "two");
    assert_eq!(
        blinc_runtime::signal::get_str("cb_bound").as_deref(),
        Some("two"),
    );
    let h = Harness::new(&dsl);
    assert!(
        h.shows("Two"),
        "the field follows the signal: {:?}",
        h.texts()
    );
}

/// `allow_custom` lets the signal carry a value no option offered, so a
/// combobox can accept something the list never had.
#[test]
fn allow_custom_keeps_a_value_no_option_offers() {
    let dsl = compile(
        r#"signal cb_custom: string = "something else"

           view {
             cn.Combobox(value = cb_custom, allow_custom = true) {
               cn.Option(value = "one", label = "One")
             }
           }"#,
        "combo_custom.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(
        h.shows("something else"),
        "a value outside the list survives: {:?}",
        h.texts()
    );
}

/// A child that is not a `cn.Option` is dropped rather than drawn
/// somewhere arbitrary.
#[test]
fn a_loose_child_is_dropped() {
    let dsl = compile(
        r#"signal cb_loose: string = "solo"

           view {
             cn.Combobox(value = cb_loose) {
               cn.Label("not an option")
               cn.Option(value = "solo")
             }
           }"#,
        "combo_loose.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(h.shows("solo"), "the value stands in: {:?}", h.texts());
    assert!(
        !h.shows("not an option"),
        "loose child dropped: {:?}",
        h.texts()
    );
}

/// The same `cn.Option` serves both parents, which is why it is not
/// named per-parent.
#[test]
fn one_option_type_serves_select_and_combobox() {
    let dsl = compile(
        r#"signal cb_shared_a: string = "x"
           signal cb_shared_b: string = "x"

           view {
             Div {
               cn.Select(value = cb_shared_a, label = "As select") {
                 cn.Option(value = "x", label = "Ex")
               }
               cn.Combobox(value = cb_shared_b, label = "As combobox") {
                 cn.Option(value = "x", label = "Ex")
               }
             }
           }"#,
        "combo_shared_option.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(
        h.shows("As select") && h.shows("As combobox"),
        "{:?}",
        h.texts()
    );
    assert!(
        h.texts().iter().filter(|t| t.contains("Ex")).count() >= 2,
        "each parent drew the shared option: {:?}",
        h.texts()
    );
}
