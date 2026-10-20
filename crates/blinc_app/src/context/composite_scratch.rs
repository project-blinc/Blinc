//! Fingerprint the pixels and group operations used by a cached subtree bake.
use blinc_core::{LayerConfig, LayerEffect};
use blinc_gpu::{PrimitiveBatch, primitives::LayerCommand};
use std::hash::{Hash, Hasher};

fn floats(values: &[f32], h: &mut impl Hasher) {
    bytemuck::cast_slice::<_, u8>(values).hash(h);
}

fn config(config: &LayerConfig, h: &mut impl Hasher) {
    config.id.hash(h);
    config
        .position
        .map(|p| [p.x.to_bits(), p.y.to_bits()])
        .hash(h);
    config
        .size
        .map(|s| [s.width.to_bits(), s.height.to_bits()])
        .hash(h);
    std::mem::discriminant(&config.blend_mode).hash(h);
    config.opacity.to_bits().hash(h);
    config.depth.hash(h);
    config
        .transform_3d
        .map(|t| [t.sin_rx, t.cos_rx, t.sin_ry, t.cos_ry, t.perspective_d].map(f32::to_bits))
        .hash(h);
    config.effects.len().hash(h);
    for effect in &config.effects {
        std::mem::discriminant(effect).hash(h);
        match effect {
            LayerEffect::Blur { radius, quality } => {
                radius.to_bits().hash(h);
                std::mem::discriminant(quality).hash(h);
            }
            LayerEffect::DropShadow {
                offset_x,
                offset_y,
                blur,
                spread,
                color,
            } => {
                floats(
                    &[
                        *offset_x, *offset_y, *blur, *spread, color.r, color.g, color.b, color.a,
                    ],
                    h,
                );
            }
            LayerEffect::Glow {
                color,
                blur,
                range,
                opacity,
            } => {
                floats(
                    &[color.r, color.g, color.b, color.a, *blur, *range, *opacity],
                    h,
                );
            }
            LayerEffect::ColorMatrix { matrix } => floats(matrix, h),
            LayerEffect::MaskImage {
                image_url,
                mask_mode,
            } => {
                image_url.hash(h);
                std::mem::discriminant(mask_mode).hash(h);
            }
        }
    }
}

pub(super) fn hash(batch: &PrimitiveBatch) -> u64 {
    let h = &mut std::collections::hash_map::DefaultHasher::new();
    bytemuck::cast_slice::<_, u8>(&batch.primitives).hash(h);
    bytemuck::cast_slice::<_, u8>(&batch.foreground_primitives).hash(h);
    bytemuck::cast_slice::<_, u8>(&batch.aux_data).hash(h);
    let path = &batch.paths;
    bytemuck::cast_slice::<_, u8>(&path.vertices).hash(h);
    path.indices.hash(h);
    floats(&path.clip_bounds, h);
    floats(&path.clip_radius, h);
    path.clip_type.hash(h);
    path.use_gradient_texture.hash(h);
    path.gradient_stops.as_ref().map(Vec::len).hash(h);
    if let Some(stops) = &path.gradient_stops {
        for stop in stops {
            floats(
                &[
                    stop.offset,
                    stop.color.r,
                    stop.color.g,
                    stop.color.b,
                    stop.color.a,
                ],
                h,
            );
        }
    }
    path.use_image_texture.hash(h);
    path.image_source.hash(h);
    floats(&path.image_uv_bounds, h);
    path.use_glass_effect.hash(h);
    floats(&path.glass_params, h);
    floats(&path.glass_tint, h);
    batch.dynamic_images.len().hash(h);
    for image in &batch.dynamic_images {
        image.paint_order.hash(h);
        image.data.hash(h);
        image.width.hash(h);
        image.height.hash(h);
        floats(
            &[
                image.dest.x(),
                image.dest.y(),
                image.dest.width(),
                image.dest.height(),
                image.opacity,
                image.corner_radius,
            ],
            h,
        );
        floats(&image.clip_bounds, h);
        floats(&image.clip_radius, h);
    }
    batch.layer_commands.len().hash(h);
    for entry in &batch.layer_commands {
        entry.primitive_index.hash(h);
        entry.foreground_primitive_index.hash(h);
        entry.path_vertex_index.hash(h);
        entry.path_index_count.hash(h);
        entry.foreground_path_vertex_index.hash(h);
        entry.foreground_path_index_count.hash(h);
        std::mem::discriminant(&entry.command).hash(h);
        match &entry.command {
            LayerCommand::Push { config: c } => config(c, h),
            LayerCommand::Pop => {}
            LayerCommand::Sample { id, source, dest } => {
                id.hash(h);
                floats(
                    &[
                        source.x(),
                        source.y(),
                        source.width(),
                        source.height(),
                        dest.x(),
                        dest.y(),
                        dest.width(),
                        dest.height(),
                    ],
                    h,
                );
            }
        }
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::{Color, Path, Rect};

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
}
