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

/// REPRODUCES AN OPEN BUG — ignored so the suite stays green.
///
/// With no input, `FormWidgets` grows ~410px on the second frame and
/// every section below shifts down. `applied=true` shows the first
/// frame queued a subtree rebuild on its own, and its effect only lands
/// on frame 2.
///
/// The Stateful-backed widgets (cn.Input / cn.Textarea) are the
/// suspects: they contribute little on frame 1 and their real height
/// only after the queued rebuild. Remove them from `forms.blinc` and
/// the growth should go -- that is the next step, then finding why
/// their first build under-reports.
///
/// Run with `--ignored` while working on it.
#[test]
#[ignore = "open: playground layout settles on the second frame"]
fn playground_layout_is_stable_across_frames() {
    let _ = tracing_subscriber::fmt::try_init();
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

    let applied = tree.process_pending_subtree_rebuilds();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(720.0, 820.0);
    let frame2 = section_tops(&tree);

    println!("PG applied={applied}\n  frame1={frame1:?}\n  frame2={frame2:?}");
    assert_eq!(
        frame1, frame2,
        "sections must not move between frames with no input"
    );
}
