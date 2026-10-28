//! Named native layers replay within their ordered paint pass.
use super::*;
use blinc_core::{DrawContext, LayerConfig, LayerId, Point, Rect, Size};

fn target(context: &RenderContext) -> wgpu::Texture {
    context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Named layer test"),
        size: wgpu::Extent3d {
            width: 160,
            height: 120,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: context.texture_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn source(paint: &mut dyn DrawContext, id: LayerId, color: Color, opacity: f32) {
    paint.push_layer(
        LayerConfig::new()
            .id(id)
            .position(Point::new(8.0, 8.0))
            .size(Size::new(32.0, 32.0))
            .opacity(opacity),
    );
    paint.fill_rect(Rect::new(8.0, 8.0, 32.0, 32.0), 0.0.into(), color.into());
    paint.pop_layer();
}

#[test]
#[ignore] // Requires GPU
fn named_layer_sample_copies_opaque_output_in_paint_order() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    source(&mut paint, LayerId(7), Color::RED, 1.0);
    paint.sample_layer(
        LayerId(7),
        Rect::new(0.0, 0.0, 32.0, 32.0),
        Rect::new(72.0, 8.0, 32.0, 32.0),
    );
    paint.fill_rect(
        Rect::new(88.0, 8.0, 8.0, 32.0),
        0.0.into(),
        Color::GREEN.into(),
    );
    let mut batch = paint.take_batch();
    for samples in [1, 4] {
        for _ in 0..2 {
            let page = frame(&mut context, &batch, samples);
            pixel(&page, (20, 20), [255, 0, 0, 255]);
            pixel(&page, (84, 20), [255, 0, 0, 255]);
            pixel(&page, (92, 20), [0, 255, 0, 255]);
        }
    }
    for entry in &mut batch.layer_commands {
        if let blinc_gpu::primitives::LayerCommand::Sample { paint, .. } = &mut entry.command {
            *paint = None;
        }
    }
    pixel(&frame(&mut context, &batch, 1), (84, 20), [255, 0, 0, 255]);
}

fn frame(context: &mut RenderContext, batch: &PrimitiveBatch, samples: u32) -> image::RgbaImage {
    let target = target(context);
    let view = target.create_view(&Default::default());
    context.renderer.resize(160, 120);
    if samples == 1 {
        context
            .renderer
            .render_with_clear(&view, batch, [0.0, 0.0, 1.0, 1.0]);
    } else {
        context
            .renderer
            .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
        context.renderer.render_overlay_msaa(&view, batch, samples);
    }
    crate::tests::read_rendered_pixels(context.device(), context.queue(), &target, 160, 120)
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
fn named_layer_sample_keeps_motion_foreground_scratch_and_shared_owners() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    for route in 0..5 {
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        if route == 1 {
            paint.push_motion_subtree();
        }
        if route == 2 {
            paint.push_composite_layer(99);
        }
        if route == 3 {
            paint.set_foreground_layer(true);
        }
        if route == 4 {
            assert!(paint.push_cross_stream_layer(17, LayerConfig::new().opacity(0.5)));
        }
        source(&mut paint, LayerId(7), Color::RED, 1.0);
        if route == 4 {
            paint.set_foreground_layer(true);
        }
        paint.sample_layer(
            LayerId(7),
            Rect::new(0.0, 0.0, 32.0, 32.0),
            Rect::new(72.0, 8.0, 32.0, 32.0),
        );
        if route == 1 {
            paint.pop_motion_subtree();
        }
        if route == 2 {
            paint.pop_composite_layer();
        }
        if route == 4 {
            paint.pop_layer();
        }
        let mut batch = match route {
            1 => paint.take_dynamic_batch(),
            2 => paint.take_composite_layer_batches().remove(&99).unwrap(),
            _ => paint.take_batch(),
        };
        if route == 3 {
            batch = batch.take_foreground();
        }
        let expected = if route == 4 {
            [128, 0, 127, 255]
        } else {
            [255, 0, 0, 255]
        };
        for samples in [1, 4] {
            let page = frame(&mut context, &batch, samples);
            pixel(&page, (20, 20), expected);
            pixel(&page, (84, 20), expected);
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn named_layer_sample_uses_effect_output_crop_and_nested_destination_state() {
    use blinc_core::{ClipShape, LayerEffect, Path, Transform};
    let mut context = super::main_text_clip_tests::context(true, 1);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_layer(
        LayerConfig::new()
            .id(LayerId(7))
            .position(Point::new(8.0, 8.0))
            .size(Size::new(32.0, 32.0))
            .opacity(0.5)
            .effect(LayerEffect::blur(0.0)),
    );
    paint.fill_path(
        &Path::rect(Rect::new(8.0, 8.0, 16.0, 32.0)),
        Color::RED.into(),
    );
    paint.draw_rgba_pixels(&[0, 255, 0, 255], 1, 1, Rect::new(24.0, 8.0, 16.0, 32.0));
    paint.pop_layer();
    paint.push_layer(LayerConfig::new().opacity(0.5));
    paint.push_opacity(0.5);
    paint.push_transform(Transform::translate(72.0, 48.0));
    paint.push_clip(ClipShape::Circle {
        center: Point::new(16.0, 16.0),
        radius: 12.0,
    });
    paint.sample_layer(
        LayerId(7),
        Rect::new(16.0, 0.0, 16.0, 32.0),
        Rect::new(0.0, 0.0, 32.0, 32.0),
    );
    paint.pop_clip();
    paint.pop_transform();
    paint.pop_opacity();
    paint.pop_layer();
    let batch = paint.take_batch();
    for samples in [1, 4] {
        let page = frame(&mut context, &batch, samples);
        pixel(&page, (16, 20), [128, 0, 127, 255]);
        pixel(&page, (32, 20), [0, 128, 127, 255]);
        pixel(&page, (88, 64), [0, 32, 223, 255]);
        pixel(&page, (74, 50), [0, 0, 255, 255]);
    }
}

#[test]
#[ignore] // Requires GPU
fn named_layer_sample_replaces_zero_outputs_and_never_reuses_another_pass() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.sample_layer(
        LayerId(7),
        Rect::new(0.0, 0.0, 32.0, 32.0),
        Rect::new(72.0, 8.0, 32.0, 32.0),
    );
    source(&mut paint, LayerId(7), Color::RED, 1.0);
    paint.sample_layer(
        LayerId(7),
        Rect::new(0.0, 0.0, 32.0, 32.0),
        Rect::new(72.0, 48.0, 32.0, 32.0),
    );
    source(&mut paint, LayerId(7), Color::GREEN, 1.0);
    paint.sample_layer(
        LayerId(7),
        Rect::new(0.0, 0.0, 32.0, 32.0),
        Rect::new(112.0, 8.0, 32.0, 32.0),
    );
    source(&mut paint, LayerId(7), Color::RED, 0.0);
    paint.sample_layer(
        LayerId(7),
        Rect::new(0.0, 0.0, 32.0, 32.0),
        Rect::new(112.0, 48.0, 32.0, 32.0),
    );
    let batch = paint.take_batch();
    for samples in [1, 4] {
        let page = frame(&mut context, &batch, samples);
        pixel(&page, (84, 20), [0, 0, 255, 255]);
        pixel(&page, (84, 60), [255, 0, 0, 255]);
        pixel(&page, (124, 20), [0, 255, 0, 255]);
        pixel(&page, (124, 60), [0, 0, 255, 255]);
    }
    source(&mut paint, LayerId(7), Color::RED, 1.0);
    frame(&mut context, &paint.take_batch(), 1);
    paint.sample_layer(
        LayerId(7),
        Rect::new(0.0, 0.0, 32.0, 32.0),
        Rect::new(72.0, 8.0, 32.0, 32.0),
    );
    let batch = paint.take_batch();
    assert!(!batch.is_empty());
    let page = frame(&mut context, &batch, 1);
    pixel(&page, (84, 20), [0, 0, 255, 255]);
}

#[test]
#[ignore] // Requires GPU
fn named_layer_sample_preserves_offscreen_margins_and_pool_independence() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    context.renderer.resize(160, 120);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_layer(
        LayerConfig::new()
            .id(LayerId(7))
            .position(Point::new(200.25, 8.5))
            .size(Size::new(32.0, 32.0))
            .opacity(1.0),
    );
    paint.fill_rect(
        Rect::new(208.25, 16.5, 16.0, 16.0),
        0.0.into(),
        Color::RED.into(),
    );
    paint.pop_layer();
    paint.sample_layer(
        LayerId(7),
        Rect::new(0.0, 0.0, 32.0, 32.0),
        Rect::new(72.0, 8.0, 32.0, 32.0),
    );
    let batch = paint.take_batch();
    let first = frame(&mut context, &batch, 1);
    pixel(&first, (88, 24), [255, 0, 0, 255]);
    pixel(&first, (74, 10), [0, 0, 255, 255]);
    context.renderer.layer_texture_cache_mut().clear_all();
    let device = context.device().clone();
    let large = context
        .renderer
        .layer_texture_cache_mut()
        .acquire(&device, (96, 96), false);
    context.renderer.layer_texture_cache_mut().release(large);
    let second = frame(&mut context, &batch, 1);
    assert!(
        first
            .as_raw()
            .iter()
            .zip(second.as_raw())
            .all(|(a, b)| a.abs_diff(*b) <= 2)
    );
}

#[test]
#[ignore] // Requires GPU
fn named_layer_sample_reads_post_effect_pixels() {
    let mut context = super::main_text_clip_tests::context(true, 1);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_layer(
        LayerConfig::new()
            .id(LayerId(7))
            .position(Point::new(8.0, 8.0))
            .size(Size::new(32.0, 32.0))
            .opacity(1.0)
            .effect(blinc_core::LayerEffect::brightness(0.5)),
    );
    paint.fill_rect(
        Rect::new(8.0, 8.0, 32.0, 32.0),
        0.0.into(),
        Color::RED.into(),
    );
    paint.pop_layer();
    paint.sample_layer(
        LayerId(7),
        Rect::new(0.0, 0.0, 32.0, 32.0),
        Rect::new(72.0, 8.0, 32.0, 32.0),
    );
    let batch = paint.take_batch();
    let page = frame(&mut context, &batch, 1);
    pixel(&page, (20, 20), [128, 0, 0, 255]);
    pixel(&page, (84, 20), [128, 0, 0, 255]);
}

#[test]
#[ignore] // Requires GPU
fn named_layer_sample_survives_canvas_group_repaint_and_compositor_replay() {
    use blinc_animation::AnimationScheduler;
    use std::sync::atomic::{AtomicUsize, Ordering};
    for compositor in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = calls.clone();
        let mut tree = RenderTree::from_element(
            &div().w(160.0).h(120.0).bg(Color::BLUE).child(
                div()
                    .id("owner")
                    .absolute()
                    .w(160.0)
                    .h(120.0)
                    .opacity(0.5)
                    .child(
                        canvas(move |ctx, _| {
                            callback_calls.fetch_add(1, Ordering::Relaxed);
                            source(ctx, LayerId(7), Color::RED, 1.0);
                            ctx.sample_layer(
                                LayerId(7),
                                Rect::new(0.0, 0.0, 32.0, 32.0),
                                Rect::new(72.0, 8.0, 32.0, 32.0),
                            );
                        })
                        .absolute()
                        .w(160.0)
                        .h(120.0),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(120.0)
                            .top(80.0)
                            .child(text("X").color(Color::WHITE)),
                    ),
            ),
        );
        tree.compute_layout(160.0, 120.0);
        let owner = tree.query_by_id("owner").unwrap();
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        let mut context = super::main_text_clip_tests::context(true, 1);
        let target = target(&context);
        let view = target.create_view(&Default::default());
        for opacity in [0.5_f32, 0.0, 1.0, 0.75, 0.5] {
            tree.update_render_props(owner, |props| props.opacity = opacity);
            context.invalidate_render_cache();
            for _ in 0..2 {
                let before = calls.load(Ordering::Relaxed);
                context
                    .render_tree_with_motion_opt(
                        &tree,
                        &state,
                        160,
                        120,
                        &view,
                        compositor.then_some(&target),
                        true,
                    )
                    .unwrap();
                if opacity > 0.0 {
                    assert_eq!(calls.load(Ordering::Relaxed) - before, 1);
                }
                let page = crate::tests::read_rendered_pixels(
                    context.device(),
                    context.queue(),
                    &target,
                    160,
                    120,
                );
                let expected = [
                    (255.0 * opacity).round() as u8,
                    0,
                    (255.0 * (1.0 - opacity)).round() as u8,
                    255,
                ];
                pixel(&page, (20, 20), expected);
                pixel(&page, (84, 20), expected);
            }
        }
    }
}
