//! What does one `Grow` dispatch actually cost?
//!
//! Grow only mutates context fields that are bound as PROPS
//! (`Progress(value)`, `Skeleton(w/h/rounded)`, `Separator(opacity)`).
//! Those write through the binding registry, so the frame should be a
//! repaint: no re-render of the view, no subtree rebuild, no layout.

use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

static RENDERS: AtomicUsize = AtomicUsize::new(0);

/// Counts the "stateful container: on_state re-render" line the DSL's
/// root stateful logs, which is one full re-render of the program.
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
            // Two globals hold a scheduler: this one, and
            // `blinc_layout::render_state`'s, which a real app fills
            // from `RenderState::new`. Widgets that animate read the
            // second, so a test that sets only the first panics with a
            // message naming neither.
            blinc_layout::render_state::set_global_scheduler(s.handle());
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

#[test]
fn grow_does_not_re_render_the_program() {
    init();
    let root_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = BlincDsl::new().unwrap();
    blinc_cn_dsl::register_all(&dsl).unwrap();
    dsl.compile_project(&root_dir.join("main.blinc"), &root_dir)
        .unwrap();
    // The rail starts on the forms page; everything measured here
    // lives on the reactive one, so navigate before building or the
    // assertions run against a page that was never mounted.
    dsl.set_signal_string("page", "reactive");

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

    RENDERS.store(0, Ordering::Relaxed);
    let t0 = std::time::Instant::now();
    blinc_runtime::fsm::dispatch_default("Play", "Grow").expect("Grow dispatches");
    let dispatch = t0.elapsed();
    let renders_after_dispatch = RENDERS.load(Ordering::Relaxed);

    let t1 = std::time::Instant::now();
    let applied = tree.process_pending_subtree_rebuilds();
    let rebuild = t1.elapsed();
    let renders_total = RENDERS.load(Ordering::Relaxed);

    println!(
        "GROW dispatch={dispatch:?} rebuild={rebuild:?} applied={applied} \
         re-renders: {renders_after_dispatch} during dispatch, {renders_total} total"
    );
    assert_eq!(
        renders_total, 0,
        "Grow drives bound props only; re-rendering the whole program for it \
         costs {dispatch:?} + {rebuild:?}"
    );
}

/// The other half of the contract: `Busy` flips a field the body reads
/// as a value (`if Play.busy.get()`), and a branch swap is structural,
/// so it MUST still re-render.
#[test]
fn busy_still_re_renders() {
    init();
    let root_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = BlincDsl::new().unwrap();
    blinc_cn_dsl::register_all(&dsl).unwrap();
    dsl.compile_project(&root_dir.join("main.blinc"), &root_dir)
        .unwrap();
    // The rail starts on the forms page; everything measured here
    // lives on the reactive one, so navigate before building or the
    // assertions run against a page that was never mounted.
    dsl.set_signal_string("page", "reactive");
    let host = div().w(720.0).h(820.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(720.0, 820.0);
    tree.process_pending_subtree_rebuilds();

    blinc_runtime::fsm::dispatch_default("Play", "Reset").expect("Reset dispatches");
    tree.process_pending_subtree_rebuilds();
    RENDERS.store(0, Ordering::Relaxed);

    blinc_runtime::fsm::dispatch_default("Play", "Busy").expect("Busy dispatches");
    let renders = RENDERS.load(Ordering::Relaxed);
    println!("BUSY re-renders: {renders}");
    assert!(
        renders >= 1,
        "Busy swaps a branch; without a re-render the panel never updates"
    );
    assert!(
        renders <= 1,
        "Busy writes busy AND caption, but only `busy` is read as a value: \
         {renders} re-renders means the caption is in the deps too"
    );
}
