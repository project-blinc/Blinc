//! A cn Button must size to its CURRENT label.
//!
//! Reported symptom: the container is one update behind the text --
//! "Save" -> "Saving..." keeps the "Save" width and clips, then back to
//! "Save" keeps the wider box.
//!
//! These are CHARACTERISATION tests: every one of them passes, which is
//! the point. They pin down where the fault is NOT, so the search does
//! not repeat itself:
//!
//!   * measurement is sound -- a longer label measures wider
//!   * the tree hash already distinguishes two labels, so the renderer
//!     is not reading the tree as unchanged
//!   * successive renders sharing an ElementRegistry, the shape the
//!     windowed host uses, track the current label correctly
//!
//! What they do NOT cover, and where the fault therefore lives: the
//! in-place Stateful refresh. In the app the label changes through a
//! `deps()` notification, which refreshes the existing Stateful rather
//! than rebuilding the tree from scratch as these tests do. Reproducing
//! that needs a driven refresh, not a fresh `RenderTree`.

use blinc_cn::button;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// The pending-subtree-rebuild queue and the stateful-deps registry are
/// process-global. One test's `Signal::set` therefore queues work that
/// another test's `process_pending_subtree_rebuilds` can drain, so the
/// refresh-driven tests take turns.
fn rebuild_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        let _ = tracing_subscriber::fmt::try_init();
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(AtomicBool::new(false)),
            );
        }
    });
}

/// Width of a freshly built button carrying `label`.
fn button_width(label: &str) -> f32 {
    let host = div().w(800.0).h(200.0).child(button(label));
    let mut tree = RenderTree::from_element(&host);
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(800.0, 200.0);
    let root = tree.root().expect("root");
    let btn = tree.layout_tree.children(root)[0];
    tree.layout_tree
        .get_layout(btn)
        .map(|l| l.size.width)
        .unwrap_or(0.0)
}

#[test]
fn longer_label_yields_a_wider_button() {
    init();
    let short = button_width("Save");
    let long = button_width("Saving...");
    println!("WIDTHS short={short} long={long}");
    assert!(
        long > short,
        "a longer label must produce a wider button: Save={short}, Saving...={long}"
    );
}

/// A Button's builder tree must expose its content.
///
/// `Stateful::children_builders()` returns `&[]` ("children handled via
/// build()"), and cn Button puts its label inside the Stateful's
/// `on_state`. Anything that walks the BUILDER tree -- diffing,
/// hashing, content measurement -- therefore sees a childless box,
/// while `build()` produces the real subtree.
#[test]
fn button_builder_tree_exposes_its_label() {
    use blinc_layout::div::ElementBuilder;
    init();
    let b = button("Saving...");
    fn count(e: &dyn ElementBuilder) -> usize {
        1 + e
            .children_builders()
            .iter()
            .map(|c| count(c.as_ref()))
            .sum::<usize>()
    }
    let before_build = count(&b);
    // Build it, then ask again: distinguishes "callback never runs" from
    // "callback only runs during build".
    let mut t = blinc_layout::tree::LayoutTree::new();
    let _ = b.build(&mut t);
    let after_build = count(&b);
    println!("BUILDER before={before_build} after={after_build}");
    let builder_nodes = after_build;

    let host = div().w(800.0).h(200.0).child(button("Saving..."));
    let mut tree = blinc_layout::renderer::RenderTree::from_element(&host);
    tree.compute_layout(800.0, 200.0);
    let root = tree.root().expect("root");
    let mut layout_nodes = 0;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        layout_nodes += 1;
        stack.extend(tree.layout_tree.children(id));
    }

    println!("BUILDER nodes={builder_nodes}  LAYOUT nodes={layout_nodes}");
    assert!(
        builder_nodes > 1,
        "the button's builder tree reports {builder_nodes} node(s); its content is invisible \
         to any builder-tree walk while build() produces {layout_nodes} layout nodes"
    );
}

