//! Motion commands and their index snapshots use the children's batch.
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
fn motion_groups_record_local_indices_without_changing_static_commands() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    rect(&mut ctx);
    ctx.pop_layer();
    ctx.push_motion_subtree();
    ctx.push_motion_subtree();
    assert!(DrawContext::bg_batch_is_dynamic(&ctx));
    ctx.push_opacity(0.5);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    assert_eq!(ctx.bg_layer_command_count(), 1);
    rect(&mut ctx);
    ctx.push_layer(LayerConfig::new().opacity(0.0));
    assert_eq!(ctx.bg_layer_command_count(), 2);
    rect(&mut ctx);
    ctx.pop_layer();
    ctx.pop_layer();
    ctx.pop_opacity();
    ctx.pop_motion_subtree();
    rect(&mut ctx);
    ctx.pop_motion_subtree();
    assert!(!DrawContext::bg_batch_is_dynamic(&ctx));
    let dynamic = ctx.take_dynamic_batch();
    assert_eq!(dynamic.layer_commands.len(), 4);
    assert_eq!(
        dynamic
            .layer_commands
            .iter()
            .map(|e| e.primitive_index)
            .collect::<Vec<_>>(),
        [0, 1, 2, 2]
    );
    assert_eq!(
        dynamic
            .primitives
            .iter()
            .map(|p| p.color[3])
            .collect::<Vec<_>>(),
        [0.5, 0.5, 1.0]
    );
    let background = ctx.take_batch();
    assert_eq!(background.layer_commands.len(), 2);
    assert_eq!(background.primitives[0].color[3], 1.0);
}

#[test]
fn pop_records_in_the_push_owner_after_the_motion_route_has_changed() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    ctx.push_motion_subtree();
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    rect(&mut ctx);
    ctx.pop_motion_subtree();
    ctx.pop_layer();
    assert!(ctx.take_batch().layer_commands.is_empty());
    let dynamic = ctx.take_dynamic_batch();
    assert_eq!(dynamic.layer_commands.len(), 2);
    assert!(matches!(
        dynamic.layer_commands[1].command,
        LayerCommand::Pop
    ));
}
