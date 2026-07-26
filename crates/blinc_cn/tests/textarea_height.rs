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
