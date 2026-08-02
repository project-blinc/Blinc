//! `cn.RadioGroup` — a set of `cn.Radio` children, one picked.
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
        // The app installs this; without it a signal write never reaches
        // a stateful that declared deps on it.
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

/// One tree, rebuilt in place — what a running app does.
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

/// Every option draws, whichever is picked, and the group's own label
/// sits above them.
///
/// One test rather than several: signals are process-global by name, so
/// two programs declaring the same one read each other's value.
#[test]
fn a_group_draws_its_options_and_its_label() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal plan_probe: string = "free"

           view {
             cn.RadioGroup(value = plan_probe, label = "Plan") {
               cn.Radio(value = "free", label = "Free")
               cn.Radio(value = "pro", label = "Pro")
               cn.Radio(value = "team", label = "Team", disabled = true)
             }
           }"#,
        "radio.blinc",
    )
    .expect("compile");

    let h = Harness::new(&dsl);
    assert!(h.shows("Plan"), "the group's label: {:?}", h.texts());
    for option in ["Free", "Pro", "Team"] {
        assert!(h.shows(option), "option {option}: {:?}", h.texts());
    }
}

/// An option with only a value still reads as something, and a child
/// that is not a `cn.Radio` is dropped rather than drawn somewhere
/// arbitrary.
#[test]
fn a_bare_value_labels_itself_and_a_loose_child_is_dropped() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal loose_plan: string = "one"

           view {
             cn.RadioGroup(value = loose_plan) {
               cn.Label("not an option")
               cn.Radio(value = "solo")
             }
           }"#,
        "radio_loose.blinc",
    )
    .expect("compile");

    let h = Harness::new(&dsl);
    assert!(
        h.shows("solo"),
        "the value stands in for a missing label: {:?}",
        h.texts()
    );
    assert!(
        !h.shows("not an option"),
        "the loose child is not drawn: {:?}",
        h.texts()
    );
}
