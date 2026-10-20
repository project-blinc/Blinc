use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{LayerConfig, Path};

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
fn image(paint: &mut GpuPaintContext<'_>, xy: f32, color: [u8; 4]) {
    paint.draw_rgba_pixels(&color, 1, 1, Rect::new(xy, xy, 60.0, 60.0));
}

#[test]
#[ignore] // Requires GPU
fn rgba_order_images_do_not_cover_later_shapes_or_paths() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    for path in [false, true] {
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        image(&mut paint, 20.0, [255, 0, 0, 255]);
        if path {
            paint.fill_path(
                &Path::rect(Rect::new(40.0, 40.0, 60.0, 60.0)),
                Color::GREEN.into(),
            );
        } else {
            paint.fill_rect(
                Rect::new(40.0, 40.0, 60.0, 60.0),
                0.0.into(),
                Color::GREEN.into(),
            );
        }
        let batch = paint.take_batch();
        let (texture, view) =
            create_test_texture(context.device(), context.texture_format(), 160, 120);
        context
            .renderer
            .render_with_clear(&view, &batch, [0.0, 0.0, 1.0, 1.0]);
        context
            .renderer
            .render_unordered_dynamic_images(&view, &batch.dynamic_images);
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        pixel(&page, (55, 55), [0, 255, 0, 255]);
        pixel(&page, (30, 30), [255, 0, 0, 255]);
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_order_dynamic_image_only_group_reaches_cached_dispatch() {
    let mut app = app();
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_motion_subtree();
    paint.push_layer(LayerConfig::new().opacity(0.5));
    image(&mut paint, 20.0, [255, 0, 0, 255]);
    image(&mut paint, 40.0, [0, 255, 0, 255]);
    paint.pop_layer();
    paint.pop_motion_subtree();
    let context = app.context();
    context.renderer.resize(160, 120);
    context.cached_dynamic_batch = Some(paint.take_dynamic_batch());
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
fn rgba_order_public_routes_keep_mixed_nested_groups_and_path_runs() {
    use blinc_gpu::primitives::LayerCommand;
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_layer(LayerConfig::new().opacity(0.5));
    image(&mut paint, 20.0, [255, 0, 0, 255]);
    paint.push_layer(LayerConfig::new().opacity(0.5));
    image(&mut paint, 40.0, [0, 255, 0, 255]);
    paint.pop_layer();
    paint.pop_layer();
    let mut batch = paint.take_batch();
    // A true path run after the image-only group must stay outside its alpha.
    batch.push_path(blinc_gpu::path::tessellate_fill(
        &Path::rect(Rect::new(110.0, 20.0, 20.0, 20.0)),
        &Color::WHITE.into(),
    ));
    assert!(matches!(batch.layer_commands[3].command, LayerCommand::Pop));
    for route in 0..5 {
        let (texture, view) =
            create_test_texture(context.device(), context.texture_format(), 160, 120);
        context
            .renderer
            .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
        match route {
            0 => context
                .renderer
                .render_with_clear(&view, &batch, [0.0, 0.0, 1.0, 1.0]),
            1 => context.renderer.render_overlay(&view, &batch),
            2 | 3 => {
                context
                    .renderer
                    .render_overlay_msaa(&view, &batch, if route == 2 { 1 } else { 4 })
            }
            _ => {
                let msaa = context.device().create_texture(&wgpu::TextureDescriptor {
                    label: Some("RGBA MSAA control"),
                    size: wgpu::Extent3d {
                        width: 160,
                        height: 120,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 4,
                    dimension: wgpu::TextureDimension::D2,
                    format: context.texture_format(),
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                context.renderer.render_msaa(
                    &msaa.create_view(&Default::default()),
                    &view,
                    &batch,
                    [0.0, 0.0, 1.0, 1.0],
                );
            }
        }
        // The old explicit image API still draws exactly the supplied sequence.
        // Application dispatch filters already-consumed background records.
        context
            .renderer
            .render_unordered_dynamic_images(&view, &batch.dynamic_images);
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        pixel(&page, (55, 55), [64, 64, 127, 255]);
        pixel(&page, (30, 30), [128, 0, 127, 255]);
        pixel(&page, (120, 30), [255, 255, 255, 255]);
        pixel(&page, (5, 5), [0, 0, 255, 255]);
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_order_bakes_image_only_scratch_and_rebakes_changed_pixels() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_composite_layer(7);
    paint.push_layer(LayerConfig::new().opacity(0.5));
    image(&mut paint, 20.0, [255, 0, 0, 255]);
    image(&mut paint, 40.0, [0, 255, 0, 255]);
    paint.pop_layer();
    let bounds = paint.composite_layer_bounds().unwrap();
    assert_eq!(bounds, [20.0, 20.0, 80.0, 80.0]);
    paint.pop_composite_layer();
    assert!(paint.take_batch().dynamic_images.is_empty());
    let mut batch = paint.take_composite_layer_batches().remove(&7).unwrap();
    let hash = RenderContext::hash_composite_scratch(&batch);
    for changed in [false, true] {
        if changed {
            batch.dynamic_images[1].data = vec![255, 255, 0, 255];
            assert_ne!(hash, RenderContext::hash_composite_scratch(&batch));
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
            0.5,
            blinc_core::BlendMode::Normal,
            None,
            None,
            0.0,
        );
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        pixel(
            &page,
            (55, 55),
            if changed {
                [64, 64, 191, 255]
            } else {
                [0, 64, 191, 255]
            },
        );
        context.renderer.layer_texture_cache_mut().release(baked);
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_order_rectangles_empty_clips_and_unclipped_draws_keep_their_extent() {
    use blinc_core::ClipShape;
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_clip(ClipShape::rect(Rect::new(30.0, 20.0, 30.0, 50.0)));
    paint.push_layer(LayerConfig::new().opacity(0.5));
    image(&mut paint, 20.0, [255, 0, 0, 255]);
    paint.pop_layer();
    paint.push_clip(ClipShape::rect(Rect::new(110.0, 20.0, 10.0, 10.0)));
    paint.draw_rgba_pixels(
        &[255, 255, 255, 255],
        1,
        1,
        Rect::new(0.0, 0.0, 160.0, 120.0),
    );
    paint.pop_clip();
    paint.pop_clip();
    paint.draw_rgba_pixels(&[0, 255, 0, 255], 1, 1, Rect::new(90.0, 60.0, 30.0, 30.0));
    let batch = paint.take_batch();
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), 160, 120);
    context
        .renderer
        .render_with_clear(&view, &batch, [0.0, 0.0, 1.0, 1.0]);
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    pixel(&page, (35, 35), [128, 0, 127, 255]);
    pixel(&page, (65, 35), [0, 0, 255, 255]);
    pixel(&page, (25, 25), [0, 0, 255, 255]);
    pixel(&page, (100, 70), [0, 255, 0, 255]);
    pixel(&page, (5, 5), [0, 0, 255, 255]);
}

#[test]
#[ignore] // Requires GPU
fn rgba_order_canvas_frames_preserve_visit_order_updates_and_foreground_meshes() {
    use blinc_animation::AnimationScheduler;
    use std::sync::atomic::{AtomicUsize, Ordering};
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    for (compositor, glass) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut app = app();
        let changed = Arc::new(AtomicUsize::new(0));
        let pixels = changed.clone();
        let first = changed.clone();
        let ui = div()
            .w(160.0)
            .h(120.0)
            .bg(Color::BLUE)
            .child(
                canvas(move |ctx, _| {
                    ctx.push_layer(LayerConfig::new().opacity(0.5));
                    if first.load(Ordering::Relaxed) < 2 {
                        ctx.draw_rgba_pixels(
                            &[255, 0, 0, 255],
                            1,
                            1,
                            Rect::new(20.0, 20.0, 60.0, 60.0),
                        );
                        ctx.draw_rgba_pixels(
                            &[0, 255, 0, 255],
                            1,
                            1,
                            Rect::new(40.0, 40.0, 60.0, 60.0),
                        );
                    } else {
                        ctx.fill_rect(
                            Rect::new(20.0, 20.0, 60.0, 60.0),
                            0.0.into(),
                            Color::RED.into(),
                        );
                    }
                    ctx.pop_layer();
                })
                .absolute()
                .w(160.0)
                .h(120.0),
            )
            .child(
                canvas(move |ctx, _| {
                    let color = if pixels.load(Ordering::Relaxed) == 1 {
                        [255, 0, 255, 255]
                    } else {
                        [255, 255, 0, 255]
                    };
                    if pixels.load(Ordering::Relaxed) < 2 {
                        ctx.draw_rgba_pixels(&color, 1, 1, Rect::new(50.0, 50.0, 10.0, 10.0));
                        ctx.set_z_layer(2);
                        ctx.fill_rect(
                            Rect::new(60.0, 60.0, 10.0, 10.0),
                            0.0.into(),
                            Color::rgba(1.0, 1.0, 1.0, 0.5).into(),
                        );
                    }
                    ctx.fill_path(
                        &Path::rect(Rect::new(110.0, 20.0, 20.0, 20.0)),
                        Color::WHITE.into(),
                    );
                    ctx.set_foreground_layer(true);
                    ctx.fill_path(
                        &Path::rect(Rect::new(5.0, 5.0, 10.0, 10.0)),
                        Color::WHITE.into(),
                    );
                    ctx.set_foreground_layer(false);
                })
                .absolute()
                .w(160.0)
                .h(120.0),
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
            changed.store(frame, Ordering::Relaxed);
            let texture = app.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("RGBA compositor surface"),
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
            pixel(
                &page,
                (55, 55),
                if frame == 0 {
                    [255, 255, 0, 255]
                } else if frame == 1 {
                    [255, 0, 255, 255]
                } else {
                    [128, 0, 127, 255]
                },
            );
            pixel(&page, (30, 30), [128, 0, 127, 255]);
            pixel(
                &page,
                (75, 75),
                if frame < 2 {
                    [0, 128, 127, 255]
                } else {
                    [128, 0, 127, 255]
                },
            );
            pixel(
                &page,
                (65, 65),
                if frame < 2 {
                    [128, 191, 191, 255]
                } else {
                    [128, 0, 127, 255]
                },
            );
            pixel(&page, (120, 30), [255, 255, 255, 255]);
            assert_eq!(
                page.get_pixel(8, 8).0,
                [255u8; 4],
                "foreground: compositor={compositor}, glass={glass}, frame={frame}"
            );
            let records = tree.canvas_paint_records();
            assert_eq!(records.len(), 2);
            let mut records = records.values().collect::<Vec<_>>();
            records.sort_by_key(|r| r.visit_seq);
            if frame < 2 {
                assert_eq!(
                    records[0].primitive_range.start,
                    records[0].primitive_range.end
                );
            }
            assert!(records[0].visit_seq < records[1].visit_seq);
            if frame < 2 {
                assert_eq!(
                    records[0].primitive_range.start,
                    records[1].primitive_range.start
                );
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_order_primitive_cache_patchers_bail_before_mutation() {
    let mut app = app();
    let tree = blinc_layout::RenderTree::from_element(&div().id("rgba-cache-guard"));
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    image(&mut paint, 20.0, [255, 0, 0, 255]);
    paint.fill_rect(
        Rect::new(40.0, 40.0, 60.0, 60.0),
        0.0.into(),
        Color::GREEN.into(),
    );
    let batch = paint.take_batch();
    let hash = RenderContext::hash_composite_scratch(&batch);
    let context = app.context();
    context.cached_bg_batch = Some(batch.clone());
    context.cached_dynamic_batch = Some(batch);
    assert!(!context.redraw_canvases(&tree, 160, 120));
    assert!(matches!(
        context.apply_css_deltas(&tree, 1.0),
        CssDeltaResult::Bail
    ));
    assert!(!context.apply_binding_deltas(&tree, 1.0));
    assert_eq!(
        hash,
        RenderContext::hash_composite_scratch(context.cached_bg_batch.as_ref().unwrap())
    );
    assert_eq!(
        hash,
        RenderContext::hash_composite_scratch(context.cached_dynamic_batch.as_ref().unwrap())
    );
}

#[test]
#[ignore] // Requires GPU
fn rgba_order_downsampled_backdrop_and_explicit_image_api_preserve_alpha() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_layer(LayerConfig::new().opacity(0.5));
    image(&mut paint, 20.0, [255, 0, 0, 255]);
    paint.pop_layer();
    paint.set_foreground_layer(true);
    paint.fill_path(
        &Path::rect(Rect::new(110.0, 20.0, 20.0, 20.0)),
        Color::WHITE.into(),
    );
    paint.set_foreground_layer(false);
    let batch = paint.take_batch();
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), 80, 60);
    context
        .renderer
        .render_to_backdrop(&view, (80, 60), &batch, false);
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, 80, 60);
    pixel(&page, (20, 20), [128, 0, 0, 255]);
    pixel(&page, (2, 2), [0, 0, 0, 255]);
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), 160, 120);
    context
        .renderer
        .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
    context
        .renderer
        .render_dynamic_images(&view, &batch.dynamic_images);
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    // The explicit API renders the supplied image, with group composition left to its caller.
    pixel(&page, (30, 30), [255, 0, 0, 255]);
    for preserve in [false, true] {
        context.renderer.clear_target(&view, wgpu::Color::BLUE);
        let (backdrop, backdrop_view) =
            create_test_texture(context.device(), context.texture_format(), 80, 60);
        context
            .renderer
            .clear_target(&backdrop_view, wgpu::Color::BLUE);
        context
            .renderer
            .render_glass_frame(&view, &backdrop_view, (80, 60), &batch, preserve);
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        pixel(&page, (120, 30), [255, 255, 255, 255]);
        pixel(
            &page,
            (30, 30),
            if preserve {
                [128, 0, 127, 255]
            } else {
                [128, 0, 0, 255]
            },
        );
        let page = read_rendered_pixels(context.device(), context.queue(), &backdrop, 80, 60);
        pixel(
            &page,
            (20, 20),
            if preserve {
                [128, 0, 127, 255]
            } else {
                [128, 0, 0, 255]
            },
        );
    }
}
