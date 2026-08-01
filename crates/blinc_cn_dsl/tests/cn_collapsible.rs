//! `cn.Collapsible` — does the section fold when the bound signal
//! changes?
//!
//! Measures the rendered height, since every earlier attempt built and
//! reported correctly while never moving.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::{Arc, Mutex};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// Height of the widget's own box — the child of the page div, not the
/// host, which would report its fixed size either way.
fn fold_height(dsl: &BlincDsl) -> f32 {
    let host = div().w(400.0).h(400.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 400.0);
    let root = tree.root().expect("root");
    let view_root = *tree.layout_tree.children(root).first().expect("view root");
    let page = *tree.layout_tree.children(view_root).first().expect("page");
    let fold = *tree
        .layout_tree
        .children(page)
        .first()
        .expect("collapsible");
    tree.layout_tree
        .get_layout(fold)
        .expect("laid out")
        .size
        .height
}

#[test]
fn toggling_the_signal_changes_the_rendered_height() {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(
        r#"signal col_fold: bool = false
           view { Div(class="p") { cn.Collapsible(open = col_fold) { cn.Label("body text here") } } }"#,
        "col_fold.blinc",
    )
    .expect("compile");

    let shut = fold_height(&dsl);
    blinc_runtime::signal::set_bool("col_fold", true);
    let open = fold_height(&dsl);

    assert!(
        open > shut,
        "opening must make the section taller: shut={shut}, open={open}"
    );
}
