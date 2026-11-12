//! Drop and inner shadows for rectangles and circles.

use super::GpuPaintContext;
use crate::primitives::{FillType, GpuPrimitive, PrimitiveType};
use blinc_core::{CornerRadius, Point, Rect, Shadow};

impl GpuPaintContext<'_> {
    pub(super) fn draw_shadow_impl(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        shadow: Shadow,
    ) {
        // A shadow with no colour paints nothing, but the shader would still run over its whole
        // blurred area.
        if shadow.color.a * self.combined_opacity() <= 0.0 {
            return;
        }
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Scale shadow values by the current transform's uniform scale (DPI + CSS transforms).
        // Shadow offset, blur, and spread are in logical pixels but the shader
        // operates in physical pixel space after the DPI transform.
        let s = self.current_uniform_scale();

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color: [0.0, 0.0, 0.0, 0.0], // Shadow is not filled
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [
                shadow.offset_x * s,
                shadow.offset_y * s,
                shadow.blur * s,
                shadow.spread * s,
            ],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
            rotation: self.current_rotation_sincos(),
            local_affine: self.current_local_affine(),
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
                PrimitiveType::Shadow as u32,
                FillType::Solid as u32,
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

    pub(super) fn draw_inner_shadow_impl(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        shadow: Shadow,
    ) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Scale shadow values by the current transform's uniform scale (DPI + CSS transforms)
        let s = self.current_uniform_scale();

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: [
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            ],
            color: [0.0, 0.0, 0.0, 0.0], // Inner shadow is not filled
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [
                shadow.offset_x * s,
                shadow.offset_y * s,
                shadow.blur * s,
                shadow.spread * s,
            ],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
            rotation: self.current_rotation_sincos(),
            local_affine: self.current_local_affine(),
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
                PrimitiveType::InnerShadow as u32,
                FillType::Solid as u32,
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

    pub(super) fn draw_circle_shadow_impl(&mut self, center: Point, radius: f32, shadow: Shadow) {
        let transformed_center = self.transform_point(center);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Store circle as bounds where the circle fits
        let size = radius * 2.0;
        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - radius,
                transformed_center.y - radius,
                size,
                size,
            ],
            corner_radius: [radius, radius, radius, radius], // Used as circle radius indicator
            color: [0.0, 0.0, 0.0, 0.0],
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
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
                PrimitiveType::CircleShadow as u32,
                FillType::Solid as u32,
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

    pub(super) fn draw_circle_inner_shadow_impl(
        &mut self,
        center: Point,
        radius: f32,
        shadow: Shadow,
    ) {
        let transformed_center = self.transform_point(center);
        let opacity = self.combined_opacity();
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        let size = radius * 2.0;
        let primitive = GpuPrimitive {
            bounds: [
                transformed_center.x - radius,
                transformed_center.y - radius,
                size,
                size,
            ],
            corner_radius: [radius, radius, radius, radius],
            color: [0.0, 0.0, 0.0, 0.0],
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [0.0; 4],
            border_color: [0.0; 4],
            shadow: [shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread],
            shadow_color: [
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a * opacity,
            ],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: [0.0, 0.0, 1.0, 0.0],
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
                PrimitiveType::CircleInnerShadow as u32,
                FillType::Solid as u32,
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
