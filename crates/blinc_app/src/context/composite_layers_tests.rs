use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{LayerConfig, Path};
use blinc_gpu::primitives::LayerCommand;

fn app() -> BlincApp {
    BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required")
}

fn pixel(page: &image::RgbaImage, xy: (u32, u32), expected: [u8; 4]) {
    let actual = page.get_pixel(xy.0, xy.1).0;
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
        "{xy:?}: {actual:?}, expected {expected:?}"
    );
}

#[test]
#[ignore] // Requires GPU
fn composite_layers_bake_isolated_overlap_and_straight_alpha() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_composite_layer(7);
    paint.push_layer(LayerConfig::new().opacity(0.5));
    paint.fill_rect(
        Rect::new(20.0, 20.0, 60.0, 60.0),
        0.0.into(),
        Color::RED.into(),
    );
    paint.fill_rect(
        Rect::new(40.0, 40.0, 60.0, 60.0),
        0.0.into(),
        Color::GREEN.into(),
    );
    paint.pop_layer();
    paint.pop_composite_layer();
    let batch = paint.take_composite_layer_batches().remove(&7).unwrap();
    let (baked, size) =
        context
            .renderer
            .render_subtree_to_layer_texture(&batch, (20.0, 20.0), (80.0, 80.0));
    for opacity in [1.0, 0.5, 0.0, 0.75] {
        let (texture, view) =
            create_test_texture(context.device(), context.texture_format(), 160, 120);
        context
            .renderer
            .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
        context.renderer.blit_tight_texture_to_target(
            &baked.view,
            size,
            &view,
            (20.0, 20.0),
            (80.0, 80.0),
            opacity,
            blinc_core::BlendMode::Normal,
            None,
            None,
            0.0,
        );
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        let alpha = opacity * 0.5;
        pixel(
            &page,
            (30, 30),
            [
                (255.0 * alpha).round() as u8,
                0,
                (255.0 * (1.0 - alpha)).round() as u8,
                255,
            ],
        );
        pixel(
            &page,
            (55, 55),
            [
                0,
                (255.0 * alpha).round() as u8,
                (255.0 * (1.0 - alpha)).round() as u8,
                255,
            ],
        );
        pixel(&page, (5, 5), [0, 0, 255, 255]);
    }
    assert_eq!(baked.size, (80, 80));
    context.renderer.layer_texture_cache_mut().release(baked);
}

#[test]
#[ignore] // Requires GPU
fn composite_layers_bake_path_only_nested_ranges() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    let mut batch = PrimitiveBatch::new();
    for _ in 0..2 {
        batch.push_layer_command(LayerCommand::Push {
            config: LayerConfig::new().opacity(0.5),
        });
    }
    batch.push_path(blinc_gpu::path::tessellate_fill(
        &Path::rect(Rect::new(40.0, 40.0, 40.0, 40.0)),
        &Color::GREEN.into(),
    ));
    for _ in 0..2 {
        batch.push_layer_command(LayerCommand::Pop);
    }
    let (baked, size) =
        context
            .renderer
            .render_subtree_to_layer_texture(&batch, (20.0, 20.0), (80.0, 80.0));
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), 160, 120);
    context
        .renderer
        .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
    context.renderer.blit_tight_texture_to_target(
        &baked.view,
        size,
        &view,
        (20.0, 20.0),
        (80.0, 80.0),
        1.0,
        blinc_core::BlendMode::Normal,
        None,
        None,
        0.0,
    );
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    pixel(&page, (55, 55), [0, 64, 191, 255]);
    pixel(&page, (25, 25), [0, 0, 255, 255]);
    assert_eq!(baked.size, (80, 80));
    context.renderer.layer_texture_cache_mut().release(baked);
}

