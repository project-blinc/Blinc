use super::*;
use crate::app::{BlincApp, BlincConfig};
use crate::tests::{create_test_texture, read_rendered_pixels};
use blinc_core::{LayerConfig, Transform, layer::Affine2D};
const W: u32 = 180;
const H: u32 = 150;
const RECT: [f32; 4] = [20.0, 20.0, 60.0, 40.0];
const MATRICES: [[f32; 6]; 6] = [
    [0.0, 1.0, -1.0, 0.0, 130.0, 0.0],
    [1.0, 0.3, 0.7, 1.0, 0.0, 0.0],
    [1.8, 0.0, 0.0, 0.55, 0.0, 20.0],
    [-1.0, 0.0, 0.0, 1.0, 135.0, 0.0],
    [
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
        -std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
        80.0,
        0.0,
    ],
    [0.0, 2.0, -2.0, 0.0, 160.0, -10.0],
];
fn app() -> BlincApp {
    BlincApp::with_config(BlincConfig {
        sample_count: 1,
        ..Default::default()
    })
    .expect("offscreen GPU required")
}
fn pixels() -> Vec<u8> {
    let colors = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [255, 255, 0, 255],
        [255, 255, 255, 255],
    ];
    let mut out = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            out.extend_from_slice(&colors[(y / 4) * 2 + x / 4]);
        }
    }
    out
}
fn emit(paint: &mut GpuPaintContext<'_>, matrix: [f32; 6]) {
    paint.push_transform(Transform::Affine2D(Affine2D { elements: matrix }));
    paint.draw_rgba_pixels(
        &pixels(),
        8,
        8,
        Rect::new(RECT[0], RECT[1], RECT[2], RECT[3]),
    );
    paint.pop_transform();
}
fn pixel(page: &image::RgbaImage, xy: (u32, u32), expected: [u8; 4]) {
    let actual = page.get_pixel(xy.0, xy.1).0;
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
        "{xy:?}: {actual:?}, expected {expected:?}"
    );
}
fn reference(page: &image::RgbaImage, matrix: [f32; 6], opacity: f32) {
    let [a, b, c, d, tx, ty] = matrix;
    let det = a * d - b * c;
    let colors = [
        [255.0, 0.0, 0.0],
        [0.0, 255.0, 0.0],
        [255.0, 255.0, 0.0],
        [255.0, 255.0, 255.0],
    ];
    let mut inside = 0;
    for y in (3..H).step_by(3) {
        for x in (3..W).step_by(3) {
            let px = x as f32 + 0.5 - tx;
            let py = y as f32 + 0.5 - ty;
            let u = ((d * px - c * py) / det - RECT[0]) / RECT[2];
            let v = ((-b * px + a * py) / det - RECT[1]) / RECT[3];
            if (0.1..0.9).contains(&u)
                && (0.1..0.9).contains(&v)
                && (u - 0.5).abs() > 0.2
                && (v - 0.5).abs() > 0.2
            {
                let color = colors[(usize::from(v > 0.5)) * 2 + usize::from(u > 0.5)];
                pixel(
                    page,
                    (x, y),
                    [
                        (color[0] * opacity).round() as u8,
                        (color[1] * opacity).round() as u8,
                        (color[2] * opacity + 255.0 * (1.0 - opacity)).round() as u8,
                        255,
                    ],
                );
                inside += 1;
            } else if !(-0.1..=1.1).contains(&u) || !(-0.1..=1.1).contains(&v) {
                pixel(page, (x, y), [0, 0, 255, 255]);
            }
        }
    }
    assert!(inside > 6, "reference needs interior samples: {matrix:?}");
}

#[test]
#[ignore] // Requires GPU
fn rgba_affine_public_background_and_foreground_preserve_texture_orientation() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    for foreground in [false, true] {
        for matrix in MATRICES {
            let mut paint = GpuPaintContext::new(W as f32, H as f32);
            paint.set_foreground_layer(foreground);
            emit(&mut paint, matrix);
            let batch = paint.take_batch();
            let (texture, view) =
                create_test_texture(context.device(), context.texture_format(), W, H);
            context
                .renderer
                .render_with_clear(&view, &batch, [0.0, 0.0, 1.0, 1.0]);
            context.renderer.render_foreground(&view, &batch);
            let page = read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
            eprintln!("foreground={foreground},matrix={matrix:?}");
            reference(&page, matrix, 1.0);
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_affine_grouped_scratch_bounds_cover_the_transformed_quad() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    let matrix = MATRICES[1];
    let mut paint = GpuPaintContext::new(W as f32, H as f32);
    paint.push_composite_layer(7);
    paint.push_layer(LayerConfig::new().opacity(0.5));
    emit(&mut paint, matrix);
    paint.pop_layer();
    let bounds = paint.composite_layer_bounds().unwrap();
    for (actual, expected) in bounds.into_iter().zip([34.0, 26.0, 88.0, 58.0]) {
        assert!((actual - expected).abs() < 0.001, "{bounds:?}");
    }
    paint.pop_composite_layer();
    let batch = paint.take_composite_layer_batches().remove(&7).unwrap();
    let (bake, size) = context.renderer.render_subtree_to_layer_texture(
        &batch,
        (bounds[0], bounds[1]),
        (bounds[2], bounds[3]),
    );
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), W, H);
    context.renderer.clear_target(&view, wgpu::Color::BLUE);
    context.renderer.blit_tight_texture_to_target(
        &bake.view,
        size,
        &view,
        (bounds[0], bounds[1]),
        (bounds[2], bounds[3]),
        1.0,
        blinc_core::BlendMode::Normal,
        None,
        None,
        0.0,
    );
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
    reference(&page, matrix, 0.5);
    context.renderer.layer_texture_cache_mut().release(bake);
}

