use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{ClipShape, FillRule, LayerConfig, Point, Transform, Vec2, layer::Affine2D};

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
fn star() -> Vec<Point> {
    [0, 2, 4, 1, 3]
        .map(|n| {
            let angle = n as f32 * std::f32::consts::TAU / 5.0 - std::f32::consts::FRAC_PI_2;
            Point::new(40.0 + 38.0 * angle.cos(), 40.0 + 38.0 * angle.sin())
        })
        .to_vec()
}

fn frame_clip(paint: &mut dyn DrawContext, frame: usize) {
    if frame == 1 {
        paint.set_clip_fill_rule(FillRule::EvenOdd);
    }
    paint.push_clip(if frame < 2 {
        ClipShape::Polygon(star())
    } else {
        ClipShape::Ellipse {
            center: Point::new(65.0, 60.0),
            radii: Vec2::new(35.0, 22.0),
        }
    });
    paint.draw_rgba_pixels(&RED, 1, 1, Rect::new(25.0, 20.0, 80.0, 80.0));
    paint.pop_clip();
}

#[test]
fn rgba_clip_scratch_hash_changes_when_only_polygon_fill_rule_changes() {
    let batch = |frame| {
        let mut paint = GpuPaintContext::new(W as f32, H as f32);
        frame_clip(&mut paint, frame);
        paint.take_batch()
    };
    let nonzero = batch(0);
    let even_odd = batch(1);
    assert_eq!(nonzero.aux_data, even_odd.aux_data);
    assert_eq!(
        nonzero.dynamic_images[0].data,
        even_odd.dynamic_images[0].data
    );
    assert_eq!(
        nonzero.dynamic_images[0].dest,
        even_odd.dynamic_images[0].dest
    );
    assert_ne!(
        RenderContext::hash_composite_scratch(&nonzero),
        RenderContext::hash_composite_scratch(&even_odd)
    );
}

#[test]
#[ignore] // Requires GPU
fn rgba_clip_curves_keep_scissor_and_tight_origin_in_each_owner() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    for route in 0..3 {
        for foreground in [false, true] {
            for ellipse in [false, true] {
                for scissor in [false, true] {
                    let mut paint = GpuPaintContext::new(W as f32, H as f32);
                    paint.set_foreground_layer(foreground);
                    if route == 1 {
                        paint.push_motion_subtree();
                    } else if route == 2 {
                        paint.push_composite_layer(7);
                    }
                    if scissor {
                        paint.push_clip(ClipShape::rect(Rect::new(65.0, 20.0, 55.0, 90.0)));
                    }
                    paint.push_clip(if ellipse {
                        ClipShape::Ellipse {
                            center: Point::new(70.0, 60.0),
                            radii: Vec2::new(40.0, 22.0),
                        }
                    } else {
                        ClipShape::Circle {
                            center: Point::new(70.0, 60.0),
                            radius: 30.0,
                        }
                    });
                    paint.push_layer(LayerConfig::new().opacity(0.5));
                    paint.draw_rgba_pixels(&RED, 1, 1, Rect::new(20.0, 20.0, 100.0, 80.0));
                    paint.pop_layer();
                    let batch = match route {
                        1 => paint.take_dynamic_batch(),
                        2 => paint.take_composite_layer_batches().remove(&7).unwrap(),
                        _ => paint.take_batch(),
                    };
                    let (texture, view) =
                        create_test_texture(context.device(), context.texture_format(), W, H);
                    context
                        .renderer
                        .render_with_clear(&view, &batch, [0.0, 0.0, 1.0, 1.0]);
                    context.renderer.render_foreground(&view, &batch);
                    let page =
                        read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
                    eprintln!(
                        "route={route},foreground={foreground},ellipse={ellipse},scissor={scissor}"
                    );
                    pixel(&page, (70, 60), HALF);
                    pixel(&page, (25, 25), BLUE);
                    pixel(&page, (110, 90), BLUE);
                    pixel(&page, (50, 60), if scissor { BLUE } else { HALF });
                    pixel(&page, (105, 60), if ellipse { HALF } else { BLUE });
                }
            }
        }
    }
}

