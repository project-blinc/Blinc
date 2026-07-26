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
