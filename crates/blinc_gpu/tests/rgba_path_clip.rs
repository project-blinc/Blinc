//! Path clip captures survive foreground moves, owner drains and nested clips.
use blinc_core::{
    ClipShape, Color, CornerRadius, DrawContext, FillRule, Path, Point, Rect, Transform,
};
use blinc_gpu::{GpuPaintContext, PrimitiveBatch, primitives::ClipType};

fn draw(paint: &mut GpuPaintContext<'_>) {
    paint.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(10.0, 10.0, 100.0, 100.0));
}

#[test]
fn rgba_path_clip_survives_foreground_merge_and_repeated_owner_drains() {
    let mut merged = PrimitiveBatch::new();
    let mut paint = GpuPaintContext::new(180.0, 150.0);
    paint.set_foreground_layer(true);
    paint.push_transform(Transform::translate(15.0, 10.0));
    paint.set_clip_fill_rule(FillRule::EvenOdd);
    paint.push_clip(ClipShape::Path(Path::rect(Rect::new(
        20.0, 20.0, 50.0, 50.0,
    ))));
    paint.pop_transform();
    for _ in 0..2 {
        paint.push_motion_subtree();
        draw(&mut paint);
        paint.pop_motion_subtree();
        let batch = paint.take_dynamic_batch();
        assert!(batch.aux_data.is_empty());
        merged.merge(batch);
    }
    let batch = merged.take_foreground();
    assert_eq!(batch.dynamic_images.len(), 2);
    for image in &batch.dynamic_images {
        assert_eq!(image.clip_aux, batch.dynamic_images[0].clip_aux);
        assert_eq!(image.clip_aux[7][2], 1.0);
        assert_eq!(&image.clip_aux[8][..2], [35.0, 30.0]);
    }
}

#[test]
fn rgba_path_clip_strips_scratch_ancestors_and_retains_rectangular_scissors() {
    let mut paint = GpuPaintContext::new(180.0, 150.0);
    paint.push_clip(ClipShape::Path(Path::new()));
    paint.push_composite_layer(7);
    draw(&mut paint);
    paint.push_clip(ClipShape::Path(Path::circle(Point::new(60.0, 60.0), 30.0)));
    paint.push_clip(ClipShape::rect(Rect::new(40.0, 30.0, 40.0, 50.0)));
    draw(&mut paint);
    paint.pop_clip();
    paint.pop_clip();
    paint.pop_composite_layer();
    let scratch = paint.take_composite_layer_batches().remove(&7).unwrap();
    assert!(scratch.dynamic_images[0].clip_aux.is_empty());
    assert!(!scratch.dynamic_images[1].clip_aux.is_empty());
    assert_eq!(
        scratch.dynamic_images[1].clip_bounds,
        [40.0, 30.0, 40.0, 50.0]
    );
    draw(&mut paint);
    let outer = paint.take_batch();
    assert_eq!(outer.dynamic_images[0].clip_aux[7][1], 0.0);
}

#[test]
fn rgba_path_clip_rule_is_consumed_once_and_topmost_shape_still_wins() {
    let mut paint = GpuPaintContext::new(180.0, 150.0);
    paint.set_clip_fill_rule(FillRule::EvenOdd);
    paint.push_clip(ClipShape::Path(Path::rect(Rect::new(
        20.0, 20.0, 50.0, 50.0,
    ))));
    draw(&mut paint);
    paint.push_clip(ClipShape::Circle {
        center: Point::new(60.0, 60.0),
        radius: 25.0,
    });
    draw(&mut paint);
    paint.pop_clip();
    paint.pop_clip();
    paint.push_clip(ClipShape::Path(Path::rect(Rect::new(
        20.0, 20.0, 50.0, 50.0,
    ))));
    draw(&mut paint);
    let batch = paint.take_batch();
    assert_eq!(batch.dynamic_images[0].clip_aux[7][2], 1.0);
    assert_eq!(batch.dynamic_images[1].clip_aux[6][2], 1.0);
    assert_eq!(
        batch.dynamic_images[1].clip_aux[7],
        [60.0, 60.0, 25.0, 25.0]
    );
    assert_eq!(batch.dynamic_images[2].clip_aux[7][2], 0.0);
}

#[test]
fn rgba_path_clip_keeps_other_native_primitives_on_their_existing_fallback() {
    let mut paint = GpuPaintContext::new(180.0, 150.0);
    paint.push_clip(ClipShape::rect(Rect::new(40.0, 30.0, 40.0, 50.0)));
    paint.push_clip(ClipShape::Path(Path::new()));
    paint.fill_rect(
        Rect::new(10.0, 10.0, 100.0, 100.0),
        CornerRadius::ZERO,
        Color::RED.into(),
    );
    draw(&mut paint);
    let batch = paint.take_batch();
    assert_eq!(batch.primitives[0].type_info[2], ClipType::None as u32);
    assert_eq!(
        batch.primitives[0].clip_bounds,
        [-10000.0, -10000.0, 100000.0, 100000.0]
    );
    assert_eq!(
        batch.dynamic_images[0].clip_bounds,
        [40.0, 30.0, 40.0, 50.0]
    );
}