#[test]
#[ignore] // Requires GPU
fn composite_layers_promoted_walker_bakes_base_groups_and_blits_live_opacity_once() {
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    use slotmap::Key as _;
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let mut app = app();
    let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
    let opacity = Arc::new(Mutex::new(AnimatedValue::with_default(
        scheduler.lock().unwrap().handle(),
        0.5,
    )));
    opacity.lock().unwrap().set_target(1.0);
    let ui = div()
        .w(160.0)
        .h(120.0)
        .child(
            div()
                .w(120.0)
                .h(120.0)
                .opacity(0.5)
                .child(
                    div()
                        .absolute()
                        .left(20.0)
                        .top(20.0)
                        .w(60.0)
                        .h(60.0)
                        .bg(Color::RED),
                )
                .child(
                    div()
                        .absolute()
                        .left(40.0)
                        .top(40.0)
                        .w(60.0)
                        .h(60.0)
                        .bg(Color::GREEN),
                ),
        )
        .motion()
        .opacity(opacity.clone());
    let mut tree = blinc_layout::RenderTree::from_element(&ui);
    tree.compute_layout(160.0, 120.0);
    tree.compute_animation_status();
    let root = tree.root().unwrap();
    assert!(tree.is_subtree_texture_candidate(root));
    let state = blinc_layout::RenderState::new(scheduler);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    tree.render_with_motion(&mut paint, &state);
    assert!(tree.composite_bindings().is_empty());
    assert!(paint.take_batch().layer_commands.is_empty());
    assert!(paint.take_dynamic_batch().layer_commands.is_empty());
    let batch = paint
        .take_composite_layer_batches()
        .remove(&root.data().as_ffi())
        .unwrap();
    assert_eq!(batch.layer_commands.len(), 2);
    assert!(batch.primitives.iter().all(|p| p.color[3] == 1.0));
    let region = tree.dynamic_regions().get(&root).unwrap().clone();
    let blinc_layout::renderer::DynamicKind::MotionSubtreeTexture { natural_size, .. } =
        region.kind
    else {
        panic!("promotion missing")
    };
    let context = app.context();
    context.renderer.resize(160, 120);
    let (baked, size) = context.renderer.render_subtree_to_layer_texture(
        &batch,
        (region.screen_aabb[0], region.screen_aabb[1]),
        (natural_size.0 as f32, natural_size.1 as f32),
    );
    context
        .motion_subtree_textures
        .insert(root.data().as_ffi(), (baked, size));
    let hash = RenderContext::hash_composite_scratch(&batch);
    for value in [0.5, 0.0, 0.75, 1.0] {
        opacity.lock().unwrap().set_immediate(value);
        let (texture, view) =
            create_test_texture(context.device(), context.texture_format(), 160, 120);
        context
            .renderer
            .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
        context.composite_motion_layers_overlay(&tree, &view);
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        let alpha = value * 0.5;
        pixel(
            &page,
            (55, 55),
            [
                0,
                (255.0 * alpha).round() as u8,
                (255.0 * (1.0 - alpha)).round() as u8,
                255,
            ],
        );
        let mut next = GpuPaintContext::new(160.0, 120.0);
        tree.render_with_motion(&mut next, &state);
        let next = next
            .take_composite_layer_batches()
            .remove(&root.data().as_ffi())
            .unwrap();
        assert_eq!(
            hash,
            RenderContext::hash_composite_scratch(&next),
            "live root opacity must not dirty the bake"
        );
    }
}

#[test]
#[ignore] // Requires GPU
fn composite_layers_scratch_css_indices_request_full_repaint() {
    use blinc_layout::renderer::CssAnimPaintMeta;
    let mut app = app();
    let tree = blinc_layout::RenderTree::from_element(&div().id("scratch-css-record"));
    let root = tree.root().unwrap();
    tree.css_anim_paint_records_mut().insert(
        root,
        CssAnimPaintMeta {
            dynamic_batch: false,
            composite_batch: true,
            stable_id: tree.stable_id(root).unwrap(),
            primitive_range: 0..1,
            layer_push_index: Some(0),
            last_opacity: 0.5,
            last_translate: (0.0, 0.0),
            last_scale: (1.0, 1.0),
            last_rotation_rad: 0.0,
            last_rotate_x_rad: 0.0,
            last_rotate_y_rad: 0.0,
            last_background_color: None,
            last_border_color: None,
            last_corner_radius: [0.0; 4],
            last_border_width: 0.0,
            last_shadow_params: [0.0; 4],
            last_shadow_color: [0.0; 4],
            last_filter_a: [0.0; 4],
            last_filter_b: [1.0, 1.0, 1.0, 0.0],
            centre: (50.0, 50.0),
            last_screen_aabb: None,
        },
    );
    let context = app.context();
    let mut batch = PrimitiveBatch::new();
    batch.push(blinc_gpu::GpuPrimitive::rect(0.0, 0.0, 10.0, 10.0));
    let original = batch.primitives[0];
    context.cached_bg_batch = Some(batch);
    assert!(matches!(
        context.apply_css_deltas(&tree, 1.0),
        CssDeltaResult::Bail
    ));
    assert_eq!(
        bytemuck::bytes_of(&context.cached_bg_batch.as_ref().unwrap().primitives[0]),
        bytemuck::bytes_of(&original)
    );
}

