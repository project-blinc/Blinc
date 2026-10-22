use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{
    Affine2D, ClipShape, FillRule, Gradient, GradientStop, LayerConfig, Path, Point, Shadow,
    TextStyle, Transform,
};
const W: u32 = 180;
const H: u32 = 150;
const BLUE: [u8; 4] = [0, 0, 255, 255];
fn app() -> BlincApp {
    BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required")
}
fn draw(p: &mut dyn DrawContext, kind: usize) {
    let r = Rect::new(10.0, 10.0, 150.0, 120.0);
    match kind {
        0 => p.fill_rect(r, CornerRadius::ZERO, Color::RED.into()),
        1 => p.fill_path(&Path::rect(r), Color::RED.into()),
        2 => p.fill_path(
            &Path::rect(r),
            Brush::Gradient(Gradient::linear_with_stops(
                Point::new(10.0, 10.0),
                Point::new(160.0, 130.0),
                vec![
                    GradientStop::new(0.0, Color::RED),
                    GradientStop::new(0.5, Color::GREEN),
                    GradientStop::new(1.0, Color::RED),
                ],
            )),
        ),
        3 => p.stroke_path(
            &Path::line(Point::new(10.0, 70.0), Point::new(160.0, 70.0)),
            &Stroke::new(60.0),
            Color::RED.into(),
        ),
        4 => p.draw_inner_shadow(
            Rect::new(30.0, 30.0, 100.0, 80.0),
            CornerRadius::ZERO,
            Shadow::new(0.0, 0.0, 20.0, Color::RED),
        ),
        6 => p.draw_shadow(
            Rect::new(60.0, 50.0, 35.0, 35.0),
            CornerRadius::ZERO,
            Shadow::new(0.0, 0.0, 12.0, Color::RED),
        ),
        _ => p.draw_text(
            "MMMMMMMM",
            Point::new(12.0, 80.0),
            &TextStyle::new(30.0).with_color(Color::RED),
        ),
    }
}
fn render(c: &mut RenderContext, b: &PrimitiveBatch) -> image::RgbaImage {
    c.rebind_glyph_atlas_for_overlay();
    let (t, v) = create_test_texture(c.device(), c.texture_format(), W, H);
    c.renderer.render_with_clear(&v, b, [0.0, 0.0, 1.0, 1.0]);
    read_rendered_pixels(c.device(), c.queue(), &t, W, H)
}
fn compare(
    page: &image::RgbaImage,
    baseline: &image::RgbaImage,
    member: impl Fn(f32, f32) -> Option<bool>,
) {
    let mut covered = [0; 2];
    for y in 5..H - 5 {
        for x in 5..W - 5 {
            let Some(inside) = member(x as f32 + 0.5, y as f32 + 0.5) else {
                continue;
            };
            let base = baseline.get_pixel(x, y).0;
            if base[0] > 10 || base[1] > 10 {
                covered[usize::from(inside)] += 1;
            }
            let expected = if inside { base } else { BLUE };
            let actual = page.get_pixel(x, y).0;
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
                "({x},{y}): {actual:?}, expected {expected:?}"
            );
        }
    }
    assert!(
        covered.into_iter().all(|n| n > 10),
        "insufficient painted samples: {covered:?}"
    );
}
#[test]
#[ignore] // Requires GPU
fn native_path_clip_shapes_meshes_shadows_and_text_follow_owners_and_scissors() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    for kind in 0..7 {
        let mut p = GpuPaintContext::with_text_context(W as f32, H as f32, &mut c.text_ctx);
        if kind != 6 {
            p.push_layer(LayerConfig::new().opacity(0.5));
        }
        draw(&mut p, kind);
        if kind != 6 {
            p.pop_layer();
        }
        let baseline_batch = p.take_batch();
        drop(p);
        let baseline = render(c, &baseline_batch);
        for route in 0..3 {
            for foreground in [false, true] {
                let mut p = GpuPaintContext::with_text_context(W as f32, H as f32, &mut c.text_ctx);
                p.set_foreground_layer(foreground);
                if route == 1 {
                    p.push_motion_subtree();
                }
                if route == 2 {
                    p.push_composite_layer(7);
                }
                p.push_clip(ClipShape::Path(Path::circle(Point::new(75.0, 70.0), 40.0)));
                p.push_clip(ClipShape::rect(Rect::new(60.0, 20.0, 65.0, 100.0)));
                if kind != 6 {
                    p.push_layer(LayerConfig::new().opacity(0.5));
                }
                draw(&mut p, kind);
                if kind != 6 {
                    p.pop_layer();
                }
                let mut b = match route {
                    1 => p.take_dynamic_batch(),
                    2 => p.take_composite_layer_batches().remove(&7).unwrap(),
                    _ => p.take_batch(),
                };
                if foreground && route != 2 {
                    b = b.take_foreground();
                }
                let page = render(c, &b);
                eprintln!("kind={kind},route={route},foreground={foreground}");
                compare(&page, &baseline, |x, y| {
                    let distance = (x - 75.0).hypot(y - 70.0);
                    if (distance - 40.0).abs() < 2.0 || (x - 60.0).abs() < 2.0 {
                        None
                    } else {
                        Some(distance < 40.0 && x > 60.0)
                    }
                });
            }
        }
    }
}
fn rings() -> Path {
    Path::new()
        .move_to(20.0, 20.0)
        .line_to(100.0, 20.0)
        .line_to(100.0, 100.0)
        .line_to(20.0, 100.0)
        .move_to(45.0, 45.0)
        .line_to(75.0, 45.0)
        .line_to(75.0, 75.0)
        .line_to(45.0, 75.0)
}
#[test]
#[ignore] // Requires GPU
fn native_path_clip_push_frame_fill_rules_and_tight_origins_survive_shared_rows() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    for matrix in [
        [1.0, 0.2, 0.45, 1.0, 5.0, 0.0],
        [0.0, 1.0, -1.0, 0.0, 145.0, 0.0],
        [-1.1, 0.0, 0.0, 1.1, 150.0, 0.0],
    ] {
        for rule in [FillRule::NonZero, FillRule::EvenOdd] {
            let mut p = GpuPaintContext::new(W as f32, H as f32);
            p.push_clip(ClipShape::Path(Path::new())); // Ancestor must be stripped.
            p.push_composite_layer(7);
            p.push_transform(Transform::scale(2.0, 2.0));
            p.push_transform(Transform::Affine2D(Affine2D {
                elements: matrix.map(|v| v * 0.5),
            }));
            p.set_clip_fill_rule(rule);
            p.push_clip(ClipShape::Path(rings()));
            p.pop_transform();
            p.pop_transform();
            p.push_layer(LayerConfig::new().opacity(0.5));
            draw(&mut p, 0);
            draw(&mut p, 1);
            p.pop_layer();
            let b = p.take_composite_layer_batches().remove(&7).unwrap();
            let (bake, size) =
                c.renderer
                    .render_subtree_to_layer_texture(&b, (10.0, 10.0), (150.0, 120.0));
            let (t, v) = create_test_texture(c.device(), c.texture_format(), W, H);
            c.renderer.clear_target(&v, wgpu::Color::BLUE);
            c.renderer.blit_tight_texture_to_target(
                &bake.view,
                size,
                &v,
                (10.0, 10.0),
                (150.0, 120.0),
                1.0,
                blinc_core::BlendMode::Normal,
                None,
                None,
                0.0,
            );
            let page = read_rendered_pixels(c.device(), c.queue(), &t, W, H);
            let baseline = image::RgbaImage::from_pixel(W, H, image::Rgba([128, 0, 127, 255]));
            let [a, b, c0, d, tx, ty] = matrix;
            let det = a * d - b * c0;
            compare(&page, &baseline, |x, y| {
                if !(13.0..157.0).contains(&x) || !(13.0..127.0).contains(&y) {
                    return None;
                }
                let u = (d * (x - tx) - c0 * (y - ty)) / det;
                let v = (-b * (x - tx) + a * (y - ty)) / det;
                if [20.0, 100.0, 45.0, 75.0]
                    .into_iter()
                    .any(|e| (u - e).abs() < 3.0 || (v - e).abs() < 3.0)
                {
                    return None;
                }
                let outer = (20.0..100.0).contains(&u) && (20.0..100.0).contains(&v);
                let inner = (45.0..75.0).contains(&u) && (45.0..75.0).contains(&v);
                Some(outer && !(inner && rule == FillRule::EvenOdd))
            });
            c.renderer.layer_texture_cache_mut().release(bake);
        }
    }
}

