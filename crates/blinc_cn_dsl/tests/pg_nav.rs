//! The playground's rail actually navigates.
//!
//! `pg_render` proves the project compiles and builds; this proves the
//! sidebar and the page `match` are wired to each other, which building
//! alone cannot show — a dropped match arm still builds.
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};

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
                Arc::new(AtomicBool::new(false)),
            );
        }
    });
}

fn texts(dsl: &BlincDsl) -> Vec<String> {
    let host = div().w(1200.0).h(800.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(1200.0, 800.0);
    let mut out = Vec::new();
    let mut stack = vec![tree.root().expect("root")];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
        {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    out
}

/// One program walked through every page: the rail's rows are always
/// there, and the content beside them swaps.
///
/// `page.set(…)` is the write each row's `on_click` makes.
#[test]
fn every_row_reaches_its_page() {
    init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile");

    let found = texts(&dsl);
    for row in ["Forms", "Feedback", "Media", "Bindings"] {
        assert!(found.iter().any(|t| t == row), "row {row}: {found:?}");
    }
    for title in ["WIDGETS", "REACTIVITY"] {
        assert!(found.iter().any(|t| t == title), "section {title}");
    }

    // Each page names something only it renders.
    let pages = [
        ("forms", "button variants"),
        ("feedback", "badge: variant x style"),
        ("media", "avatar: size x shape"),
        ("reactive", "progress: value = pct (in-place)"),
    ];
    for (key, marker) in pages {
        dsl.set_signal_string("page", key);
        let found = texts(&dsl);
        assert!(
            found.iter().any(|t| t == marker),
            "page {key} renders {marker:?}: {found:?}"
        );
        // And nothing from a page that is not showing.
        for (other, other_marker) in pages {
            if other != key {
                assert!(
                    !found.iter().any(|t| t == other_marker),
                    "page {key} must not also render {other}'s {other_marker:?}"
                );
            }
        }
    }
}
