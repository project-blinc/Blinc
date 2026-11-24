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

/// Widest node anywhere under `root`.
fn widest(tree: &RenderTree, root: blinc_layout::tree::LayoutNodeId) -> f32 {
    let mut max = 0.0f32;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(l) = tree.layout_tree.get_layout(id) {
            max = max.max(l.size.width);
        }
        stack.extend(tree.layout_tree.children(id));
    }
    max
}

/// Without an explicit width the textarea fills its parent on the FIRST
/// frame, rather than building at the 300px default and adopting the
/// measured width a frame later.
///
/// The late adoption used to emit a fixed style width exactly equal to
/// the parent's available width, which makes taffy size ancestors by
/// min-content. See `pg_settle` in blinc_cn_dsl.
#[test]
fn textarea_fills_its_parent_on_the_first_frame() {
    init();
    let state = blinc_layout::widgets::text_area::text_area_state();
    let host = div()
        .w(620.0)
        .h(820.0)
        .flex_col()
        .child(blinc_cn::textarea(&state).rows(3).build_component());
    let mut tree = RenderTree::from_element(&host);
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(620.0, 820.0);
    let root = tree.root().unwrap();
    let first = widest(&tree, root);
    assert!(
        first >= 619.0,
        "textarea should span its 620px parent on frame 1, widest node is {first}"
    );

    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(620.0, 820.0);
    assert_eq!(
        first,
        widest(&tree, root),
        "the measured-width rebuild must not change the layout"
    );
}

/// An explicit width still pins the box.
#[test]
fn textarea_honours_an_explicit_width() {
    init();
    let state = blinc_layout::widgets::text_area::text_area_state();
    let host = div().w(620.0).h(820.0).flex_col().child(
        blinc_cn::textarea(&state)
            .rows(3)
            .w(240.0)
            .build_component(),
    );
    let mut tree = RenderTree::from_element(&host);
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(620.0, 820.0);
    let root = tree.root().unwrap();
    let ta = tree.layout_tree.children(root)[0];
    let w = tree.layout_tree.get_layout(ta).unwrap().size.width;
    assert!((w - 240.0).abs() < 1.0, "expected a 240px box, got {w}");
}

#[test]
fn textarea_height_matches_rows() {
    init();
    let state = blinc_layout::widgets::text_area::text_area_state();
    let host = div().w(620.0).h(820.0).flex_col().child(
        blinc_cn::textarea(&state)
            .rows(3)
            .placeholder("a few words")
            .build_component(),
    );
    let mut tree = RenderTree::from_element(&host);
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(620.0, 820.0);
    let root = tree.root().unwrap();
    let ta = tree.layout_tree.children(root)[0];
    let l = tree.layout_tree.get_layout(ta).unwrap();
    println!("TEXTAREA h={} w={}", l.size.height, l.size.width);
    assert!(
        l.size.height < 200.0,
        "a 3-row textarea must not reserve {}px",
        l.size.height
    );
}
