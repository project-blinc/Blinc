//! Raw images share their shape stream's active batch and paint cursor.
use super::GpuPaintContext;
use crate::primitives::{DynamicImage, ImagePaintOrder};
use blinc_core::{DrawContext, Rect};

impl GpuPaintContext<'_> {
    pub(super) fn emit_rgba_pixels(&mut self, data: &[u8], width: u32, height: u32, dest: Rect) {
        if width == 0
            || height == 0
            || width
                .checked_mul(height)
                .and_then(|n| n.checked_mul(4))
                .map(|n| n as usize)
                != Some(data.len())
        {
            return;
        }
        let transformed = self.transform_rect(dest);
        let foreground = self.emit_foreground();
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, _, _) = self.get_clip_data();
        let batch = self.active_batch_mut();
        let paint_order = Some(if foreground {
            ImagePaintOrder {
                primitive_index: batch.foreground_primitives.len(),
                path_vertex_index: batch.foreground_paths.vertices.len(),
                path_index_count: batch.foreground_paths.indices.len(),
                layer_command_count: batch.foreground_layer_commands.len(),
            }
        } else {
            ImagePaintOrder {
                primitive_index: batch.primitives.len(),
                path_vertex_index: batch.paths.vertices.len(),
                path_index_count: batch.paths.indices.len(),
                layer_command_count: batch.layer_commands.len(),
            }
        });
        let images = if foreground {
            &mut batch.foreground_dynamic_images
        } else {
            &mut batch.dynamic_images
        };
        images.push(DynamicImage {
            paint_order,
            data: data.to_vec(),
            width,
            height,
            dest: transformed,
            opacity,
            corner_radius: 0.0,
            clip_bounds,
            clip_radius,
        });
    }
}
