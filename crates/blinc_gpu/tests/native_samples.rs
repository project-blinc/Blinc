//! Sample commands follow their geometry's native paint owner.
use blinc_core::{ClipShape, DrawContext, LayerConfig, LayerId, Point, Rect, Transform};
use blinc_gpu::{GpuPaintContext, PrimitiveBatch, primitives::LayerCommand};

#[test]
fn native_samples_capture_the_same_geometry_and_clips_as_images() {
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    paint.push_transform(Transform::translate(10.0, 20.0));
    paint.push_transform(Transform::rotate(0.3));
    paint.push_opacity(0.5);
    paint.push_clip(ClipShape::Circle {
        center: Point::new(20.0, 20.0),
        radius: 15.0,
    });
    let dest = Rect::new(0.0, 0.0, 40.0, 40.0);
    paint.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, dest);
    paint.sample_layer(LayerId(7), dest, dest);
    let batch = paint.take_batch();
    let LayerCommand::Sample {
        paint: Some(sample),
        ..
    } = &batch.layer_commands[0].command
    else {
        panic!("sample")
    };
    let image = &batch.dynamic_images[0];
    assert_eq!(sample.dest, image.dest);
    assert_eq!(sample.transform, image.transform);
    assert_ne!(sample.transform, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(sample.opacity, image.opacity);
    assert_eq!(sample.clip_bounds, image.clip_bounds);
    assert_eq!(sample.clip_radius, image.clip_radius);
    assert_eq!(sample.clip_aux, image.clip_aux);
}

#[test]
fn native_samples_keep_owner_and_merge_cursors_without_primitive_draws() {
    let rect = Rect::new(0.0, 0.0, 32.0, 32.0);
    for route in 0..4 {
        let mut paint = GpuPaintContext::new(160.0, 120.0);
        if route == 1 {
            paint.push_motion_subtree();
        }
        if route == 2 {
            paint.push_composite_layer(9);
        }
        if route == 3 {
            paint.set_foreground_layer(true);
        }
        paint.sample_layer(LayerId(7), rect, rect);
        if route == 1 {
            paint.pop_motion_subtree();
        }
        if route == 2 {
            paint.pop_composite_layer();
        }
        let mut batch = match route {
            1 => paint.take_dynamic_batch(),
            2 => paint.take_composite_layer_batches().remove(&9).unwrap(),
            _ => paint.take_batch(),
        };
        assert!(!batch.is_empty());
        if route == 3 {
            assert!(batch.has_foreground_content());
            batch = batch.take_foreground();
        }
        assert!(batch.has_ordered_draws());
        let mut merged = PrimitiveBatch::new();
        merged.merge(batch.clone());
        merged.merge(batch);
        assert_eq!(merged.layer_commands.len(), 2);
        assert!(
            merged
                .layer_commands
                .iter()
                .all(|entry| entry.primitive_index == 0)
        );
        merged.clear();
        assert!(merged.is_empty());
    }
}

#[test]
fn native_samples_reject_invalid_destination_frames_and_retain_raw_commands() {
    let mut paint = GpuPaintContext::new(160.0, 120.0);
    let rect = Rect::new(0.0, 0.0, 32.0, 32.0);
    paint.push_transform(Transform::scale(0.0, 1.0));
    paint.sample_layer(LayerId(7), rect, rect);
    assert!(paint.take_batch().is_empty());
    let mut batch = PrimitiveBatch::new();
    batch.push_layer_command(LayerCommand::Sample {
        id: LayerId(7),
        source: rect,
        dest: rect,
        paint: None,
    });
    assert!(!batch.is_empty());
    assert!(batch.has_ordered_draws());
    let mut named = PrimitiveBatch::new();
    named.push_layer_command(LayerCommand::Push {
        config: LayerConfig::new().id(LayerId(7)).opacity(1.0),
    });
    named.push_layer_command(LayerCommand::Pop);
    assert!(named.is_empty());
    assert!(!named.has_ordered_draws());
}