#[test]
fn native_path_clip_scratch_hash_tracks_contours_and_fill_rule() {
    let batch = |rule, x| {
        let mut p = GpuPaintContext::new(W as f32, H as f32);
        p.set_clip_fill_rule(rule);
        p.push_clip(ClipShape::Path(Path::circle(Point::new(x, 70.0), 40.0)));
        draw(&mut p, 0);
        p.take_batch()
    };
    let a = batch(FillRule::NonZero, 70.0);
    for b in [
        batch(FillRule::EvenOdd, 70.0),
        batch(FillRule::NonZero, 90.0),
    ] {
        assert_eq!(a.primitives[0].bounds, b.primitives[0].bounds);
        assert_ne!(
            RenderContext::hash_composite_scratch(&a),
            RenderContext::hash_composite_scratch(&b)
        );
    }
}

#[test]
#[ignore] // Requires GPU
fn native_path_clip_multiple_canvases_replay_changed_and_empty_clips() {
    use blinc_animation::AnimationScheduler;
    use std::sync::atomic::{AtomicUsize, Ordering};
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    for route in 0..3 {
        for foreground in [false, true] {
            for grouped in [false, true] {
                let mut app = app();
                let frame = Arc::new(AtomicUsize::new(0));
                let current = frame.clone();
                let ui = div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::BLUE)
                    .child(
                        canvas(|ctx, _| {
                            ctx.fill_path(
                                &Path::rect(Rect::new(200.0, 200.0, 10.0, 10.0)),
                                Color::GREEN.into(),
                            );
                        })
                        .absolute()
                        .w(W as f32)
                        .h(H as f32),
                    )
                    .child(
                        canvas(move |ctx, _| {
                            ctx.set_foreground_layer(foreground);
                            let n = current.load(Ordering::Relaxed);
                            let path = if n == 2 {
                                Path::new()
                            } else {
                                Path::circle(
                                    Point::new(if n == 0 { 50.0 } else { 110.0 }, 70.0),
                                    25.0,
                                )
                            };
                            ctx.push_clip(ClipShape::Path(path));
                            if grouped {
                                ctx.push_layer(LayerConfig::new().opacity(0.5));
                            }
                            draw(ctx, 0);
                            draw(ctx, 1);
                            if grouped {
                                ctx.pop_layer();
                            }
                            ctx.pop_clip();
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
                        label: Some("Native path canvas surface"),
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
                    eprintln!("route={route},foreground={foreground},grouped={grouped},frame={n}");
                    for (x, visible) in [(50, n == 0), (110, n == 1), (20, false)] {
                        let expected = if visible {
                            if grouped {
                                [128, 0, 127, 255]
                            } else {
                                [255, 0, 0, 255]
                            }
                        } else {
                            BLUE
                        };
                        let actual = page.get_pixel(x, 70).0;
                        assert!(
                            actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
                            "({x},70): {actual:?}, expected {expected:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn native_path_clip_empty_invalid_and_singular_clips_cover_no_pixels() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    let invalid = Path::new()
        .move_to(20.0, 20.0)
        .quad_to(f32::NAN, 60.0, 100.0, 100.0);
    for kind in [0, 1, 4, 5] {
        for (path, matrix) in [
            (Path::new(), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
            (invalid.clone(), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
            (rings(), [0.0; 6]),
        ] {
            let mut p = GpuPaintContext::with_text_context(W as f32, H as f32, &mut c.text_ctx);
            p.push_transform(Transform::Affine2D(Affine2D { elements: matrix }));
            p.push_clip(ClipShape::Path(path));
            p.pop_transform();
            draw(&mut p, kind);
            let b = p.take_batch();
            drop(p);
            let page = render(c, &b);
            assert!(
                page.pixels().all(|p| p.0 == BLUE),
                "kind={kind},matrix={matrix:?}"
            );
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn native_path_clip_merged_headers_reach_legacy_and_msaa_overlays() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    for foreground in [false, true] {
        let mut p = GpuPaintContext::new(W as f32, H as f32);
        p.set_foreground_layer(foreground);
        p.push_motion_subtree();
        p.push_clip(ClipShape::Path(Path::circle(Point::new(75.0, 70.0), 35.0)));
        p.push_clip(ClipShape::rect(Rect::new(60.0, 20.0, 65.0, 100.0)));
        p.push_layer(LayerConfig::new().opacity(0.5));
        draw(&mut p, 0);
        draw(&mut p, 1);
        p.pop_layer();
        let mut b = PrimitiveBatch::new();
        b.aux_data = vec![[999.0; 4]; 7];
        b.merge(p.take_dynamic_batch());
        if foreground {
            b = b.take_foreground();
        }
        for route in 0..3 {
            let (t, v) = create_test_texture(c.device(), c.texture_format(), W, H);
            c.renderer.clear_target(&v, wgpu::Color::BLUE);
            match route {
                0 => c.renderer.render_overlay(&v, &b),
                1 => c.renderer.render_overlay_msaa(&v, &b, 1),
                _ => c.renderer.render_overlay_msaa(&v, &b, 4),
            }
            let page = read_rendered_pixels(c.device(), c.queue(), &t, W, H);
            let baseline = image::RgbaImage::from_pixel(W, H, image::Rgba([128, 0, 127, 255]));
            eprintln!("foreground={foreground},route={route}");
            compare(&page, &baseline, |x, y| {
                if (x - 60.0).abs() < 2.0 || ((x - 75.0).hypot(y - 70.0) - 35.0).abs() < 2.0 {
                    None
                } else {
                    Some(x > 60.0 && (x - 75.0).hypot(y - 70.0) < 35.0)
                }
            });
        }
    }
}
