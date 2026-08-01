//! Diagnostic: why a repeated dispatch settles slower than an
//! alternating one.
//!
//! Ignored by default — it prints timings and proves nothing on its
//! own. Run with `--ignored --nocapture` when picking this up.
//!
//! What it established, on both sides of the observed-deps branch (so
//! this is pre-existing, not caused by it):
//!
//! - The extra time is entirely in `process_pending_subtree_rebuilds`,
//!   not in `compute_layout`. Rebuild goes ~740us -> ~1.25ms while
//!   layout stays flat at ~65-110us.
//! - It is a STEP, not accumulation: the first dispatch is fast, every
//!   repeat after it is slow, and it never climbs further. An
//!   alternating Busy/Reset sequence never steps up at all.
//! - Not the drain loop's pass cap, and not stale-entry drops. Both
//!   read zero.
//! - Both cases end up performing FOUR real rebuilds. The alternating
//!   case queues seven and drops three as superseded; the repeat case
//!   queues four and drops none.
//!
//! The hypothesis that fits: each rebuild pays a fixed per-subtree cost
//! (its own `apply_stylesheet_base_styles_for_subtree` among others),
//! so four disjoint fine-grained rebuilds cost more than one coarse
//! rebuild covering the same nodes. The alternating case gets the
//! supersede dedup and the repeat case does not, so the repeat case
//! pays that fixed cost four times.
//!
//! NOT yet confirmed: which nodes the four disjoint rebuilds target,
//! and why an ancestor is queued in one case but not the other. That is
//! where to pick this up.

use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

static PROCESSED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static DRAINS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static STALE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static CAPTURE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Counts the drain loop giving up after MAX_PASSES — the signal that a
/// rebuild is queuing more rebuilds as it mounts.
struct CountDeferred;
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CountDeferred {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        struct Find(bool);
        impl tracing::field::Visit for Find {
            fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
                if f.name() == "message" {
                    let m = format!("{v:?}");
                    if CAPTURE.load(std::sync::atomic::Ordering::Relaxed) {
                        LINES
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push(m.clone());
                    }
                    if let Some(rest) = m.strip_prefix("Processing ") {
                        if let Some(n) = rest.split_whitespace().next() {
                            if let Ok(n) = n.parse::<usize>() {
                                PROCESSED.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
                                DRAINS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                    }
                    if m.contains("Dropped") && m.contains("stale") {
                        STALE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        }
        let mut find = Find(false);
        event.record(&mut find);
        let _ = find.0;
    }
}

fn init() {
    use tracing_subscriber::prelude::*;
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        tracing_subscriber::registry()
            .with(CountDeferred)
            .with(tracing_subscriber::filter::LevelFilter::DEBUG)
            .init();
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

/// The playground itself, with the settle split in two. The question
/// is whether a repeat dispatch spends its extra time queuing/applying
/// subtree rebuilds or in the layout pass afterwards.
fn playground_split(label: &str, actions: &[&str]) {
    init();
    let root_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
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

    println!("--- {label}");
    for (i, action) in actions.iter().enumerate() {
        let capturing = i == 1;
        if capturing {
            LINES.lock().unwrap_or_else(|e| e.into_inner()).clear();
            CAPTURE.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        blinc_runtime::fsm::dispatch_default("Play", action);
        PROCESSED.store(0, std::sync::atomic::Ordering::Relaxed);
        DRAINS.store(0, std::sync::atomic::Ordering::Relaxed);
        STALE.store(0, std::sync::atomic::Ordering::Relaxed);
        let t0 = std::time::Instant::now();
        tree.process_pending_subtree_rebuilds();
        let rebuild = t0.elapsed();
        let t1 = std::time::Instant::now();
        tree.compute_layout(720.0, 820.0);
        let layout = t1.elapsed();
        if capturing {
            CAPTURE.store(false, std::sync::atomic::Ordering::Relaxed);
            for l in LINES.lock().unwrap_or_else(|e| e.into_inner()).iter() {
                if l.contains("subtree") || l.contains("Rebuil") || l.contains("Processing") {
                    println!("      | {l}");
                }
            }
        }
        println!(
            "  {action:>6}: rebuild {rebuild:>9.1?}  layout {layout:>9.1?}  \
             rebuilds={} passes={} stale-drops={}",
            PROCESSED.load(std::sync::atomic::Ordering::Relaxed),
            DRAINS.load(std::sync::atomic::Ordering::Relaxed),
            STALE.load(std::sync::atomic::Ordering::Relaxed)
        );
    }
}

#[test]
#[ignore = "diagnostic: prints timings, asserts nothing"]
fn where_does_the_repeat_dispatch_time_go() {
    playground_split("Busy x6", &["Busy"; 6]);
    playground_split(
        "Busy/Reset x6",
        &["Busy", "Reset", "Busy", "Reset", "Busy", "Reset"],
    );
}
