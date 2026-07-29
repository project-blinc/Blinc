//! Baseline for the effects-reactivity change: how many re-renders does
//! each playground interaction cost, and how long does the dispatch take?
//!
//! The claim under test is that observed dependencies (a read handler
//! recording what a body actually read) over-subscribe LESS than
//! inferred ones (an AST scrape guessing what it will read).
//! Over-subscription is what shows up as jank, so re-render count per
//! interaction is the metric, not throughput.
//!
//! Run this on both sides of the change and compare. It prints rather
//! than asserting tight numbers: the point is a recorded before/after,
//! and an assertion tuned to today's counts would just have to be
//! rewritten by the change it exists to measure.
//!
//! The one thing it DOES assert is that every interaction still
//! produces a correct tree — a change that re-renders less by rendering
//! nothing would otherwise look like an improvement.

use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

static RENDERS: AtomicUsize = AtomicUsize::new(0);

/// Counts every scoped or whole-program re-render. Both log lines carry
/// "on_state re-render", so this survives the region rework.
struct CountRenders;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CountRenders {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        struct Find(bool);
        impl tracing::field::Visit for Find {
            fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
                if f.name() == "message" && format!("{v:?}").contains("on_state re-render") {
                    self.0 = true;
                }
            }
        }
        let mut find = Find(false);
        event.record(&mut find);
        if find.0 {
            RENDERS.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        use tracing_subscriber::prelude::*;
        tracing_subscriber::registry().with(CountRenders).init();
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(AtomicBool::new(false)),
            );
        }
    });
    blinc_core::BlincContextState::get().set_viewport_size(720.0, 820.0);
}

fn node_count(tree: &RenderTree) -> usize {
    let Some(root) = tree.root() else {
        return 0;
    };
    let mut n = 0;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        n += 1;
        stack.extend(tree.layout_tree.children(id));
    }
    n
}

/// Dispatch every playground action in turn, recording re-renders and
/// wall time for each, plus the resulting tree size.
#[test]
fn playground_interaction_cost() {
    init();
    let root_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = BlincDsl::new().unwrap();
    blinc_cn_dsl::register_all(&dsl).unwrap();
    dsl.compile_project(&root_dir.join("main.blinc"), &root_dir)
        .unwrap();

    let css: String = blinc_core::BlincContextState::get()
        .drain_stylesheets()
        .join("\n");
    let host = div().w(720.0).h(820.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(blinc_layout::css_parser::Stylesheet::parse(&css).unwrap());
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(720.0, 820.0);
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(720.0, 820.0);

    let mounted = node_count(&tree);
    assert!(mounted > 50, "the playground must actually mount: {mounted}");
    println!("mounted nodes: {mounted}");

    // Grow drives bound props only. Busy swaps a branch. Reset restores
    // everything. Grow twice in a row is the interesting repeat: the
    // second one changes nothing structural either.
    for action in ["Grow", "Grow", "Busy", "Reset", "Busy", "Busy"] {
        RENDERS.store(0, Ordering::Relaxed);
        let start = std::time::Instant::now();
        blinc_runtime::fsm::dispatch_default("Play", action)
            .unwrap_or_else(|| panic!("{action} must dispatch"));
        let dispatch = start.elapsed();
        let during = RENDERS.load(Ordering::Relaxed);

        let start = std::time::Instant::now();
        tree.process_pending_subtree_rebuilds();
        tree.compute_layout(720.0, 820.0);
        let settle = start.elapsed();
        let total = RENDERS.load(Ordering::Relaxed);

        let nodes = node_count(&tree);
        println!(
            "{action:>6}: {total} re-render(s) ({during} during dispatch), \
             dispatch {dispatch:?}, settle {settle:?}, {nodes} nodes"
        );
        assert!(
            nodes > 50,
            "{action} must leave a populated tree, not an empty one: {nodes}"
        );
    }
}
