//! Raw-image mapping agrees with the complete affine in every active stream.
use blinc_core::{DrawContext, Rect, Transform, layer::Affine2D};
use blinc_gpu::GpuPaintContext;

#[test]
fn rgba_affine_all_streams_retain_corners_and_reflected_uv_axes() {
    for matrix in [
        [0.0, 2.0, -2.0, 0.0, 160.0, 10.0],
        [1.0, 0.3, 0.7, 1.0, 15.0, 20.0],
        [-2.0, 0.0, 0.0, 0.5, 135.0, 0.0],
    ] {
        for route in 0..5 {
            let mut paint = GpuPaintContext::new(180.0, 150.0);
            paint.set_foreground_layer(route % 2 == 1);
            if route >= 2 {
                paint.push_motion_subtree();
            }
            if route == 4 {
                paint.push_composite_layer(7);
            }
            paint.push_transform(Transform::Affine2D(Affine2D { elements: matrix }));
            paint.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(20.0, 20.0, 60.0, 40.0));
            paint.pop_transform();
            if route == 4 {
                paint.pop_composite_layer();
            }
            if route >= 2 {
                paint.pop_motion_subtree();
            }
            let batch = if route == 4 {
                paint.take_composite_layer_batches().remove(&7).unwrap()
            } else if route >= 2 {
                paint.take_dynamic_batch()
            } else {
                paint.take_batch()
            };
            let fg = batch.foreground_batch();
            let images = if route % 2 == 1 {
                &fg.dynamic_images
            } else {
                &batch.dynamic_images
            };
            assert_eq!(images.len(), 1);
            let image = &images[0];
            let [a, b, c, d] = image.transform;
            let [ma, mb, mc, md, tx, ty] = matrix;
            for (u, v) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.2, 0.7)] {
                let dx = (u - 0.5) * image.dest.width();
                let dy = (v - 0.5) * image.dest.height();
                let actual = [
                    image.dest.x() + image.dest.width() * 0.5 + a * dx + c * dy,
                    image.dest.y() + image.dest.height() * 0.5 + b * dx + d * dy,
                ];
                let local = [20.0 + u * 60.0, 20.0 + v * 40.0];
                let expected = [
                    ma * local[0] + mc * local[1] + tx,
                    mb * local[0] + md * local[1] + ty,
                ];
                for (a, b) in actual.into_iter().zip(expected) {
                    assert!(
                        (a - b).abs() < 0.0001,
                        "{route}: {actual:?} != {expected:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn rgba_affine_composed_dpi_and_translation_keep_the_screen_frame() {
    let mut paint = GpuPaintContext::new(180.0, 150.0);
    paint.push_transform(Transform::scale(2.0, 2.0));
    paint.push_transform(Transform::translate(12.0, 7.0));
    paint.push_transform(Transform::Affine2D(Affine2D {
        elements: [0.0, 1.0, -1.0, 0.0, 0.0, 0.0],
    }));
    paint.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(0.0, 0.0, 30.0, 20.0));
    let batch = paint.take_batch();
    let image = &batch.dynamic_images[0];
    assert_eq!(image.transform, [0.0, 1.0, -1.0, 0.0]);
    assert_eq!(image.screen_bounds(), Rect::new(-16.0, 14.0, 40.0, 60.0));
}

#[test]
fn rgba_affine_bounds_cover_all_corners_even_when_width_and_height_swap() {
    let mut paint = GpuPaintContext::new(180.0, 150.0);
    paint.push_transform(Transform::Affine2D(Affine2D {
        elements: [0.0, 1.0, -1.0, 0.0, 130.0, 0.0],
    }));
    paint.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(20.0, 20.0, 60.0, 40.0));
    let batch = paint.take_batch();
    assert_eq!(
        batch.dynamic_images[0].screen_bounds(),
        Rect::new(70.0, 20.0, 40.0, 60.0)
    );
}

#[test]
fn rgba_affine_invalid_and_near_singular_geometry_never_emits_pixels() {
    for matrix in [
        [0.0; 6],
        [1.0, 0.0, 0.0, 1e-14, 0.0, 0.0],
        [f32::NAN, 0.0, 0.0, 1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 1.0, f32::INFINITY, 0.0],
    ] {
        let mut paint = GpuPaintContext::new(180.0, 150.0);
        paint.push_transform(Transform::Affine2D(Affine2D { elements: matrix }));
        paint.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(20.0, 20.0, 60.0, 40.0));
        assert!(paint.take_batch().is_empty());
    }
    for rect in [
        Rect::new(f32::NAN, 0.0, 20.0, 20.0),
        Rect::new(0.0, 0.0, -20.0, 20.0),
        Rect::new(0.0, 0.0, 0.0, 20.0),
        Rect::new(0.0, 0.0, f32::MAX, f32::MAX),
    ] {
        let mut paint = GpuPaintContext::new(180.0, 150.0);
        paint.push_transform(Transform::scale(2.0, 2.0));
        paint.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, rect);
        assert!(paint.take_batch().is_empty());
    }
}
