//! Diagnostic: what keeps the DSL playground's frame loop alive at idle?
//!
//! cn_demo (native, same widgets) floors at 0% CPU; the DSL playground
//! sits at ~40% with nothing on screen moving. Ignored by default.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
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

fn report(label: &str) {
    let snap = blinc_layout::stateful::animating_statefuls_snapshot();
    let unresolved = snap.iter().filter(|(_, id)| id.is_none()).count();
    // The three things that keep a window from parking: an armed
    // redraw, a queued rebuild, or a stateful claiming to animate.
    println!(
        "  {label:<22} anim-registered={:<3} unresolved={unresolved} \
         needs-redraw={:<5} pending-rebuilds={}",
        snap.len(),
        blinc_layout::peek_needs_redraw(),
        blinc_layout::stateful::has_pending_subtree_rebuilds(),
    );
    println!(
        "  {:<24} dep-registry entries={}",
        "",
        blinc_layout::stateful::stateful_deps_registered()
    );
}

#[test]
#[ignore = "diagnostic: prints registry contents, asserts nothing"]
fn what_is_registered_as_animating_at_idle() {
    init();
    let root_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = BlincDsl::new().unwrap();
    blinc_cn_dsl::register_all(&dsl).unwrap();
    dsl.compile_project(&root_dir.join("main.blinc"), &root_dir)
        .unwrap();
    let css: String = blinc_core::BlincContextState::get()
        .drain_stylesheets()
        .join("\n");

    report("before mount");
    let host = div().w(720.0).h(820.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(blinc_layout::css_parser::Stylesheet::parse(&css).unwrap());
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(720.0, 820.0);
    report("after first layout");
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(720.0, 820.0);
    report("after settle");

    // Simulate idle frames: drain and lay out repeatedly with no input
    // and no writes. Anything that re-arms itself here is what pins the
    // loop in a real window.
    for i in 0..5 {
        let _ = blinc_layout::take_needs_redraw();
        std::thread::sleep(std::time::Duration::from_millis(40));
        tree.process_pending_subtree_rebuilds();
        tree.compute_layout(720.0, 820.0);
        report(&format!("idle frame {i}"));
    }
}

/// The host's UI builder calls `view_widget()` on every invocation,
/// which re-runs the whole JIT program and mounts fresh `Stateful`s.
/// Their registry keys are `Arc::as_ptr`, so each rebuild registers
/// under a new key. If the old entries are never reclaimed, every
/// rebuild permanently adds work to every future signal write.
#[test]
#[ignore = "diagnostic"]
fn does_rebuilding_the_view_leak_stateful_registrations() {
    init();
    let root_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = BlincDsl::new().unwrap();
    blinc_cn_dsl::register_all(&dsl).unwrap();
    dsl.compile_project(&root_dir.join("main.blinc"), &root_dir)
        .unwrap();
    let _ = blinc_core::BlincContextState::get().drain_stylesheets();

    for i in 0..6 {
        let host = div().w(720.0).h(820.0).child_box(dsl.view_widget());
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(720.0, 820.0);
        tree.process_pending_subtree_rebuilds();
        tree.compute_layout(720.0, 820.0);
        println!(
            "  builder run {i}: dep-registry={} anim-registry={}",
            blinc_layout::stateful::stateful_deps_registered(),
            blinc_layout::stateful::animating_statefuls_snapshot().len()
        );
    }
}

/// Do DSL-built nodes resolve absolute bounds?
///
/// Paint's viewport gate records a node as painted when its bounds do
/// NOT resolve — it fails open. A node that never resolves is therefore
/// permanently "on screen", and any animation on it pins the redraw
/// chain no matter where the user scrolled. This needs layout, not a
/// GPU, so it is answerable here.
#[test]
#[ignore = "diagnostic"]
fn do_dsl_nodes_resolve_absolute_bounds() {
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

    let root = tree.root().expect("root");
    let mut stack = vec![root];
    let (mut total, mut unresolved, mut zero_sized) = (0usize, 0usize, 0usize);
    let mut below_fold = 0usize;
    while let Some(id) = stack.pop() {
        total += 1;
        match tree.layout_tree.get_absolute_bounds(id) {
            None => unresolved += 1,
            Some(b) => {
                if b.width <= 0.0 || b.height <= 0.0 {
                    zero_sized += 1;
                }
                // The page is 820 tall; anything starting past it is
                // off-screen at rest and should be culled.
                if b.y > 820.0 {
                    below_fold += 1;
                }
            }
        }
        stack.extend(tree.layout_tree.children(id));
    }
    println!(
        "nodes={total} unresolved-bounds={unresolved} zero-sized={zero_sized} \
         below-fold={below_fold}"
    );
}

/// Does an animation tick wake a rebuild that re-mounts statefuls?
///
/// The hypothesis: the spinner's endless timeline ticks, a Stateful
/// registered for animation refresh re-runs its callback, the DSL
/// callback re-renders through the JIT and mounts FRESH Statefuls whose
/// registry keys are `Arc::as_ptr` — so every tick permanently grows the
/// dep registry and each later frame walks a bigger one. That is what
/// CPU climbing rather than sitting flat looks like.
///
/// Ticking `RenderState` is what a frame does with no input, so if the
/// registry grows here it grows in the window.
#[test]
#[ignore = "diagnostic"]
fn does_ticking_animations_grow_the_registry() {
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

    println!(
        "  after mount:      dep-registry={} anim-statefuls={}",
        blinc_layout::stateful::stateful_deps_registered(),
        blinc_layout::stateful::animating_statefuls_snapshot().len()
    );

    // Simulate idle frames at ~60Hz: tick animations, drain rebuilds,
    // lay out. No input, no signal writes.
    for round in 0..5 {
        for _ in 0..60 {
            blinc_layout::check_stateful_animations();
            tree.process_pending_subtree_rebuilds();
        }
        tree.compute_layout(720.0, 820.0);
        println!(
            "  after {:>3} frames: dep-registry={} anim-statefuls={}",
            (round + 1) * 60,
            blinc_layout::stateful::stateful_deps_registered(),
            blinc_layout::stateful::animating_statefuls_snapshot().len()
        );
    }
}
