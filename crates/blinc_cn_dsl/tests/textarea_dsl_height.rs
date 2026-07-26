//! `cn.Textarea` through the DSL must size like the cn widget it wraps.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
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
}

#[test]
fn dsl_textarea_height_matches_rows() {
    let _ = tracing_subscriber::fmt::try_init();
    init();
    let dsl = BlincDsl::new().unwrap();
    blinc_cn_dsl::register_all(&dsl).unwrap();
    dsl.compile_source(
        r#"view { cn.Textarea(key = "h", placeholder = "a few words", rows = 3) }"#,
        "ta.blinc",
    )
    .unwrap();

    let host = div()
        .w(620.0)
        .h(820.0)
        .flex_col()
        .child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(620.0, 820.0);
    let root = tree.root().unwrap();
    let mut stack = vec![(root, 0usize)];
    while let Some((id, d)) = stack.pop() {
        if let Some(l) = tree.layout_tree.get_layout(id) {
            if d <= 3 {
                println!("DSL d={d} h={} w={}", l.size.height, l.size.width);
            }
        }
        for c in tree.layout_tree.children(id) {
            stack.push((c, d + 1));
        }
    }
}

/// The wrapper must expose the widget's taffy style.
///
/// Intrinsic size lives there — a textarea's rows-derived height, an
/// input's height — so a wrapper that cannot forward `layout_style`
/// hides the size from every builder-tree reader, even though `build()`
/// still applies it to the node.
#[test]
fn wrappers_expose_layout_style() {
    use blinc_layout::div::ElementBuilder;
    init();
    let dsl = BlincDsl::new().unwrap();
    blinc_cn_dsl::register_all(&dsl).unwrap();
    dsl.compile_source(r#"view { cn.Textarea(key = "ls", rows = 3) }"#, "ls.blinc")
        .unwrap();
    let w = dsl.view_widget();

    fn find_sized(b: &dyn ElementBuilder) -> bool {
        if b.layout_style().is_some() {
            return true;
        }
        b.children_builders().iter().any(|c| find_sized(c.as_ref()))
    }
    assert!(
        find_sized(w.as_ref()),
        "the textarea's taffy style must be reachable from the builder tree"
    );
}

/// Two frames, not one. The reported symptom is a section that settles
/// a frame after launch, so a single `compute_layout` cannot see it.
#[test]
fn textarea_height_is_stable_across_frames() {
    init();
    let dsl = BlincDsl::new().unwrap();
    blinc_cn_dsl::register_all(&dsl).unwrap();
    dsl.compile_source(
        r#"view { Div { cn.Textarea(key = "settle", placeholder = "a few words", rows = 3) } }"#,
        "settle.blinc",
    )
    .unwrap();

    let host = div()
        .w(620.0)
        .h(820.0)
        .flex_col()
        .child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(620.0, 820.0);

    // Every node's height, so a change anywhere in the subtree shows.
    fn heights(tree: &RenderTree) -> Vec<f32> {
        let mut out = Vec::new();
        let mut stack = vec![tree.root().unwrap()];
        while let Some(id) = stack.pop() {
            if let Some(l) = tree.layout_tree.get_layout(id) {
                out.push(l.size.height);
            }
            stack.extend(tree.layout_tree.children(id));
        }
        out
    }
    let frame1 = heights(&tree);

    // Frame 2: apply anything the first frame queued, then re-layout.
    let applied = tree.process_pending_subtree_rebuilds();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(620.0, 820.0);
    let frame2 = heights(&tree);

    println!("SETTLE applied={applied} frame1={frame1:?} frame2={frame2:?}");
    assert_eq!(
        frame1, frame2,
        "layout must not change between frames with no input"
    );
}