/// The tree hash must distinguish two buttons whose only difference is
/// the label.
///
/// A `Stateful` populates its children during `build()`, so hashing the
/// element tree beforehand walked a childless box and never reached the
/// label. Two different labels hashed identically, the tree read as
/// unchanged, and the previous layout carried over -- the button stayed
/// one generation behind its text.
#[test]
fn tree_hash_distinguishes_button_labels() {
    init();
    let short = div().w(800.0).h(200.0).child(button("Save"));
    let long = div().w(800.0).h(200.0).child(button("Saving..."));
    let a = blinc_layout::renderer::RenderTree::from_element(&short);
    let b = blinc_layout::renderer::RenderTree::from_element(&long);
    assert_ne!(
        a.tree_hash(),
        b.tree_hash(),
        "labels differ, so the tree hash must differ"
    );
}

/// Two successive renders sharing an `ElementRegistry`, as the windowed
/// host does: the second render's width must reflect the second label,
/// not the first.
#[test]
fn successive_renders_track_the_current_label() {
    let _guard = rebuild_lock();
    use blinc_layout::selector::ElementRegistry;
    use std::sync::Arc as StdArc;
    init();

    let registry = StdArc::new(ElementRegistry::default());

    let mut width_for = |label: &str| {
        let host = div().w(800.0).h(200.0).child(button(label));
        let mut tree = blinc_layout::renderer::RenderTree::from_element_with_registry(
            &host,
            StdArc::clone(&registry),
        );
        tree.apply_stylesheet_layout_overrides();
        tree.compute_layout(800.0, 200.0);
        let root = tree.root().expect("root");
        let btn = tree.layout_tree.children(root)[0];
        tree.layout_tree
            .get_layout(btn)
            .map(|l| l.size.width)
            .unwrap_or(0.0)
    };

    let first = width_for("Save");
    let second = width_for("Saving...");
    let third = width_for("Save");
    println!("SUCCESSIVE first={first} second={second} third={third}");

    assert!(
        second > first,
        "after the label grows, the button must widen: {first} -> {second}"
    );
    assert!(
        (third - first).abs() < 1.0,
        "back to the short label, the width must return: {first} -> {second} -> {third}"
    );
}

/// Drive the real path: a bound label changed through the reactive
/// graph, refreshing the existing Stateful in place.
///
/// This is the shape the app uses and the one the tests above do not
/// cover. `set()` notifies the stateful deps, the Stateful refreshes
/// its subtree without the tree being rebuilt, and the next layout pass
/// must reflect the new label.
#[test]
fn bound_label_resizes_on_signal_set() {
    let _guard = rebuild_lock();
    use blinc_core::reactive::State;
    init();

    // Wire the stateful-deps notifier the way the windowed / web hosts
    // do; without it `Signal::set` notifies nobody and no refresh runs.
    static NOTIFIER: std::sync::Once = std::sync::Once::new();
    NOTIFIER.call_once(|| {
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
    });

    let label = State::new(
        blinc_core::reactive::signal::<String>("Save".to_string()),
        blinc_core::reactive::global_graph(),
        blinc_core::reactive::global_dirty_flag(),
    );

    let host = div().w(800.0).h(200.0).child(button(&label));
    let mut tree = blinc_layout::renderer::RenderTree::from_element(&host);
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(800.0, 200.0);

    let root = tree.root().expect("root");
    let btn = tree.layout_tree.children(root)[0];
    let width = |t: &blinc_layout::renderer::RenderTree| {
        t.layout_tree
            .get_layout(btn)
            .map(|l| l.size.width)
            .unwrap_or(0.0)
    };

    let before = width(&tree);

    // Write through the Signal, as the DSL host does. `State::set`
    // notifies statefuls only via a per-instance callback that
    // `State::new` does not install; `Signal::set` fires the global
    // stateful-deps notifier.
    blinc_core::reactive::Signal::<String>::from_id(label.signal_id()).set("Saving...".to_string());

    // Next frame: the refresh queues a subtree rebuild, the host
    // applies it, then layout runs again on the SAME tree.
    let applied = tree.process_pending_subtree_rebuilds();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(800.0, 200.0);
    let after = width(&tree);
    println!("APPLIED rebuilds={applied}");

    println!("BOUND before={before} after={after}");
    assert!(
        after > before,
        "a bound label that grew must widen its button in the next layout \
         pass: {before} -> {after}"
    );
}

