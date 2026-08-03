//! `cn.InputOTP` — slots bound to one string signal.
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
        let host = div().w(600.0).h(200.0).child_box(dsl.view_widget());
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(600.0, 200.0);
        Self { tree }
    }

    /// Apply whatever a signal write queued, as a frame would.
    fn frame(&mut self) {
        self.tree.process_pending_subtree_rebuilds();
        self.tree.compute_layout(600.0, 200.0);
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
}

/// The whole contract in one test — signals are process-global by name,
/// so splitting it would have the programs reading each other's code.
///
/// A bound value fills the slots at first build, and a later write is
/// applied without anything being typed — the "Prefill" button case.
#[test]
fn slots_follow_the_bound_signal() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal otp_probe: string = "12"

           view {
             cn.InputOTP(value = otp_probe, length = 4.0, numeric_only = true)
           }"#,
        "otp.blinc",
    )
    .expect("compile");

    let mut h = Harness::new(&dsl);
    let opened: String = h.texts().concat();
    assert!(
        opened.contains('1') && opened.contains('2'),
        "prefilled characters land in the slots: {:?}",
        h.texts()
    );

    // A write from outside — nothing typed, no rebuild requested.
    dsl.set_signal_string("otp_probe", "9876");
    h.frame();
    let after: String = h.texts().concat();
    for c in ["9", "8", "7", "6"] {
        assert!(after.contains(c), "slot shows {c}: {:?}", h.texts());
    }
}
