//! Group opacity belongs to composition; explicit opacity still colours children.
use blinc_core::{Color, DrawContext, LayerConfig, Path, Rect, Stroke};
use blinc_gpu::{GpuPaintContext, PrimitiveBatch, primitives::LayerCommand};

fn draw(ctx: &mut GpuPaintContext<'_>) {
    let rect = Rect::new(20.0, 20.0, 40.0, 40.0);
    ctx.fill_rect(rect, 0.0.into(), Color::RED.into());
    ctx.fill_path(&Path::rect(rect), Color::RED.into());
    ctx.stroke_path(&Path::rect(rect), &Stroke::new(2.0), Color::RED.into());
}

fn alpha(batch: &PrimitiveBatch, expected: f32) {
    assert!(batch.primitives.len() >= 3);
    for primitive in &batch.primitives {
        assert_eq!(primitive.color[3], expected);
    }
}

#[test]
fn group_opacity_is_not_baked_into_background_children() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    draw(&mut ctx);
    ctx.pop_layer();
    let batch = ctx.take_batch();
    alpha(&batch, 1.0);
    let LayerCommand::Push { config } = &batch.layer_commands[0].command else {
        panic!("missing group");
    };
    assert_eq!(config.opacity, 0.5);
    assert!(batch.has_layer_effects());
}

#[test]
fn opacity_groups_stay_on_the_layer_route_and_out_of_z_overlays() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    let draw = |ctx: &mut GpuPaintContext<'_>| {
        ctx.fill_rect(
            Rect::new(20.0, 20.0, 40.0, 40.0),
            0.0.into(),
            Color::RED.into(),
        );
    };
    ctx.set_z_layer(1);
    draw(&mut ctx);
    let first = ctx.bg_primitive_count();
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    draw(&mut ctx);
    let end = ctx.bg_primitive_count();
    ctx.pop_layer();
    draw(&mut ctx);
    let batch = ctx.take_batch();
    assert!(batch.has_layer_effects());
    let indices = batch.effect_layer_indices();
    assert_eq!(indices, (first..end).collect());
    let overlay = batch.primitives_for_layer_excluding_effects(1, &indices);
    assert_eq!(overlay.len(), batch.primitives.len() - (end - first));
}

#[test]
fn explicit_opacity_scopes_survive_nested_and_zero_opacity_groups() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    ctx.push_opacity(0.5);
    ctx.push_layer(LayerConfig::new().opacity(0.0));
    ctx.push_opacity(0.5);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    assert_eq!(ctx.current_opacity(), 0.0);
    draw(&mut ctx);
    ctx.pop_layer();
    ctx.pop_opacity();
    ctx.pop_layer();
    assert_eq!(ctx.current_opacity(), 0.5);
    draw(&mut ctx);
    ctx.pop_opacity();
    draw(&mut ctx);
    let batch = ctx.take_batch();
    let count = batch.primitives.len() / 3;
    for (chunk, expected) in batch.primitives.chunks(count).zip([0.25, 0.5, 1.0]) {
        for primitive in chunk {
            assert_eq!(primitive.color[3], expected);
        }
    }
}

#[test]
fn group_pop_restores_unbalanced_child_opacity_scopes() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    ctx.push_opacity(0.5);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    ctx.push_opacity(0.25);
    ctx.pop_layer();
    assert_eq!(ctx.current_opacity(), 0.5);
    draw(&mut ctx);
    alpha(&ctx.take_batch(), 0.5);
}

#[test]
fn routed_children_keep_only_opacity_unrepresented_in_their_batch() {
    for composite in [false, true] {
        let mut ctx = GpuPaintContext::new(100.0, 100.0);
        ctx.push_opacity(0.5);
        ctx.push_layer(LayerConfig::new().opacity(0.5));
        ctx.push_motion_subtree();
        if composite {
            ctx.push_composite_layer(7);
        }
        ctx.push_layer(LayerConfig::new().opacity(0.5));
        draw(&mut ctx);
        ctx.pop_layer();
        if composite {
            ctx.pop_composite_layer();
        }
        ctx.pop_motion_subtree();
        draw(&mut ctx);
        ctx.pop_layer();
        ctx.pop_opacity();
        let routed = if composite {
            ctx.take_composite_layer_batches().remove(&7).unwrap()
        } else {
            ctx.take_dynamic_batch()
        };
        if composite {
            assert!(routed.layer_commands.is_empty());
            alpha(&routed, 0.125);
        } else {
            assert_eq!(routed.layer_commands.len(), 2);
            alpha(&routed, 0.25);
        }
        alpha(&ctx.take_batch(), 0.5);
    }
}

#[test]
fn foreground_and_late_images_keep_effective_opacity() {
    let mut ctx = GpuPaintContext::new(100.0, 100.0);
    ctx.push_opacity(0.5);
    ctx.push_layer(LayerConfig::new().opacity(0.5));
    ctx.set_foreground_layer(true);
    draw(&mut ctx);
    ctx.draw_rgba_pixels(&[255, 0, 0, 255], 1, 1, Rect::new(0.0, 0.0, 1.0, 1.0));
    ctx.set_foreground_layer(false);
    ctx.pop_layer();
    ctx.pop_opacity();
    let batch = ctx.take_batch();
    assert!(batch.primitives.is_empty());
    assert!(!batch.foreground_primitives.is_empty());
    for primitive in &batch.foreground_primitives {
        assert_eq!(primitive.color[3], 0.25);
    }
    assert_eq!(batch.dynamic_images[0].opacity, 0.25);
}
