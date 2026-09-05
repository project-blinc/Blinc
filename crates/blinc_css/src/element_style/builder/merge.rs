//! Merging one style over another, and emptiness checks.

use crate::element_style::*;

impl ElementStyle {
    // =========================================================================
    // Merging
    // =========================================================================

    /// Merge another style on top of this one
    ///
    /// Properties from `other` will override properties in `self` if they are set.
    /// Unset properties in `other` will not override.
    pub fn merge(&self, other: &ElementStyle) -> ElementStyle {
        ElementStyle {
            // Visual
            background: other.background.clone().or_else(|| self.background.clone()),
            corner_radius: other.corner_radius.or(self.corner_radius),
            corner_shape: other.corner_shape.or(self.corner_shape),
            shadow: if !other.shadow.is_empty() {
                other.shadow.clone()
            } else {
                self.shadow.clone()
            },
            transform: other.transform.clone().or_else(|| self.transform.clone()),
            material: other.material.clone().or_else(|| self.material.clone()),
            render_layer: other.render_layer.or(self.render_layer),
            opacity: other.opacity.or(self.opacity),
            text_color: other.text_color.or(self.text_color),
            font_size: other.font_size.or(self.font_size),
            text_shadow: other.text_shadow.or(self.text_shadow),
            font_weight: other.font_weight.or(self.font_weight),
            font_style: other.font_style.or(self.font_style),
            text_decoration: other.text_decoration.or(self.text_decoration),
            line_height: other.line_height.or(self.line_height),
            text_align: other.text_align.or(self.text_align),
            letter_spacing: other.letter_spacing.or(self.letter_spacing),
            rotate: other.rotate.or(self.rotate),
            scale_x: other.scale_x.or(self.scale_x),
            scale_y: other.scale_y.or(self.scale_y),
            skew_x: other.skew_x.or(self.skew_x),
            skew_y: other.skew_y.or(self.skew_y),
            transform_origin: other.transform_origin.or(self.transform_origin),
            animation: other.animation.clone().or_else(|| self.animation.clone()),
            transition: other.transition.clone().or_else(|| self.transition.clone()),
            // 3D
            rotate_x: other.rotate_x.or(self.rotate_x),
            rotate_y: other.rotate_y.or(self.rotate_y),
            perspective: other.perspective.or(self.perspective),
            shape_3d: other.shape_3d.clone().or_else(|| self.shape_3d.clone()),
            depth: other.depth.or(self.depth),
            light_direction: other.light_direction.or(self.light_direction),
            light_intensity: other.light_intensity.or(self.light_intensity),
            ambient: other.ambient.or(self.ambient),
            specular: other.specular.or(self.specular),
            translate_z: other.translate_z.or(self.translate_z),
            op_3d: other.op_3d.clone().or_else(|| self.op_3d.clone()),
            blend_3d: other.blend_3d.or(self.blend_3d),
            // Clip-path
            clip_path: other.clip_path.clone().or_else(|| self.clip_path.clone()),
            filter: other.filter.or(self.filter),
            // Layout
            width: other.width.or(self.width),
            height: other.height.or(self.height),
            min_width: other.min_width.or(self.min_width),
            min_height: other.min_height.or(self.min_height),
            max_width: other.max_width.or(self.max_width),
            max_height: other.max_height.or(self.max_height),
            display: other.display.or(self.display),
            flex_direction: other.flex_direction.or(self.flex_direction),
            flex_wrap: other.flex_wrap.or(self.flex_wrap),
            flex_grow: other.flex_grow.or(self.flex_grow),
            flex_shrink: other.flex_shrink.or(self.flex_shrink),
            align_items: other.align_items.or(self.align_items),
            justify_content: other.justify_content.or(self.justify_content),
            align_self: other.align_self.or(self.align_self),
            padding: other.padding.or(self.padding),
            margin: other.margin.or(self.margin),
            gap: other.gap.or(self.gap),
            overflow: other.overflow.or(self.overflow),
            overflow_x: other.overflow_x.or(self.overflow_x),
            overflow_y: other.overflow_y.or(self.overflow_y),
            overflow_fade: other.overflow_fade.or(self.overflow_fade),
            border_width: other.border_width.or(self.border_width),
            border_color: other.border_color.or(self.border_color),
            outline_width: other.outline_width.or(self.outline_width),
            outline_color: other.outline_color.or(self.outline_color),
            outline_offset: other.outline_offset.or(self.outline_offset),
            // Form element properties
            caret_color: other.caret_color.or(self.caret_color),
            selection_color: other.selection_color.or(self.selection_color),
            placeholder_color: other.placeholder_color.or(self.placeholder_color),
            accent_color: other.accent_color.or(self.accent_color),
            // Scrollbar
            scrollbar_color: other.scrollbar_color.or(self.scrollbar_color),
            scrollbar_width: other.scrollbar_width.or(self.scrollbar_width),
            // SVG
            fill: other.fill.or(self.fill),
            stroke: other.stroke.or(self.stroke),
            stroke_width: other.stroke_width.or(self.stroke_width),
            stroke_dasharray: other
                .stroke_dasharray
                .clone()
                .or(self.stroke_dasharray.clone()),
            stroke_dashoffset: other.stroke_dashoffset.or(self.stroke_dashoffset),
            svg_path_data: other.svg_path_data.clone().or(self.svg_path_data.clone()),
            position: other.position.or(self.position),
            top: other.top.or(self.top),
            right: other.right.or(self.right),
            bottom: other.bottom.or(self.bottom),
            left: other.left.or(self.left),
            z_index: other.z_index.or(self.z_index),
            visibility: other.visibility.or(self.visibility),
            // Image
            object_fit: other.object_fit.or(self.object_fit),
            object_position: other.object_position.or(self.object_position),
            loading_strategy: other.loading_strategy.or(self.loading_strategy),
            image_placeholder_type: other.image_placeholder_type.or(self.image_placeholder_type),
            image_placeholder_color: other
                .image_placeholder_color
                .or(self.image_placeholder_color),
            image_placeholder_image: other
                .image_placeholder_image
                .clone()
                .or_else(|| self.image_placeholder_image.clone()),
            fade_duration_ms: other.fade_duration_ms.or(self.fade_duration_ms),
            // Interaction
            pointer_events: other.pointer_events.or(self.pointer_events),
            cursor: other.cursor.or(self.cursor),
            // Blend mode
            mix_blend_mode: other.mix_blend_mode.or(self.mix_blend_mode),
            // Text decoration enhancements
            text_decoration_color: other.text_decoration_color.or(self.text_decoration_color),
            text_decoration_thickness: other
                .text_decoration_thickness
                .or(self.text_decoration_thickness),
            // Text overflow
            text_overflow: other.text_overflow.or(self.text_overflow),
            white_space: other.white_space.or(self.white_space),
            // Mask
            mask_image: other
                .mask_image
                .as_ref()
                .or(self.mask_image.as_ref())
                .cloned(),
            mask_mode: other.mask_mode.clone().or(self.mask_mode.clone()),
            // Flow DAG
            flow: other.flow.clone().or_else(|| self.flow.clone()),
            // Pointer query
            pointer_space: other
                .pointer_space
                .clone()
                .or_else(|| self.pointer_space.clone()),
            // Dynamic properties (merge: other's override self's for same property type)
            dynamic_properties: match (&self.dynamic_properties, &other.dynamic_properties) {
                (None, None) => None,
                (Some(a), None) => Some(a.clone()),
                (None, Some(b)) => Some(b.clone()),
                (Some(a), Some(b)) => {
                    let mut merged = a.clone();
                    merged.extend(b.iter().cloned());
                    Some(merged)
                }
            },
        }
    }

