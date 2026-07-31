//! The playground's layout must not change between frames with no input.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
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
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(AtomicBool::new(false)),
            );
        }
    });
    blinc_core::BlincContextState::get().set_viewport_size(720.0, 820.0);
}

/// Top edge of every section, which is what visibly jumps when a widget
/// settles a frame late.
fn section_tops(tree: &RenderTree) -> Vec<f32> {
    let root = tree.root().unwrap();
    let page = tree.layout_tree.children(root)[0];
    let page = tree.layout_tree.children(page)[0];
    tree.layout_tree
        .children(page)
        .iter()
        .filter_map(|c| tree.layout_tree.get_layout(*c))
        .map(|l| l.location.y)
        .collect()
}

/// A widget that measures itself and rebuilds must land on the layout it
/// already had.
///
/// The textarea used to write its measured width back as a fixed style
/// width. That value equals the parent's available width by
/// construction, and a child exactly filling its parent makes taffy size
/// the ancestor by min-content: the enclosing section jumped 519 -> 929
/// on the second frame and every section below it shifted down, visibly,
/// during launch.
#[test]
fn playground_layout_is_stable_across_frames() {
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
    let frame1 = section_tops(&tree);

    // A first frame that queues its own rebuild is expected: widgets
    // learn their measured size here. Applying it must not move anything.
    tree.process_pending_subtree_rebuilds();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(720.0, 820.0);
    let frame2 = section_tops(&tree);

    assert_eq!(
        frame1, frame2,
        "sections must not move between frames with no input"
    );
}
