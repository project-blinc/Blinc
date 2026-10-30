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
fn pixel(page: &image::RgbaImage, xy: (u32, u32), expected: [u8; 4]) {
    let actual = page.get_pixel(xy.0, xy.1).0;
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
        "{xy:?}: {actual:?}, expected {expected:?}"
    );
}
fn group(motion: bool) -> PrimitiveBatch {
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.set_foreground_layer(true);
    if motion {
        paint.push_motion_subtree();
    }
    paint.push_layer(LayerConfig::new().opacity(0.5));
    paint.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(20.0, 20.0, 60.0, 60.0));
    paint.draw_rgba_pixels(&[0, 255, 0, 255], 1, 1, Rect::new(40.0, 40.0, 60.0, 60.0));
    paint.pop_layer();
    if motion {
        paint.pop_motion_subtree();
        paint.take_dynamic_batch()
    } else {
        paint.take_batch()
    }
}

#[test]
#[ignore] // Requires GPU
fn foreground_rgba_image_only_groups_reach_static_canvas_and_motion_overlays() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    for route in 0..3 {
        context.cached_bg_batch = None;
        context.cached_dynamic_batch = None;
        let mut canvas = CanvasOverlay::default();
        match route {
            0 => context.cached_bg_batch = Some(group(false)),
            1 => canvas.finish_batches(vec![group(false)]),
            _ => context.cached_dynamic_batch = Some(group(true)),
        }
        let (texture, view) =
            create_test_texture(context.device(), context.texture_format(), 160, 120);
        context
            .renderer
            .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
        context.render_foreground_overlay(&view, &[&canvas]);
        context.dispatch_dynamic_batch_overlay(&view);
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        pixel(&page, (55, 55), [0, 128, 127, 255]);
        pixel(&page, (30, 30), [128, 0, 127, 255]);
    }
}

#[test]
#[ignore] // Requires GPU
fn foreground_rgba_later_shapes_cover_earlier_images() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.set_foreground_layer(true);
    paint.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(20.0, 20.0, 60.0, 60.0));
    paint.fill_rect(
        Rect::new(40.0, 40.0, 60.0, 60.0),
        0.0.into(),
        Color::GREEN.into(),
    );
    let batch = paint.take_batch();
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), 160, 120);
    context
        .renderer
        .render_with_clear(&view, &PrimitiveBatch::new(), [0.0, 0.0, 1.0, 1.0]);
    context.renderer.render_foreground(&view, &batch);
    context
        .renderer
        .render_unordered_dynamic_images(&view, &batch.dynamic_images);
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    pixel(&page, (55, 55), [0, 255, 0, 255]);
    pixel(&page, (30, 30), [255, 0, 0, 255]);
}

