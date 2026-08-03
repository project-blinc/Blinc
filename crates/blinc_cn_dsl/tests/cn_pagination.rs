//! `cn.Select` — a bound signal and `cn.Option` children.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::{Arc, Mutex};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

struct Harness {
    tree: RenderTree,
}

impl Harness {
    fn new(dsl: &BlincDsl) -> Self {
        let host = div().w(600.0).h(400.0).child_box(dsl.view_widget());
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(600.0, 400.0);
        Self { tree }
    }

    fn texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        let Some(root) = self.tree.root() else {
            return out;
        };
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if let Some(node) = self.tree.get_render_node(id)
                && let ElementType::Text(t) = &node.element_type
            {
                out.push(t.content.clone());
            }
            stack.extend(self.tree.layout_tree.children(id).iter().copied());
        }
        out
    }

    fn shows(&self, needle: &str) -> bool {
        self.texts().iter().any(|t| t.contains(needle))
    }
}

fn compile(src: &str, name: &str) -> BlincDsl {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

/// The numbers are derived from `total`, not declared as children.
#[test]
fn the_pages_come_from_total() {
    let dsl = compile(
        r#"signal pg_page: i32 = 1

           view { cn.Pagination(page = pg_page, total = 5.0) }"#,
        "pagination_total.blinc",
    );

    let h = Harness::new(&dsl);
    for n in ["1", "2", "3", "4", "5"] {
        assert!(h.shows(n), "page {n} drawn: {:?}", h.texts());
    }
}

/// `visible` bounds the window, so a long book does not print every
/// page number.
#[test]
fn visible_bounds_the_window() {
    let dsl = compile(
        r#"signal pg_many: i32 = 1

           view { cn.Pagination(page = pg_many, total = 50.0, visible = 3.0) }"#,
        "pagination_visible.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(
        !h.shows("50"),
        "the last page is not in a 3-wide window: {:?}",
        h.texts()
    );
}

/// Writing the signal moves the highlight.
#[test]
fn writing_the_signal_moves_the_page() {
    let dsl = compile(
        r#"signal pg_bound: i32 = 1

           view { cn.Pagination(page = pg_bound, total = 9.0) }"#,
        "pagination_bound.blinc",
    );

    let _ = Harness::new(&dsl);
    blinc_runtime::signal::set_i32("pg_bound", 4);
    assert_eq!(blinc_runtime::signal::get_i32("pg_bound"), Some(4));

    // Still renders, and the window has moved with it.
    let h = Harness::new(&dsl);
    assert!(h.shows("4"), "the current page is drawn: {:?}", h.texts());
}

/// The binding writes back to the signal the author declared rather
/// than to a widget-owned copy, so its id is the one that was bound.
#[test]
fn the_page_keeps_the_authors_signal() {
    let dsl = compile(
        r#"signal pg_ident: i32 = 2

           view { cn.Pagination(page = pg_ident, total = 6.0) }"#,
        "pagination_identity.blinc",
    );

    let _ = Harness::new(&dsl);
    // A widget that narrowed to its own copy would leave this at 2.
    blinc_runtime::signal::set_i32("pg_ident", 5);
    let h = Harness::new(&dsl);
    assert!(
        h.shows("5"),
        "the widget reads the author's signal: {:?}",
        h.texts()
    );
    assert_eq!(blinc_runtime::signal::get_i32("pg_ident"), Some(5));
}

/// Page numbers are 1-based, so a signal seeded at 0 still reads as a
/// page rather than clamping to nothing.
#[test]
fn a_page_below_one_reads_as_the_first() {
    let dsl = compile(
        r#"signal pg_zero: i32 = 0

           view { cn.Pagination(page = pg_zero, total = 3.0) }"#,
        "pagination_zero.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(h.shows("1"), "still draws a first page: {:?}", h.texts());
}

/// Picking a page must not blank the view.
///
/// The other tests build a fresh tree per assertion, which is not what a
/// running app does: there the write lands in a tree that already
/// exists and the queued rebuild is applied in place. A widget that
/// disappears on that path still passes a fresh-build test.
#[test]
fn picking_a_page_keeps_the_view() {
    let dsl = compile(
        r#"signal pg_live: i32 = 1

           view {
             Div {
               cn.Pagination(page = pg_live, total = 8.0)
               cn.Label("still here")
             }
           }"#,
        "pagination_live.blinc",
    );

    let host = div().w(600.0).h(400.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(600.0, 400.0);

    let before = count_nodes(&tree);
    assert!(before > 1, "the pagination drew something to begin with");

    // What a click does: write the bound page, then let the frame apply
    // whatever that queued.
    blinc_runtime::signal::set_i32("pg_live", 3);
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(600.0, 400.0);

    let after = count_nodes(&tree);
    let texts = tree_texts(&tree);
    println!("PROBE before={before} after={after} texts={texts:?}");
    assert!(
        texts.iter().any(|t| t.contains("still here")),
        "the sibling survived the page change: {texts:?}",
    );
    assert!(
        texts.iter().any(|t| t.contains("3")),
        "the new page is drawn: {texts:?}",
    );
}

fn tree_texts(tree: &RenderTree) -> Vec<String> {
    let Some(root) = tree.root() else {
        return Vec::new();
    };
    let (mut out, mut stack) = (Vec::new(), vec![root]);
    while let Some(id) = stack.pop() {
        if let Some(n) = tree.get_render_node(id)
            && let ElementType::Text(t) = &n.element_type
        {
            out.push(t.content.clone());
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    out
}

fn count_nodes(tree: &RenderTree) -> usize {
    let Some(root) = tree.root() else { return 0 };
    let (mut n, mut stack) = (0, vec![root]);
    while let Some(id) = stack.pop() {
        n += 1;
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    n
}
