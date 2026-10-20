//! Foreground groups retain their own composition boundary.
use blinc_core::{Color, DrawContext, LayerConfig, Rect};
use blinc_gpu::GpuPaintContext;

#[test]
fn foreground_layers_own_opacity_in_static_and_motion_streams() {
    for motion in [false, true] {
        let mut ctx = GpuPaintContext::new(160.0, 120.0);
        ctx.set_foreground_layer(true);
        if motion {
            ctx.push_motion_subtree();
        }
        ctx.push_opacity(0.5);
        ctx.push_layer(LayerConfig::new().opacity(0.5));
        ctx.fill_rect(
            Rect::new(20.0, 20.0, 60.0, 60.0),
            0.0.into(),
            Color::RED.into(),
        );
        ctx.fill_rect(
            Rect::new(40.0, 40.0, 60.0, 60.0),
            0.0.into(),
            Color::GREEN.into(),
        );
        ctx.pop_layer();
        ctx.pop_opacity();
        if motion {
            ctx.pop_motion_subtree();
        }
        let batch = if motion {
            ctx.take_dynamic_batch()
        } else {
            ctx.take_batch()
        };
        assert_eq!(batch.foreground_primitives.len(), 2);
        assert_eq!(batch.foreground_primitives[0].color[3], 0.5);
        assert!(batch.layer_commands.is_empty());
    }
}

#[test]
fn foreground_layers_keep_background_ancestor_opacity_for_images() {
    let mut ctx = GpuPaintContext::new(160.0, 120.0);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    ctx.set_foreground_layer(true);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    ctx.fill_rect(
        Rect::new(20.0, 20.0, 60.0, 60.0),
        0.0.into(),
        Color::RED.into(),
    );
    ctx.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(110.0, 20.0, 20.0, 20.0));
    ctx.pop_layer();
    ctx.set_foreground_layer(false);
    ctx.pop_layer();
    let batch = ctx.take_batch();
    assert_eq!(batch.foreground_primitives[0].color[3], 0.5);
    assert_eq!(batch.foreground_dynamic_images[0].opacity, 0.5);
    assert_eq!(batch.layer_commands.len(), 2);
}

#[test]
fn foreground_layers_pop_returns_to_saved_stream_and_local_cursors() {
    use blinc_gpu::primitives::LayerCommand;
    let mut ctx = GpuPaintContext::new(160.0, 120.0);
    ctx.fill_rect(
        Rect::new(1.0, 1.0, 2.0, 2.0),
        0.0.into(),
        Color::WHITE.into(),
    );
    ctx.push_motion_subtree();
    ctx.set_foreground_layer(true);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    ctx.fill_rect(
        Rect::new(20.0, 20.0, 40.0, 40.0),
        0.0.into(),
        Color::RED.into(),
    );
    ctx.pop_motion_subtree();
    ctx.set_foreground_layer(false);
    ctx.pop_layer();
    let dynamic = ctx.take_dynamic_batch();
    assert!(dynamic.layer_commands.is_empty());
    assert_eq!(dynamic.foreground_layer_commands.len(), 2);
    assert_eq!(dynamic.foreground_layer_commands[0].primitive_index, 0);
    assert_eq!(dynamic.foreground_layer_commands[1].primitive_index, 1);
    assert!(matches!(
        dynamic.foreground_layer_commands[1].command,
        LayerCommand::Pop
    ));
    assert!(ctx.take_batch().foreground_layer_commands.is_empty());
}

#[test]
fn foreground_layers_projection_moves_paths_and_merge_rebases_cursors() {
    use blinc_core::Path;
    use blinc_gpu::{PrimitiveBatch, path::tessellate_fill, primitives::LayerCommand};
    let mut batch = PrimitiveBatch::new();
    for _ in 0..2 {
        let mut child = PrimitiveBatch::new();
        child.push_foreground_layer_command(LayerCommand::Push {
            config: LayerConfig::new().opacity(0.5),
        });
        child.push_foreground_path(tessellate_fill(
            &Path::rect(Rect::new(20.0, 20.0, 20.0, 20.0)),
            &Color::RED.into(),
        ));
        child.push_foreground_layer_command(LayerCommand::Pop);
        batch.merge(child);
    }
    let snapshot = batch.foreground_batch();
    assert_eq!(snapshot.layer_commands.len(), 4);
    assert_eq!(
        snapshot.layer_commands[2].path_vertex_index,
        snapshot.layer_commands[1].path_vertex_index
    );
    assert_eq!(
        snapshot.layer_commands[2].path_index_count,
        snapshot.layer_commands[1].path_index_count
    );
    assert_eq!(
        snapshot.layer_commands[3].path_index_count,
        snapshot.paths.indices.len()
    );
    let moved = batch.take_foreground();
    assert_eq!(moved.paths.indices, snapshot.paths.indices);
    assert!(!batch.has_foreground_content());
    assert!(!batch.has_foreground_layers());
    batch.clear();
    assert!(batch.foreground_layer_commands.is_empty());
}
