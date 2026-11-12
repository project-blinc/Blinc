//! Circle fills and strokes.

use super::GpuPaintContext;
use crate::primitives::{FillType, GpuGlassPrimitive, GpuPrimitive, PrimitiveType};
use blinc_core::{Brush, Point, Rect, Stroke};

impl GpuPaintContext<'_> {
    pub(super) fn fill_circle_impl(&mut self, center: Point, radius: f32, brush: Brush) {
        let transformed_center = self.transform_point(center);
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale = ((a * a + b * b).sqrt() + (c * c + d * d).sqrt()) / 2.0;
        let transformed_radius = radius * scale;

        // Handle glass brush specially - push to glass primitives
        if let Brush::Glass(style) = &brush {
            let mut glass = GpuGlassPrimitive::circle(
                transformed_center.x,
                transformed_center.y,
                transformed_radius,
            )
            .with_blur(style.blur)
            .with_tint(style.tint.r, style.tint.g, style.tint.b, style.tint.a)
            .with_saturation(style.saturation)
            .with_brightness(style.brightness)
            .with_noise(style.noise)
            .with_border_thickness(style.border_thickness);
            if let Some(bc) = style.border_color {
                glass = glass.with_border_color(bc.r, bc.g, bc.b, bc.a);
            }
            self.emit_native_glass(glass, style.depth > 0);
            return;
        }

        let (color, color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Convert OBB (0..1) gradient coords to circle bounding rect pixel coords
        let circle_rect = Rect::new(
            center.x - radius,
            center.y - radius,
            radius * 2.0,
            radius * 2.0,
        );
        let gradient_params =
            Self::obb_to_rect_coords(&brush, gradient_params, circle_rect, fill_type);

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - transformed_radius,
                transformed_center.y - transformed_radius,
                transformed_radius * 2.0,
                transformed_radius * 2.0,
            ],
            corner_radius: [0.0; 4], // Not used for circles
            color,
            color2,
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: transformed_gradient_params,
            rotation: [0.0, 1.0, 0.0, 1.0],
            local_affine: [1.0, 0.0, 0.0, 1.0],
            perspective: self.current_perspective_params(),
            sdf_3d: self.current_sdf_3d_params(),
            light: self.current_light_params(),
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::Circle as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);
    }

    pub(super) fn stroke_circle_impl(
        &mut self,
        center: Point,
        radius: f32,
        stroke: &Stroke,
        brush: Brush,
    ) {
        let transformed_center = self.transform_point(center);
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale = ((a * a + b * b).sqrt() + (c * c + d * d).sqrt()) / 2.0;
        let transformed_radius = radius * scale;

        let (color, _, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Convert OBB (0..1) gradient coords to circle bounding rect pixel coords
        let circle_rect = Rect::new(
            center.x - radius,
            center.y - radius,
            radius * 2.0,
            radius * 2.0,
        );
        let gradient_params =
            Self::obb_to_rect_coords(&brush, gradient_params, circle_rect, fill_type);

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - transformed_radius,
                transformed_center.y - transformed_radius,
                transformed_radius * 2.0,
                transformed_radius * 2.0,
            ],
            corner_radius: [0.0; 4],
            color: [0.0, 0.0, 0.0, 0.0], // Transparent fill
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [stroke.width * scale, 0.0, 0.0, 0.0],
            border_color: color,
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: transformed_gradient_params,
            rotation: [0.0, 1.0, 0.0, 1.0],
            local_affine: [1.0, 0.0, 0.0, 1.0],
            perspective: self.current_perspective_params(),
            sdf_3d: self.current_sdf_3d_params(),
            light: self.current_light_params(),
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::Circle as u32,
                fill_type as u32,
                clip_type as u32,
                self.z_layer,
            ],
            side_color_top: [0.0; 4],
            side_color_right: [0.0; 4],
            side_color_bottom: [0.0; 4],
            side_color_left: [0.0; 4],
            gradient_table: [0.0; 4],
            local_clip_bounds: [0.0; 4],
            local_clip_radius: [0.0; 4],
            clip_fade_bounds: [0.0; 4],
            notch_info: [0.0; 4],
            clip_shape: [0.0; 4],
        };

        self.emit_native_primitive(primitive);
    }
}
