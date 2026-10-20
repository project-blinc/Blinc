use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::LayerConfig;

fn app() -> BlincApp {
    BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required")
}
fn group() -> PrimitiveBatch {
    let mut ctx = GpuPaintContext::new(160.0, 120.0);
    ctx.set_foreground_layer(true);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    ctx.fill_rect(
        Rect::new(20.0, 20.0, 60.0, 60.0),
        0.0.into(),
        Color::RED.into(),
    );
    ctx.fill_rect(
        Rect::new(40.0, 40.0, 60.0, 60.0),
        0.0.into(),
        Color::GREEN.into(),
    );
    ctx.pop_layer();
    ctx.take_batch()
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
fn foreground_layers_cached_static_and_canvas_keep_isolation() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    for canvas in [false, true] {
        let mut overlay = CanvasOverlay::default();
        if canvas {
            overlay.finish_batches(vec![group()]);
            context.cached_bg_batch = None;
        } else {
            context.cached_bg_batch = Some(group());
        }
        let (texture, view) =
            create_test_texture(context.device(), context.texture_format(), 160, 120);
        context
            .renderer
            .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
        context.render_foreground_overlay(&view, &[&overlay]);
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        pixel(&page, (55, 55), [0, 128, 127, 255]);
        pixel(&page, (30, 30), [128, 0, 127, 255]);
    }
}

#[test]
#[ignore] // Requires GPU
fn foreground_layers_cached_dynamic_reaches_dispatch() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    context.cached_dynamic_batch = Some(group());
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), 160, 120);
    context
        .renderer
        .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
    context.dispatch_dynamic_batch_overlay(&view);
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    pixel(&page, (55, 55), [0, 128, 127, 255]);
}

