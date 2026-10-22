use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{Affine2D, BlurStyle, ClipShape, FillRule, GlassStyle, Path, Point, Transform};
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
    let style = GlassStyle {
        blur: 0.0,
        tint: Color::rgba(1.0, 0.0, 0.0, 0.7),
        noise: 0.0,
        border_thickness: 0.0,
        simple: kind == 0,
        depth: if kind == 4 { 1 } else { 0 },
        ..Default::default()
    };
    if kind == 2 {
        p.fill_rect(
            Rect::new(10.0, 10.0, 150.0, 120.0),
            CornerRadius::ZERO,
            Brush::Blur(BlurStyle::with_radius(0.0).tint(Color::RED)),
        );
    } else if kind == 3 {
        p.fill_circle(Point::new(75.0, 70.0), 65.0, Brush::Glass(style));
    } else {
        p.fill_rect(
            Rect::new(10.0, 10.0, 150.0, 120.0),
            CornerRadius::ZERO,
            Brush::Glass(style),
        );
    }
}
fn render(c: &mut RenderContext, b: &PrimitiveBatch, route: usize) -> image::RgbaImage {
    render_color(c, b, route, wgpu::Color::BLUE)
}
fn render_color(
    c: &mut RenderContext,
    b: &PrimitiveBatch,
    route: usize,
    color: wgpu::Color,
) -> image::RgbaImage {
    let (t, v) = create_test_texture(c.device(), c.texture_format(), W, H);
    let (w, h) = if route == 2 { (W / 2, H / 2) } else { (W, H) };
    let _back = c.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Glass sampleable backdrop"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: c.texture_format(),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let back = _back.create_view(&Default::default());
    c.renderer.clear_target(&v, wgpu::Color::BLUE);
    c.renderer.clear_target(&back, color);
    if route == 0 {
        c.renderer.render_glass(&v, &back, b);
    } else {
        c.renderer.render_glass_frame(&v, &back, (w, h), b, true);
    }
    read_rendered_pixels(c.device(), c.queue(), &t, W, H)
}
fn compare(
    page: &image::RgbaImage,
    baseline: &image::RgbaImage,
    member: impl Fn(f32, f32) -> Option<bool>,
) {
    let mut count = [0; 2];
    for y in 5..H - 5 {
        for x in 5..W - 5 {
            let Some(inside) = member(x as f32 + 0.5, y as f32 + 0.5) else {
                continue;
            };
            let base = baseline.get_pixel(x, y).0;
            if base[0] > 10 {
                count[usize::from(inside)] += 1;
            }
            let expected = if inside { base } else { BLUE };
            let actual = page.get_pixel(x, y).0;
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
                "({x},{y}): {actual:?}, expected {expected:?}"
            );
        }
    }
    assert!(count.into_iter().all(|n| n > 10), "{count:?}");
}
#[test]
#[ignore] // Requires GPU
fn glass_path_clip_materials_retain_scissors_in_all_owners() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    for kind in 0..5 {
        let mut p = GpuPaintContext::new(W as f32, H as f32);
        draw(&mut p, kind);
        let baseline = render(c, &p.take_batch(), 1);
        for owner in 0..3 {
            let mut p = GpuPaintContext::new(W as f32, H as f32);
            if owner == 1 {
                p.push_motion_subtree();
            }
            if owner == 2 {
                p.push_composite_layer(7);
            }
            p.push_clip(ClipShape::Path(Path::circle(Point::new(75.0, 70.0), 35.0)));
            p.push_clip(ClipShape::rect(Rect::new(60.0, 20.0, 65.0, 100.0)));
            draw(&mut p, kind);
            let b = match owner {
                1 => p.take_dynamic_batch(),
                2 => p.take_composite_layer_batches().remove(&7).unwrap(),
                _ => p.take_batch(),
            };
            let page = render(c, &b, 1);
            eprintln!("kind={kind},owner={owner}");
            compare(&page, &baseline, |x, y| {
                let d = (x - 75.0).hypot(y - 70.0);
                if (d - 35.0).abs() < 2.0 || (x - 60.0).abs() < 2.0 {
                    None
                } else {
                    Some(x > 60.0 && d < 35.0)
                }
            });
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
fn glass_path_clip_push_frame_and_fill_rule_survive_public_and_reduced_backdrops() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    for kind in [0, 1] {
        let mut p = GpuPaintContext::new(W as f32, H as f32);
        draw(&mut p, kind);
        let baseline = render(c, &p.take_batch(), 0);
        for matrix in [
            [1.0, 0.2, 0.45, 1.0, 5.0, 0.0],
            [0.0, 1.0, -1.0, 0.0, 145.0, 0.0],
            [-1.1, 0.0, 0.0, 1.1, 150.0, 0.0],
        ] {
            for rule in [FillRule::NonZero, FillRule::EvenOdd] {
                for route in 0..3 {
                    let mut p = GpuPaintContext::new(W as f32, H as f32);
                    p.push_transform(Transform::scale(2.0, 2.0));
                    p.push_transform(Transform::Affine2D(Affine2D {
                        elements: matrix.map(|v| v * 0.5),
                    }));
                    p.set_clip_fill_rule(rule);
                    p.push_clip(ClipShape::Path(rings()));
                    p.pop_transform();
                    p.pop_transform();
                    draw(&mut p, kind);
                    let page = render(c, &p.take_batch(), route);
                    let [a, b, c0, d, tx, ty] = matrix;
                    let det = a * d - b * c0;
                    compare(&page, &baseline, |x, y| {
                        let u = (d * (x - tx) - c0 * (y - ty)) / det;
                        let v = (-b * (x - tx) + a * (y - ty)) / det;
                        if [20.0, 100.0, 45.0, 75.0]
                            .into_iter()
                            .any(|e| (u - e).abs() < 3.0 || (v - e).abs() < 3.0)
                        {
                            None
                        } else {
                            Some(
                                (20.0..100.0).contains(&u)
                                    && (20.0..100.0).contains(&v)
                                    && !(rule == FillRule::EvenOdd
                                        && (45.0..75.0).contains(&u)
                                        && (45.0..75.0).contains(&v)),
                            )
                        }
                    });
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_path_clip_merge_and_instance_growth_keep_rebased_headers() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    let mut p = GpuPaintContext::new(W as f32, H as f32);
    draw(&mut p, 0);
    let baseline = render(c, &p.take_batch(), 0);
    let mut p = GpuPaintContext::new(W as f32, H as f32);
    for x in [50.0, 120.0] {
        p.push_clip(ClipShape::Path(Path::circle(Point::new(x, 70.0), 20.0)));
        draw(&mut p, 0);
        p.pop_clip();
    }
    let mut b = PrimitiveBatch::new();
    b.aux_data = vec![[0.0; 4]; 2048];
    b.merge(p.take_batch());
    for route in 0..3 {
        let page = render(c, &b, route);
        compare(&page, &baseline, |x, y| {
            let d = (x - 50.0).hypot(y - 70.0).min((x - 120.0).hypot(y - 70.0));
            if (d - 20.0).abs() < 2.0 {
                None
            } else {
                Some(d < 20.0)
            }
        });
    }
}
#[test]
#[ignore] // Requires GPU
fn glass_path_clip_empty_invalid_clips_never_draw() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    for path in [
        Path::new(),
        Path::new()
            .move_to(20.0, 20.0)
            .quad_to(f32::NAN, 60.0, 100.0, 100.0),
    ] {
        for kind in 0..5 {
            let mut p = GpuPaintContext::new(W as f32, H as f32);
            p.push_clip(ClipShape::Path(path.clone()));
            draw(&mut p, kind);
            let page = render(c, &p.take_batch(), 1);
            assert!(page.pixels().all(|p| p.0 == BLUE), "kind={kind}");
        }
    }
}
#[test]
#[ignore] // Requires GPU
fn glass_path_clip_bindings_follow_backdrop_changes_without_resizing() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    let mut p = GpuPaintContext::new(W as f32, H as f32);
    p.push_clip(ClipShape::Path(Path::circle(Point::new(75.0, 70.0), 35.0)));
    p.fill_rect(
        Rect::new(10.0, 10.0, 150.0, 120.0),
        CornerRadius::ZERO,
        Brush::Glass(GlassStyle {
            simple: true,
            blur: 0.0,
            noise: 0.0,
            border_thickness: 0.0,
            tint: Color::TRANSPARENT,
            ..Default::default()
        }),
    );
    let b = p.take_batch();
    for color in [wgpu::Color::RED, wgpu::Color::GREEN, wgpu::Color::RED] {
        for route in [0, 1] {
            let page = render_color(c, &b, route, color);
            let expected = [(color.r * 255.0) as u8, (color.g * 255.0) as u8, 0, 255];
            let actual = page.get_pixel(75, 70).0;
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
                "{actual:?} expected {expected:?}"
            );
            assert_eq!(page.get_pixel(20, 20).0, BLUE);
        }
    }
}
