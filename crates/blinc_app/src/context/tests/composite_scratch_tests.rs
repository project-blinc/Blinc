use super::*;
use blinc_core::{Color, Path, Rect};

#[test]
fn composite_layers_scratch_hash_tracks_named_sample_paint_state() {
    use blinc_core::{DrawContext, LayerId};
    let mut paint = blinc_gpu::GpuPaintContext::new(160.0, 120.0);
    let rect = Rect::new(0.0, 0.0, 32.0, 32.0);
    paint.sample_layer(LayerId(7), rect, rect);
    let batch = paint.take_batch();
    let original = hash(&batch);
    assert_eq!(original, hash(&batch.clone()));
    let mut changed = batch;
    let LayerCommand::Sample {
        paint: Some(paint), ..
    } = &mut changed.layer_commands[0].command
    else {
        panic!("sample")
    };
    paint.opacity = 0.5;
    assert_ne!(original, hash(&changed));
    let original = hash(&changed);
    let LayerCommand::Sample {
        paint: Some(paint), ..
    } = &mut changed.layer_commands[0].command
    else {
        panic!("sample")
    };
    paint.clip_aux = vec![[1.0; 4]; 5];
    assert_ne!(original, hash(&changed));
}

#[test]
fn composite_layers_scratch_hash_tracks_paths_effects_and_command_ranges() {
    let mut batch = PrimitiveBatch::new();
    batch.push_layer_command(LayerCommand::Push {
        config: LayerConfig::new().opacity(0.5),
    });
    batch.push_path(blinc_gpu::path::tessellate_fill(
        &Path::rect(Rect::new(20.0, 30.0, 40.0, 50.0)),
        &Color::GREEN.into(),
    ));
    batch.push_layer_command(LayerCommand::Pop);
    let initial = hash(&batch);
    assert_eq!(initial, hash(&batch.clone()));
    let mut changed = batch.clone();
    changed.paths.vertices[0].position[0] += 1.0;
    assert_ne!(initial, hash(&changed));
    changed = batch.clone();
    changed.layer_commands[1].path_index_count = 0;
    assert_ne!(initial, hash(&changed));
    changed = batch.clone();
    let LayerCommand::Push { config } = &mut changed.layer_commands[0].command else {
        unreachable!()
    };
    config.effects.push(LayerEffect::blur(2.0));
    assert_ne!(initial, hash(&changed));
    let effect = hash(&changed);
    let LayerCommand::Push { config } = &mut changed.layer_commands[0].command else {
        unreachable!()
    };
    config.effects[0] = LayerEffect::blur(3.0);
    assert_ne!(effect, hash(&changed));
    changed = batch.clone();
    changed.paths.use_gradient_texture = true;
    changed.paths.gradient_stops = Some(vec![blinc_core::GradientStop::new(0.0, Color::RED)]);
    assert_ne!(initial, hash(&changed));
}
