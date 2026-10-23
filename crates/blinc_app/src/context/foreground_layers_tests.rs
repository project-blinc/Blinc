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

#[test]
#[ignore] // Requires GPU
fn foreground_layers_static_group_retains_motion_descendants() {
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    for compositor in [false, true] {
        let mut app = app();
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let x = Arc::new(Mutex::new(AnimatedValue::with_default(
            scheduler.lock().unwrap().handle(),
            0.0,
        )));
        let tree = foreground_static_motion_tree(x.clone(), false, true);
        let state = RenderState::new(scheduler.clone());
        for (frame, dx) in [0.0, 10.0, -10.0, 0.0].into_iter().enumerate() {
            x.lock().unwrap().set_immediate(dx);
            x.lock().unwrap().set_target(dx + 20.0);
            tree.compute_animation_status();
            let texture = app.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("Foreground static motion group surface"),
                size: wgpu::Extent3d {
                    width: 160,
                    height: 120,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: app.texture_format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
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
                frame > 0,
            )
            .unwrap();
            let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
            eprintln!("compositor={compositor} frame={frame} dx={dx}");
            pixel(&page, (55, 55), [0, 128, 127, 255]);
            pixel(&page, (25, 25), [128, 0, 127, 255]);
            pixel(&page, (5, 5), [0, 0, 255, 255]);
            let c = app.context();
            assert!(tree.composite_bindings().values().any(|m| !m.dynamic_batch));
            assert!(
                c.cached_dynamic_batch
                    .as_ref()
                    .is_none_or(|b| b.foreground_primitives.is_empty())
            );
            assert_eq!(
                c.cached_bg_batch
                    .as_ref()
                    .unwrap()
                    .foreground_primitives
                    .len(),
                2
            );
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn foreground_layers_retained_motion_images_replay_with_owning_rows() {
    use blinc_core::{ClipShape, Path};
    let mut app = app();
    let c = app.context();
    c.renderer.resize(160, 120);
    for value in [1.0_f32, 0.5, 0.0, 0.75, 1.0] {
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        paint.set_foreground_layer(true);
        paint.push_layer(LayerConfig::new().opacity(0.5));
        paint.fill_rect(
            Rect::new(20.0, 20.0, 60.0, 60.0),
            0.0.into(),
            Color::RED.into(),
        );
        paint.push_motion_subtree();
        paint.push_opacity(value);
        paint.push_clip(ClipShape::Path(
            Path::new()
                .move_to(0.0, 0.0)
                .line_to(140.0, 0.0)
                .line_to(0.0, 140.0)
                .close(),
        ));
        paint.push_clip(ClipShape::rect(Rect::new(45.0, 45.0, 45.0, 45.0)));
        paint.draw_rgba_pixels(&[0, 255, 0, 255], 1, 1, Rect::new(40.0, 40.0, 60.0, 60.0));
        paint.pop_clip();
        paint.pop_clip();
        paint.pop_opacity();
        paint.pop_motion_subtree();
        paint.pop_layer();
        c.cached_bg_batch = Some(paint.take_batch());
        assert!(paint.take_dynamic_batch().is_empty());
        for _ in 0..2 {
            let (texture, view) = create_test_texture(c.device(), c.texture_format(), 160, 120);
            c.renderer.clear_target(&view, wgpu::Color::BLUE);
            c.renderer.update_aux_data_slice(&vec![[0.0; 4]; 4096]);
            c.render_foreground_overlay(&view, &[]);
            let page = read_rendered_pixels(c.device(), c.queue(), &texture, 160, 120);
            pixel(
                &page,
                (55, 55),
                [
                    (127.5 * (1.0 - value)).round() as u8,
                    (127.5 * value).round() as u8,
                    127,
                    255,
                ],
            );
            pixel(&page, (75, 75), [128, 0, 127, 255]);
            pixel(&page, (42, 55), [128, 0, 127, 255]);
            pixel(&page, (95, 55), [0, 0, 255, 255]);
        }
    }
}

fn foreground_static_motion_tree(
    value: blinc_layout::motion::SharedAnimatedValue,
    opacity: bool,
    foreground: bool,
) -> RenderTree {
    let motion = div()
        .w(120.0)
        .h(120.0)
        .child(
            div()
                .absolute()
                .left(40.0)
                .top(40.0)
                .w(60.0)
                .h(60.0)
                .bg(Color::GREEN),
        )
        .motion();
    let motion = if opacity {
        motion.opacity(value)
    } else {
        motion.translate_x(value)
    };
    let mut parent = div().absolute().w(160.0).h(120.0).opacity(0.5);
    if foreground {
        parent = parent.foreground();
    }
    let ui = div().w(160.0).h(120.0).bg(Color::BLUE).child(
        parent
            .child(
                div()
                    .absolute()
                    .left(20.0)
                    .top(20.0)
                    .w(60.0)
                    .h(60.0)
                    .bg(Color::RED),
            )
            .child(motion),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(160.0, 120.0);
    tree
}

#[test]
#[ignore] // Requires GPU
fn foreground_layers_static_group_opacity_endpoints_retain_nested_commands() {
    use blinc_animation::{AnimatedValue, AnimationScheduler};
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    for (foreground, compositor) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut app = app();
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));
        let opacity = Arc::new(Mutex::new(AnimatedValue::with_default(
            scheduler.lock().unwrap().handle(),
            1.0,
        )));
        let tree = foreground_static_motion_tree(opacity.clone(), true, foreground);
        let state = RenderState::new(scheduler.clone());
        for (frame, value) in [1.0_f32, 0.5, 0.0, 0.75, 1.0].into_iter().enumerate() {
            opacity.lock().unwrap().set_immediate(value);
            tree.compute_animation_status();
            let texture = app.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("Foreground retained opacity surface"),
                size: wgpu::Extent3d {
                    width: 160,
                    height: 120,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: app.texture_format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
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
                frame > 0,
            )
            .unwrap();
            let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
            pixel(
                &page,
                (55, 55),
                [
                    (127.5 * (1.0 - value)).round() as u8,
                    (127.5 * value).round() as u8,
                    127,
                    255,
                ],
            );
            pixel(&page, (25, 25), [128, 0, 127, 255]);
            let c = app.context();
            let b = c.cached_bg_batch.as_ref().unwrap();
            let commands = if foreground {
                &b.foreground_layer_commands
            } else {
                &b.layer_commands
            };
            assert_eq!(commands.len(), 4);
            assert!(
                matches!(&commands[1].command, blinc_gpu::primitives::LayerCommand::Push { config } if config.opacity == value)
            );
            assert_eq!(tree.composite_bindings().len(), 1);
            assert!(tree.composite_bindings().values().all(|m| !m.dynamic_batch));
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn foreground_layers_static_group_css_updates_repaint_retained_content() {
    use blinc_animation::{AnimationScheduler, MultiKeyframeAnimation};
    use blinc_layout::render_state::ActiveCssAnimation;
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let mut app = app();
    let ui = div().w(160.0).h(120.0).bg(Color::BLUE).child(
        div()
            .absolute()
            .w(160.0)
            .h(120.0)
            .foreground()
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
                    .id("retained-css")
                    .absolute()
                    .left(40.0)
                    .top(40.0)
                    .w(60.0)
                    .h(60.0)
                    .bg(Color::GREEN)
                    .opacity(0.5),
            ),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(160.0, 120.0);
    let child = tree.query_by_id("retained-css").unwrap();
    let stable = tree.stable_id(child).unwrap();
    let mut animation = ActiveCssAnimation::new(MultiKeyframeAnimation::new(1000));
    animation.current_properties.opacity = Some(0.5);
    tree.css_anim_store()
        .lock()
        .unwrap()
        .animations
        .insert(stable, animation);
    let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    for compositor in [false, true] {
        app.context().invalidate_render_cache();
        for (frame, value) in [0.5_f32, 0.0, 0.75, 1.0].into_iter().enumerate() {
            tree.update_render_props(child, |p| p.opacity = value);
            tree.css_anim_store()
                .lock()
                .unwrap()
                .animations
                .get_mut(&stable)
                .unwrap()
                .current_properties
                .opacity = Some(value);
            tree.compute_animation_status();
            assert!(tree.composite_promotion().contains(&child));
            let texture = app.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("Foreground retained CSS surface"),
                size: wgpu::Extent3d {
                    width: 160,
                    height: 120,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: app.texture_format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
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
                frame > 0,
            )
            .unwrap();
            let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
            eprintln!("css compositor={compositor} frame={frame} value={value}");
            pixel(
                &page,
                (55, 55),
                [
                    (127.5 * (1.0 - value)).round() as u8,
                    (127.5 * value).round() as u8,
                    127,
                    255,
                ],
            );
            assert!(
                tree.css_anim_paint_records()
                    .get(&child)
                    .unwrap()
                    .foreground_batch
            );
            let c = app.context();
            let saved = c.cached_bg_batch.take();
            let mut sentinel = PrimitiveBatch::new();
            sentinel.primitives = vec![GpuPrimitive::rect(1.0, 2.0, 3.0, 4.0); 8];
            c.cached_bg_batch = Some(sentinel);
            assert!(matches!(
                c.apply_css_deltas(&tree, 1.0),
                CssDeltaResult::Bail
            ));
            assert!(
                c.cached_bg_batch
                    .as_ref()
                    .unwrap()
                    .primitives
                    .iter()
                    .all(|p| p.bounds == [1.0, 2.0, 3.0, 4.0])
            );
            c.cached_bg_batch = saved;
        }
    }
}