// Independent ray crossings and segment distance keep the oracle away from AA edges.
fn polygon_reference(page: &image::RgbaImage, matrix: [f32; 6], even_odd: bool) {
    let points = star();
    let [a, b, c, d, tx, ty] = matrix;
    let det = a * d - b * c;
    let mut samples = [0; 2];
    for y in (4..H).step_by(3) {
        for x in (4..W).step_by(3) {
            let px = x as f32 + 0.5 - tx;
            let py = y as f32 + 0.5 - ty;
            let u = (d * px - c * py) / det - 25.0;
            let v = (-b * px + a * py) / det - 20.0;
            if !(2.0..78.0).contains(&u) || !(2.0..78.0).contains(&v) {
                continue;
            }
            let mut winding = 0_i32;
            let mut distance = f32::INFINITY;
            for n in 0..points.len() {
                let p = points[n];
                let q = points[(n + 1) % points.len()];
                let dx = q.x - p.x;
                let dy = q.y - p.y;
                let side = dx * (v - p.y) - dy * (u - p.x);
                if p.y <= v && q.y > v && side > 0.0 {
                    winding += 1;
                } else if p.y > v && q.y <= v && side < 0.0 {
                    winding -= 1;
                }
                let t = (((u - p.x) * dx + (v - p.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                distance = distance.min((u - p.x - t * dx).hypot(v - p.y - t * dy));
            }
            if distance < 2.5 {
                continue;
            }
            let inside = if even_odd {
                winding % 2 != 0
            } else {
                winding != 0
            };
            pixel(page, (x, y), if inside { HALF } else { BLUE });
            samples[usize::from(inside)] += 1;
        }
    }
    assert!(samples.into_iter().all(|n| n > 10), "{samples:?}");
}

#[test]
#[ignore] // Requires GPU
fn rgba_clip_polygon_retains_local_affine_dpi_fill_rule_and_drained_payload() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    for foreground in [false, true] {
        for even_odd in [false, true] {
            for matrix in [
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                [1.0, 0.15, 0.4, 1.0, 0.0, 0.0],
                [0.0, 1.0, -1.0, 0.0, 145.0, 0.0],
                [-1.0, 0.0, 0.0, 1.0, 150.0, 0.0],
                [1.4, 0.0, 0.0, 1.4, 0.0, 0.0],
            ] {
                let dpi = 1.4;
                let mut paint = GpuPaintContext::new(W as f32, H as f32);
                paint.push_transform(Transform::scale(dpi, dpi));
                paint.set_foreground_layer(foreground);
                paint.push_transform(Transform::Affine2D(Affine2D {
                    elements: matrix.map(|v| v / dpi),
                }));
                if even_odd {
                    paint.set_clip_fill_rule(FillRule::EvenOdd);
                }
                paint.push_clip(ClipShape::Polygon(star()));
                for _ in 0..2 {
                    paint.push_motion_subtree();
                    paint.push_layer(LayerConfig::new().opacity(0.5));
                    paint.draw_rgba_pixels(&RED, 1, 1, Rect::new(25.0, 20.0, 80.0, 80.0));
                    paint.pop_layer();
                    paint.pop_motion_subtree();
                    let mut batch = paint.take_dynamic_batch();
                    if foreground {
                        batch = batch.take_foreground();
                    }
                    // Image-only foreground extraction must retain its payload independently.
                    batch.aux_data.clear();
                    batch.aux_data.push([999.0; 4]);
                    let (texture, view) =
                        create_test_texture(context.device(), context.texture_format(), W, H);
                    context
                        .renderer
                        .render_with_clear(&view, &batch, [0.0, 0.0, 1.0, 1.0]);
                    let page =
                        read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
                    eprintln!("foreground={foreground},even_odd={even_odd},matrix={matrix:?}");
                    polygon_reference(&page, matrix, even_odd);
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_clip_application_frames_replay_changed_shapes_in_all_entries() {
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
                pixel(&page, (65, 60), if n == 1 { BLUE } else { HALF });
                pixel(&page, (64, 34), if n == 2 { BLUE } else { HALF });
                pixel(&page, (30, 25), BLUE);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_clip_native_entries_keep_screen_scissor_and_ellipse_frame() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    for foreground in [false, true] {
        let mut paint = GpuPaintContext::new(W as f32, H as f32);
        paint.set_foreground_layer(foreground);
        paint.push_motion_subtree();
        paint.push_clip(ClipShape::rect(Rect::new(65.0, 20.0, 55.0, 90.0)));
        paint.push_clip(ClipShape::Ellipse {
            center: Point::new(70.0, 60.0),
            radii: Vec2::new(40.0, 22.0),
        });
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
                ((105, 60), HALF),
                ((70, 30), BLUE),
                ((40, 60), BLUE),
            ] {
                pixel(&page, (xy.0 / divide, xy.1 / divide), expected);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_clip_scratch_bake_preserves_polygon_origin_and_strips_ancestors() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    let mut paint = GpuPaintContext::new(W as f32, H as f32);
    // Scratch strips ancestor clips, including shaped clips.
    paint.push_clip(ClipShape::Circle {
        center: Point::new(0.0, 0.0),
        radius: 1.0,
    });
    paint.push_composite_layer(7);
    paint.push_layer(LayerConfig::new().opacity(0.5));
    frame_clip(&mut paint, 1);
    paint.pop_layer();
    paint.push_clip(ClipShape::Polygon(vec![
        Point::new(0.0, 0.0),
        Point::new(30.0, 0.0),
        Point::new(0.0, 30.0),
    ]));
    paint.fill_rect(
        Rect::new(130.0, 30.0, 30.0, 30.0),
        blinc_core::CornerRadius::ZERO,
        Color::rgba(0.0, 1.0, 0.0, 1.0).into(),
    );
    let batch = paint.take_composite_layer_batches().remove(&7).unwrap();
    let (bake, size) =
        context
            .renderer
            .render_subtree_to_layer_texture(&batch, (25.0, 20.0), (135.0, 80.0));
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), W, H);
    context.renderer.clear_target(&view, wgpu::Color::BLUE);
    context.renderer.blit_tight_texture_to_target(
        &bake.view,
        size,
        &view,
        (25.0, 20.0),
        (135.0, 80.0),
        1.0,
        blinc_core::BlendMode::Normal,
        None,
        None,
        0.0,
    );
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
    polygon_reference(&page, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], true);
    // The primitive after the image reads the batch's polygon rows, not the image's header.
    pixel(&page, (135, 35), [0, 255, 0, 255]);
    pixel(&page, (155, 55), BLUE);
    context.renderer.layer_texture_cache_mut().release(bake);
}
