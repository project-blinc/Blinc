use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{Affine2D, ClipShape, FillRule, Path, Point, Transform};
const W: u32 = 160;
const H: u32 = 130;
const BLUE: [u8; 4] = [0, 0, 255, 255];
fn app() -> BlincApp {
    BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required")
}
fn glyph(color: bool) -> GpuGlyph {
    GpuGlyph {
        bounds: [10.0, 10.0, 140.0, 110.0],
        uv_bounds: [0.0, 0.0, 2.0, 2.0],
        color: [1.0, 0.0, 0.0, 1.0],
        flags: [f32::from(u8::from(color)), 0.0, 0.0, 0.0],
        ..Default::default()
    }
}
fn atlas(c: &RenderContext, color: [u8; 4]) -> (wgpu::Texture, wgpu::TextureView) {
    let t = c.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("Prepared glyph test atlas"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    c.queue().write_texture(
        t.as_image_copy(),
        &color.repeat(4),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    let v = t.create_view(&Default::default());
    (t, v)
}
fn render(c: &mut RenderContext, b: &PrimitiveBatch, unified: bool) -> image::RgbaImage {
    let (t, v) = create_test_texture(c.device(), c.texture_format(), W, H);
    let (_gray, gray) = atlas(c, [255; 4]);
    let (_color, color) = atlas(c, [255, 0, 0, 255]);
    let sampler = c.device().create_sampler(&Default::default());
    c.renderer.clear_target(&v, wgpu::Color::BLUE);
    if unified {
        c.renderer.update_aux_data_for_batch(b);
        c.renderer.render_primitives_overlay_with_glyphs(
            &v,
            &b.get_unified_foreground_primitives(),
            &gray,
            &color,
        );
    } else {
        c.renderer.render_text_batch(&v, b, &gray, &color, &sampler);
    }
    read_rendered_pixels(c.device(), c.queue(), &t, W, H)
}
#[test]
#[ignore] // Requires GPU
fn prepared_glyph_clip_direct_and_unified_read_the_same_path() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    for color in [false, true] {
        let mut p = GpuPaintContext::new(W as f32, H as f32);
        p.push_clip(ClipShape::Path(Path::circle(Point::new(75.0, 65.0), 35.0)));
        p.push_clip(ClipShape::rect(Rect::new(65.0, 20.0, 70.0, 90.0)));
        p.push_prepared_glyphs(&[glyph(color)]);
        let b = p.take_batch();
        for unified in [false, true] {
            let page = render(c, &b, unified);
            let mut inside = 0;
            let mut outside = 0;
            for y in 15..H - 15 {
                for x in 15..W - 15 {
                    let d = (x as f32 + 0.5 - 75.0).hypot(y as f32 + 0.5 - 65.0);
                    if (d - 35.0).abs() < 2.0 || (x as f32 - 65.0).abs() < 2.0 {
                        continue;
                    }
                    let keep = d < 35.0 && x > 65;
                    let expected = if keep {
                        inside += 1;
                        [255, 0, 0, 255]
                    } else {
                        outside += 1;
                        BLUE
                    };
                    assert_eq!(
                        page.get_pixel(x, y).0,
                        expected,
                        "({x},{y}) color={color} unified={unified}"
                    );
                }
            }
            assert!(inside > 50 && outside > 50);
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
fn prepared_glyph_clip_push_frames_fill_rules_and_owners() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    for matrix in [
        [1.0, 0.1, 0.3, 1.0, 0.0, 0.0],
        [0.0, 1.0, -1.0, 0.0, 140.0, 0.0],
        [-1.1, 0.0, 0.0, 1.1, 145.0, 0.0],
    ] {
        for rule in [FillRule::NonZero, FillRule::EvenOdd] {
            for owner in 0..3 {
                let mut p = GpuPaintContext::new(W as f32, H as f32);
                if owner == 1 {
                    p.push_motion_subtree();
                }
                if owner == 2 {
                    p.push_composite_layer(7);
                }
                p.push_transform(Transform::scale(2.0, 2.0));
                p.push_transform(Transform::Affine2D(Affine2D {
                    elements: matrix.map(|v| v * 0.5),
                }));
                p.set_clip_fill_rule(rule);
                p.push_clip(ClipShape::Path(rings()));
                p.pop_transform();
                p.pop_transform();
                p.push_prepared_glyphs(&[glyph(owner == 1)]);
                let b = match owner {
                    1 => p.take_dynamic_batch(),
                    2 => p.take_composite_layer_batches().remove(&7).unwrap(),
                    _ => p.take_batch(),
                };
                for unified in [false, true] {
                    let page = render(c, &b, unified);
                    let [a, b, cc, d, tx, ty] = matrix;
                    let det = a * d - b * cc;
                    let mut counts = [0; 2];
                    for y in 15..H - 15 {
                        for x in 15..W - 15 {
                            let u = (d * (x as f32 + 0.5 - tx) - cc * (y as f32 + 0.5 - ty)) / det;
                            let v = (-b * (x as f32 + 0.5 - tx) + a * (y as f32 + 0.5 - ty)) / det;
                            if [20.0, 100.0, 45.0, 75.0]
                                .into_iter()
                                .any(|e| (u - e).abs() < 3.0 || (v - e).abs() < 3.0)
                            {
                                continue;
                            }
                            let keep = (20.0..100.0).contains(&u)
                                && (20.0..100.0).contains(&v)
                                && !(rule == FillRule::EvenOdd
                                    && (45.0..75.0).contains(&u)
                                    && (45.0..75.0).contains(&v));
                            counts[usize::from(keep)] += 1;
                            assert_eq!(
                                page.get_pixel(x, y).0,
                                if keep { [255, 0, 0, 255] } else { BLUE },
                                "({x},{y}) matrix={matrix:?} rule={rule:?} owner={owner} unified={unified}"
                            );
                        }
                    }
                    assert!(counts.into_iter().all(|n| n > 50));
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn prepared_glyph_clip_merged_headers_follow_aux_growth_and_reuploads() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    let mut small = GpuPaintContext::new(W as f32, H as f32);
    small.push_clip(ClipShape::Path(Path::circle(Point::new(50.0, 65.0), 20.0)));
    small.push_prepared_glyphs(&[glyph(false)]);
    let small = small.take_batch();
    render(c, &small, false);
    let mut out = PrimitiveBatch::new();
    out.aux_data = vec![[0.0; 4]; 2048];
    for x in [50.0, 110.0] {
        let mut p = GpuPaintContext::new(W as f32, H as f32);
        p.push_clip(ClipShape::Path(Path::circle(Point::new(x, 65.0), 20.0)));
        p.push_prepared_glyphs(&[glyph(x > 80.0)]);
        out.merge(p.take_batch());
    }
    for b in [&out, &small, &out] {
        for unified in [false, true] {
            c.renderer.update_aux_data_slice(&vec![[0.0; 4]; 4096]);
            let page = render(c, b, unified);
            for y in 15..H - 15 {
                for x in 15..W - 15 {
                    let d0 = (x as f32 + 0.5 - 50.0).hypot(y as f32 + 0.5 - 65.0);
                    let d1 = if b.glyphs.len() == 2 {
                        (x as f32 + 0.5 - 110.0).hypot(y as f32 + 0.5 - 65.0)
                    } else {
                        f32::INFINITY
                    };
                    let d = d0.min(d1);
                    if (d - 20.0).abs() < 2.0 {
                        continue;
                    }
                    assert_eq!(
                        page.get_pixel(x, y).0,
                        if d < 20.0 { [255, 0, 0, 255] } else { BLUE }
                    );
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn prepared_glyph_clip_empty_invalid_and_legacy_unclipped() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    for path in [
        Path::new(),
        Path::new()
            .move_to(20.0, 20.0)
            .quad_to(f32::NAN, 60.0, 100.0, 100.0),
    ] {
        for color in [false, true] {
            let mut p = GpuPaintContext::new(W as f32, H as f32);
            p.push_clip(ClipShape::Path(path.clone()));
            p.push_prepared_glyphs(&[glyph(color)]);
            let b = p.take_batch();
            for unified in [false, true] {
                assert!(render(c, &b, unified).pixels().all(|p| p.0 == BLUE));
            }
        }
    }
    // A zero header must never consume the previous draw's empty contour rows.
    let mut b = PrimitiveBatch::new();
    b.glyphs.push(glyph(false));
    for unified in [false, true] {
        assert_eq!(render(c, &b, unified).get_pixel(75, 65).0, [255, 0, 0, 255]);
    }
}

#[test]
#[ignore] // Requires GPU
fn prepared_glyph_clip_atlas_replacement_and_damage_scissor() {
    let mut app = app();
    let c = app.context();
    c.renderer.resize(W, H);
    let mut p = GpuPaintContext::new(W as f32, H as f32);
    p.push_clip(ClipShape::Path(Path::circle(Point::new(75.0, 65.0), 35.0)));
    p.push_prepared_glyphs(&[glyph(true)]);
    let b = p.take_batch();
    let (_gray, gray) = atlas(c, [255; 4]);
    let (_red, mut color) = atlas(c, [255, 0, 0, 255]);
    let sampler = c.device().create_sampler(&Default::default());
    let view_address = &color as *const wgpu::TextureView;
    for (n, expected) in [[255, 0, 0, 255], [0, 255, 0, 255], [255, 0, 0, 255]]
        .into_iter()
        .enumerate()
    {
        let (_texture, replacement) = atlas(c, expected);
        color = replacement;
        assert_eq!(&color as *const wgpu::TextureView, view_address);
        let (t, v) = create_test_texture(c.device(), c.texture_format(), W, H);
        c.renderer.clear_target(&v, wgpu::Color::BLUE);
        if n == 1 {
            c.renderer.set_pending_scissor((75, 0, W - 75, H));
        }
        c.renderer
            .render_text_batch(&v, &b, &gray, &color, &sampler);
        c.renderer.clear_pending_scissor();
        let page = read_rendered_pixels(c.device(), c.queue(), &t, W, H);
        assert_eq!(page.get_pixel(80, 65).0, expected);
        assert_eq!(
            page.get_pixel(65, 65).0,
            if n == 1 { BLUE } else { expected }
        );
        assert_eq!(page.get_pixel(20, 20).0, BLUE);
    }
}
