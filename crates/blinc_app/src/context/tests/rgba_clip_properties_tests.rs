use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{ClipShape, CornerRadius, CornerShape, LayerConfig, Path, Point, Transform};
const W: u32 = 220;
const H: u32 = 200;
const RADII: [f32; 4] = [30.0, 25.0, 35.0, 20.0];
const SHAPES: [[f32; 4]; 2] = [[2.0, 0.0, -1.0, 100.0], [-100.0, 2.0, 100.0, -2.0]];
const FADE: [f32; 4] = [12.0, 28.0, 20.0, 36.0];
fn app() -> BlincApp {
    BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required")
}
fn image(ctx: &mut dyn DrawContext) {
    ctx.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(10.0, 10.0, 120.0, 120.0));
}
fn rounded(shapes: [f32; 4]) -> ClipShape {
    ClipShape::rounded_rect_shaped(
        Rect::new(20.0, 20.0, 100.0, 100.0),
        CornerRadius::new(RADII[0], RADII[1], RADII[2], RADII[3]),
        CornerShape::new(shapes[0], shapes[1], shapes[2], shapes[3]),
    )
}
fn pixel(page: &image::RgbaImage, x: u32, y: u32, alpha: f32) {
    let expected = [
        (255.0 * alpha).round() as u8,
        0,
        (255.0 * (1.0 - alpha)).round() as u8,
        255,
    ];
    let actual = page.get_pixel(x, y).0;
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
        "({x},{y}): {actual:?}, expected {expected:?}"
    );
}

// Membership in a Lamé corner, skipping a generous band around its boundary.
fn member(x: f32, y: f32, shapes: [f32; 4]) -> Option<bool> {
    if [20.0, 120.0]
        .into_iter()
        .any(|e| (x - e).abs() < 2.5 || (y - e).abs() < 2.5)
    {
        return None;
    }
    if !(20.0..120.0).contains(&x) || !(20.0..120.0).contains(&y) {
        return Some(false);
    }
    let index = match (y < 70.0, x > 70.0) {
        (true, false) => 0,
        (true, true) => 1,
        (false, true) => 2,
        (false, false) => 3,
    };
    let r = RADII[index];
    let n = shapes[index];
    let qx = (x - 70.0).abs() - 50.0;
    let qy = (y - 70.0).abs() - 50.0;
    if n >= 100.0 {
        return Some(true);
    }
    if n <= -100.0 {
        if (qx + r).abs() < 2.5 || (qy + r).abs() < 2.5 {
            return None;
        }
        return Some(!(qx + r > 0.0 && qy + r > 0.0));
    }
    let p = 2.0_f32.powf(n.abs());
    let sum = if n < 0.0 {
        (qx.abs() / r).powf(p) + (qy.abs() / r).powf(p)
    } else {
        ((qx + r).max(0.0) / r).powf(p) + ((qy + r).max(0.0) / r).powf(p)
    };
    if (sum - 1.0).abs() < 0.3 {
        return None;
    }
    Some(if n < 0.0 { sum > 1.0 } else { sum < 1.0 })
}