#[test]
#[ignore] // Requires GPU
fn composite_layers_css_promoted_root_opacity_stays_on_live_blit() {
    use blinc_animation::{AnimationScheduler, MultiKeyframeAnimation};
    use blinc_layout::render_state::ActiveCssAnimation;
    use slotmap::Key as _;
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let mut app = app();
    let ui = div()
        .id("css-composite")
        .w(160.0)
        .h(120.0)
        .opacity(0.5)
        .child(
            div()
                .w(120.0)
                .h(120.0)
                .opacity(0.5)
                .child(
                    div()
                        .absolute()
                        .left(20.0)
                        .top(20.0)
                        .w(60.0)
                        .h(60.0)
                        .bg(Color::RED),
                )
                .child(
                    div()
                        .absolute()
                        .left(40.0)
                        .top(40.0)
                        .w(60.0)
                        .h(60.0)
                        .bg(Color::GREEN),
                ),
        );
    let mut tree = blinc_layout::RenderTree::from_element(&ui);
    tree.compute_layout(160.0, 120.0);
    let root = tree.root().unwrap();
    let stable = tree.stable_id(root).unwrap();
    let mut animation = ActiveCssAnimation::new(MultiKeyframeAnimation::new(1000));
    animation.current_properties.opacity = Some(0.5);
    tree.css_anim_store()
        .lock()
        .unwrap()
        .animations
        .insert(stable, animation);
    tree.compute_animation_status();
    assert!(tree.composite_promotion().contains(&root));
    let state = blinc_layout::RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    tree.render_with_motion(&mut paint, &state);
    assert!(paint.take_batch().layer_commands.is_empty());
    let batch = paint
        .take_composite_layer_batches()
        .remove(&root.data().as_ffi())
        .unwrap();
    assert_eq!(batch.layer_commands.len(), 2);
    assert!(batch.primitives.iter().all(|p| p.color[3] == 1.0));
    let region = tree.dynamic_regions().get(&root).unwrap().clone();
    let blinc_layout::renderer::DynamicKind::CssAnimated { natural_size, .. } = region.kind else {
        panic!("CSS promotion missing")
    };
    let context = app.context();
    context.renderer.resize(160, 120);
    let baked = context.renderer.render_subtree_to_layer_texture(
        &batch,
        (region.screen_aabb[0], region.screen_aabb[1]),
        (natural_size.0 as f32, natural_size.1 as f32),
    );
    context
        .css_composited_textures
        .insert(root.data().as_ffi(), baked);
    let hash = RenderContext::hash_composite_scratch(&batch);
    for value in [0.5, 0.0, 0.75, 1.0] {
        tree.css_anim_store()
            .lock()
            .unwrap()
            .animations
            .get_mut(&stable)
            .unwrap()
            .current_properties
            .opacity = Some(value);
        let (texture, view) =
            create_test_texture(context.device(), context.texture_format(), 160, 120);
        context
            .renderer
            .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
        context.composite_css_layers_overlay(&tree, &view);
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        let alpha = value * 0.5;
        pixel(
            &page,
            (55, 55),
            [
                0,
                (255.0 * alpha).round() as u8,
                (255.0 * (1.0 - alpha)).round() as u8,
                255,
            ],
        );
        let mut next = GpuPaintContext::new(160.0, 120.0);
        tree.render_with_motion(&mut next, &state);
        let next = next
            .take_composite_layer_batches()
            .remove(&root.data().as_ffi())
            .unwrap();
        assert_eq!(hash, RenderContext::hash_composite_scratch(&next));
    }
}

