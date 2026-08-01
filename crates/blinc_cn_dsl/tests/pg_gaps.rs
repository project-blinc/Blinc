//! A bound SIZE prop repaints but does not relayout.
//!
//! `cn.Skeleton(w = …, h = …)` binds through the property channel: a
//! write patches render props and asks for a repaint, which is right for
//! a colour or an opacity and wrong for a dimension. The widget paints
//! at its new size while its layout box keeps the old one, so it
//! overlaps whatever follows and the surrounding rows never move --
//! visible as uneven vertical gaps that no CSS explains.
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

#[test]
#[ignore = "open: a bound size prop repaints without relayout"]
fn a_bound_size_prop_moves_the_layout_box() {
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

    // Grow five times: bar_w/bar_h/radius are bound props, written in
    // place. Does the LAYOUT follow?
    for _ in 0..5 {
        // `reactive$Play`, not `Play`: an imported module compiles under a
        // namespace derived from its path, and only the entry is unnamespaced.
        blinc_runtime::fsm::dispatch_default("reactive$Play", "Grow").expect("Grow");
    }
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(720.0, 820.0);
    println!(
        "after 5x Grow: bar_h={:?} bar_w={:?}",
        blinc_runtime::signal::get_f64("__fsm_ctx_Play_bar_h"),
        blinc_runtime::signal::get_f64("__fsm_ctx_Play_bar_w"),
    );

    let root = tree.root().unwrap();
    let skeleton_row = {
        let page = tree.layout_tree.children(root)[0];
        let page = tree.layout_tree.children(page)[0];
        tree.layout_tree.children(page)[6]
    };
    let h = tree
        .layout_tree
        .get_layout(skeleton_row)
        .unwrap()
        .size
        .height;
    assert!(
        h > 95.0,
        "bar_h grew from 90 to 100, so the row holding the skeleton must \
         follow it; still {h}"
    );

    let page = tree.layout_tree.children(root)[0];
    let page = tree.layout_tree.children(page)[0];
    for (i, c) in tree.layout_tree.children(page).into_iter().enumerate() {
        let l = tree.layout_tree.get_layout(c).unwrap();
        let kids = tree.layout_tree.children(c);
        let tallest = kids
            .iter()
            .filter_map(|k| tree.layout_tree.get_layout(*k))
            .map(|k| k.size.height)
            .fold(0.0f32, f32::max);
        let slack = l.size.height - tallest;
        let flag = if slack > 4.0 && !kids.is_empty() {
            "  <<< slack"
        } else {
            ""
        };
        println!(
            "row {i:>2}: y={:>6.1} h={:>6.1} kids={} tallest_kid={tallest:>6.1}{flag}",
            l.location.y,
            l.size.height,
            kids.len()
        );
    }
}