#[test]
#[ignore] // Requires GPU
fn rgba_clip_properties_asymmetric_corners_survive_dpi_owners_and_opacity() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    for route in 0..3 {
        for foreground in [false, true] {
            for scale in [1.0, 1.5] {
                for shapes in SHAPES {
                    for grouped in [false, true] {
                        let mut paint = GpuPaintContext::new(W as f32, H as f32);
                        paint.set_foreground_layer(foreground);
                        if route == 1 {
                            paint.push_motion_subtree();
                        }
                        if route == 2 {
                            paint.push_composite_layer(7);
                        }
                        paint.push_transform(Transform::scale(scale, scale));
                        paint.push_clip(rounded(shapes));
                        if grouped {
                            paint.push_layer(LayerConfig::new().opacity(0.5));
                        }
                        image(&mut paint);
                        if grouped {
                            paint.pop_layer();
                        }
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
                            "route={route},foreground={foreground},scale={scale},shapes={shapes:?},grouped={grouped}"
                        );
                        let mut samples = [0; 2];
                        for y in (15..(126.0 * scale) as u32).step_by(4) {
                            for x in (15..(126.0 * scale) as u32).step_by(4) {
                                if let Some(inside) = member(
                                    (x as f32 + 0.5) / scale,
                                    (y as f32 + 0.5) / scale,
                                    shapes,
                                ) {
                                    pixel(
                                        &page,
                                        x,
                                        y,
                                        if inside {
                                            if grouped { 0.5 } else { 1.0 }
                                        } else {
                                            0.0
                                        },
                                    );
                                    samples[usize::from(inside)] += 1;
                                }
                            }
                        }
                        assert!(samples.into_iter().all(|n| n > 10), "{samples:?}");
                    }
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_clip_properties_fades_keep_original_edges_with_scissors_and_shapes() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    for route in 0..3 {
        for foreground in [false, true] {
            for scale in [1.0, 1.5] {
                for style in 0..3 {
                    let mut paint = GpuPaintContext::new(W as f32, H as f32);
                    paint.set_foreground_layer(foreground);
                    if route == 1 {
                        paint.push_motion_subtree();
                    }
                    if route == 2 {
                        paint.push_composite_layer(7);
                    }
                    paint.push_transform(Transform::scale(scale, scale));
                    paint.set_overflow_fade(FADE);
                    paint.push_clip(if style == 1 {
                        rounded(SHAPES[0])
                    } else {
                        ClipShape::rect(Rect::new(20.0, 20.0, 100.0, 100.0))
                    });
                    if style == 2 {
                        paint
                            .push_clip(ClipShape::Path(Path::circle(Point::new(70.0, 70.0), 50.0)));
                    }
                    if style != 1 {
                        paint.push_clip(ClipShape::rect(Rect::new(30.0, 25.0, 80.0, 85.0)));
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
                    let (texture, view) =
                        create_test_texture(context.device(), context.texture_format(), W, H);
                    context
                        .renderer
                        .render_with_clear(&view, &batch, [0.0, 0.0, 1.0, 1.0]);
                    let page =
                        read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
                    eprintln!("route={route},foreground={foreground},scale={scale},style={style}");
                    for y in (15..(126.0 * scale) as u32).step_by(5) {
                        for x in (15..(126.0 * scale) as u32).step_by(5) {
                            let u = (x as f32 + 0.5) / scale;
                            let v = (y as f32 + 0.5) / scale;
                            let mut inside = if style == 1 {
                                let Some(inside) = member(u, v, SHAPES[0]) else {
                                    continue;
                                };
                                inside
                            } else {
                                if [30.0, 110.0].into_iter().any(|e| (u - e).abs() < 3.0)
                                    || [25.0, 110.0].into_iter().any(|e| (v - e).abs() < 3.0)
                                {
                                    continue;
                                }
                                (30.0..110.0).contains(&u) && (25.0..110.0).contains(&v)
                            };
                            if style == 2 {
                                let distance = (u - 70.0).hypot(v - 70.0);
                                if (distance - 50.0).abs() < 3.0 {
                                    continue;
                                }
                                inside &= distance < 50.0;
                            }
                            let fade = ((v - 20.0) / FADE[0]).clamp(0.0, 1.0)
                                * ((120.0 - u) / FADE[1]).clamp(0.0, 1.0)
                                * ((120.0 - v) / FADE[2]).clamp(0.0, 1.0)
                                * ((u - 20.0) / FADE[3]).clamp(0.0, 1.0);
                            pixel(&page, x, y, if inside { 0.5 * fade } else { 0.0 });
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn rgba_clip_properties_hash_tracks_corner_and_fade_only_changes() {
    let capture = |shape: f32, fade: f32| {
        let mut paint = GpuPaintContext::new(W as f32, H as f32);
        paint.set_overflow_fade([0.0, 0.0, 0.0, fade]);
        paint.push_clip(rounded([shape; 4]));
        image(&mut paint);
        paint.take_batch()
    };
    let first = capture(2.0, 10.0);
    let hash = RenderContext::hash_composite_scratch(&first);
    for next in [capture(0.0, 10.0), capture(2.0, 25.0)] {
        let a = &first.dynamic_images[0];
        let b = &next.dynamic_images[0];
        assert_eq!(a.data, b.data);
        assert_eq!(a.dest, b.dest);
        assert_eq!(a.clip_bounds, b.clip_bounds);
        assert_eq!(a.clip_radius, b.clip_radius);
        assert_eq!(first.aux_data, next.aux_data);
        assert_ne!(hash, RenderContext::hash_composite_scratch(&next));
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_clip_properties_tight_scratch_bake_strips_ancestor_fades() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    for intrinsic_fade in [false, true] {
        let mut paint = GpuPaintContext::new(W as f32, H as f32);
        paint.set_foreground_layer(true);
        paint.set_overflow_fade([1000.0; 4]);
        paint.push_clip(ClipShape::rect(Rect::new(0.0, 0.0, 1.0, 1.0)));
        paint.push_composite_layer(7);
        if intrinsic_fade {
            paint.set_overflow_fade(FADE);
        }
        paint.push_clip(rounded(SHAPES[0]));
        paint.push_layer(LayerConfig::new().opacity(0.5));
        image(&mut paint);
        paint.pop_layer();
        let batch = paint.take_composite_layer_batches().remove(&7).unwrap();
        let (bake, size) =
            context
                .renderer
                .render_subtree_to_layer_texture(&batch, (10.0, 10.0), (120.0, 120.0));
        let (texture, view) = create_test_texture(context.device(), context.texture_format(), W, H);
        context.renderer.clear_target(&view, wgpu::Color::BLUE);
        context.renderer.blit_tight_texture_to_target(
            &bake.view,
            size,
            &view,
            (10.0, 10.0),
            (120.0, 120.0),
            1.0,
            blinc_core::BlendMode::Normal,
            None,
            None,
            0.0,
        );
        let page = read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
        for y in (15..126).step_by(4) {
            for x in (15..126).step_by(4) {
                let u = x as f32 + 0.5;
                let v = y as f32 + 0.5;
                let Some(inside) = member(u, v, SHAPES[0]) else {
                    continue;
                };
                let fade = if intrinsic_fade {
                    ((v - 20.0) / FADE[0]).clamp(0.0, 1.0)
                        * ((120.0 - u) / FADE[1]).clamp(0.0, 1.0)
                        * ((120.0 - v) / FADE[2]).clamp(0.0, 1.0)
                        * ((u - 20.0) / FADE[3]).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                pixel(&page, x, y, if inside { 0.5 * fade } else { 0.0 });
            }
        }
        context.renderer.layer_texture_cache_mut().release(bake);
    }
}
