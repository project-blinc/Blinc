use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::read_rendered_pixels;
use blinc_animation::AnimationScheduler;
use blinc_core::{ClipShape, Path};
use std::sync::atomic::{AtomicUsize, Ordering};

fn pixel(page: &image::RgbaImage, xy: (u32, u32), expected: [u8; 4]) {
    let actual = page.get_pixel(xy.0, xy.1).0;
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
        "{xy:?}: {actual:?}, expected {expected:?}"
    );
}

#[derive(Clone, Copy, Debug)]
enum CanvasOwner {
    Static,
    Motion,
    Overlay,
}

fn with_owner(owner: CanvasOwner, ui: Div, parent: Div, foreground: bool) -> Div {
    match owner {
        CanvasOwner::Static => ui.child(parent),
        CanvasOwner::Overlay => ui.child(parent.overlay_root()),
        CanvasOwner::Motion => {
            use blinc_animation::AnimatedValue;
            let scheduler = AnimationScheduler::new();
            let opacity = Arc::new(Mutex::new(AnimatedValue::with_default(
                scheduler.handle(),
                1.0,
            )));
            let content = div()
                .absolute()
                .w(160.0)
                .h(120.0)
                .child(parent)
                .motion()
                .opacity(opacity);
            if foreground {
                ui.child(
                    div()
                        .absolute()
                        .w(160.0)
                        .h(120.0)
                        .foreground()
                        .child(content),
                )
            } else {
                ui.child(content)
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn retained_canvas_static_ancestor_keeps_images_shapes_and_empty_frames_isolated() {
    canvas_ancestor_frames(CanvasOwner::Static);
}

#[test]
#[ignore] // Requires GPU
fn retained_canvas_dynamic_ancestors_keep_callback_and_cache_ownership() {
    canvas_ancestor_frames(CanvasOwner::Motion);
    canvas_ancestor_frames(CanvasOwner::Overlay);
}

fn canvas_ancestor_frames(owner: CanvasOwner) {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    for (foreground, compositor) in [(false, true), (false, false), (true, true), (true, false)] {
        let mut app = BlincApp::with_config(BlincConfig {
            sample_count: 1,
            ..Default::default()
        })
        .expect("offscreen GPU required");
        let frame = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let independent_calls = Arc::new(AtomicUsize::new(0));
        let current = frame.clone();
        let painted = calls.clone();
        let independent = independent_calls.clone();
        let mut parent = div().absolute().w(160.0).h(120.0).opacity(0.5);
        if foreground {
            parent = parent.foreground();
        }
        let parent = parent
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
                canvas(move |ctx, _| {
                    painted.fetch_add(1, Ordering::Relaxed);
                    let n = current.load(Ordering::Relaxed);
                    ctx.push_clip(ClipShape::Path(
                        Path::new()
                            .move_to(0.0, 0.0)
                            .line_to(140.0, 0.0)
                            .line_to(0.0, 140.0)
                            .close(),
                    ));
                    ctx.push_clip(ClipShape::rect(Rect::new(45.0, 45.0, 45.0, 45.0)));
                    if n == 2 {
                        ctx.fill_rect(
                            Rect::new(40.0, 40.0, 60.0, 60.0),
                            0.0.into(),
                            Color::GREEN.into(),
                        );
                    } else if n != 3 {
                        let color = if n == 1 {
                            [255, 255, 0, 255]
                        } else {
                            [0, 255, 0, 255]
                        };
                        ctx.draw_rgba_pixels(&color, 1, 1, Rect::new(40.0, 40.0, 60.0, 60.0));
                    }
                    ctx.pop_clip();
                    ctx.pop_clip();
                })
                .absolute()
                .w(160.0)
                .h(120.0),
            );
        let ui = div().w(160.0).h(120.0).bg(Color::BLUE);
        let ui = with_owner(owner, ui, parent, foreground);
        let ui = ui.child(
            canvas(move |ctx, _| {
                independent.fetch_add(1, Ordering::Relaxed);
                ctx.fill_rect(
                    Rect::new(130.0, 5.0, 10.0, 10.0),
                    0.0.into(),
                    Color::rgba(1.0, 1.0, 1.0, 0.5).into(),
                );
            })
            .absolute()
            .w(160.0)
            .h(120.0),
        );
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(160.0, 120.0);
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        for n in 0..5 {
            frame.store(n, Ordering::Relaxed);
            let texture = app.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("Retained canvas surface"),
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
                n > 0,
            )
            .unwrap();
            let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
            eprintln!("owner={owner:?} foreground={foreground} compositor={compositor} frame={n}");
            pixel(
                &page,
                (55, 55),
                match n {
                    1 => [128, 128, 127, 255],
                    3 => [128, 0, 127, 255],
                    _ => [0, 128, 127, 255],
                },
            );
            pixel(&page, (25, 25), [128, 0, 127, 255]);
            pixel(&page, (75, 75), [128, 0, 127, 255]);
            pixel(&page, (42, 55), [128, 0, 127, 255]);
            pixel(&page, (95, 55), [0, 0, 255, 255]);
            pixel(&page, (135, 10), [128, 128, 255, 255]);
            assert_eq!(calls.load(Ordering::Relaxed), n + 1);
            assert_eq!(independent_calls.load(Ordering::Relaxed), n + 1);
            assert!(tree.had_retained_canvas_painted());
            assert!(tree.visible_anim_active());
            assert!(tree.subtree_texture_candidates().is_empty());
            assert_eq!(tree.canvas_paint_records().len(), 1);
            assert!(tree.overlay_canvas_paint_records().is_empty());
            assert!(!app.context().redraw_canvases(&tree, 160, 120));
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn retained_canvas_nested_opacity_keeps_commands_and_redraw_signal() {
    nested_canvas_frames(CanvasOwner::Static);
}

#[test]
#[ignore] // Requires GPU
fn retained_canvas_dynamic_nested_opacity_recovers_from_zero() {
    nested_canvas_frames(CanvasOwner::Motion);
    nested_canvas_frames(CanvasOwner::Overlay);
}

fn nested_canvas_frames(owner: CanvasOwner) {
    use blinc_core::LayerConfig;
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    for (foreground, compositor) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut app = BlincApp::with_config(BlincConfig {
            sample_count: 1,
            ..Default::default()
        })
        .expect("offscreen GPU required");
        let frame = Arc::new(AtomicUsize::new(0));
        let current = frame.clone();
        let mut parent = div().absolute().w(160.0).h(120.0).opacity(0.5);
        if foreground {
            parent = parent.foreground();
        }
        let parent = parent
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
                canvas(move |ctx, _| {
                    let opacity = [1.0, 0.5, 0.0, 0.75, 1.0][current.load(Ordering::Relaxed)];
                    ctx.push_layer(LayerConfig::new().opacity(opacity));
                    ctx.fill_rect(
                        Rect::new(40.0, 40.0, 60.0, 60.0),
                        0.0.into(),
                        Color::GREEN.into(),
                    );
                    ctx.fill_path(
                        &Path::new()
                            .move_to(60.0, 40.0)
                            .line_to(90.0, 40.0)
                            .line_to(60.0, 70.0)
                            .close(),
                        Color::BLUE.into(),
                    );
                    ctx.pop_layer();
                })
                .absolute()
                .w(160.0)
                .h(120.0),
            );
        let ui = with_owner(
            owner,
            div().w(160.0).h(120.0).bg(Color::BLUE),
            parent,
            foreground,
        );
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(160.0, 120.0);
        let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
        for (n, opacity) in [1.0_f32, 0.5, 0.0, 0.75, 1.0].into_iter().enumerate() {
            frame.store(n, Ordering::Relaxed);
            let texture = app.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("Retained nested canvas surface"),
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
                n > 0,
            )
            .unwrap();
            let page = read_rendered_pixels(app.device(), app.queue(), &texture, 160, 120);
            eprintln!(
                "nested owner={owner:?} foreground={foreground} compositor={compositor} opacity={opacity}"
            );
            pixel(
                &page,
                (55, 55),
                [
                    (127.5 * (1.0 - opacity)).round() as u8,
                    (127.5 * opacity).round() as u8,
                    127,
                    255,
                ],
            );
            pixel(
                &page,
                (65, 45),
                [
                    (127.5 * (1.0 - opacity)).round() as u8,
                    0,
                    (127.5 * (1.0 + opacity)).round() as u8,
                    255,
                ],
            );
            pixel(&page, (25, 25), [128, 0, 127, 255]);
            assert!(tree.had_retained_canvas_painted());
            assert!(tree.visible_anim_active());
            assert!(tree.subtree_texture_candidates().is_empty());
            assert!(tree.canvas_paint_records().is_empty());
            assert!(tree.overlay_canvas_paint_records().is_empty());
            assert!(
                tree.dynamic_regions()
                    .values()
                    .all(|r| !matches!(r.kind, blinc_layout::renderer::DynamicKind::Canvas { .. }))
            );
            let c = app.context();
            let b = match owner {
                CanvasOwner::Static => c.cached_bg_batch.as_ref().unwrap(),
                CanvasOwner::Motion | CanvasOwner::Overlay => {
                    c.cached_dynamic_batch.as_ref().unwrap()
                }
            };
            let commands = if foreground {
                &b.foreground_layer_commands
            } else {
                &b.layer_commands
            };
            let nested = match owner {
                CanvasOwner::Motion => 2,
                CanvasOwner::Static | CanvasOwner::Overlay => 1,
            };
            assert_eq!(commands.len(), (nested + 1) * 2);
            assert!(
                matches!(&commands[nested].command, blinc_gpu::primitives::LayerCommand::Push { config } if config.opacity == opacity)
            );
        }
    }
}

#[test]
fn retained_canvas_owner_metadata_resets_when_group_no_longer_retains() {
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let painted = calls.clone();
    let ui = div().w(160.0).h(120.0).child(
        div()
            .id("owner")
            .opacity(0.5)
            .w(160.0)
            .h(120.0)
            .child(div().absolute().w(60.0).h(60.0).bg(Color::RED))
            .child(
                canvas(move |ctx, _| {
                    painted.fetch_add(1, Ordering::Relaxed);
                    ctx.push_clip(ClipShape::Path(Path::rect(Rect::new(0.0, 0.0, 80.0, 80.0))));
                    ctx.fill_rect(
                        Rect::new(20.0, 20.0, 60.0, 60.0),
                        0.0.into(),
                        Color::GREEN.into(),
                    );
                    ctx.pop_clip();
                })
                .absolute()
                .w(160.0)
                .h(120.0),
            ),
    );
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(160.0, 120.0);
    let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    tree.set_skip_canvas_drawing(true);
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    tree.render_with_motion(&mut paint, &state);
    assert!(tree.had_retained_canvas_painted());
    assert!(tree.canvas_paint_records().is_empty());
    assert!(
        tree.dynamic_regions()
            .values()
            .all(|r| !matches!(r.kind, blinc_layout::renderer::DynamicKind::Canvas { .. }))
    );
    let b = paint.take_batch();
    assert_eq!(b.primitives.len(), 2);
    assert_eq!(b.layer_commands.len(), 2);
    assert_eq!(b.primitives[1].color[3], 1.0);
    assert!(b.primitives[1].clip_shape[0] > 0.0);
    assert!(!b.aux_data.is_empty());
    assert_eq!(calls.load(Ordering::Relaxed), 1);

    let owner = tree.query_by_id("owner").unwrap();
    tree.update_render_props(owner, |p| p.opacity = 1.0);
    tree.render_with_motion(&mut GpuPaintContext::new(160.0, 120.0), &state);
    assert!(!tree.had_retained_canvas_painted());
    assert_eq!(tree.canvas_paint_records().len(), 1);
    assert_eq!(calls.load(Ordering::Relaxed), 1);

    tree.update_render_props(owner, |p| p.opacity = 0.0);
    tree.render_with_motion(&mut GpuPaintContext::new(160.0, 120.0), &state);
    assert!(!tree.had_retained_canvas_painted());
    assert!(tree.canvas_paint_records().is_empty());
    assert!(!tree.visible_anim_active());
}

#[test]
fn retained_canvas_explicit_scratch_owner_restores_static_routing() {
    use blinc_core::LayerConfig;
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let painted = calls.clone();
    let ui = canvas(move |ctx, _| {
        painted.fetch_add(1, Ordering::Relaxed);
        ctx.push_layer(LayerConfig::new().opacity(0.5));
        ctx.push_clip(ClipShape::Path(Path::rect(Rect::new(0.0, 0.0, 80.0, 80.0))));
        ctx.fill_rect(
            Rect::new(10.0, 10.0, 40.0, 40.0),
            0.0.into(),
            Color::RED.into(),
        );
        ctx.draw_rgba_pixels(&[0, 255, 0, 255], 1, 1, Rect::new(20.0, 20.0, 40.0, 40.0));
        ctx.pop_clip();
        ctx.pop_layer();
    })
    .w(160.0)
    .h(120.0);
    let mut tree = RenderTree::from_element(&ui);
    tree.compute_layout(160.0, 120.0);
    tree.set_skip_canvas_drawing(true);
    let state = RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_composite_layer(91);
    tree.render_with_motion(&mut paint, &state);
    assert!(tree.had_retained_canvas_painted());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(tree.canvas_paint_records().is_empty());
    assert!(tree.overlay_canvas_paint_records().is_empty());
    assert!(
        tree.dynamic_regions()
            .values()
            .all(|r| !matches!(r.kind, blinc_layout::renderer::DynamicKind::Canvas { .. }))
    );
    paint.pop_composite_layer();
    assert!(!paint.bg_batch_is_composite());
    assert!(!paint.bg_batch_is_dynamic());
    paint.fill_rect(
        Rect::new(130.0, 5.0, 10.0, 10.0),
        0.0.into(),
        Color::WHITE.into(),
    );
    let scratch = paint.take_composite_layer_batches().remove(&91).unwrap();
    assert_eq!(scratch.primitives.len(), 1);
    assert_eq!(scratch.layer_commands.len(), 2);
    assert!(!scratch.aux_data.is_empty());
    assert!(scratch.primitives[0].clip_shape[0] > 0.0);
    assert_eq!(scratch.dynamic_images.len(), 1);
    assert!(!scratch.dynamic_images[0].clip_aux.is_empty());
    let static_batch = paint.take_batch();
    assert_eq!(static_batch.primitives.len(), 1);
    assert_eq!(static_batch.primitives[0].color[3], 1.0);
    assert_eq!(static_batch.primitives[0].clip_shape[0], 0.0);
    assert!(static_batch.layer_commands.is_empty());
    assert!(static_batch.dynamic_images.is_empty());
    assert!(paint.take_dynamic_batch().primitives.is_empty());
}
