//! What a 3D layer draws.
use super::*;

/// What a frame draws at the middle of a green image over blue, in an element
/// with a 3D transform and no text.
fn turned_image(opacity: f32, mode: u8) -> image::RgbaImage {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let image = blinc_layout::image(super::overlay_group_tests::IMAGE)
        .size(40.0, 30.0)
        .fade_in(std::time::Duration::ZERO)
        .opacity(opacity);
    let root = div().w(160.0).h(120.0).bg(Color::BLUE).child(
        div()
            .absolute()
            .left(40.0)
            .top(30.0)
            .id("holder")
            .child(image),
    );
    let mut tree = RenderTree::from_element(&root);
    tree.compute_layout(160.0, 120.0);
    let holder = tree.query_by_id("holder").unwrap();
    tree.update_render_props(holder, |p| p.rotate_y = Some(0.01));
    let state = RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )));
    super::gpu_pass_group_tests::frame(&mut context, &tree, &state, mode)
}

#[test]
#[ignore] // Requires GPU
fn an_image_in_a_3d_layer_without_text_is_drawn() {
    for mode in 0..3 {
        let drawn = turned_image(1.0, mode);
        super::opacity_group_tests::pixel(&drawn, (60, 45), [0, 255, 0, 255]);
    }
}
