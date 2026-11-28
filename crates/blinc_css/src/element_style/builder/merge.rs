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
        // The sides below, without what a uniform border above covers.
        let mut under = ElementStyle {
            border_top: self.border_top,
            border_right: self.border_right,
            border_bottom: self.border_bottom,
            border_left: self.border_left,
            ..ElementStyle::default()
        };
        under.reset_border_sides(other.border_width.is_some(), other.border_color.is_some());
        let mut merged = ElementStyle {
            component: self.component && other.component,
            important_colors: ImportantColors::combine(
                self.important_colors.as_ref(),
                other.important_colors.as_ref(),
            ),
            // Visual
            background: other.background.clone().or_else(|| self.background.clone()),
            corner_radius: other.corner_radius.or(self.corner_radius),
            corner_shape: other.corner_shape.or(self.corner_shape),
            // `box-shadow` sets both lists at once, so a style that sets either
            // replaces both: an outer-only value clears an earlier inset.
            shadow: if other.sets_box_shadow() {
                other.shadow.clone()
            } else {
                self.shadow.clone()
            },
            inner_shadow: if other.sets_box_shadow() {
                other.inner_shadow.clone()
            } else {
                self.inner_shadow.clone()
            },
            transform: other.transform.clone().or_else(|| self.transform.clone()),
            material: other.material.clone().or_else(|| self.material.clone()),
            render_layer: other.render_layer.or(self.render_layer),
            opacity: other.opacity.or(self.opacity),
            text_color: other.text_color.or(self.text_color),
            font_family: other
                .font_family
                .clone()
                .or_else(|| self.font_family.clone()),
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
            skew_in_transform: if other.transform.is_some() {
                other.skew_in_transform
            } else if other.skew_x.is_some() || other.skew_y.is_some() {
                false
            } else {
                self.skew_in_transform
            },
            skew_y: other.skew_y.or(self.skew_y),
            translate_pct: other.translate_pct.or(self.translate_pct),
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
            padding: merge_spacing(self.padding, other.padding),
            margin_auto: merge_margin_auto(self, other),
            flex_basis: other.flex_basis.or(self.flex_basis),
            aspect_ratio: other.aspect_ratio.or(self.aspect_ratio),
            grid_template_columns: other
                .grid_template_columns
                .clone()
                .or_else(|| self.grid_template_columns.clone()),
            grid_template_rows: other
                .grid_template_rows
                .clone()
                .or_else(|| self.grid_template_rows.clone()),
            grid_column: other.grid_column.or(self.grid_column),
            grid_row: other.grid_row.or(self.grid_row),
            row_gap: other
                .row_gap
                .or(self.row_gap.filter(|_| other.gap.is_none())),
            column_gap: other
                .column_gap
                .or(self.column_gap.filter(|_| other.gap.is_none())),
            margin: merge_spacing(self.margin, other.margin),
            gap: other.gap.or(self.gap),
            overflow: other.overflow.or(self.overflow),
            overflow_x: other
                .overflow_x
                .or(self.overflow_x.filter(|_| other.overflow.is_none())),
            overflow_y: other
                .overflow_y
                .or(self.overflow_y.filter(|_| other.overflow.is_none())),
            overflow_fade: other.overflow_fade.or(self.overflow_fade),
            border_width: other.border_width.or(self.border_width),
            border_color: other.border_color.or(self.border_color),
            border_top: merge_side(under.border_top, other.border_top),
            border_right: merge_side(under.border_right, other.border_right),
            border_bottom: merge_side(under.border_bottom, other.border_bottom),
            border_left: merge_side(under.border_left, other.border_left),
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
        };
        merged.restore_important_colors();
        merged
    }

    /// Whether this style sets `box-shadow`, whose outer and inset layers
    /// live in `shadow` and `inner_shadow`.
    pub fn sets_box_shadow(&self) -> bool {
        !self.shadow.is_empty() || !self.inner_shadow.is_empty()
    }

    /// Check if any visual property is set
    pub fn has_visual_props(&self) -> bool {
        self.background.is_some()
            || self.corner_radius.is_some()
            || self.corner_shape.is_some()
            || !self.shadow.is_empty()
            || !self.inner_shadow.is_empty()
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
            || self.row_gap.is_some()
            || self.column_gap.is_some()
            || self.flex_basis.is_some()
            || self.aspect_ratio.is_some()
            || self.grid_template_columns.is_some()
            || self.grid_template_rows.is_some()
            || self.grid_column.is_some()
            || self.grid_row.is_some()
            || self.margin_auto.iter().any(|a| *a)
            || self.overflow.is_some()
            || self.overflow_x.is_some()
            || self.overflow_y.is_some()
            || self.border_width.is_some()
            || self.border_color.is_some()
            || self.font_family.is_some()
            || self.border_top.is_some()
            || self.border_right.is_some()
            || self.border_bottom.is_some()
            || self.border_left.is_some()
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

impl ElementStyle {
    /// Drops the width, the colour or both from every per-side border, as a
    /// uniform declaration of them does.
    pub(crate) fn reset_border_sides(&mut self, width: bool, color: bool) {
        for slot in [
            &mut self.border_top,
            &mut self.border_right,
            &mut self.border_bottom,
            &mut self.border_left,
        ] {
            *slot = slot
                .map(|side| BorderSideStyle {
                    width: side.width.filter(|_| !width),
                    color: side.color.filter(|_| !color),
                })
                .filter(BorderSideStyle::is_set);
        }
    }
}

fn merge_spacing(a: Option<SpacingRect>, b: Option<SpacingRect>) -> Option<SpacingRect> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.merge(b)),
        (a, b) => b.or(a),
    }
}

/// Which margin sides are `auto`: a side `over` declares takes its answer
/// from `over`, and any other keeps what is below.
fn merge_margin_auto(under: &ElementStyle, over: &ElementStyle) -> [bool; 4] {
    let declared = over.margin.map_or([None; 4], SpacingRect::sides);
    std::array::from_fn(|side| {
        over.margin_auto[side] || (declared[side].is_none() && under.margin_auto[side])
    })
}

fn merge_side(a: Option<BorderSideStyle>, b: Option<BorderSideStyle>) -> Option<BorderSideStyle> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.merge(b)),
        (a, b) => b.or(a),
    }
}