    /// Check if any visual property is set
    pub fn has_visual_props(&self) -> bool {
        self.background.is_some()
            || self.corner_radius.is_some()
            || self.corner_shape.is_some()
            || !self.shadow.is_empty()
            || self.transform.is_some()
            || self.material.is_some()
            || self.render_layer.is_some()
            || self.opacity.is_some()
            || self.animation.is_some()
            || self.z_index.is_some()
            || self.visibility.is_some()
            || self.overflow_fade.is_some()
    }

    /// Check if any layout property is set
    pub fn has_layout_props(&self) -> bool {
        self.width.is_some()
            || self.height.is_some()
            || self.min_width.is_some()
            || self.min_height.is_some()
            || self.max_width.is_some()
            || self.max_height.is_some()
            || self.display.is_some()
            || self.flex_direction.is_some()
            || self.flex_wrap.is_some()
            || self.flex_grow.is_some()
            || self.flex_shrink.is_some()
            || self.align_items.is_some()
            || self.justify_content.is_some()
            || self.align_self.is_some()
            || self.padding.is_some()
            || self.margin.is_some()
            || self.gap.is_some()
            || self.overflow.is_some()
            || self.overflow_x.is_some()
            || self.overflow_y.is_some()
            || self.border_width.is_some()
            || self.border_color.is_some()
            || self.position.is_some()
            || self.top.is_some()
            || self.right.is_some()
            || self.bottom.is_some()
            || self.left.is_some()
            || self.visibility.is_some()
    }

    /// Check if no property is set
    pub fn is_empty(&self) -> bool {
        !self.has_visual_props() && !self.has_layout_props()
    }
}
