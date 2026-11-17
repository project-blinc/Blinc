//! Additional paint layers visit their own branches and preserve inherited content.

use blinc_core::{Brush, Color, DrawCommand, GlassStyle, RecordingContext, Size};
use blinc_layout::{div::div, element::RenderLayer, renderer::RenderTree};

fn scene() -> RenderTree {
    let mut background = div().w(100.0).h(100.0).flex_col();
    for _ in 0..128 {
        background = background.child(div().w(2.0).h(2.0).bg(Color::BLUE));
    }
    let ui = div()
        .w(320.0)
        .h(240.0)
        .child(background)
        .child(
            div()
                .id("left")
                .w(40.0)
                .h(40.0)
                .child(div().id("mark").w(10.0).h(10.0).bg(Color::RED)),
        )
        .child(div().id("right").w(40.0).h(40.0));
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(320.0, 240.0);
    tree
}

fn paint(tree: &RenderTree) -> (usize, usize) {
    let mut ctx = RecordingContext::new(Size::new(320.0, 240.0));
    tree.render_snapshot(&mut ctx);
    let visits = ctx
        .commands()
        .iter()
        .filter(|command| matches!(command, DrawCommand::PushTransform(_)))
        .count();
    let marks = ctx
        .commands()
        .iter()
        .filter(|command| {
            matches!(command, DrawCommand::FillRect { brush: Brush::Solid(color), .. } if *color == Color::RED)
        })
        .count();
    (visits, marks)
}

#[test]
fn foreground_pass_skips_unrelated_background_and_tracks_layer_changes() {
    let mut tree = scene();
    let (background_visits, marks) = paint(&tree);
    assert_eq!(marks, 1);
    let mark = tree.query_by_id("mark").unwrap();
    tree.update_render_props(mark, |props| props.layer = RenderLayer::Foreground);
    let (visits, marks) = paint(&tree);
    assert_eq!(marks, 1, "foreground content must still be drawn");
    assert!(
        visits <= background_visits + 32,
        "foreground pass revisited the large background branch: {visits} vs {background_visits}"
    );
    tree.update_render_props(mark, |props| props.layer = RenderLayer::Background);
    assert_eq!(paint(&tree), (background_visits, 1));
}

#[test]
fn glass_passes_keep_ordinary_descendants_without_visiting_unrelated_content() {
    let mut tree = scene();
    let (background_visits, _) = paint(&tree);
    let left = tree.query_by_id("left").unwrap();
    tree.update_render_props(left, |props| {
        props.background = Some(Brush::Glass(GlassStyle::default()));
    });
    let (visits, marks) = paint(&tree);
    assert_eq!(marks, 1, "glass children belong in the foreground pass");
    assert!(
        visits <= background_visits + 64,
        "glass passes revisited the large background branch: {visits} vs {background_visits}"
    );
}

#[test]
fn foreground_branch_index_follows_layout_only_reparenting() {
    let mut tree = scene();
    let mark = tree.query_by_id("mark").unwrap();
    let left = tree.query_by_id("left").unwrap();
    let right = tree.query_by_id("right").unwrap();
    tree.update_render_props(mark, |props| props.layer = RenderLayer::Foreground);
    assert_eq!(paint(&tree).1, 1);
    assert_eq!(tree.layout_tree.replace_children(left, vec![]), vec![mark]);
    tree.layout_tree.replace_children(right, vec![mark]);
    tree.compute_layout(320.0, 240.0);
    assert_eq!(
        paint(&tree).1,
        1,
        "reparented foreground content was skipped"
    );
}

#[test]
fn foreground_children_inherit_their_owners_layer() {
    let mut tree = scene();
    let left = tree.query_by_id("left").unwrap();
    tree.update_render_props(left, |props| props.layer = RenderLayer::Foreground);
    assert_eq!(
        paint(&tree).1,
        1,
        "ordinary foreground descendants were skipped"
    );
}
