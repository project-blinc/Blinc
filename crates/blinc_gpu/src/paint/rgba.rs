//! Raw images share their shape stream's active batch and paint cursor.
use super::GpuPaintContext;
use crate::primitives::{ClipType, DynamicImage, ImagePaintOrder};
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
        let affine = self.current_affine_elements();
        let [a, b, c, d, ..] = affine;
        let determinant = a * d - b * c;
        if affine.iter().any(|v| !v.is_finite())
            || !determinant.is_finite()
            || determinant.abs() <= 1e-12
        {
            return;
        }
        let transformed = self.transform_rect(dest);
        let transform = self.current_local_affine();
        if !DynamicImage::valid_geometry(transformed, transform) {
            return;
        }
        let foreground = self.emit_foreground();
        let opacity = self.combined_opacity();
        let (clip_bounds, mut clip_radius, _, clip_type) = self.get_clip_data();
        let clip_aux = self.rgba_clip_aux(clip_type, clip_radius, transformed, transform);
        if matches!(
            clip_type,
            ClipType::Circle | ClipType::Ellipse | ClipType::Polygon
        ) {
            clip_radius = [0.0; 4];
        }
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
            transform,
            opacity,
            corner_radius: 0.0,
            clip_bounds,
            clip_radius,
            clip_aux,
        });
    }

    fn rgba_clip_aux(
        &self,
        kind: ClipType,
        radius: [f32; 4],
        dest: Rect,
        transform: [f32; 4],
    ) -> Vec<[f32; 4]> {
        if !matches!(
            kind,
            ClipType::Circle | ClipType::Ellipse | ClipType::Polygon
        ) {
            return Vec::new();
        }
        let count = if kind == ClipType::Polygon {
            let count = radius[2] as usize;
            if count < 3 {
                return Vec::new();
            }
            count
        } else {
            0
        };
        // Five image clip rows precede the independent shape's frame/info/data.
        // Polygon points are copied so image-only drains never depend on batch aux offsets.
        let extra = if count == 0 {
            0
        } else {
            (count + 1).div_ceil(2)
        };
        let mut aux = Vec::with_capacity(8 + extra);
        aux.resize(8, [0.0; 4]);
        aux[4] = [1.0, 0.75, 0.0, 6.0];
        aux[5] = [1.0, 0.0, 0.0, 1.0];
        aux[6][2] = 1.0;
        aux[7] = match kind {
            ClipType::Circle => [radius[0], radius[1], radius[2], radius[2]],
            ClipType::Ellipse => radius,
            ClipType::Polygon => {
                let first = radius[3] as usize;
                let points = &self.active_batch().aux_data[first..first + count.div_ceil(2)];
                for n in (0..=count).step_by(2) {
                    let point = |i: usize| {
                        let i = if i == count { 0 } else { i };
                        let row = points[i / 2];
                        if i % 2 == 0 {
                            [row[0], row[1]]
                        } else {
                            [row[2], row[3]]
                        }
                    };
                    let a = point(n);
                    let b = if n < count { point(n + 1) } else { [0.0; 2] };
                    aux.push([a[0], a[1], b[0], b[1]]);
                }
                let [a, b, c, d] = transform;
                let det = a * d - b * c;
                let inverse = [d / det, -b / det, -c / det, a / det];
                let cx = dest.x() + dest.width() * 0.5;
                let cy = dest.y() + dest.height() * 0.5;
                aux[5] = inverse;
                aux[6] = [
                    dest.width() * 0.5 - inverse[0] * cx - inverse[2] * cy,
                    dest.height() * 0.5 - inverse[1] * cx - inverse[3] * cy,
                    3.0,
                    0.0,
                ];
                [3.0, (count + 1) as f32, radius[0], 0.0]
            }
            _ => unreachable!(),
        };
        aux
    }
}
