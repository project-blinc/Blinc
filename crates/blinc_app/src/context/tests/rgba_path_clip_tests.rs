use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{ClipShape, FillRule, LayerConfig, Path, Point, Transform, Vec2, layer::Affine2D};
const W: u32 = 180;
const H: u32 = 150;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const HALF: [u8; 4] = [128, 0, 127, 255];
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
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
        "{xy:?}: {actual:?}, expected {expected:?}"
    );
}
fn curve(kind: usize) -> Path {
    match kind {
        0 => Path::circle(Point::new(70.0, 60.0), 30.0),
        1 => Path::new()
            .move_to(30.0, 60.0)
            .quad_to(70.0, 0.0, 110.0, 60.0)
            .quad_to(70.0, 120.0, 30.0, 60.0),
        2 => Path::new()
            .move_to(110.0, 60.0)
            .arc_to(Vec2::new(40.0, 22.0), 0.0, false, true, 30.0, 60.0)
            .arc_to(Vec2::new(40.0, 22.0), 0.0, false, true, 110.0, 60.0)
            .close(),
        _ => {
            let d = 40.0 * std::f32::consts::FRAC_1_SQRT_2;
            Path::new()
                .move_to(70.0 + d, 60.0 + d)
                .arc_to(
                    Vec2::new(40.0, 20.0),
                    std::f32::consts::FRAC_PI_4,
                    false,
                    true,
                    70.0 - d,
                    60.0 - d,
                )
                .arc_to(
                    Vec2::new(40.0, 20.0),
                    std::f32::consts::FRAC_PI_4,
                    false,
                    true,
                    70.0 + d,
                    60.0 + d,
                )
                .close()
        }
    }
}
fn image(paint: &mut dyn DrawContext) {
    paint.draw_rgba_pixels(&RED, 1, 1, Rect::new(10.0, 10.0, 160.0, 130.0));
}

fn frame_clip(paint: &mut dyn DrawContext, frame: usize) {
    let path = if frame == 2 {
        Path::new()
    } else {
        Path::circle(
            Point::new(if frame == 0 { 70.0 } else { 100.0 }, 60.0),
            30.0,
        )
    };
    paint.push_clip(ClipShape::Path(path));
    image(paint);
    paint.pop_clip();
}

