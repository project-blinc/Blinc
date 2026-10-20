//! Scratch groups own local command ranges and exclude represented alpha.
use blinc_core::{Color, DrawContext, LayerConfig, Rect};
use blinc_gpu::{GpuPaintContext, primitives::LayerCommand};

fn rect(ctx: &mut GpuPaintContext<'_>) {
    ctx.fill_rect(
        Rect::new(10.0, 10.0, 30.0, 30.0),
        0.0.into(),
        Color::RED.into(),
    );
}

#[test]
fn composite_layers_keep_commands_and_only_unrepresented_ancestor_opacity() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    rect(&mut ctx);
    ctx.push_motion_subtree();
    ctx.push_composite_layer(7);
    assert!(DrawContext::bg_batch_is_composite(&ctx));
    ctx.push_opacity(0.5);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    assert_eq!(ctx.bg_layer_command_count(), 1);
    rect(&mut ctx);
    ctx.push_composite_layer(8);
    ctx.push_layer(LayerConfig::new().opacity(0.0));
    rect(&mut ctx);
    ctx.pop_layer();
    ctx.pop_composite_layer();
    ctx.pop_layer();
    ctx.pop_opacity();
    ctx.pop_composite_layer();
    assert!(!DrawContext::bg_batch_is_composite(&ctx));
    ctx.pop_motion_subtree();
    ctx.pop_layer();
    let mut scratch = ctx.take_composite_layer_batches();
    assert_eq!(scratch.len(), 1);
    let batch = scratch.remove(&7).unwrap();
    assert_eq!(batch.layer_commands.len(), 4);
    assert_eq!(
        batch
            .layer_commands
            .iter()
            .map(|e| e.primitive_index)
            .collect::<Vec<_>>(),
        [0, 1, 2, 2]
    );
    assert_eq!(
        batch
            .primitives
            .iter()
            .map(|p| p.color[3])
            .collect::<Vec<_>>(),
        [0.25, 0.25]
    );
    assert_eq!(ctx.take_batch().layer_commands.len(), 2);
    assert!(ctx.take_dynamic_batch().layer_commands.is_empty());
}

#[test]
fn composite_layers_pop_returns_to_saved_scratch_owner() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    ctx.push_composite_layer(7);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    rect(&mut ctx);
    ctx.pop_composite_layer();
    ctx.push_composite_layer(8);
    rect(&mut ctx);
    ctx.pop_layer();
    ctx.pop_composite_layer();
    let mut scratch = ctx.take_composite_layer_batches();
    let first = scratch.remove(&7).unwrap();
    assert_eq!(first.layer_commands.len(), 2);
    assert!(matches!(first.layer_commands[1].command, LayerCommand::Pop));
    let second = scratch.remove(&8).unwrap();
    assert!(second.layer_commands.is_empty());
    assert_eq!(second.primitives[0].color[3], 0.5);
    assert!(ctx.take_batch().layer_commands.is_empty());
}

#[test]
fn composite_layers_footprint_includes_paths_and_nested_effect_margins() {
    use blinc_core::{LayerEffect, Path};
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    assert!(ctx.composite_layer_bounds().is_none());
    ctx.push_composite_layer(7);
    assert!(ctx.composite_layer_bounds().is_none());
    let mut config = LayerConfig::new().opacity(0.5);
    config.effects.push(LayerEffect::blur(3.0));
    ctx.push_layer(config);
    ctx.fill_rect(
        Rect::new(20.0, 20.0, 60.0, 60.0),
        0.0.into(),
        Color::RED.into(),
    );
    // The outer group is still open when the walker records its region.
    assert_eq!(ctx.composite_layer_bounds(), Some([13.0, 13.0, 74.0, 74.0]));
    ctx.pop_layer();
    ctx.pop_composite_layer();
    ctx.push_composite_layer(8);
    ctx.fill_path(
        &Path::rect(Rect::new(10.0, 10.0, 30.0, 30.0)),
        Color::RED.into(),
    );
    assert_eq!(ctx.composite_layer_bounds(), Some([9.0, 9.0, 32.0, 32.0]));
    ctx.pop_composite_layer();
}