#[test]
#[ignore] // Requires GPU
fn foreground_layers_nested_paths_clips_and_public_glass_dispatch() {
    use blinc_core::{ClipShape, Path};
    use blinc_gpu::{path::tessellate_fill, primitives::LayerCommand};
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.set_foreground_layer(true);
    paint.push_clip(ClipShape::rect(Rect::new(25.0, 20.0, 75.0, 80.0)));
    paint.push_layer(LayerConfig::new().opacity(0.5));
    paint.fill_rect(
        Rect::new(20.0, 20.0, 60.0, 60.0),
        0.0.into(),
        Color::RED.into(),
    );
    paint.push_layer(LayerConfig::new().opacity(0.5));
    paint.fill_path(
        &Path::rect(Rect::new(40.0, 40.0, 60.0, 60.0)),
        Color::GREEN.into(),
    );
    paint.pop_layer();
    paint.pop_layer();
    paint.pop_clip();
    let mut batch = paint.take_batch();
    // A true tessellated path after the groups exercises foreground path cursors.
    batch.push_foreground_layer_command(LayerCommand::Push {
        config: LayerConfig::new().opacity(0.5),
    });
    batch.push_foreground_path(tessellate_fill(
        &Path::rect(Rect::new(110.0, 20.0, 20.0, 20.0)),
        &Color::WHITE.into(),
    ));
    batch.push_foreground_layer_command(LayerCommand::Pop);
    for route in 0..4 {
        let (texture, view) =
            create_test_texture(context.device(), context.texture_format(), 160, 120);
        let backdrop_texture = context.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("Foreground glass backdrop"),
            size: wgpu::Extent3d {
                width: 80,
                height: 60,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: context.texture_format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let backdrop = backdrop_texture.create_view(&Default::default());
        context
            .renderer
            .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
        match route {
            0 => context.renderer.render_foreground(&view, &batch),
            1 => context
                .renderer
                .render_overlay_msaa(&view, &batch.foreground_batch(), 4),
            2 => context
                .renderer
                .render_glass_frame(&view, &backdrop, (80, 60), &batch, true),
            _ => {
                // The image-aware glass-frame hand-off shares foreground dispatch.
                let mut image_paint = GpuPaintContext::new(160.0, 120.0);
                image_paint.draw_rgba_pixels(
                    &[255, 255, 255, 255],
                    1,
                    1,
                    Rect::new(140.0, 100.0, 5.0, 5.0),
                );
                let mut with_image = image_paint.take_batch();
                with_image.foreground_primitives = batch.foreground_primitives.clone();
                with_image.foreground_paths = batch.foreground_paths.clone();
                with_image.foreground_layer_commands = batch.foreground_layer_commands.clone();
                with_image.aux_data = batch.aux_data.clone();
                context
                    .renderer
                    .render_glass_frame(&view, &backdrop, (80, 60), &with_image, true);
            }
        }
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        pixel(&page, (55, 55), [64, 64, 127, 255]);
        pixel(&page, (30, 30), [128, 0, 127, 255]);
        pixel(&page, (22, 30), [0, 0, 255, 255]);
        pixel(&page, (120, 30), [128, 128, 255, 255]);
    }
}

#[test]
#[ignore] // Requires GPU
fn foreground_layers_cache_patchers_request_full_paint_without_mutation() {
    use blinc_animation::AnimationScheduler;
    let mut app = app();
    let ui = div().w(160.0).h(120.0);
    let mut tree = blinc_layout::RenderTree::from_element(&ui);
    tree.compute_layout(160.0, 120.0);
    let _state = blinc_layout::RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    let context = app.context();
    context.cached_dynamic_batch = Some(group());
    context.cached_bg_batch = Some(group());
    assert!(!context.apply_binding_deltas(&tree, 1.0));
    assert!(!context.redraw_canvases(&tree, 160, 120));
    assert!(matches!(
        context.apply_css_deltas(&tree, 1.0),
        CssDeltaResult::Bail
    ));
    for batch in [
        context.cached_dynamic_batch.as_ref().unwrap(),
        context.cached_bg_batch.as_ref().unwrap(),
    ] {
        assert_eq!(batch.foreground_primitives.len(), 2);
        assert_eq!(batch.foreground_primitives[0].color[3], 1.0);
        assert_eq!(batch.foreground_layer_commands.len(), 2);
    }
}

#[test]
#[ignore] // Requires GPU
fn foreground_layers_application_frames_blend_once_after_canvas_and_glass() {
    use blinc_animation::AnimationScheduler;
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    for (compositor, glass) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut app = app();
        let ui = div()
            .w(160.0)
            .h(120.0)
            .bg(Color::BLUE)
            .child(
                canvas(|ctx, _| {
                    ctx.fill_rect(
                        Rect::new(0.0, 0.0, 10.0, 10.0),
                        0.0.into(),
                        Color::WHITE.into(),
                    );
                })
                .absolute()
                .w(160.0)
                .h(120.0),
            )
            .child(
                div()
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .layer(RenderLayer::Foreground)
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
        let ui = if glass {
            ui.child(
                div()
                    .absolute()
                    .left(140.0)
                    .top(100.0)
                    .w(5.0)
                    .h(5.0)
                    .effect(GlassMaterial::new().blur(0.0)),
            )
        } else {
            ui
        };
        let mut tree = blinc_layout::RenderTree::from_element(&ui);
        tree.compute_layout(160.0, 120.0);
        let state = blinc_layout::RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        for frame in 0..3 {
            let texture = app.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("Foreground compositor surface"),
                size: wgpu::Extent3d {
                    width: 160,
                    height: 120,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: app.texture_format(),
                usage: wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            app.render_tree_with_motion_opt(
                &tree,
                &state,
                &view,
                compositor.then_some(&texture),
                160,
                120,
                frame != 0,
            )
            .unwrap();
            let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
            eprintln!("compositor={compositor},glass={glass},frame={frame}");
            pixel(&page, (55, 55), [0, 128, 127, 255]);
            pixel(&page, (30, 30), [128, 0, 127, 255]);
            pixel(&page, (5, 5), [255, 255, 255, 255]);
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn foreground_layers_motion_opacity_endpoints_request_repaint_and_recover() {
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    for compositor in [false, true] {
        let mut app = app();
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let opacity = Arc::new(Mutex::new(AnimatedValue::with_default(
            scheduler.lock().unwrap().handle(),
            1.0,
        )));
        let ui = div().w(160.0).h(120.0).bg(Color::BLUE).child(
            div()
                .absolute()
                .w(160.0)
                .h(120.0)
                .layer(RenderLayer::Foreground)
                .child(
                    div()
                        .w(160.0)
                        .h(120.0)
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
                        )
                        .motion()
                        .opacity(opacity.clone()),
                ),
        );
        let mut tree = blinc_layout::RenderTree::from_element(&ui);
        tree.compute_layout(160.0, 120.0);
        let state = blinc_layout::RenderState::new(scheduler);
        for (frame, value) in [1.0, 0.5, 0.0, 0.75, 1.0].into_iter().enumerate() {
            opacity.lock().unwrap().set_immediate(value);
            let texture = app.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("Foreground motion surface"),
                size: wgpu::Extent3d {
                    width: 160,
                    height: 120,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: app.texture_format(),
                usage: wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            app.render_tree_with_motion_opt(
                &tree,
                &state,
                &view,
                compositor.then_some(&texture),
                160,
                120,
                frame != 0,
            )
            .unwrap();
            let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
            eprintln!("motion compositor={compositor},frame={frame},opacity={value}");
            pixel(
                &page,
                (55, 55),
                [
                    0,
                    (255.0 * value).round() as u8,
                    (255.0 * (1.0 - value)).round() as u8,
                    255,
                ],
            );
        }
    }
}
