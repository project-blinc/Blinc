//! What a frame draws for an image: its shadow, its place and its placeholder.
use super::*;

fn state() -> RenderState {
    RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )))
}

/// A frame by each path: direct, with motion, through the window compositor and as an overlay.
fn frame(
    context: &mut RenderContext,
    tree: &RenderTree,
    state: &RenderState,
    mode: u8,
) -> image::RgbaImage {
    if mode < 3 {
        return super::gpu_pass_group_tests::frame(context, tree, state, mode);
    }
    let scale = tree.scale_factor() as u32;
    let (texture, target) = super::gpu_pass_group_tests::target(context, scale);
    context.renderer.clear_target(&target, wgpu::Color::BLUE);
    context
        .render_overlay_tree_with_motion(tree, state, 160 * scale, 120 * scale, &target)
        .unwrap();
    crate::tests::read_rendered_pixels(
        context.device(),
        context.queue(),
        &texture,
        160 * scale,
        120 * scale,
    )
}

fn laid_out(root: Div, scale: u32) -> RenderTree {
    let mut tree = RenderTree::from_element(&root);
    tree.set_scale_factor(scale as f32);
    tree.compute_layout(160.0, 120.0);
    tree
}

fn page() -> Div {
    div().w(160.0).h(120.0).bg(Color::BLUE)
}

fn at(x: f32, y: f32, child: impl ElementBuilder + 'static) -> Div {
    div().absolute().left(x).top(y).child(child)
}

#[test]
#[ignore] // Requires GPU
fn an_image_shadow_is_drawn_once() {
    let shadow = blinc_core::Shadow::new(0.0, 40.0, 0.0, Color::rgba(1.0, 0.0, 0.0, 0.5));
    for mode in 0..4 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let green = super::overlay_group_tests::IMAGE;
        let element = blinc_layout::image(green)
            .size(40.0, 30.0)
            .fade_in(std::time::Duration::ZERO)
            .shadow(shadow);
        let background = div()
            .w(40.0)
            .h(30.0)
            .background(blinc_core::Brush::Image(blinc_core::ImageBrush::new(green)))
            .shadow(shadow);
        let tree = laid_out(
            page()
                .child(at(20.0, 20.0, element))
                .child(at(90.0, 20.0, background)),
            1,
        );
        let drawn = frame(&mut context, &tree, &state(), mode);
        for x in [40, 110] {
            super::opacity_group_tests::pixel(&drawn, (x, 75), [128, 0, 127, 255]);
        }
    }
}