#[test]
fn rgba_path_clip_hash_tracks_path_and_fill_rule_with_unchanged_image() {
    let batch = |rule, reverse| {
        let mut paint = GpuPaintContext::new(W as f32, H as f32);
        paint.set_clip_fill_rule(rule);
        paint.push_clip(ClipShape::Path(rings(reverse)));
        image(&mut paint);
        paint.take_batch()
    };
    let a = batch(FillRule::NonZero, false);
    for b in [
        batch(FillRule::EvenOdd, false),
        batch(FillRule::NonZero, true),
    ] {
        assert_eq!(a.dynamic_images[0].dest, b.dynamic_images[0].dest);
        assert_eq!(a.dynamic_images[0].data, b.dynamic_images[0].data);
        assert_eq!(a.aux_data, b.aux_data);
        assert_ne!(
            RenderContext::hash_composite_scratch(&a),
            RenderContext::hash_composite_scratch(&b)
        );
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_path_clip_empty_invalid_and_over_budget_paths_never_leak_pixels() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    let mut many = Path::new().move_to(20.0, 20.0);
    for n in 0..65_537 {
        many = many.line_to(20.0 + (n % 70) as f32, 30.0);
    }
    for (name, path, matrix) in [
        ("empty", Path::new(), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        (
            "move",
            Path::new().move_to(40.0, 40.0),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        ),
        (
            "line",
            Path::line(Point::new(20.0, 20.0), Point::new(100.0, 100.0)),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        ),
        (
            "nan-control",
            Path::new()
                .move_to(20.0, 20.0)
                .quad_to(f32::NAN, 60.0, 100.0, 100.0),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        ),
        (
            "nan-arc",
            Path::new().move_to(20.0, 20.0).arc_to(
                Vec2::new(20.0, 30.0),
                f32::NAN,
                false,
                true,
                100.0,
                100.0,
            ),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        ),
        (
            "huge-curve",
            Path::circle(Point::new(70.0, 60.0), f32::MAX),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        ),
        ("singular", curve(0), [0.0; 6]),
        (
            "nonfinite-transform",
            curve(0),
            [1.0, 0.0, 0.0, 1.0, f32::INFINITY, 0.0],
        ),
        ("budget", many, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        (
            "subdivision-budget",
            Path::circle(Point::new(0.0, 0.0), 1e12),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        ),
    ] {
        let mut paint = GpuPaintContext::new(W as f32, H as f32);
        paint.push_transform(Transform::Affine2D(Affine2D { elements: matrix }));
        paint.push_clip(ClipShape::Path(path));
        paint.pop_transform();
        image(&mut paint);
        let batch = paint.take_batch();
        let (texture, view) = create_test_texture(context.device(), context.texture_format(), W, H);
        context
            .renderer
            .render_with_clear(&view, &batch, [0.0, 0.0, 1.0, 1.0]);
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
        eprintln!("{name}");
        for y in (14..138).step_by(11) {
            for x in (14..168).step_by(11) {
                pixel(&page, (x, y), BLUE);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_path_clip_curves_and_scissors_survive_every_owner_and_stream() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    for route in 0..3 {
        for foreground in [false, true] {
            for kind in 0..4 {
                for scissor in [false, true] {
                    let mut paint = GpuPaintContext::new(W as f32, H as f32);
                    paint.set_foreground_layer(foreground);
                    if route == 1 {
                        paint.push_motion_subtree();
                    }
                    if route == 2 {
                        paint.push_composite_layer(7);
                    }
                    paint.push_clip(ClipShape::Path(curve(kind)));
                    if scissor {
                        paint.push_clip(ClipShape::rect(Rect::new(65.0, 20.0, 55.0, 90.0)));
                    }
                    paint.push_layer(LayerConfig::new().opacity(0.5));
                    image(&mut paint);
                    paint.pop_layer();
                    let mut batch = match route {
                        1 => paint.take_dynamic_batch(),
                        2 => paint.take_composite_layer_batches().remove(&7).unwrap(),
                        _ => paint.take_batch(),
                    };
                    if foreground && route != 2 {
                        batch = batch.take_foreground();
                    }
                    batch.aux_data.clear();
                    let (texture, view) =
                        create_test_texture(context.device(), context.texture_format(), W, H);
                    context
                        .renderer
                        .render_with_clear(&view, &batch, [0.0, 0.0, 1.0, 1.0]);
                    let page =
                        read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
                    eprintln!(
                        "route={route},foreground={foreground},kind={kind},scissor={scissor}"
                    );
                    pixel(&page, (70, 60), HALF);
                    pixel(&page, (25, 25), BLUE);
                    pixel(&page, (70, 100), BLUE);
                    pixel(&page, (50, 60), if scissor { BLUE } else { HALF });
                    pixel(
                        &page,
                        (105, 60),
                        if kind == 0 || kind == 3 { BLUE } else { HALF },
                    );
                    if kind == 3 {
                        for y in (15..136).step_by(5) {
                            for x in (15..166).step_by(5) {
                                let dx = x as f32 + 0.5 - 70.0;
                                let dy = y as f32 + 0.5 - 60.0;
                                let u = (dx + dy) * std::f32::consts::FRAC_1_SQRT_2 / 40.0;
                                let v = (-dx + dy) * std::f32::consts::FRAC_1_SQRT_2 / 20.0;
                                let radius = u.hypot(v);
                                if (radius - 1.0).abs() < 0.2 || (x as i32 - 65).abs() < 3 {
                                    continue;
                                }
                                let inside = radius < 1.0 && (!scissor || x >= 65);
                                pixel(&page, (x, y), if inside { HALF } else { BLUE });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn rings(reverse: bool) -> Path {
    // The open contours must be closed for filling, with no edge between them.
    let outer = Path::new()
        .move_to(20.0, 20.0)
        .line_to(100.0, 20.0)
        .line_to(100.0, 100.0)
        .line_to(20.0, 100.0);
    let inner = if reverse {
        [[45.0, 45.0], [45.0, 75.0], [75.0, 75.0], [75.0, 45.0]]
    } else {
        [[45.0, 45.0], [75.0, 45.0], [75.0, 75.0], [45.0, 75.0]]
    };
    outer
        .move_to(inner[0][0], inner[0][1])
        .line_to(inner[1][0], inner[1][1])
        .line_to(inner[2][0], inner[2][1])
        .line_to(inner[3][0], inner[3][1])
}
fn reference(page: &image::RgbaImage, matrix: [f32; 6], hole: bool) {
    let [a, b, c, d, tx, ty] = matrix;
    let det = a * d - b * c;
    let mut samples = [0; 2];
    for y in (14..138).step_by(5) {
        for x in (14..168).step_by(5) {
            let px = x as f32 + 0.5 - tx;
            let py = y as f32 + 0.5 - ty;
            let u = (d * px - c * py) / det;
            let v = (-b * px + a * py) / det;
            if [20.0, 100.0, 45.0, 75.0]
                .into_iter()
                .any(|edge| (u - edge).abs() < 3.0 || (v - edge).abs() < 3.0)
            {
                continue;
            }
            let outer = (20.0..100.0).contains(&u) && (20.0..100.0).contains(&v);
            let inner = (45.0..75.0).contains(&u) && (45.0..75.0).contains(&v);
            let inside = outer && !(hole && inner);
            pixel(page, (x, y), if inside { HALF } else { BLUE });
            samples[usize::from(inside)] += 1;
        }
    }
    assert!(samples.into_iter().all(|n| n > 10), "{samples:?}");
}

#[test]
#[ignore] // Requires GPU
fn rgba_path_clip_captures_push_frame_and_multicontour_fill_in_tight_bakes() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    for matrix in [
        [1.0, 0.2, 0.45, 1.0, 5.0, 0.0],
        [0.0, 1.0, -1.0, 0.0, 145.0, 0.0],
        [-1.1, 0.0, 0.0, 1.1, 150.0, 0.0],
    ] {
        for rule in [FillRule::NonZero, FillRule::EvenOdd] {
            for reverse in [false, true] {
                let mut paint = GpuPaintContext::new(W as f32, H as f32);
                paint.push_composite_layer(7);
                paint.push_transform(Transform::scale(2.0, 2.0));
                paint.push_transform(Transform::Affine2D(Affine2D {
                    elements: matrix.map(|v| v * 0.5),
                }));
                paint.set_clip_fill_rule(rule);
                paint.push_clip(ClipShape::Path(rings(reverse)));
                paint.pop_transform();
                paint.pop_transform();
                paint.push_layer(LayerConfig::new().opacity(0.5));
                image(&mut paint);
                paint.pop_layer();
                let batch = paint.take_composite_layer_batches().remove(&7).unwrap();
                let (bake, size) = context.renderer.render_subtree_to_layer_texture(
                    &batch,
                    (10.0, 10.0),
                    (160.0, 130.0),
                );
                let (texture, view) =
                    create_test_texture(context.device(), context.texture_format(), W, H);
                context.renderer.clear_target(&view, wgpu::Color::BLUE);
                context.renderer.blit_tight_texture_to_target(
                    &bake.view,
                    size,
                    &view,
                    (10.0, 10.0),
                    (160.0, 130.0),
                    1.0,
                    blinc_core::BlendMode::Normal,
                    None,
                    None,
                    0.0,
                );
                let page = read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
                eprintln!("matrix={matrix:?},rule={rule:?},reverse={reverse}");
                reference(&page, matrix, rule == FillRule::EvenOdd || reverse);
                context.renderer.layer_texture_cache_mut().release(bake);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_path_clip_application_frames_replay_changed_and_empty_paths() {
    use blinc_animation::AnimationScheduler;
    use std::sync::atomic::{AtomicUsize, Ordering};
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    for route in 0..3 {
        for foreground in [false, true] {
            let mut app = app();
            let frame = Arc::new(AtomicUsize::new(0));
            let current = frame.clone();
            let ui = div().w(W as f32).h(H as f32).bg(Color::BLUE).child(
                canvas(move |ctx, _| {
                    ctx.set_foreground_layer(foreground);
                    ctx.push_layer(LayerConfig::new().opacity(0.5));
                    frame_clip(ctx, current.load(Ordering::Relaxed));
                    ctx.pop_layer();
                    ctx.set_foreground_layer(false);
                })
                .absolute()
                .w(W as f32)
                .h(H as f32),
            );
            let mut tree = blinc_layout::RenderTree::from_element(&ui);
            tree.compute_layout(W as f32, H as f32);
            let state =
                blinc_layout::RenderState::new(Arc::new(Mutex::new(AnimationScheduler::new())));
            for n in 0..3 {
                frame.store(n, Ordering::Relaxed);
                let texture = app.device().create_texture(&wgpu::TextureDescriptor {
                    label: Some("Clipped RGBA surface"),
                    size: wgpu::Extent3d {
                        width: W,
                        height: H,
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
                    app.render_tree(&tree, &view, W, H).unwrap();
                } else {
                    app.render_tree_with_motion_opt(
                        &tree,
                        &state,
                        &view,
                        (route == 2).then_some(&texture),
                        W,
                        H,
                        n != 0,
                    )
                    .unwrap();
                }
                let page = read_rendered_pixels(app.device(), app.queue(), &texture, W, H);
                eprintln!("route={route},foreground={foreground},frame={n}");
                pixel(&page, (50, 60), if n == 0 { HALF } else { BLUE });
                pixel(&page, (110, 60), if n == 1 { HALF } else { BLUE });
                pixel(&page, (30, 25), BLUE);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_path_clip_native_entries_keep_path_and_scissor() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    for foreground in [false, true] {
        let mut paint = GpuPaintContext::new(W as f32, H as f32);
        paint.set_foreground_layer(foreground);
        paint.push_motion_subtree();
        paint.push_clip(ClipShape::rect(Rect::new(65.0, 20.0, 55.0, 90.0)));
        paint.push_clip(ClipShape::Path(curve(0)));
        paint.push_layer(LayerConfig::new().opacity(0.5));
        paint.draw_rgba_pixels(&RED, 1, 1, Rect::new(20.0, 20.0, 100.0, 80.0));
        paint.pop_layer();
        let batch = paint.take_dynamic_batch();
        for route in 0..6 {
            if route == 4 && foreground {
                continue;
            }
            let (w, h) = if route == 4 { (W / 2, H / 2) } else { (W, H) };
            let (texture, view) =
                create_test_texture(context.device(), context.texture_format(), w, h);
            context.renderer.clear_target(&view, wgpu::Color::BLUE);
            let projected = if foreground {
                batch.foreground_batch()
            } else {
                batch.clone()
            };
            match route {
                0 => {
                    context.cached_dynamic_batch = Some(batch.clone());
                    context.dispatch_dynamic_batch_overlay(&view);
                }
                1 => context.renderer.render_overlay_msaa(&view, &projected, 4),
                2 => {
                    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
                        label: Some("Clipped image glass backdrop"),
                        size: wgpu::Extent3d {
                            width: W / 2,
                            height: H / 2,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: context.texture_format(),
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    let backdrop = texture.create_view(&Default::default());
                    context.renderer.render_glass_frame(
                        &view,
                        &backdrop,
                        (W / 2, H / 2),
                        &batch,
                        true,
                    );
                }
                3 => context.renderer.render_overlay(&view, &projected),
                4 => context
                    .renderer
                    .render_to_backdrop(&view, (w, h), &batch, true),
                _ => {
                    let mut legacy = projected.dynamic_images;
                    for image in &mut legacy {
                        image.paint_order = None;
                        image.opacity = 0.5;
                    }
                    context
                        .renderer
                        .render_unordered_dynamic_images(&view, &legacy);
                }
            }
            let page = read_rendered_pixels(context.device(), context.queue(), &texture, w, h);
            let divide = if route == 4 { 2 } else { 1 };
            eprintln!("foreground={foreground},route={route}");
            for (xy, expected) in [
                ((70, 60), HALF),
                ((105, 60), BLUE),
                ((70, 25), BLUE),
                ((40, 60), BLUE),
            ] {
                pixel(&page, (xy.0 / divide, xy.1 / divide), expected);
            }
        }
    }
}