/// The shrink direction. A `w_fit` container that grows but never
/// shrinks looks identical to "not resizing" for a label that starts
/// long: cn_demo's trigger opens as "Custom Trigger" (140px) and
/// switches to "Close Menu" (109px).
#[test]
fn bound_label_shrinks_on_signal_set() {
    let _guard = rebuild_lock();
    use blinc_core::reactive::State;
    init();

    static NOTIFIER: std::sync::Once = std::sync::Once::new();
    NOTIFIER.call_once(|| {
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
    });

    let label = State::new(
        blinc_core::reactive::signal::<String>("Custom Trigger".to_string()),
        blinc_core::reactive::global_graph(),
        blinc_core::reactive::global_dirty_flag(),
    );

    let host = div().w(800.0).h(200.0).child(button(&label));
    let mut tree = blinc_layout::renderer::RenderTree::from_element(&host);
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(800.0, 200.0);

    let root = tree.root().expect("root");
    let btn = tree.layout_tree.children(root)[0];
    let width = |t: &blinc_layout::renderer::RenderTree| {
        t.layout_tree
            .get_layout(btn)
            .map(|l| l.size.width)
            .unwrap_or(0.0)
    };

    let long = width(&tree);
    blinc_core::reactive::Signal::<String>::from_id(label.signal_id())
        .set("Close Menu".to_string());
    tree.process_pending_subtree_rebuilds();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(800.0, 200.0);
    let short = width(&tree);

    println!("SHRINK long={long} short={short}");
    assert!(
        short < long,
        "a shorter label must narrow the button: {long} -> {short}"
    );
}

/// The cn_demo custom-trigger shape: a parent Stateful whose callback
/// builds a FRESH button whose label depends on the parent's state.
///
/// Nothing is bound -- each refresh constructs a new `cn::button` with a
/// different `&str`, so the fix for bound labels does not apply here.
#[test]
fn parent_stateful_swapping_button_labels_resizes() {
    let _guard = rebuild_lock();
    use blinc_layout::stateful::{ButtonState, stateful};
    init();

    static NOTIFIER: std::sync::Once = std::sync::Once::new();
    NOTIFIER.call_once(|| {
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
    });

    let open = blinc_core::reactive::signal::<i32>(0);
    let host = div()
        .w(800.0)
        .h(200.0)
        .child(
            stateful::<ButtonState>()
                .deps([open.id()])
                .on_state(move |_ctx| {
                    let is_open = open.try_get().unwrap_or(0) != 0;
                    let label = if is_open {
                        "Close Menu"
                    } else {
                        "Custom Trigger"
                    };
                    div().w_fit().child(button(label))
                }),
        );

    let mut tree = blinc_layout::renderer::RenderTree::from_element(&host);
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(800.0, 200.0);

    let root = tree.root().expect("root");
    let outer = tree.layout_tree.children(root)[0];
    let width = |t: &blinc_layout::renderer::RenderTree| {
        t.layout_tree
            .get_layout(outer)
            .map(|l| l.size.width)
            .unwrap_or(0.0)
    };

    let closed = width(&tree);
    blinc_core::reactive::Signal::<i32>::from_id(open.id()).set(1);
    tree.process_pending_subtree_rebuilds();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(800.0, 200.0);
    let opened = width(&tree);

    println!("TRIGGER closed={closed} opened={opened}");
    assert!(
        opened < closed,
        "the trigger must narrow when its label shortens: {closed} -> {opened}"
    );
}
