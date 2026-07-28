//! After a reload, a changed literal must MEASURE as its new text.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::io::Write;
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

fn widest_leaf(tree: &RenderTree) -> f32 {
    let mut max = 0.0f32;
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        let kids = tree.layout_tree.children(id);
        if kids.is_empty()
            && let Some(l) = tree.layout_tree.get_layout(id)
        {
            max = max.max(l.size.width);
        }
        stack.extend(kids);
    }
    max
}

fn measure(dsl: &BlincDsl) -> f32 {
    let host = div().w(600.0).h(200.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 200.0);
    widest_leaf(&tree)
}

#[test]
fn a_reloaded_literal_measures_as_its_new_text() {
    init();
    let dir = std::env::temp_dir().join("blinc_reload_text");
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("main.blinc");
    let part = dir.join("part.blinc");
    std::fs::File::create(&entry)
        .unwrap()
        .write_all(b"import { Part } from \"./part\"\nview { Div { Part() } }")
        .unwrap();

    let write_part = |label: &str| {
        std::fs::File::create(&part)
            .unwrap()
            .write_all(
                format!(
                    r#"component Part {{ view {{ Div {{ cn.Badge("{label}", variant = "secondary") }} }} }}"#
                )
                .as_bytes(),
            )
            .unwrap();
    };

    write_part("nested");
    let a = BlincDsl::reload_project(&entry, &dir, |d| blinc_cn_dsl::register_all(d)).unwrap();
    let short = measure(&a);

    write_part("nested content that is much longer");
    let b = BlincDsl::reload_project(&entry, &dir, |d| blinc_cn_dsl::register_all(d)).unwrap();
    let long = measure(&b);

    println!("RELOADED TEXT widths: {short} -> {long}");
    assert!(
        long > short * 1.5,
        "a longer label must measure wider after a reload: {short} -> {long}"
    );
}
