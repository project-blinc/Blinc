//! A parent that states `align-items` outranks an INCIDENTAL
//! `align_self`, and only that one.
//!
//! `w_fit`/`h_fit` set `align_self: Start` for one reason: a
//! content-sized item otherwise stretches to fill the cross axis. Taffy
//! gives `align_self` precedence, which is CSS-correct but means that
//! internal anti-stretch measure silently beats what the parent asked
//! for — a `cn.Button` (which is `w_fit` inside) would not centre in a
//! row, and rode high against taller siblings.
//!
//! An `align_self` an author WROTE keeps CSS's precedence: that is
//! control they are entitled to, and the fix must not cost it.
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;

/// The short child's top edge, and the row's height.
fn row_geometry(build: fn() -> blinc_layout::div::Div) -> (f32, f32) {
    let host = build();
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    let root = tree.root().expect("root");
    let row = tree.layout_tree.children(root)[0];
    let short = tree.layout_tree.children(row)[1];
    let row_h = tree.get_absolute_bounds(row).expect("row").height;
    let short_y = tree.get_absolute_bounds(short).expect("short").y;
    (short_y, row_h)
}

/// A tall sibling, then a `w_fit` child, in a row that centres.
fn centred_row() -> blinc_layout::div::Div {
    div().child(
        div()
            .flex_row()
            .items_center()
            .child(div().w(40.0).h(32.0))
            .child(div().w(40.0).h(24.0).w_fit()),
    )
}

/// The same, with the row stating nothing.
fn unaligned_row() -> blinc_layout::div::Div {
    div().child(
        div()
            .flex_row()
            .child(div().w(40.0).h(32.0))
            .child(div().w(40.0).h(24.0).w_fit()),
    )
}

/// The same, with the child naming its own alignment.
fn explicitly_started_row() -> blinc_layout::div::Div {
    div().child(
        div()
            .flex_row()
            .items_center()
            .child(div().w(40.0).h(32.0))
            .child(div().w(40.0).h(24.0).w_fit().align_self_start()),
    )
}

#[test]
fn an_explicit_align_items_beats_a_w_fit_default() {
    let (y, row_h) = row_geometry(centred_row);
    assert_eq!(row_h, 32.0, "the tall child sets the row height");
    assert_eq!(y, 4.0, "the short child centres: (32 - 24) / 2");
}

/// With no `align-items` on the parent there is nothing to obey, so the
/// anti-stretch default stands.
#[test]
fn a_silent_parent_leaves_the_default_alone() {
    let (y, _) = row_geometry(unaligned_row);
    assert_eq!(y, 0.0, "no request to honour, so it stays at cross-start");
}

/// An authored `align_self` keeps CSS's precedence and beats the
/// parent. Only the value `w_fit` sets incidentally yields — taking
/// this away would remove control an author is entitled to.
#[test]
fn an_authored_align_self_still_wins() {
    let (y, _) = row_geometry(explicitly_started_row);
    assert_eq!(y, 0.0, "align_self_start was asked for, so it holds");
}

/// The reason `w_fit` sets it at all: without any `align_self` a
/// content-sized child stretches across a column's cross axis.
#[test]
fn w_fit_still_prevents_a_column_stretch() {
    let host = div().child(
        div()
            .flex_col()
            .w(200.0)
            .h(100.0)
            .child(div().h(24.0).w_fit()),
    );
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    let root = tree.root().expect("root");
    let col = tree.layout_tree.children(root)[0];
    let child = tree.layout_tree.children(col)[0];
    let w = tree.get_absolute_bounds(child).expect("child").width;
    assert!(
        w < 200.0,
        "the child hugs its content rather than stretching: {w}"
    );
}

/// A CSS `align-self` on a `w_fit` node is authored, so it wins.
///
/// This is the case the marking could most easily get wrong: the rule
/// lands on the very node `w_fit` marked, so the mark has to be dropped
/// when CSS writes one, or the author's rule is silently discarded.
#[test]
fn a_css_align_self_on_a_w_fit_node_is_authored() {
    use blinc_layout::css_parser::Stylesheet;

    let host = div().child(
        div()
            .flex_row()
            .items_center()
            .child(div().w(40.0).h(32.0))
            .child(div().w(40.0).h(24.0).w_fit().class("pinned")),
    );
    let mut tree = RenderTree::from_element(&host);
    tree.set_stylesheet(Stylesheet::parse(".pinned { align-self: flex-start; }").expect("parses"));
    tree.apply_stylesheet_base_styles();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(400.0, 200.0);

    let root = tree.root().expect("root");
    let row = tree.layout_tree.children(root)[0];
    let pinned = tree.layout_tree.children(row)[1];
    let y = tree.get_absolute_bounds(pinned).expect("pinned").y;
    assert_eq!(y, 0.0, "the CSS rule holds against the row's centring");
}
