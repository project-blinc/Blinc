use super::opacity_group_tests::{init_theme, surface};
use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::read_rendered_pixels;
use blinc_animation::AnimationScheduler;

/// Faded cards in a row, each with a label over its own fill.
fn cards() -> Div {
    (0..8).fold(
        div()
            .w(160.0)
            .h(120.0)
            .bg(Color::rgb(0.1, 0.1, 0.3))
            .flex_row()
            .flex_wrap(),
        |grid, i| {
            let tint = i as f32 / 8.0;
            grid.child(
                div()
                    .w(40.0)
                    .h(24.0)
                    .bg(Color::rgb(0.9 - tint * 0.5, 0.3 + tint * 0.4, 0.4))
                    .opacity(0.6)
                    .items_center()
                    .justify_center()
                    .child(text("Ab").size(14.0).color(Color::WHITE)),
            )
        },
    )
}

#[test]
#[ignore] // Requires GPU
fn faded_labelled_cards_through_an_atlas_match_the_cards_drawn_alone() {
    init_theme();
    let mut app = BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required");
    let mut tree = RenderTree::from_element(&cards());
    tree.compute_layout(160.0, 120.0);
    let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    let mut render = |batched: bool| {
        app.context().renderer.set_batch_opacity_layers(batched);
        app.context().invalidate_render_cache();
        let before = app.context().renderer.opacity_atlas_count();
        let texture = surface(&app);
        let view = texture.create_view(&Default::default());
        app.render_tree_with_motion_opt(&tree, &state, &view, None, 160, 120, false)
            .unwrap();
        let atlases = app.context().renderer.opacity_atlas_count() - before;
        (
            read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120),
            atlases,
        )
    };
    let (alone, none) = render(false);
    let (batched, atlases) = render(true);
    assert_eq!(none, 0);
    assert!(atlases > 0, "the cards did not take the atlas path");
    let worst = alone
        .pixels()
        .zip(batched.pixels())
        .flat_map(|(a, b)| a.0.into_iter().zip(b.0).map(|(a, b)| a.abs_diff(b)))
        .max()
        .unwrap();
    assert!(worst <= 2, "pixels differ by {worst}");
    // The labels are drawn: some pixel of a card differs from the card's fill by a lot.
    let first = alone.get_pixel(2, 2).0;
    assert!(
        (0..40).any(|x| (0..24).any(|y| {
            let p = batched.get_pixel(x, y).0;
            p.iter().zip(first).any(|(a, b)| a.abs_diff(b) > 40)
        })),
        "no label ink in the first card"
    );
}
