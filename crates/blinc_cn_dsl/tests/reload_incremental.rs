//! A reload reaches the window through `incremental_update`, not a
//! fresh `from_element`. That is the path that has to pick up a changed
//! literal: the windowed app keeps one `RenderTree` for the lifetime of
//! the window and diffs each new element tree into it.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
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

/// Every text node's content plus the width of the box it sits in.
fn texts(tree: &RenderTree) -> Vec<(String, f32, f32)> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            let w = tree
                .layout_tree
                .get_layout(id)
                .map(|l| l.size.width)
                .unwrap_or(0.0);
            out.push((t.content.clone(), w, t.measured_width));
        }
        stack.extend(tree.layout_tree.children(id));
    }
    out
}

#[test]
fn an_incremental_reload_updates_both_the_box_and_the_glyphs() {
    init();
    let dir = std::env::temp_dir().join("blinc_reload_incremental");
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
                    r#"component Part {{ view {{ cn.Card {{ cn.Badge("{label}", variant = "secondary") }} }} }}"#
                )
                .as_bytes(),
            )
            .unwrap();
    };

    write_part("Another");
    let a = BlincDsl::reload_project(&entry, &dir, blinc_cn_dsl::register_all).unwrap();
    let host = div().w(600.0).h(200.0).child_box(a.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 200.0);
    println!("BEFORE {:?}", texts(&tree));

    // Same edit the user makes: extend the literal, save, reload.
    write_part("Another content");
    let b = BlincDsl::reload_project(&entry, &dir, blinc_cn_dsl::register_all).unwrap();
    let host2 = div().w(600.0).h(200.0).child_box(b.view_widget());
    let result = tree.incremental_update(&host2);
    tree.compute_layout(600.0, 200.0);
    let after = texts(&tree);
    println!("AFTER  {result:?} {after:?}");

    assert!(
        after.iter().any(|(c, _, _)| c == "Another content"),
        "the glyph run must carry the new literal, got {after:?}"
    );

    // The window builds the view once per frame, so the reloaded
    // instance is asked for it again and again. Every call has to
    // produce the same program -- including after the instance it
    // replaced is dropped, which is what the swap in the host does.
    drop(a);
    let host3 = div().w(600.0).h(200.0).child_box(b.view_widget());
    tree.incremental_update(&host3);
    tree.compute_layout(600.0, 200.0);
    let again = texts(&tree);
    println!("AGAIN  {again:?}");
    assert_eq!(
        after, again,
        "a second view_widget() on the same instance must build the same program"
    );
}
