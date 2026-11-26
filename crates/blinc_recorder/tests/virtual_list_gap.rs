#![cfg(feature = "headless")]
//! A virtual list's gap separates its rows, and the space held for the rows it has not built
//! counts it.

use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::selector::ScrollRef;
use blinc_layout::widgets::virtual_list::virtual_list;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const ROWS: usize = 200;
const ROW: f32 = 30.0;
const GAP: f32 = 10.0;

/// The node with `class`.
fn find(ui: &Headless, class: &str) -> LayoutNodeId {
    let tree = ui.tree();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if tree.element_registry().has_class(node, class) {
            return node;
        }
        stack.extend(tree.layout_tree.children(node));
    }
    panic!("no .{class}");
}

/// How far below the top of the list's content a row is, if it is built.
fn row_top(ui: &Headless, row: usize) -> Option<f32> {
    let tree = ui.tree();
    let content = tree.get_absolute_bounds(find(ui, "gap-content"))?;
    let node = tree.query_by_id(&format!("gap-row-{row}"))?;
    Some(tree.get_absolute_bounds(node)?.y - content.y)
}

fn content_height(ui: &Headless) -> f32 {
    ui.tree()
        .get_absolute_bounds(find(ui, "gap-content"))
        .unwrap()
        .height
}

#[test]
fn rows_stand_a_gap_apart_wherever_the_list_is_scrolled() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(360, 300).unwrap();
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    let reference = ScrollRef::new();
    let bound = reference.clone();
    ui.mount(move |_| {
        virtual_list(ROWS, |i| div().id(format!("gap-row-{i}")).w_full().h(ROW))
            .estimated_item_height(ROW)
            .window_size(20)
            .gap_px(GAP)
            .content_class("gap-content")
            .bind(&bound)
            .w(360.0)
            .h(300.0)
            .into_div()
    });
    ui.settle();

    let whole = ROWS as f32 * ROW + (ROWS - 1) as f32 * GAP;
    assert_eq!(row_top(&ui, 0), Some(0.0));
    assert_eq!(row_top(&ui, 1), Some(ROW + GAP));
    assert_eq!(content_height(&ui), whole);

    for row in [100, ROWS - 1] {
        reference.set_scroll_offset(0.0, (row as f32 * (ROW + GAP)).min(whole - 300.0));
        ui.settle_scroll();
        ui.settle();
        assert_eq!(
            row_top(&ui, row),
            Some(row as f32 * (ROW + GAP)),
            "row {row}"
        );
        assert_eq!(content_height(&ui), whole, "scrolled to row {row}");
    }
}
