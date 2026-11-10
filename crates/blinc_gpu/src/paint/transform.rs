//! Current-transform queries: affine, scale and rotation extraction, and rect and radius mapping.

use super::GpuPaintContext;
use blinc_core::{Affine2D, CornerRadius, Point, Rect};

impl GpuPaintContext<'_> {
    /// Get the current transform
    pub(super) fn current_affine(&self) -> Affine2D {
        self.transform_stack
            .last()
            .copied()
            .unwrap_or(Affine2D::IDENTITY)
    }

    /// Opacity baked into primitive colours. Groups in this batch and stream
    /// own their opacity; unrepresented ancestors still need it flattened.
    pub(super) fn combined_opacity(&self) -> f32 {
        let route = self.aux_batch();
        let foreground = self.emit_foreground();
        self.opacity_stack.iter().product::<f32>()
            * self
                .layer_stack
                .iter()
                .filter(|state| {
                    (state.foreground != foreground && state.cross_stream.is_none())
                        || state.command_batch != route
                })
                .map(|state| state.config.opacity)
                .filter(|opacity| *opacity < 1.0)
                .product::<f32>()
    }

    /// Transform a point by the current transform
    pub(super) fn transform_point(&self, p: Point) -> Point {
        let affine = self.current_affine();
        // elements = [a, b, c, d, tx, ty]
        // | a  c  tx |   | x |
        // | b  d  ty | * | y |
        // | 0  0   1 |   | 1 |
        Point::new(
            affine.elements[0] * p.x + affine.elements[2] * p.y + affine.elements[4],
            affine.elements[1] * p.x + affine.elements[3] * p.y + affine.elements[5],
        )
    }

    /// Extract the uniform scale factor from the current transform.
    /// This accounts for DPI scaling and any CSS transforms.
    pub(super) fn current_uniform_scale(&self) -> f32 {
        let affine = self.current_affine();
        let [a, b, c, d, ..] = affine.elements;
        let det = a * d - b * c;
        det.abs().sqrt().max(1e-6)
    }

    /// Transform a rect by the current transform (rotation+skew safe)
    ///
    /// Transforms the center of the rect through the full affine. Uses the
    /// determinant-based uniform scale for dimensions so that skew transforms
    /// don't inflate the bounds (the local_affine carries the full 2x2 to the shader).
    pub(super) fn transform_rect(&self, rect: Rect) -> Rect {
        let affine = self.current_affine();
        let [a, b, c, d, ..] = affine.elements;

        // Uniform scale = sqrt(|det|) — extracts DPI + any uniform element scale.
        // This is exact for area-preserving transforms (rotation, skew) and a good
        // approximation for non-uniform scales.
        let det = a * d - b * c;
        let uniform_scale = det.abs().sqrt().max(1e-6);

        // Transform the CENTER (not origin)
        let center = Point::new(
            rect.origin.x + rect.size.width * 0.5,
            rect.origin.y + rect.size.height * 0.5,
        );
        let tc = self.transform_point(center);
        let sw = rect.size.width * uniform_scale;
        let sh = rect.size.height * uniform_scale;

        Rect::new(tc.x - sw * 0.5, tc.y - sh * 0.5, sw, sh)
    }

    /// Extract rotation sin/cos from the current affine transform
    ///
    /// Returns `[sin_rz, cos_rz, sin_ry, cos_ry]` ready for GpuPrimitive.rotation.
    /// Derives sin/cos directly from affine components without atan2.
    /// The Y rotation slots are filled from the 3D transient state.
    pub(super) fn current_rotation_sincos(&self) -> [f32; 4] {
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let scale = (a * a + b * b).sqrt();
        if scale < 1e-6 {
            return [0.0, 1.0, self.current_3d_sin_ry, self.current_3d_cos_ry];
        }
        [
            b / scale,
            a / scale,
            self.current_3d_sin_ry,
            self.current_3d_cos_ry,
        ]
    }

    /// Get the DPI scale factor from the current affine transform.
    /// On Retina 2x displays this returns ~2.0, on 1x displays ~1.0.
    /// Used to scale 3D parameters (depth, perspective_d, translate_z) from
    /// logical/CSS pixels to physical pixels to match prim.bounds.
    pub(super) fn current_dpi_scale(&self) -> f32 {
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale_x = (a * a + b * b).sqrt();
        let scale_y = (c * c + d * d).sqrt();
        (scale_x + scale_y) * 0.5
    }

    /// Extract the normalized local 2x2 affine [a, b, c, d] from the current transform.
    ///
    /// This removes the uniform scale (DPI + uniform element scale) so that the
    /// remaining 2x2 captures rotation, skew, and non-uniform scale ratios.
    /// The shader uses this to apply the full inverse transform to sample points,
    /// enabling correct SDF evaluation for skewed/rotated elements.
    pub(super) fn current_local_affine(&self) -> [f32; 4] {
        let affine = self.current_affine();
        let [a, b, c, d, ..] = affine.elements;
        let det = a * d - b * c;
        let uniform_scale = det.abs().sqrt().max(1e-6);
        [
            a / uniform_scale,
            b / uniform_scale,
            c / uniform_scale,
            d / uniform_scale,
        ]
    }

    /// Get the current 3D perspective params for GpuPrimitive.perspective.
    /// perspective_d is scaled to physical pixels to match prim.bounds.
    pub(super) fn current_perspective_params(&self) -> [f32; 4] {
        let scale = self.current_dpi_scale();
        [
            self.current_3d_sin_rx,
            self.current_3d_cos_rx,
            self.current_3d_perspective_d * scale,
            self.current_3d_shape_type,
        ]
    }

    /// Get the current 3D SDF params for GpuPrimitive.sdf_3d.
    /// depth and translate_z are scaled to physical pixels to match prim.bounds.
    pub(super) fn current_sdf_3d_params(&self) -> [f32; 4] {
        let scale = self.current_dpi_scale();
        [
            self.current_3d_depth * scale,
            self.current_3d_ambient,
            self.current_3d_specular,
            self.current_3d_translate_z * scale,
        ]
    }

    /// Get the current 3D light params for GpuPrimitive.light
    pub(super) fn current_light_params(&self) -> [f32; 4] {
        self.current_3d_light
    }

    /// Scale corner radius by the current transform's average scale factor
    pub(super) fn scale_corner_radius(&self, corner_radius: CornerRadius) -> CornerRadius {
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale_x = (a * a + b * b).sqrt();
        let scale_y = (c * c + d * d).sqrt();
        let avg_scale = (scale_x + scale_y) / 2.0;

        CornerRadius::new(
            corner_radius.top_left * avg_scale,
            corner_radius.top_right * avg_scale,
            corner_radius.bottom_right * avg_scale,
            corner_radius.bottom_left * avg_scale,
        )
    }
}