#[test]
#[ignore] // Requires GPU
fn rgba_affine_small_shear_is_not_rounded_to_identity() {
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    let mut paint = GpuPaintContext::new(W as f32, H as f32);
    paint.push_transform(Transform::Affine2D(Affine2D {
        elements: [1.0, 0.0, 0.00005, 1.0, 0.0, 0.0],
    }));
    paint.draw_rgba_pixels(
        &[255, 0, 0, 255],
        1,
        1,
        Rect::new(20.0, -49920.0, 60.0, 50000.0),
    );
    paint.pop_transform();
    let batch = paint.take_batch();
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), W, H);
    context
        .renderer
        .render_with_clear(&view, &batch, [0.0, 0.0, 1.0, 1.0]);
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
    pixel(&page, (19, 60), [0, 0, 255, 255]);
    pixel(&page, (22, 60), [255, 0, 0, 255]);
}

#[test]
fn rgba_affine_cache_hash_tracks_orientation_with_unchanged_dest() {
    let mut paint = GpuPaintContext::new(W as f32, H as f32);
    emit(&mut paint, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let mut batch = paint.take_batch();
    let hash = RenderContext::hash_composite_scratch(&batch);
    let dest = batch.dynamic_images[0].dest;
    batch.dynamic_images[0].transform = [-1.0, 0.0, 0.0, 1.0];
    assert_eq!(batch.dynamic_images[0].dest, dest);
    assert_ne!(hash, RenderContext::hash_composite_scratch(&batch));
}

#[test]
#[ignore] // Requires GPU
fn rgba_affine_application_frames_repaint_when_only_orientation_changes() {
    use blinc_animation::AnimationScheduler;
    use std::sync::atomic::{AtomicUsize, Ordering};
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Light,
    );
    let matrices = [
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        MATRICES[0],
        [-1.0, 0.0, 0.0, 1.0, 100.0, 0.0],
    ];
    for route in 0..3 {
        for foreground in [false, true] {
            let mut app = app();
            let frame = Arc::new(AtomicUsize::new(0));
            let current = frame.clone();
            let ui = div().w(W as f32).h(H as f32).bg(Color::BLUE).child(
                canvas(move |ctx, _| {
                    ctx.set_foreground_layer(foreground);
                    ctx.push_layer(LayerConfig::new().opacity(0.5));
                    ctx.push_transform(Transform::Affine2D(Affine2D {
                        elements: matrices[current.load(Ordering::Relaxed)],
                    }));
                    ctx.draw_rgba_pixels(
                        &pixels(),
                        8,
                        8,
                        Rect::new(RECT[0], RECT[1], RECT[2], RECT[3]),
                    );
                    ctx.pop_transform();
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
            for (n, matrix) in matrices.into_iter().enumerate() {
                frame.store(n, Ordering::Relaxed);
                let texture = app.device().create_texture(&wgpu::TextureDescriptor {
                    label: Some("Affine RGBA surface"),
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
                reference(&page, matrix, 0.5);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn rgba_affine_clipped_motion_msaa_glass_and_reduced_backdrop_keep_screen_frame() {
    use blinc_core::ClipShape;
    let mut app = app();
    let context = app.context();
    context.renderer.resize(W, H);
    for foreground in [false, true] {
        let mut paint = GpuPaintContext::new(W as f32, H as f32);
        paint.set_foreground_layer(foreground);
        paint.push_motion_subtree();
        paint.push_clip(ClipShape::rect(Rect::new(80.0, 20.0, 20.0, 80.0)));
        paint.push_layer(LayerConfig::new().opacity(0.5));
        emit(&mut paint, MATRICES[0]);
        paint.pop_layer();
        paint.pop_clip();
        paint.pop_motion_subtree();
        let batch = paint.take_dynamic_batch();
        for route in 0..5 {
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
                    let backdrop = context.device().create_texture(&wgpu::TextureDescriptor {
                        label: Some("Affine glass backdrop"),
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
                    context.renderer.render_glass_frame(
                        &view,
                        &backdrop.create_view(&Default::default()),
                        (W / 2, H / 2),
                        &batch,
                        true,
                    );
                }
                3 => context.renderer.render_overlay(&view, &projected),
                _ => context
                    .renderer
                    .render_to_backdrop(&view, (w, h), &batch, true),
            }
            let page = read_rendered_pixels(context.device(), context.queue(), &texture, w, h);
            let divide = if route == 4 { 2 } else { 1 };
            eprintln!("native foreground={foreground},route={route}");
            pixel(&page, (85 / divide, 30 / divide), [128, 128, 127, 255]);
            pixel(&page, (95 / divide, 65 / divide), [0, 128, 127, 255]);
            pixel(&page, (75 / divide, 30 / divide), [0, 0, 255, 255]);
        }
    }
    // Invalid manually supplied geometry is rejected before upload, including in layers.
    let mut invalid = GpuPaintContext::new(W as f32, H as f32);
    invalid.push_layer(LayerConfig::new().opacity(0.5));
    emit(&mut invalid, MATRICES[0]);
    invalid.pop_layer();
    let mut invalid = invalid.take_batch();
    invalid.dynamic_images[0].transform[0] = f32::NAN;
    let (texture, view) = create_test_texture(context.device(), context.texture_format(), W, H);
    context
        .renderer
        .render_with_clear(&view, &invalid, [0.0, 0.0, 1.0, 1.0]);
    context
        .renderer
        .render_dynamic_images(&view, &invalid.dynamic_images);
    let page = read_rendered_pixels(context.device(), context.queue(), &texture, W, H);
    pixel(&page, (90, 50), [0, 0, 255, 255]);
}
