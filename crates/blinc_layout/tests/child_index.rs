//! The children of a container with very many are found without looking at each, and the
//! children found are the ones a scan with `draws_outside` finds.

use blinc_core::Transform;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::{Div, div};
use blinc_layout::renderer::RenderTree;

const ROWS: usize = 2000;
const ROW: f32 = 20.0;

fn tree_of(rows: usize, row: impl Fn(usize) -> Div) -> (RenderTree, LayoutNodeId) {
    let mut list = div().id("list").flex_col().w(200.0);
    for i in 0..rows {
        list = list.child(row(i));
    }
    let mut tree = RenderTree::from_element(&list);
    tree.compute_layout(200.0, 400.0);
    let node = tree.query_by_id("list").unwrap();
    (tree, node)
}

fn plain(_: usize) -> Div {
    div().w_full().h(ROW)
}

/// What `visible_children` stands in for.
fn scanned(
    tree: &RenderTree,
    node: LayoutNodeId,
    shift: (f32, f32),
    view: [f32; 4],
) -> Vec<LayoutNodeId> {
    tree.layout_tree
        .children(node)
        .into_iter()
        .filter(|c| !tree.draws_outside(*c, shift, view))
        .collect()
}

/// The rows found are those a scan finds. With `exact` off, rows the scan culls may also be found:
/// the index takes a row and what draws from it as one extent, so a row with something drawn far
/// from it stays near when the scan would cull it, which only paints more.
fn agree_with(tree: &RenderTree, node: LayoutNodeId, what: &str, exact: bool) {
    for shift_y in [0.0, -500.0, -5000.0, -39_000.0, -39_900.0, 130.0] {
        for view in [[0.0, 0.0, 200.0, 100.0], [0.0, 50.0, 200.0, 300.0]] {
            // Twice: the second is answered from the index the first made.
            for _ in 0..2 {
                let found = tree.visible_children(node, (0.0, shift_y), view);
                let scan = scanned(tree, node, (0.0, shift_y), view);
                if exact {
                    assert_eq!(found, scan, "{what}: shift {shift_y}, view {view:?}");
                } else {
                    assert!(
                        scan.iter().all(|c| found.contains(c)),
                        "{what}: shift {shift_y}, view {view:?}: a row the scan keeps was culled"
                    );
                }
            }
        }
    }
}

fn agree(tree: &RenderTree, node: LayoutNodeId, what: &str) {
    agree_with(tree, node, what, true);
}

#[test]
fn a_long_list_is_culled_as_a_scan_culls_it() {
    let (tree, node) = tree_of(ROWS, plain);
    agree(&tree, node, "plain rows");
    let seen = tree.visible_children(node, (0.0, 0.0), [0.0, 0.0, 200.0, 100.0]);
    assert!(
        seen.len() < 40,
        "a screen of rows came back as {} of {ROWS}",
        seen.len()
    );
}

#[test]
fn the_overscan_widens_what_is_found() {
    let (tree, node) = tree_of(ROWS, plain);
    let view = [0.0, 0.0, 200.0, 100.0];
    let narrow = tree.visible_children(node, (0.0, 0.0), view).len();
    let was = tree.set_cull_overscan(1000.0);
    let wide = tree.visible_children(node, (0.0, 0.0), view);
    assert!(wide.len() > narrow);
    assert_eq!(wide, scanned(&tree, node, (0.0, 0.0), view));
    tree.set_cull_overscan(was);
    assert_eq!(
        tree.visible_children(node, (0.0, 0.0), view).len(),
        narrow,
        "the index kept the wider window"
    );
}

#[test]
fn a_row_that_moves_is_never_culled() {
    // Laid out at the bottom of the list, drawn far above it.
    let (tree, node) = tree_of(ROWS, |i| {
        if i == ROWS - 10 {
            plain(i).translate(0.0, -(ROWS as f32) * ROW)
        } else {
            plain(i)
        }
    });
    let mover = tree.layout_tree.children(node)[ROWS - 10];
    for _ in 0..2 {
        let seen = tree.visible_children(node, (0.0, 0.0), [0.0, 0.0, 200.0, 100.0]);
        assert!(seen.contains(&mover), "the moved row was culled");
    }
    agree(&tree, node, "a moved row");
}

#[test]
fn a_clipping_row_stands_for_what_is_under_it() {
    // The overflow is clipped, so it does not reach the view; the same overflow unclipped does.
    for clipped in [true, false] {
        let (tree, node) = tree_of(ROWS, |i| {
            let row = plain(i);
            if i == ROWS - 10 {
                let tall = div().w(10.0).h(ROWS as f32 * ROW).mt(-(ROWS as f32) * ROW);
                let row = row.child(tall);
                if clipped { row.overflow_clip() } else { row }
            } else {
                row
            }
        });
        let what = if clipped {
            "clipped overflow"
        } else {
            "overflow"
        };
        agree_with(&tree, node, what, clipped);
    }
}

#[test]
fn a_row_changed_after_the_index_was_made_is_seen() {
    let (mut tree, node) = tree_of(ROWS, plain);
    agree(&tree, node, "before");
    let row = tree.layout_tree.children(node)[ROWS - 5];
    tree.update_render_props(row, |props| {
        props.transform = Some(Transform::translate(0.0, -(ROWS as f32) * ROW));
    });
    let seen = tree.visible_children(node, (0.0, 0.0), [0.0, 0.0, 200.0, 100.0]);
    assert!(
        seen.contains(&row),
        "the index was kept past a change to a row"
    );
    agree(&tree, node, "after");
}

#[test]
fn a_relayout_is_seen() {
    let (mut tree, node) = tree_of(ROWS, plain);
    agree(&tree, node, "before");
    let row = tree.layout_tree.children(node)[3];
    let mut style = tree.layout_tree.get_style(row).unwrap();
    style.size.height = taffy::Dimension::length(ROW * 300.0);
    tree.layout_tree.set_style(row, style);
    tree.compute_layout(200.0, 400.0);
    agree(&tree, node, "after");
}

#[test]
fn a_row_of_children_is_ordered_along_x() {
    let mut list = div().id("list").flex_row().h(40.0);
    for _ in 0..ROWS {
        list = list.child(div().w(ROW).h_full().flex_shrink_0());
    }
    let mut tree = RenderTree::from_element(&list);
    tree.compute_layout(400.0, 40.0);
    let node = tree.query_by_id("list").unwrap();
    for shift_x in [0.0, -3000.0, -39_000.0] {
        for _ in 0..2 {
            let view = [0.0, 0.0, 400.0, 40.0];
            assert_eq!(
                tree.visible_children(node, (shift_x, 0.0), view),
                scanned(&tree, node, (shift_x, 0.0), view),
                "shift {shift_x}"
            );
        }
    }
}

#[test]
fn a_short_list_is_scanned() {
    let (tree, node) = tree_of(40, plain);
    let view = [0.0, 0.0, 200.0, 100.0];
    assert_eq!(
        tree.visible_children(node, (0.0, 0.0), view),
        scanned(&tree, node, (0.0, 0.0), view)
    );
}
