//! Relayout stops where an edit's effect stops, and gives what a layout from scratch gives.

use taffy::prelude::*;
use taffy::{Overflow, Point};

use super::{LayoutNodeId, LayoutTree, NodeSet, TextMeasureContext};

fn text(content: &str) -> TextMeasureContext {
    TextMeasureContext {
        content: content.into(),
        font_size: 14.0,
        line_height: 1.2,
        letter_spacing: 0.0,
        word_spacing: 0.0,
        wrap: true,
        font_name: None,
        generic_font: Default::default(),
        font_weight: 400,
        italic: false,
    }
}

fn space() -> Size<AvailableSpace> {
    Size {
        width: AvailableSpace::Definite(800.0),
        height: AvailableSpace::Definite(600.0),
    }
}

fn fixed(w: f32, h: f32) -> Style {
    Style {
        size: Size {
            width: length(w),
            height: length(h),
        },
        overflow: Point {
            x: Overflow::Hidden,
            y: Overflow::Hidden,
        },
        ..Default::default()
    }
}

fn column() -> Style {
    Style {
        flex_direction: FlexDirection::Column,
        padding: Rect {
            left: length(10.5),
            right: length(10.5),
            top: length(7.25),
            bottom: length(7.25),
        },
        gap: Size {
            width: length(3.3),
            height: length(3.3),
        },
        ..Default::default()
    }
}

/// Lay `root` out and check it against a layout from scratch. The nodes on a changed path
/// that were laid out again.
fn layout(tree: &mut LayoutTree, root: LayoutNodeId) -> NodeSet {
    tree.compute_layout(root, space());
    let stamp = tree.relayout_stamp;
    let relaid = tree
        .nodes
        .iter()
        .filter(|(_, n)| n.relaid == stamp)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(tree.check_from_scratch(root, space()), 0);
    relaid
}

/// A page: a column holding a fixed card with a label, and an auto-sized card with a label.
struct Page {
    tree: LayoutTree,
    root: LayoutNodeId,
    fixed_card: LayoutNodeId,
    fixed_label: LayoutNodeId,
    auto_card: LayoutNodeId,
    auto_label: LayoutNodeId,
}

fn page() -> Page {
    let mut tree = LayoutTree::new();
    let root = tree.create_node(column());
    let fixed_card = tree.create_node(fixed(200.3, 100.6));
    let fixed_label = tree.create_text_node(Style::default(), text("short"));
    let auto_card = tree.create_node(column());
    let auto_label = tree.create_text_node(Style::default(), text("also short"));
    tree.add_child(root, fixed_card);
    tree.add_child(fixed_card, fixed_label);
    tree.add_child(root, auto_card);
    tree.add_child(auto_card, auto_label);
    layout(&mut tree, root);
    Page {
        tree,
        root,
        fixed_card,
        fixed_label,
        auto_card,
        auto_label,
    }
}

#[test]
fn text_in_a_fixed_box_stops_at_the_box() {
    let mut p = page();
    p.tree
        .update_text(p.fixed_label, |t| t.content = "a much longer label".into());
    let relaid = layout(&mut p.tree, p.root);
    assert!(relaid.contains(&p.fixed_label) && relaid.contains(&p.fixed_card));
    assert!(!relaid.contains(&p.root));
}

#[test]
fn text_in_an_auto_box_reaches_the_root() {
    let mut p = page();
    p.tree
        .update_text(p.auto_label, |t| t.content = "a much longer label".into());
    let relaid = layout(&mut p.tree, p.root);
    assert!(relaid.contains(&p.auto_card) && relaid.contains(&p.root));
}

#[test]
fn same_text_is_answered_the_same() {
    let mut p = page();
    p.tree.update_text(p.auto_label, |_| {});
    let relaid = layout(&mut p.tree, p.root);
    assert!(relaid.contains(&p.auto_label));
    assert!(!relaid.contains(&p.auto_card));
}

#[test]
fn a_style_change_lays_out_the_parent() {
    let mut p = page();
    let mut style = p.tree.get_style(p.fixed_label).unwrap();
    style.margin.left = length(12.4);
    p.tree.set_style(p.fixed_label, style);
    let relaid = layout(&mut p.tree, p.root);
    assert!(relaid.contains(&p.fixed_card));
    assert!(!relaid.contains(&p.root));
}

#[test]
fn a_child_added_to_a_fixed_box_stays_in_it() {
    let mut p = page();
    let inner = p.tree.create_node(column());
    let label = p.tree.create_text_node(Style::default(), text("new"));
    p.tree.add_child(inner, label);
    p.tree.add_child(p.fixed_card, inner);
    let relaid = layout(&mut p.tree, p.root);
    assert!(relaid.contains(&p.fixed_card));
    assert!(!relaid.contains(&p.root));

    p.tree.remove_subtree(inner);
    let relaid = layout(&mut p.tree, p.root);
    assert!(!relaid.contains(&p.root));
}

#[test]
fn edits_on_both_sides_lay_out_both() {
    let mut p = page();
    p.tree
        .update_text(p.fixed_label, |t| t.content = "one two three four".into());
    p.tree.update_text(p.auto_label, |t| {
        t.content = "five six seven eight nine".into()
    });
    let relaid = layout(&mut p.tree, p.root);
    assert!(relaid.contains(&p.root));
}

#[test]
fn display_none_and_back() {
    let mut p = page();
    let mut style = p.tree.get_style(p.auto_card).unwrap();
    style.display = Display::None;
    p.tree.set_style(p.auto_card, style.clone());
    layout(&mut p.tree, p.root);
    style.display = Display::Flex;
    p.tree.set_style(p.auto_card, style);
    layout(&mut p.tree, p.root);
}

#[test]
fn a_child_moved_between_parents() {
    let mut p = page();
    let moved = p.tree.replace_children(p.auto_card, vec![p.fixed_label]);
    assert_eq!(moved, vec![p.auto_label]);
    assert!(p.tree.children(p.fixed_card).is_empty());
    layout(&mut p.tree, p.root);
}

#[test]
fn a_grid_item_is_asked_many_times_and_still_stops() {
    let mut tree = LayoutTree::new();
    let root = tree.create_node(Style {
        display: Display::Grid,
        grid_template_columns: vec![auto(), fr(1.0), minmax(length(40.0), max_content())],
        ..Default::default()
    });
    let mut cells = Vec::new();
    for i in 0..9 {
        let cell = tree.create_node(fixed(60.0 + i as f32, 30.0));
        let label = tree.create_text_node(Style::default(), text("cell"));
        tree.add_child(cell, label);
        tree.add_child(root, cell);
        cells.push((cell, label));
    }
    layout(&mut tree, root);
    let (cell, label) = cells[4];
    tree.update_text(label, |t| t.content = "a longer cell label".into());
    let relaid = layout(&mut tree, root);
    assert!(relaid.contains(&cell));
    assert!(!relaid.contains(&root));
}

#[test]
fn a_layout_with_nothing_changed_keeps_the_epoch() {
    let mut p = page();
    let epoch = p.tree.epoch();
    p.tree.compute_layout(p.root, space());
    assert_eq!(p.tree.epoch(), epoch);
}

#[test]
fn a_new_space_lays_out_from_the_root() {
    let mut p = page();
    p.tree
        .update_text(p.fixed_label, |t| t.content = "changed".into());
    let narrow = Size {
        width: AvailableSpace::Definite(300.0),
        height: AvailableSpace::Definite(600.0),
    };
    p.tree.compute_layout(p.root, narrow);
    assert_eq!(p.tree.check_from_scratch(p.root, narrow), 0);
}
