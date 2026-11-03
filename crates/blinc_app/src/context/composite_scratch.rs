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

fn image(image: &blinc_gpu::primitives::DynamicImage, h: &mut impl Hasher) {
    image.paint_order.hash(h);
    image.retained.is_some().hash(h);
    if let Some(retained) = &image.retained {
        retained.view.hash(h);
        bytemuck::bytes_of(&retained.style).hash(h);
    }
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
    floats(&image.transform, h);
    floats(&image.clip_bounds, h);
    floats(&image.clip_radius, h);
    bytemuck::cast_slice::<_, u8>(&image.clip_aux).hash(h);
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
    for draw in &batch.dynamic_images {
        image(draw, h);
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
            LayerCommand::Glass(glass) => {
                glass.nested.hash(h);
                bytemuck::bytes_of(&glass.primitive).hash(h);
                image(&glass.paint, h);
                glass.frame_size.hash(h);
            }
            LayerCommand::Meshes(scene) => {
                image(&scene.paint, h);
                scene.frame_size.hash(h);
                scene.meshes.len().hash(h);
                for pending in &scene.meshes {
                    (std::sync::Arc::as_ptr(&pending.mesh) as usize).hash(h);
                    for col in &pending.transform.cols {
                        floats(col, h);
                    }
                    floats(&pending.viewport.unwrap_or([0.0; 4]), h);
                    floats(
                        &[
                            pending.camera.position.x,
                            pending.camera.position.y,
                            pending.camera.position.z,
                            pending.camera.target.x,
                            pending.camera.target.y,
                            pending.camera.target.z,
                        ],
                        h,
                    );
                }
            }
            LayerCommand::GpuPass(pass) => {
                image(&pass.paint, h);
                floats(&pass.pending.viewport.unwrap_or([0.0; 4]), h);
                pass.frame_size.hash(h);
                pass.scale_factor.to_bits().hash(h);
                match &pass.pending.flow {
                    Some(flow) => {
                        // A flow that follows the clock or the cursor changes every frame.
                        (std::sync::Arc::as_ptr(&flow.shader) as usize).hash(h);
                        floats(&flow.bounds, h);
                        let live = flow.inputs.time.is_none() || flow.inputs.pointer.is_none();
                        live.then(blinc_gpu::flow_pipeline::app_clock_secs)
                            .unwrap_or_default()
                            .to_bits()
                            .hash(h);
                    }
                    None => pass.pending.pass.identity().hash(h),
                }
            }
            LayerCommand::Push { config: c } => config(c, h),
            LayerCommand::Pop => {}
            LayerCommand::DeferredText { id, opacity }
            | LayerCommand::DeferredSvg { id, opacity }
            | LayerCommand::DeferredImage { id, opacity } => {
                id.hash(h);
                opacity.to_bits().hash(h);
            }
            LayerCommand::Sample {
                id,
                source,
                dest,
                paint,
            } => {
                paint.is_some().hash(h);
                if let Some(paint) = paint {
                    image(paint, h);
                }
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
#[path = "tests/composite_scratch_tests.rs"]
mod tests;
