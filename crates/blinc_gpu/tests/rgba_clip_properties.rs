//! Image-owned corner and fade rows survive stream moves and clip scoping.
use blinc_core::{ClipShape, CornerRadius, CornerShape, DrawContext, Rect, Transform};
use blinc_gpu::{GpuPaintContext, PrimitiveBatch};
fn draw(paint: &mut GpuPaintContext<'_>) {
    paint.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(10.0, 10.0, 120.0, 120.0));
}
fn rounded() -> ClipShape {
    ClipShape::rounded_rect_shaped(
        Rect::new(20.0, 20.0, 100.0, 100.0),
        CornerRadius::uniform(30.0),
        CornerShape::new(2.0, 0.0, -1.0, 100.0),
    )
}

#[test]
fn rgba_clip_properties_rows_keep_corners_and_original_fade_rectangle_after_scissoring() {
    let mut paint = GpuPaintContext::new(220.0, 200.0);
    paint.push_transform(Transform::scale(1.5, 1.5));
    paint.set_overflow_fade([12.0, 28.0, 20.0, 36.0]);
    paint.push_clip(rounded());
    draw(&mut paint);
    paint.push_clip(ClipShape::rect(Rect::new(30.0, 25.0, 80.0, 85.0)));
    draw(&mut paint);
    let batch = paint.take_batch();
    let first = &batch.dynamic_images[0];
    let corner = first.clip_aux[4][2] as usize - 1;
    assert_eq!(first.clip_aux[corner], [2.0, 0.0, -1.0, 100.0]);
    for image in &batch.dynamic_images {
        assert_eq!(image.clip_aux[2], [30.0, 30.0, 150.0, 150.0]);
        assert_eq!(image.clip_aux[3], [18.0, 42.0, 30.0, 54.0]);
        assert_eq!(image.clip_aux[4][3], 0.0);
    }
    assert_eq!(
        batch.dynamic_images[1].clip_bounds,
        [45.0, 37.5, 120.0, 127.5]
    );
}

#[test]
fn rgba_clip_properties_foreground_rows_survive_merge_move_and_drain() {
    let mut paint = GpuPaintContext::new(220.0, 200.0);
    paint.set_foreground_layer(true);
    paint.set_overflow_fade([10.0; 4]);
    paint.push_clip(rounded());
    let mut merged = PrimitiveBatch::new();
    for _ in 0..2 {
        paint.push_motion_subtree();
        draw(&mut paint);
        paint.pop_motion_subtree();
        let mut batch = paint.take_dynamic_batch();
        batch.aux_data.clear();
        merged.merge(batch);
    }
    let batch = merged.take_foreground();
    assert!(batch.aux_data.is_empty());
    assert_eq!(batch.dynamic_images.len(), 2);
    assert_eq!(
        batch.dynamic_images[0].clip_aux,
        batch.dynamic_images[1].clip_aux
    );
    assert_eq!(batch.dynamic_images[0].clip_aux.len(), 6);
}

#[test]
fn rgba_clip_properties_fade_scope_strips_ancestors_and_uses_native_draw_scale() {
    let mut paint = GpuPaintContext::new(220.0, 200.0);
    paint.set_overflow_fade([10.0, 20.0, 30.0, 40.0]);
    paint.push_clip(ClipShape::rect(Rect::new(20.0, 20.0, 100.0, 100.0)));
    paint.push_composite_layer(7);
    draw(&mut paint);
    paint.pop_composite_layer();
    assert!(
        paint
            .take_composite_layer_batches()
            .remove(&7)
            .unwrap()
            .dynamic_images[0]
            .clip_aux
            .is_empty()
    );
    paint.push_transform(Transform::scale(2.0, 3.0));
    draw(&mut paint);
    paint.pop_transform();
    paint.pop_clip();
    draw(&mut paint);
    let batch = paint.take_batch();
    assert_eq!(batch.dynamic_images[0].clip_aux.len(), 5);
    assert_eq!(
        batch.dynamic_images[0].clip_aux[2],
        [20.0, 20.0, 100.0, 100.0]
    );
    assert_eq!(
        batch.dynamic_images[0].clip_aux[3],
        [30.0, 40.0, 90.0, 80.0]
    );
    assert!(batch.dynamic_images[1].clip_aux.is_empty());
}
