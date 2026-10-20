//! Raw-image clips remain self-contained across ownership and foreground moves.
use blinc_core::{ClipShape, DrawContext, FillRule, Point, Rect};
use blinc_gpu::{GpuPaintContext, PrimitiveBatch};

fn draw(ctx: &mut GpuPaintContext<'_>) {
    ctx.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(20.0, 20.0, 80.0, 80.0));
}
fn triangle() -> ClipShape {
    ClipShape::Polygon(vec![
        Point::new(0.0, 0.0),
        Point::new(80.0, 0.0),
        Point::new(0.0, 80.0),
    ])
}

#[test]
fn rgba_clip_foreground_merge_and_move_keep_polygon_payload_without_batch_aux() {
    let mut merged = PrimitiveBatch::new();
    for rule in [FillRule::NonZero, FillRule::EvenOdd] {
        let mut paint = GpuPaintContext::new(180.0, 150.0);
        paint.set_foreground_layer(true);
        paint.set_clip_fill_rule(rule);
        paint.push_clip(triangle());
        draw(&mut paint);
        let mut batch = paint.take_batch();
        batch.aux_data.clear();
        merged.merge(batch);
    }
    let moved = merged.take_foreground();
    assert!(moved.aux_data.is_empty());
    assert!(!merged.has_foreground_content());
    assert_eq!(moved.dynamic_images.len(), 2);
    for (index, image) in moved.dynamic_images.iter().enumerate() {
        assert_eq!(image.clip_radius, [0.0; 4]);
        assert_eq!(image.clip_aux[7], [3.0, 4.0, index as f32, 0.0]);
        assert_eq!(
            image.clip_aux[8..],
            [[0.0, 0.0, 80.0, 0.0], [0.0, 80.0, 0.0, 0.0]]
        );
    }
}

#[test]
fn rgba_clip_scratch_strips_ancestors_then_restores_them_after_drain() {
    let mut paint = GpuPaintContext::new(180.0, 150.0);
    paint.push_clip(ClipShape::Circle {
        center: Point::new(60.0, 60.0),
        radius: 30.0,
    });
    paint.push_composite_layer(7);
    draw(&mut paint);
    paint.push_clip(triangle());
    draw(&mut paint);
    paint.pop_clip();
    paint.pop_composite_layer();
    let scratch = paint.take_composite_layer_batches().remove(&7).unwrap();
    assert!(scratch.dynamic_images[0].clip_aux.is_empty());
    assert_eq!(scratch.dynamic_images[1].clip_aux[6][2], 3.0);
    draw(&mut paint);
    let outer = paint.take_batch();
    assert_eq!(
        outer.dynamic_images[0].clip_aux[7],
        [60.0, 60.0, 30.0, 30.0]
    );
}

#[test]
fn rgba_clip_rect_and_absent_clips_keep_the_allocation_free_header() {
    let mut paint = GpuPaintContext::new(180.0, 150.0);
    draw(&mut paint);
    paint.push_clip(ClipShape::rounded_rect(
        Rect::new(20.0, 20.0, 80.0, 80.0),
        10.0,
    ));
    draw(&mut paint);
    let batch = paint.take_batch();
    assert!(
        batch
            .dynamic_images
            .iter()
            .all(|image| image.clip_aux.capacity() == 0)
    );
    assert_eq!(batch.dynamic_images[1].clip_radius, [10.0; 4]);
}
