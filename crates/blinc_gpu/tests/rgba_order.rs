//! RGBA draws keep the same active owner and opacity contract as shapes.
use blinc_core::{Color, DrawContext, LayerConfig, Rect};
use blinc_gpu::GpuPaintContext;

#[test]
fn rgba_order_motion_and_composite_images_stay_in_their_active_batch() {
    for composite in [false, true] {
        let mut ctx = GpuPaintContext::new(100.0, 100.0);
        ctx.push_motion_subtree();
        if composite {
            ctx.push_composite_layer(7);
        }
        ctx.push_layer(LayerConfig::new().opacity(0.5));
        ctx.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(10.0, 10.0, 30.0, 30.0));
        ctx.pop_layer();
        if composite {
            ctx.pop_composite_layer();
        }
        ctx.pop_motion_subtree();
        assert!(ctx.take_batch().dynamic_images.is_empty());
        let batch = if composite {
            ctx.take_composite_layer_batches().remove(&7).unwrap()
        } else {
            ctx.take_dynamic_batch()
        };
        assert_eq!(batch.dynamic_images.len(), 1);
        assert_eq!(batch.dynamic_images[0].opacity, 1.0);
    }
}

#[test]
fn rgba_order_background_groups_leave_only_explicit_opacity_in_the_image() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    ctx.push_opacity(0.5);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    ctx.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(10.0, 10.0, 30.0, 30.0));
    ctx.fill_rect(
        Rect::new(20.0, 20.0, 30.0, 30.0),
        0.0.into(),
        Color::GREEN.into(),
    );
    ctx.pop_layer();
    ctx.pop_opacity();
    let batch = ctx.take_batch();
    assert_eq!(batch.dynamic_images[0].opacity, 0.5);
    assert_eq!(batch.primitives[0].color[3], 0.5);
}

#[test]
fn rgba_order_snapshots_distinguish_images_around_empty_group_boundaries() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    let draw = |ctx: &mut GpuPaintContext<'_>| {
        ctx.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(10.0, 10.0, 30.0, 30.0))
    };
    draw(&mut ctx);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    draw(&mut ctx);
    ctx.pop_layer();
    draw(&mut ctx);
    let batch = ctx.take_batch();
    assert!(!batch.is_empty());
    assert!(batch.requires_ordered_rendering());
    assert_eq!(
        batch
            .dynamic_images
            .iter()
            .map(|i| i.paint_order.unwrap().layer_command_count)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(
        batch
            .dynamic_images
            .iter()
            .all(|i| i.paint_order.unwrap().primitive_index == 0)
    );
}

#[test]
fn rgba_order_invalid_uploads_do_not_allocate_or_advance_paint_cursors() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    for (data, w, h) in [
        (&[][..], 0, 0),
        (&[255][..], 1, 1),
        (&[][..], u32::MAX, u32::MAX),
    ] {
        ctx.draw_rgba_pixels(data, w, h, Rect::new(0.0, 0.0, 1.0, 1.0));
    }
    assert!(ctx.take_batch().is_empty());
}
