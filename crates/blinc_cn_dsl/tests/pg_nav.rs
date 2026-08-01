//! The playground's rail actually navigates.
//!
//! `pg_render` proves the project compiles and builds; this proves the
//! sidebar and the page `match` are wired to each other, which building
//! alone cannot show — a dropped match arm still builds.
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use blinc_dsl_core::BlincDsl;
use blinc_layout::LayoutNodeId;
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

/// The sequence a real frame runs. The stylesheet matters here beyond
/// appearance: `overflow: scroll` on a class only becomes a scroll
/// container once the layout overrides have been applied, so a tree
/// built without them can never scroll.
fn laid_out(dsl: &BlincDsl, css: &str, w: f32, h: f32) -> RenderTree {
    let host = div().w(w).h(h).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    if !css.is_empty() {
        tree.set_stylesheet(blinc_layout::css_parser::Stylesheet::parse(css).expect("css"));
    }
    tree.apply_stylesheet_base_styles();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(w, h);
    // A region mounts its content during the rebuild, so the rules have
    // to be applied again afterwards or everything inside a `with` is
    // left unstyled — which for `overflow: scroll` means unscrollable.
    tree.process_pending_subtree_rebuilds();
    tree.apply_stylesheet_base_styles();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(w, h);
    tree
}

fn texts(tree: &RenderTree) -> Vec<String> {
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

/// The scroll container above `label`, if the page sits in one.
fn scroll_viewport_over(tree: &RenderTree, label: &str) -> Option<LayoutNodeId> {
    let mut stack = vec![(tree.root()?, Vec::new())];
    while let Some((id, path)) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
            && t.content == label
        {
            return path
                .into_iter()
                .rev()
                .find(|a| tree.is_scroll_container(*a));
        }
        let mut child_path = path.clone();
        child_path.push(id);
        for c in tree.layout_tree.children(id) {
            stack.push((c, child_path.clone()));
        }
    }
    None
}

/// The whole shell, walked in one program: every row reaches its page,
/// the page sits in a scroll viewport, and collapsing the rail leaves it
/// alone.
///
/// One test rather than three: `page` and `nav_shut` are process-global,
/// so separate tests would each inherit whatever the last one left and
/// `cargo test` runs them in an order that varies.
#[test]
fn the_shell_navigates_scrolls_and_collapses() {
    init();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/playground");
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl.compile_project(&root.join("main.blinc"), &root)
        .expect("compile");

    // A component's `style { }` block reaches the context on the first
    // `view_widget()`, so build once before draining or the sheet holds
    // only cn's own rules and every class here is silently missing.
    let _ = dsl.view_widget();
    // Drained once: the queue empties on read, so a later build would
    // get an empty stylesheet and lose the rules again.
    let css: String = blinc_core::BlincContextState::get()
        .drain_stylesheets()
        .join("\n");
    assert!(
        css.contains(".shell"),
        "the shell's own rules must be in the sheet, not just cn's"
    );

    let found = texts(&laid_out(&dsl, &css, 1200.0, 800.0));
    for row in ["Forms", "Feedback", "Media", "Bindings"] {
        assert!(found.iter().any(|t| t == row), "row {row}: {found:?}");
    }
    for title in ["WIDGETS", "REACTIVITY"] {
        assert!(found.iter().any(|t| t == title), "section {title}");
    }

    // Each page names something only it renders. `page.set(…)` is the
    // write each row's `on_click` makes.
    let pages = [
        ("forms", "button variants"),
        ("feedback", "badge: variant x style"),
        ("media", "avatar: size x shape"),
        ("reactive", "progress: value = pct (in-place)"),
    ];
    for (key, marker) in pages {
        dsl.set_signal_string("page", key);
        let found = texts(&laid_out(&dsl, &css, 1200.0, 800.0));
        assert!(
            found.iter().any(|t| t == marker),
            "page {key} renders {marker:?}: {found:?}"
        );
        for (other, other_marker) in pages {
            if other != key {
                assert!(
                    !found.iter().any(|t| t == other_marker),
                    "page {key} must not also render {other}'s {other_marker:?}"
                );
            }
        }
    }

    // Each page owns its scroller, sized to the content area.
    dsl.set_signal_string("page", "forms");
    let tree = laid_out(&dsl, &css, 1200.0, 400.0);
    let viewport = scroll_viewport_over(&tree, "button variants")
        .expect("the page sits inside a scroll container");
    let h = tree
        .layout_tree
        .get_layout(viewport)
        .expect("laid out")
        .size
        .height;
    assert!(
        h <= 400.0,
        "the viewport must stay within the window to bound anything, got {h}"
    );

    // Collapsing last: it leaves `nav_shut` set for anything after it.
    dsl.set_signal_bool("nav_shut", true);
    let found = texts(&laid_out(&dsl, &css, 1200.0, 800.0));
    assert!(
        !found.iter().any(|t| t == "Forms" || t == "WIDGETS"),
        "the rail collapsed to icons: {found:?}"
    );
    assert!(
        found.iter().any(|t| t == "button variants"),
        "and the page beside it is untouched: {found:?}"
    );
}
