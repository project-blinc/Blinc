//! A layer's translucent content is scaled by its alpha once, by every path that
//! composites a layer.
use super::*;
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{
    BlendMode, Brush, Color, CornerRadius, DrawContext, LayerConfig, LayerEffect, Rect,
};
use blinc_gpu::GpuPaintContext;

/// Layers over black holding white at half alpha, and the grey each leaves.
fn layers() -> [(&'static str, LayerConfig, u8); 4] {
    let mut identity = [0.0; 20];
    for channel in 0..4 {
        identity[channel * 6] = 1.0;
    }
    let unchanged = LayerEffect::ColorMatrix { matrix: identity };
    let normal = LayerConfig::new().opacity(1.0);
    [
        ("opacity", LayerConfig::new().opacity(0.5), 64),
        (
            "effect",
            LayerConfig {
                effects: vec![unchanged.clone()],
                ..normal.clone()
            },
            128,
        ),
        (
            "blend",
            LayerConfig {
                blend_mode: BlendMode::Screen,
                ..normal.clone()
            },
            128,
        ),
        (
            "effect and blend",
            LayerConfig {
                effects: vec![unchanged],
                blend_mode: BlendMode::Screen,
                ..normal
            },
            128,
        ),
    ]
}

#[test]
#[ignore] // Requires GPU
fn translucent_content_in_a_layer_keeps_its_alpha() {
    const SIDE: u32 = 100;
    let mut context = super::main_text_clip_tests::context(true, 1);
    context.renderer.resize(SIDE, SIDE);
    for overlay in [false, true] {
        for (name, config, grey) in layers() {
            let mut paint = GpuPaintContext::new(SIDE as f32, SIDE as f32);
            paint.push_layer(config);
            paint.fill_rect(
                Rect::new(10.0, 10.0, 80.0, 80.0),
                CornerRadius::default(),
                Brush::Solid(Color::rgba(1.0, 1.0, 1.0, 0.5)),
            );
            paint.pop_layer();
            let batch = paint.take_batch();
            let (texture, view) =
                create_test_texture(context.device(), context.texture_format(), SIDE, SIDE);
            if overlay {
                context.renderer.clear_target(&view, wgpu::Color::BLACK);
                context.renderer.render_overlay(&view, &batch);
            } else {
                context
                    .renderer
                    .render_with_clear(&view, &batch, [0.0, 0.0, 0.0, 1.0]);
            }
            let page =
                read_rendered_pixels(context.device(), context.queue(), &texture, SIDE, SIDE);
            let drawn = page.get_pixel(50, 50).0;
            assert!(
                drawn[..3].iter().all(|c| c.abs_diff(grey) <= 2),
                "{name}, overlay {overlay}: {drawn:?}, expected {grey}"
            );
        }
    }
}

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

#[test]
#[ignore] // Requires GPU
fn a_translucent_image_in_a_3d_layer_keeps_its_alpha() {
    for mode in 0..3 {
        let drawn = turned_image(0.5, mode);
        super::opacity_group_tests::pixel(&drawn, (60, 45), [0, 128, 127, 255]);
    }
}