#[test]
#[ignore] // Requires GPU
fn foreground_rgba_application_frames_update_pixels_then_shapes_then_empty() {
    use blinc_animation::AnimationScheduler;
    use blinc_core::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    for (route, glass) in [
        (0, false),
        (1, false),
        (2, false),
        (0, true),
        (1, true),
        (2, true),
    ] {
        let mut app = app();
        let frame = Arc::new(AtomicUsize::new(0));
        let pixels = frame.clone();
        let ui = div()
            .w(160.0)
            .h(120.0)
            .bg(Color::BLUE)
            .child(
                canvas(move |ctx, _| {
                    let frame = pixels.load(Ordering::Relaxed);
                    ctx.set_foreground_layer(true);
                    ctx.push_layer(LayerConfig::new().opacity(0.5));
                    if frame < 2 {
                        ctx.draw_rgba_pixels(
                            &[255, 0, 0, 255],
                            1,
                            1,
                            Rect::new(20.0, 20.0, 60.0, 60.0),
                        );
                        let color = if frame == 0 {
                            [0, 255, 0, 255]
                        } else {
                            [255, 255, 0, 255]
                        };
                        ctx.draw_rgba_pixels(&color, 1, 1, Rect::new(40.0, 40.0, 60.0, 60.0));
                    } else if frame == 2 {
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
                    }
                    ctx.pop_layer();
                    ctx.set_foreground_layer(false);
                })
                .absolute()
                .w(160.0)
                .h(120.0),
            )
            .child(
                canvas(|ctx, _| {
                    // Own mesh aux buffer follows the image-only callback.
                    ctx.set_foreground_layer(true);
                    ctx.fill_path(
                        &Path::rect(Rect::new(110.0, 20.0, 20.0, 20.0)),
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
        for n in 0..4 {
            frame.store(n, Ordering::Relaxed);
            let texture = app.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("Foreground RGBA surface"),
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
            if route == 0 {
                app.render_tree(&tree, &view, 160, 120).unwrap();
            } else {
                app.render_tree_with_motion_opt(
                    &tree,
                    &state,
                    &view,
                    (route == 2).then_some(&texture),
                    160,
                    120,
                    n != 0,
                )
                .unwrap();
            }
            let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
            eprintln!("route={route},glass={glass},frame={n}");
            pixel(
                &page,
                (55, 55),
                match n {
                    0 | 2 => [0, 128, 127, 255],
                    1 => [128, 128, 127, 255],
                    _ => [0, 0, 255, 255],
                },
            );
            pixel(
                &page,
                (30, 30),
                if n < 3 {
                    [128, 0, 127, 255]
                } else {
                    [0, 0, 255, 255]
                },
            );
            pixel(&page, (120, 30), [255, 255, 255, 255]);
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn foreground_rgba_nested_clips_paths_and_public_glass_keep_order() {
    use blinc_core::{ClipShape, Path};
    use blinc_gpu::{path::tessellate_fill, primitives::LayerCommand};
    let mut app = app();
    let context = app.context();
    context.renderer.resize(160, 120);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.set_foreground_layer(true);
    paint.draw_rgba_pixels(
        &[255, 255, 255, 255],
        1,
        1,
        Rect::new(110.0, 20.0, 20.0, 20.0),
    );
    paint.push_clip(ClipShape::rect(Rect::new(25.0, 20.0, 75.0, 80.0)));
    paint.push_layer(LayerConfig::new().opacity(0.5));
    paint.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(20.0, 20.0, 60.0, 60.0));
    paint.push_layer(LayerConfig::new().opacity(0.5));
    paint.draw_rgba_pixels(&[0, 255, 0, 255], 1, 1, Rect::new(40.0, 40.0, 60.0, 60.0));
    paint.pop_layer();
    paint.pop_layer();
    paint.pop_clip();
    paint.push_layer(LayerConfig::new().opacity(0.0));
    paint.draw_rgba_pixels(&[255, 255, 255, 255], 1, 1, Rect::new(0.0, 0.0, 15.0, 15.0));
    paint.pop_layer();
    paint.push_clip(ClipShape::rect(Rect::new(0.0, 0.0, 10.0, 10.0)));
    paint.push_clip(ClipShape::rect(Rect::new(20.0, 20.0, 10.0, 10.0)));
    paint.draw_rgba_pixels(
        &[255, 255, 255, 255],
        1,
        1,
        Rect::new(0.0, 0.0, 160.0, 120.0),
    );
    paint.pop_clip();
    paint.pop_clip();
    let mut batch = paint.take_batch();
    batch.push_foreground_layer_command(LayerCommand::Push {
        config: LayerConfig::new().opacity(0.5),
    });
    batch.push_foreground_path(tessellate_fill(
        &Path::rect(Rect::new(110.0, 60.0, 20.0, 20.0)),
        &Color::WHITE.into(),
    ));
    batch.push_foreground_layer_command(LayerCommand::Pop);
    for route in 0..5 {
        let (texture, view) =
            create_test_texture(context.device(), context.texture_format(), 160, 120);
        context.renderer.clear_target(&view, wgpu::Color::BLUE);
        let backdrop = context.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("Foreground RGBA backdrop"),
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
        let backdrop = backdrop.create_view(&Default::default());
        match route {
            0 => context.renderer.render_foreground(&view, &batch),
            1 => context
                .renderer
                .render_overlay_msaa(&view, &batch.foreground_batch(), 4),
            2 | 4 => {
                context
                    .renderer
                    .render_glass_frame(&view, &backdrop, (80, 60), &batch, route == 2)
            }
            _ => {
                let mut bg = GpuPaintContext::new(160.0, 120.0);
                bg.draw_rgba_pixels(
                    &[255, 255, 255, 255],
                    1,
                    1,
                    Rect::new(140.0, 100.0, 5.0, 5.0),
                );
                let mut bg = bg.take_batch();
                bg.foreground_dynamic_images = batch.foreground_dynamic_images.clone();
                bg.foreground_paths = batch.foreground_paths.clone();
                bg.foreground_layer_commands = batch.foreground_layer_commands.clone();
                context
                    .renderer
                    .render_glass_frame(&view, &backdrop, (80, 60), &bg, true);
            }
        }
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
        eprintln!("public route={route}");
        let blue = if route == 4 { 0 } else { 127 };
        pixel(&page, (55, 55), [64, 64, blue, 255]);
        pixel(&page, (30, 30), [128, 0, blue, 255]);
        pixel(
            &page,
            (22, 30),
            if route == 4 {
                [0, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            },
        );
        pixel(&page, (120, 30), [255, 255, 255, 255]);
        pixel(
            &page,
            (120, 70),
            [128, 128, if route == 4 { 128 } else { 255 }, 255],
        );
        pixel(
            &page,
            (5, 5),
            if route == 4 {
                [0, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            },
        );
    }
    // The explicit sequence API still accepts a legacy record with no cursor.
    let mut legacy = batch.foreground_dynamic_images[0].clone();
    legacy.paint_order = None;
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), 160, 120);
    context.renderer.clear_target(&view, wgpu::Color::BLUE);
    context
        .renderer
        .render_unordered_dynamic_images(&view, &[legacy.clone()]);
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    pixel(&page, (120, 30), [255, 255, 255, 255]);
    context.renderer.clear_target(&view, wgpu::Color::BLUE);
    context.renderer.render_dynamic_images(&view, &[legacy]);
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, 160, 120);
    pixel(&page, (120, 30), [255, 255, 255, 255]);
}

// Deterministic pixels stay in the walker's motion stream instead of the
// independently collected dynamic-canvas stream.
struct StaticPixels(blinc_layout::Canvas);
impl ElementBuilder for StaticPixels {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.0.build(tree)
    }
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }
    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.0.render_props()
    }
    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.0.element_type_id()
    }
    fn canvas_render_info(&self) -> Option<blinc_layout::canvas::CanvasRenderFn> {
        self.0.canvas_render_info()
    }
    fn is_static_canvas(&self) -> bool {
        true
    }
}

#[test]
#[ignore] // Requires GPU
fn foreground_rgba_motion_group_recovers_after_zero_opacity() {
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
                        .child(StaticPixels(
                            canvas(|ctx, _| {
                                ctx.set_foreground_layer(true);
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
                                ctx.set_foreground_layer(false);
                            })
                            .w(160.0)
                            .h(120.0),
                        ))
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
                label: Some("Foreground RGBA motion surface"),
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