#[test]
#[ignore] // Requires GPU
fn composite_layers_rebake_effect_changes_and_preserve_mesh_clips() {
    use blinc_core::{ClipShape, LayerEffect, Point};
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_composite_layer(7);
    paint.push_clip(ClipShape::Polygon(vec![
        Point::new(10.0, 10.0),
        Point::new(70.0, 10.0),
        Point::new(10.0, 70.0),
    ]));
    paint.push_layer(LayerConfig::new().opacity(0.5));
    paint.fill_path(
        &Path::rect(Rect::new(20.0, 20.0, 80.0, 80.0)),
        Color::RED.into(),
    );
    paint.pop_layer();
    paint.pop_clip();
    paint.pop_composite_layer();
    let mut batch = paint.take_composite_layer_batches().remove(&7).unwrap();
    let original = RenderContext::hash_composite_scratch(&batch);
    for green in [false, true] {
        if green {
            let LayerCommand::Push { config } = &mut batch.layer_commands[0].command else {
                unreachable!()
            };
            config.effects = vec![LayerEffect::ColorMatrix {
                matrix: [
                    0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                    0.0, 0.0, 1.0, 0.0,
                ],
            }];
            assert_ne!(original, RenderContext::hash_composite_scratch(&batch));
        }
        let (baked, size) =
            context
                .renderer
                .render_subtree_to_layer_texture(&batch, (20.0, 20.0), (80.0, 80.0));
        let (texture, view) =
            create_test_texture(context.device(), context.texture_format(), 160, 120);
        context
            .renderer
            .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
        context.renderer.blit_tight_texture_to_target(
            &baked.view,
            size,
            &view,
            (20.0, 20.0),
            (80.0, 80.0),
            1.0,
            blinc_core::BlendMode::Normal,
            None,
            None,
            0.0,
        );
        // The bake must restore screen-space uniforms and auxiliary coordinates.
        context.renderer.render_overlay_msaa(&view, &batch, 1);
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        pixel(
            &page,
            (40, 40),
            if green {
                [0, 191, 64, 255]
            } else {
                [191, 0, 64, 255]
            },
        );
        pixel(&page, (80, 80), [0, 0, 255, 255]);
        pixel(&page, (25, 25), [0, 0, 255, 255]);
        context.renderer.layer_texture_cache_mut().release(baked);
    }
}

#[test]
#[ignore] // Requires GPU
fn composite_layers_promoted_region_preserves_nested_blur_halo() {
    use blinc_animation::{AnimationScheduler, MultiKeyframeAnimation};
    use blinc_layout::render_state::ActiveCssAnimation;
    use slotmap::Key as _;
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let mut app = app();
    let ui = div().id("blurred-composite").w(160.0).h(120.0).child(
        div()
            .absolute()
            .left(40.0)
            .top(40.0)
            .w(40.0)
            .h(40.0)
            .bg(Color::RED)
            .blur(4.0),
    );
    let mut tree = blinc_layout::RenderTree::from_element(&ui);
    tree.compute_layout(160.0, 120.0);
    let root = tree.root().unwrap();
    let mut animation = ActiveCssAnimation::new(MultiKeyframeAnimation::new(1000));
    animation.current_properties.opacity = Some(0.5);
    tree.css_anim_store()
        .lock()
        .unwrap()
        .animations
        .insert(tree.stable_id(root).unwrap(), animation);
    tree.compute_animation_status();
    let state = blinc_layout::RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    tree.render_with_motion(&mut paint, &state);
    let batch = paint
        .take_composite_layer_batches()
        .remove(&root.data().as_ffi())
        .unwrap();
    let region = tree.dynamic_regions().get(&root).unwrap().clone();
    assert!(region.screen_aabb[0] < 40.0 && region.screen_aabb[1] < 40.0);
    let blinc_layout::renderer::DynamicKind::CssAnimated { natural_size, .. } = region.kind else {
        panic!("promotion missing")
    };
    let context = app.context();
    context.renderer.resize(160, 120);
    let baked = context.renderer.render_subtree_to_layer_texture(
        &batch,
        (region.screen_aabb[0], region.screen_aabb[1]),
        (natural_size.0 as f32, natural_size.1 as f32),
    );
    context
        .css_composited_textures
        .insert(root.data().as_ffi(), baked);
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), 160, 120);
    context
        .renderer
        .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
    context.composite_css_layers_overlay(&tree, &view);
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    let halo = page.get_pixel(38, 55).0;
    assert!(halo[0] > 0 && halo[2] < 255, "blur halo cropped: {halo:?}");
    pixel(&page, (5, 5), [0, 0, 255, 255]);
}
