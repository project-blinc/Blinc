//! Foreground RGBA shares shape ownership and ordered group boundaries.
use blinc_core::{DrawContext, LayerConfig, Rect};
use blinc_gpu::GpuPaintContext;

#[test]
fn foreground_rgba_projection_retains_local_order_and_explicit_opacity() {
    let mut ctx = GpuPaintContext::new(160.0, 120.0);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    ctx.set_foreground_layer(true);
    ctx.push_opacity(0.5);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    ctx.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(20.0, 20.0, 60.0, 60.0));
    ctx.pop_layer();
    ctx.pop_opacity();
    ctx.set_foreground_layer(false);
    ctx.pop_layer();
    let batch = ctx.take_batch();
    let fg = batch.foreground_batch();
    assert_eq!(fg.dynamic_images.len(), 1);
    assert_eq!(fg.dynamic_images[0].opacity, 0.25);
    assert_eq!(
        fg.dynamic_images[0]
            .paint_order
            .unwrap()
            .layer_command_count,
        1
    );
    assert!(batch.dynamic_images.is_empty());
}

#[test]
fn foreground_rgba_motion_owns_image_only_groups() {
    let mut ctx = GpuPaintContext::new(160.0, 120.0);
    ctx.set_foreground_layer(true);
    ctx.push_motion_subtree();
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    ctx.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(20.0, 20.0, 60.0, 60.0));
    ctx.pop_layer();
    ctx.pop_motion_subtree();
    let mut dynamic = ctx.take_dynamic_batch();
    assert!(!dynamic.is_empty());
    assert!(dynamic.has_foreground_content());
    let fg = dynamic.take_foreground();
    assert_eq!(fg.dynamic_images.len(), 1);
    assert_eq!(fg.dynamic_images[0].opacity, 1.0);
    assert_eq!(fg.layer_commands.len(), 2);
    assert!(ctx.take_batch().is_empty());
}

#[test]
fn foreground_rgba_image_only_boundaries_and_merge_keep_local_cursors() {
    use blinc_core::{Color, Path};
    use blinc_gpu::{PrimitiveBatch, path::tessellate_fill};
    let mut combined = PrimitiveBatch::new();
    for _ in 0..2 {
        let mut ctx = GpuPaintContext::new(160.0, 120.0);
        ctx.set_foreground_layer(true);
        let draw = |ctx: &mut GpuPaintContext<'_>| {
            ctx.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(20.0, 20.0, 20.0, 20.0))
        };
        draw(&mut ctx);
        ctx.push_layer(LayerConfig::new().opacity(0.5));
        draw(&mut ctx);
        ctx.pop_layer();
        draw(&mut ctx);
        let mut batch = ctx.take_batch();
        batch.push_foreground_path(tessellate_fill(
            &Path::rect(Rect::new(20.0, 20.0, 20.0, 20.0)),
            &Color::WHITE.into(),
        ));
        combined.merge(batch);
    }
    let fg = combined.foreground_batch();
    assert_eq!(
        fg.dynamic_images
            .iter()
            .map(|i| i.paint_order.unwrap().layer_command_count)
            .collect::<Vec<_>>(),
        [0, 1, 2, 2, 3, 4]
    );
    assert_eq!(
        fg.dynamic_images[3].paint_order.unwrap().path_index_count,
        fg.layer_commands[1].path_index_count + fg.paths.indices.len() / 2
    );
    assert!(fg.dynamic_images[3].paint_order.unwrap().path_vertex_index > 0);
    combined.clear();
    assert!(combined.is_empty());
    assert!(combined.foreground_batch().dynamic_images.is_empty());
}

#[test]
fn foreground_rgba_composite_scope_collapses_stream_and_rejects_invalid_uploads() {
    let mut ctx = GpuPaintContext::new(160.0, 120.0);
    ctx.set_foreground_layer(true);
    for (data, w, h) in [
        (&[][..], 0, 0),
        (&[255][..], 1, 1),
        (&[][..], u32::MAX, u32::MAX),
    ] {
        ctx.draw_rgba_pixels(data, w, h, Rect::new(0.0, 0.0, 20.0, 20.0));
    }
    assert!(ctx.take_batch().is_empty());
    ctx.push_composite_layer(9);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    ctx.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(20.0, 20.0, 20.0, 20.0));
    ctx.pop_layer();
    ctx.pop_composite_layer();
    let scratch = ctx.take_composite_layer_batches().remove(&9).unwrap();
    assert!(!scratch.has_foreground_content());
    assert_eq!(scratch.dynamic_images.len(), 1);
    assert_eq!(scratch.dynamic_images[0].opacity, 1.0);
    assert_eq!(scratch.layer_commands.len(), 2);
}
