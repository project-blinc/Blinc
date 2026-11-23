//! After a region's rows come and go, the CSS layout pass brings the rows' layout rules up to
//! date: rules on a row's own position, and rules that reach below a row through its position.

use std::sync::Mutex;

use blinc_core::reactive::{Signal, signal};
use blinc_layout::binding::with_registry;
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

/// The pending-write queue and the reactive graph are process-wide.
static LOCK: Mutex<()> = Mutex::new(());

struct Scene {
    tree: RenderTree,
    items: Signal<Vec<String>>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

fn scene(css: &str) -> Scene {
    let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let _ = blinc_layout::stateful::take_pending_partial_prop_updates();
    let _ = blinc_layout::stateful::take_pending_subtree_rebuilds();
    with_registry(|_| {});

    let items = signal(vec!["a".to_string()]);
    let ui = div().w(200.0).h(200.0).child(div().flex_col().for_each(
        items,
        |item: &String| item.clone(),
        |_: String| div().class("row").flex_row().child(div().class("cell")),
    ));
    let mut tree = RenderTree::from_element(&ui);
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_all_stylesheet_styles();
    tree.compute_layout(200.0, 200.0);
    Scene {
        tree,
        items,
        _guard: guard,
    }
}

impl Scene {
    /// What a runner does after a change: the queued writes, the scoped layout pass, layout.
    fn frame(&mut self) {
        let updates = blinc_layout::stateful::take_pending_partial_prop_updates();
        self.tree.apply_partial_property_updates(updates);
        self.tree.apply_stylesheet_layout_overrides_scoped(false);
        self.tree.compute_layout(200.0, 200.0);
    }

    fn set(&mut self, items: &[&str]) {
        self.items
            .set(items.iter().map(|s| s.to_string()).collect());
        self.frame();
    }

    /// The width of every row and of the cell in it, in order.
    fn widths(&self) -> Vec<(f32, f32)> {
        let root = self.tree.root().unwrap();
        let list = self.tree.layout_tree.children(root)[0];
        self.tree
            .layout_tree
            .children(list)
            .into_iter()
            .map(|row| {
                let cell = self.tree.layout_tree.children(row)[0];
                let width = |node| self.tree.get_absolute_bounds(node).unwrap().width;
                (width(row), width(cell))
            })
            .collect()
    }
}

#[test]
fn a_rule_on_a_rows_position_follows_rows_coming_and_going() {
    let mut s = scene(
        ".row { width: 50px; height: 10px; } .row:last-child { width: 80px; }
         .cell { width: 10px; height: 10px; }",
    );
    assert_eq!(s.widths(), vec![(80.0, 10.0)]);
    s.set(&["a", "b"]);
    assert_eq!(s.widths(), vec![(50.0, 10.0), (80.0, 10.0)]);
    s.set(&["a"]);
    assert_eq!(s.widths(), vec![(80.0, 10.0)]);
}

#[test]
fn a_rule_below_a_rows_position_follows_rows_coming_and_going() {
    let css = ".row { width: 50px; height: 10px; }
         .cell { width: 10px; height: 10px; } .row:last-child .cell { width: 30px; }";
    assert!(
        Stylesheet::parse(css)
            .unwrap()
            .layout_reads_ancestor_position()
    );
    let mut s = scene(css);
    assert_eq!(s.widths(), vec![(50.0, 30.0)]);
    s.set(&["a", "b"]);
    assert_eq!(s.widths(), vec![(50.0, 10.0), (50.0, 30.0)]);
    s.set(&["a"]);
    assert_eq!(s.widths(), vec![(50.0, 30.0)]);
}

#[test]
fn a_rule_below_a_sibling_combinator_follows_rows_coming_and_going() {
    let css = ".row { width: 50px; height: 10px; }
         .cell { width: 10px; height: 10px; } .row + .row .cell { width: 20px; }";
    assert!(
        Stylesheet::parse(css)
            .unwrap()
            .layout_reads_ancestor_position()
    );
    let mut s = scene(css);
    assert_eq!(s.widths(), vec![(50.0, 10.0)]);
    s.set(&["a", "b"]);
    assert_eq!(s.widths(), vec![(50.0, 10.0), (50.0, 20.0)]);
    s.set(&["b"]);
    assert_eq!(s.widths(), vec![(50.0, 10.0)]);
}

#[test]
fn rules_on_a_subjects_own_position_do_not_read_an_ancestors() {
    for css in [
        ".row:last-child { width: 1px; }",
        ".a + .b { width: 1px; }",
        ".list > .row:first-child { width: 1px; }",
        ".row:last-child .cell:hover { width: 1px; }",
        ".row:last-child .cell { color: red; }",
    ] {
        assert!(
            !Stylesheet::parse(css)
                .unwrap()
                .layout_reads_ancestor_position(),
            "{css}"
        );
    }
}
