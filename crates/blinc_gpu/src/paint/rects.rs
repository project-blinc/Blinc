//! Rectangle fills and strokes, including notches and per-side borders.

use super::GpuPaintContext;
use crate::primitives::{
    ClipType, FillType, GlassType, GpuGlassPrimitive, GpuPrimitive, PrimitiveType,
};
use blinc_core::{Brush, Color, CornerRadius, DrawContext, Rect, Shadow, Stroke};

impl GpuPaintContext<'_> {
    pub(super) fn fill_rect_impl(&mut self, rect: Rect, corner_radius: CornerRadius, brush: Brush) {
        // Diagnostic for the recurring bg-split-in-half artefact.
        // Set BLINC_DEBUG_FILL_RECT=1 to print every UNIQUE solid /
        // gradient fill (deduplicated by bounds + brush bits across
        // every frame). With dedup, opening an affected widget adds
        // a small number of lines to the log — what we want to see
        // is whether the bug correlates with two fills at the SAME
        // bounds with different colors (stacked paint) or a single
        // gradient brush sneaking through. The dump includes
        // clip_bounds / clip_type / clip_radius captured at emit
        // time so we can see whether the active clip stack is
        // truncating the primitive to half-height in the affected
        // (canvas-underneath) configurations.
        if matches!(std::env::var("BLINC_DEBUG_FILL_RECT").as_deref(), Ok("1")) {
            use std::sync::{Mutex, OnceLock};
            static SEEN: OnceLock<Mutex<std::collections::HashSet<u64>>> = OnceLock::new();
            let seen = SEEN.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
            // Cheap stable hash of (bounds, color) → u64. We bit-cast
            // the floats so any change in any field produces a
            // different hash.
            let hash_key = |bits: &[u32]| -> u64 {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut h = DefaultHasher::new();
                bits.hash(&mut h);
                h.finish()
            };
            let (cb, cr, _ccs, ct) = self.get_clip_data();
            let ct_name = match ct {
                ClipType::None => "none",
                ClipType::Rect => "rect",
                ClipType::Circle => "circle",
                ClipType::Ellipse => "ellipse",
                ClipType::Polygon => "polygon",
            };
            match &brush {
                Brush::Solid(c) => {
                    let key = hash_key(&[
                        rect.x().to_bits(),
                        rect.y().to_bits(),
                        rect.width().to_bits(),
                        rect.height().to_bits(),
                        c.r.to_bits(),
                        c.g.to_bits(),
                        c.b.to_bits(),
                        c.a.to_bits(),
                    ]);
                    if seen.lock().unwrap().insert(key) {
                        eprintln!(
                            "[fill_rect#{:x}] solid bounds=({:.1},{:.1},{:.1},{:.1}) color=({:.3},{:.3},{:.3},{:.3}) cr=({:.1},{:.1},{:.1},{:.1}) clip={}({:.1},{:.1},{:.1},{:.1}) clip_r=({:.1},{:.1},{:.1},{:.1}) fg={}",
                            key & 0xFFFF,
                            rect.x(),
                            rect.y(),
                            rect.width(),
                            rect.height(),
                            c.r,
                            c.g,
                            c.b,
                            c.a,
                            corner_radius.top_left,
                            corner_radius.top_right,
                            corner_radius.bottom_right,
                            corner_radius.bottom_left,
                            ct_name,
                            cb[0],
                            cb[1],
                            cb[2],
                            cb[3],
                            cr[0],
                            cr[1],
                            cr[2],
                            cr[3],
                            self.is_foreground,
                        );
                    }
                }
                Brush::Gradient(_) => {
                    let key = hash_key(&[
                        rect.x().to_bits(),
                        rect.y().to_bits(),
                        rect.width().to_bits(),
                        rect.height().to_bits(),
                        0x6E61_7472, // tag bytes so gradient + same bounds don't dedup
                    ]);
                    if seen.lock().unwrap().insert(key) {
                        eprintln!(
                            "[fill_rect#{:x}] GRADIENT bounds=({:.1},{:.1},{:.1},{:.1}) clip={}({:.1},{:.1},{:.1},{:.1})",
                            key & 0xFFFF,
                            rect.x(),
                            rect.y(),
                            rect.width(),
                            rect.height(),
                            ct_name,
                            cb[0],
                            cb[1],
                            cb[2],
                            cb[3],
                        );
                    }
                }
                Brush::Blur(_) | Brush::Image(_) | Brush::Glass(_) => {}
            }
        }
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);

        // Handle glass brush specially - push to glass primitives
        if let Brush::Glass(style) = &brush {
            let mut glass = GpuGlassPrimitive::new(
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            )
            .with_corner_radius_per_corner(
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            )
            .with_blur(style.blur)
            .with_tint(style.tint.r, style.tint.g, style.tint.b, style.tint.a)
            .with_saturation(style.saturation)
            .with_brightness(style.brightness)
            .with_noise(style.noise)
            .with_border_thickness(style.border_thickness);

            // Apply border color if specified
            if let Some(bc) = style.border_color {
                glass = glass.with_border_color(bc.r, bc.g, bc.b, bc.a);
            }

            // Apply shadow if present in the glass style
            // Shadow values are in CSS px but glass bounds are DPI-scaled screen px,
            // so we must scale shadow params to match.
            if let Some(ref shadow) = style.shadow {
                let [a, b, c, d, ..] = self.current_affine().elements;
                let scale = (a * d - b * c).abs().sqrt().max(1e-6);
                glass = glass.with_shadow_offset(
                    shadow.blur * scale,
                    shadow.color.a, // Use color alpha as opacity
                    shadow.offset_x * scale,
                    shadow.offset_y * scale,
                );
            }

            // Apply current clip bounds to glass primitive (for scroll containers)
            let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();
            match clip_type {
                ClipType::None => {}
                ClipType::Rect => {
                    // Check if this is a rounded rect clip (non-zero radius)
                    let has_radius = clip_radius.iter().any(|&r| r > 0.0);
                    if has_radius {
                        glass = glass.with_clip_rounded_rect_per_corner(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                            clip_radius[0],
                            clip_radius[1],
                            clip_radius[2],
                            clip_radius[3],
                        );
                    } else {
                        glass = glass.with_clip_rect(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                        );
                    }
                }
                ClipType::Circle | ClipType::Ellipse | ClipType::Polygon => {
                    // For circle/ellipse/polygon clips, use bounding rect for now
                    // Full support would require shader changes
                    glass = glass.with_clip_rect(
                        clip_bounds[0] - clip_bounds[2],
                        clip_bounds[1] - clip_bounds[3],
                        clip_bounds[2] * 2.0,
                        clip_bounds[3] * 2.0,
                    );
                }
            }

            // Set glass type based on simple flag
            if style.simple {
                glass = glass.with_glass_type(GlassType::Simple);
            }

            self.emit_native_glass(glass, style.depth > 0);
            return;
        }

        // Handle Blur brush - convert to glass primitive with just blur and optional tint
        if let Brush::Blur(style) = &brush {
            let mut glass = GpuGlassPrimitive::new(
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            )
            .with_corner_radius_per_corner(
                scaled_radius.top_left,
                scaled_radius.top_right,
                scaled_radius.bottom_right,
                scaled_radius.bottom_left,
            )
            .with_blur(style.radius)
            .with_saturation(1.0) // No saturation adjustment for pure blur
            .with_brightness(1.0); // No brightness adjustment

            // Apply tint if specified
            if let Some(ref tint) = style.tint {
                glass = glass.with_tint(tint.r, tint.g, tint.b, tint.a * style.opacity);
            } else {
                // Default to slight white tint for visibility
                glass = glass.with_tint(1.0, 1.0, 1.0, 0.1 * style.opacity);
            }

            // Apply current clip bounds to glass primitive
            let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();
            match clip_type {
                ClipType::None => {}
                ClipType::Rect => {
                    let has_radius = clip_radius.iter().any(|&r| r > 0.0);
                    if has_radius {
                        glass = glass.with_clip_rounded_rect_per_corner(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                            clip_radius[0],
                            clip_radius[1],
                            clip_radius[2],
                            clip_radius[3],
                        );
                    } else {
                        glass = glass.with_clip_rect(
                            clip_bounds[0],
                            clip_bounds[1],
                            clip_bounds[2],
                            clip_bounds[3],
                        );
                    }
                }
                ClipType::Circle | ClipType::Ellipse | ClipType::Polygon => {
                    glass = glass.with_clip_rect(
                        clip_bounds[0] - clip_bounds[2],
                        clip_bounds[1] - clip_bounds[3],
                        clip_bounds[2] * 2.0,
                        clip_bounds[3] * 2.0,
                    );
                }
            }

            self.emit_native_glass(glass, false);
            return;
        }

        let (color, color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Convert OBB (0..1) gradient coords to rect-local pixel coords
        let gradient_params = Self::obb_to_rect_coords(&brush, gradient_params, rect, fill_type);

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        // Pack group shape descriptors into aux_data if this is a 3D group
        let mut border = [0.0_f32; 4];
        if !self.current_3d_group_shapes.is_empty() {
            let aux_offset = self.active_batch_mut().aux_data.len() as f32;
            let shape_count = self.current_3d_group_shapes.len() as f32;

            // Find max depth across all child shapes for AABB
            let mut max_depth: f32 = 1.0;
            for shape in &self.current_3d_group_shapes {
                max_depth = max_depth.max(shape.params[1]); // params[1] = depth
            }

            // Push each ShapeDesc as 4 vec4s into aux_data
            for index in 0..self.current_3d_group_shapes.len() {
                let shape = self.current_3d_group_shapes[index];
                self.active_batch_mut().aux_data.extend_from_slice(&[
                    shape.offset,
                    shape.params,
                    shape.half_ext,
                    shape.color,
                ]);
            }

            // border[0] = normal border width (unused for 3D groups)
            // border[1] = group shape count
            // border[2] = aux_data offset (in vec4 units)
            // border[3] = max depth for group AABB
            border = [0.0, shape_count, aux_offset, max_depth];
        }

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
            color,
            color2,
            border,
            border_color: [0.0; 4],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: transformed_gradient_params,
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
                PrimitiveType::Rect as u32,
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

        // Visible companion to the BLINC_DEBUG_FILL_RECT log: when
        // enabled, paint a thin coloured outline around every fill.
        // The outline colour is derived from the same per-frame hash
        // we'd otherwise log, so two stacked fills at the same
        // bounds with the same brush get the SAME outline colour
        // (visually overdrawn — looks normal) while two fills with
        // DIFFERENT colour or geometry get DIFFERENT outline
        // colours, immediately surfacing the bug visually.
        // Self-recursion is safe because `stroke_rect` doesn't call
        // back into `fill_rect`.
        if matches!(std::env::var("BLINC_DEBUG_FILL_RECT").as_deref(), Ok("1"))
            && !matches!(&brush, Brush::Blur(_) | Brush::Image(_) | Brush::Glass(_))
        {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            rect.x().to_bits().hash(&mut h);
            rect.y().to_bits().hash(&mut h);
            rect.width().to_bits().hash(&mut h);
            rect.height().to_bits().hash(&mut h);
            if let Brush::Solid(c) = &brush {
                c.r.to_bits().hash(&mut h);
                c.g.to_bits().hash(&mut h);
                c.b.to_bits().hash(&mut h);
                c.a.to_bits().hash(&mut h);
            }
            let hue = (h.finish() & 0xFFFF) as f32 / 65536.0;
            // Pleasant saturated outline regardless of underlying
            // bg: 6-segment hue wheel, full saturation.
            let (r, g, b) = match (hue * 6.0) as u32 {
                0 => (1.0, hue * 6.0, 0.0),
                1 => (2.0 - hue * 6.0, 1.0, 0.0),
                2 => (0.0, 1.0, hue * 6.0 - 2.0),
                3 => (0.0, 4.0 - hue * 6.0, 1.0),
                4 => (hue * 6.0 - 4.0, 0.0, 1.0),
                _ => (1.0, 0.0, 6.0 - hue * 6.0),
            };
            let outline = Color::rgba(r, g, b, 0.95);
            self.stroke_rect(
                rect,
                corner_radius,
                &Stroke::new(1.0),
                Brush::Solid(outline),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fill_notch_impl(
        &mut self,
        rect: Rect,
        corner_radii: [f32; 4],
        corner_types: [f32; 4],
        top_mod: [f32; 4],
        bottom_mod: [f32; 4],
        border: Option<(f32, Color)>,
        shadow: Option<Shadow>,
        brush: Brush,
    ) {
        let transformed = self.transform_rect(rect);

        // DPI-scale the per-corner radii. `scale_corner_radius` wants a
        // `CornerRadius` struct, so we route through that — the magnitudes
        // match field-for-field.
        let scaled_radii = self.scale_corner_radius(CornerRadius {
            top_left: corner_radii[0],
            top_right: corner_radii[1],
            bottom_right: corner_radii[2],
            bottom_left: corner_radii[3],
        });
        let scaled_radii_arr = [
            scaled_radii.top_left,
            scaled_radii.top_right,
            scaled_radii.bottom_right,
            scaled_radii.bottom_left,
        ];

        // Top/bottom edge modifiers: (type, width, height_or_depth, corner_r).
        // Scale the geometric dimensions uniformly by the current DPI +
        // element scale, leaving the modifier type untouched. Non-uniform
        // scale / rotation / skew is already applied via `local_affine` in
        // the fragment shader, so we only need the isotropic scale here.
        let dpi_scale = self.current_dpi_scale();
        let scale_mod = |m: [f32; 4]| -> [f32; 4] {
            [m[0], m[1] * dpi_scale, m[2] * dpi_scale, m[3] * dpi_scale]
        };
        let scaled_top_mod = scale_mod(top_mod);
        let scaled_bottom_mod = scale_mod(bottom_mod);

        let (color, color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Transform gradient params into rect-local then screen space, just
        // like `fill_rect` does.
        let gradient_params = Self::obb_to_rect_coords(&brush, gradient_params, rect, fill_type);
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        // Shadow (DPI-scaled). The shader's PRIM_NOTCH shadow path
        // traces the notch's actual outline via `sd_notch` so the
        // drop shadow follows the shape's outer edge (including
        // concave arcs, bulges, scoops, cuts, peaks) rather than the
        // rectangular bbox. The notch's element-level canvas clip is
        // now opt-in (via `overflow_clip`), so the shadow's blur
        // expansion can render past the element's layout box.
        let shadow_vec = if let Some(sh) = shadow {
            [
                sh.offset_x * dpi_scale,
                sh.offset_y * dpi_scale,
                sh.blur * dpi_scale,
                sh.spread * dpi_scale,
            ]
        } else {
            [0.0; 4]
        };
        let shadow_color_vec = shadow
            .map(|s| [s.color.r, s.color.g, s.color.b, s.color.a])
            .unwrap_or([0.0; 4]);

        // Border (DPI-scaled).
        let (border_vec, border_color_vec) = if let Some((width, color)) = border {
            (
                [width * dpi_scale, 0.0, 0.0, 0.0],
                [color.r, color.g, color.b, color.a],
            )
        } else {
            ([0.0; 4], [0.0; 4])
        };

        let primitive = GpuPrimitive {
            bounds: [
                transformed.x(),
                transformed.y(),
                transformed.width(),
                transformed.height(),
            ],
            corner_radius: scaled_radii_arr,
            color,
            color2,
            border: border_vec,
            border_color: border_color_vec,
            shadow: shadow_vec,
            shadow_color: shadow_color_vec,
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: transformed_gradient_params,
            rotation: self.current_rotation_sincos(),
            local_affine: self.current_local_affine(),
            // 3D slots repurposed for notch parameters — see `PrimitiveType::Notch`
            // doc comment in blinc_gpu::primitives for the contract.
            perspective: scaled_top_mod,
            sdf_3d: scaled_bottom_mod,
            light: corner_types,
            filter_a: self.current_filter_a,
            filter_b: self.current_filter_b,
            mask_params: self.current_mask_params,
            mask_info: self.current_mask_info,
            corner_shape: self.current_corner_shape,
            clip_fade: self.get_clip_fade(),
            type_info: [
                PrimitiveType::Notch as u32,
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

    pub(super) fn fill_rect_with_per_side_border_impl(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        brush: Brush,
        border_widths: [f32; 4],
        border_color: Color,
    ) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let (color, color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Scale border widths by transform
        let affine = self.current_affine();
        let a = affine.elements[0];
        let b = affine.elements[1];
        let c = affine.elements[2];
        let d = affine.elements[3];
        let scale_x = (a * a + b * b).sqrt();
        let scale_y = (c * c + d * d).sqrt();

        let scaled_borders = [
            border_widths[0] * scale_y, // top (vertical scale)
            border_widths[1] * scale_x, // right (horizontal scale)
            border_widths[2] * scale_y, // bottom (vertical scale)
            border_widths[3] * scale_x, // left (horizontal scale)
        ];

        // Convert OBB (0..1) gradient coords to rect-local pixel coords
        let gradient_params = Self::obb_to_rect_coords(&brush, gradient_params, rect, fill_type);

        // Transform gradient params to screen space
        let is_radial = fill_type == FillType::RadialGradient;
        let transformed_gradient_params = if fill_type != FillType::Solid {
            self.transform_gradient_params(gradient_params, is_radial)
        } else {
            gradient_params
        };

        let opacity = self.combined_opacity();
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
            color,
            color2,
            border: scaled_borders,
            border_color: [
                border_color.r,
                border_color.g,
                border_color.b,
                border_color.a * opacity,
            ],
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params: transformed_gradient_params,
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
                PrimitiveType::Rect as u32,
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

    pub(super) fn stroke_rect_impl(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        stroke: &Stroke,
        brush: Brush,
    ) {
        let transformed = self.transform_rect(rect);
        let scaled_radius = self.scale_corner_radius(corner_radius);
        let (color, _color2, gradient_params, fill_type) = self.brush_to_colors(&brush);
        let (clip_bounds, clip_radius, clip_corner_shape, clip_type) = self.get_clip_data();

        // Mirror BLINC_DEBUG_FILL_RECT for stroke_rect — CSS outlines
        // (focus rings) emit through this path, and the bg-split-in-
        // half symptom catches the focused outline too, so the dump
        // has to see both fills and strokes to triangulate which
        // ancestor clip is truncating the visible rect.
        if matches!(std::env::var("BLINC_DEBUG_FILL_RECT").as_deref(), Ok("1")) {
            use std::sync::{Mutex, OnceLock};
            static SEEN: OnceLock<Mutex<std::collections::HashSet<u64>>> = OnceLock::new();
            let seen = SEEN.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            rect.x().to_bits().hash(&mut h);
            rect.y().to_bits().hash(&mut h);
            rect.width().to_bits().hash(&mut h);
            rect.height().to_bits().hash(&mut h);
            stroke.width.to_bits().hash(&mut h);
            color[0].to_bits().hash(&mut h);
            color[1].to_bits().hash(&mut h);
            color[2].to_bits().hash(&mut h);
            color[3].to_bits().hash(&mut h);
            let key = h.finish();
            if seen.lock().unwrap().insert(key) {
                let ct_name = match clip_type {
                    ClipType::None => "none",
                    ClipType::Rect => "rect",
                    ClipType::Circle => "circle",
                    ClipType::Ellipse => "ellipse",
                    ClipType::Polygon => "polygon",
                };
                eprintln!(
                    "[stroke_rect#{:x}] bounds=({:.1},{:.1},{:.1},{:.1}) w={:.1} color=({:.3},{:.3},{:.3},{:.3}) cr=({:.1},{:.1},{:.1},{:.1}) clip={}({:.1},{:.1},{:.1},{:.1}) clip_r=({:.1},{:.1},{:.1},{:.1}) fg={}",
                    key & 0xFFFF,
                    rect.x(),
                    rect.y(),
                    rect.width(),
                    rect.height(),
                    stroke.width,
                    color[0],
                    color[1],
                    color[2],
                    color[3],
                    corner_radius.top_left,
                    corner_radius.top_right,
                    corner_radius.bottom_right,
                    corner_radius.bottom_left,
                    ct_name,
                    clip_bounds[0],
                    clip_bounds[1],
                    clip_bounds[2],
                    clip_bounds[3],
                    clip_radius[0],
                    clip_radius[1],
                    clip_radius[2],
                    clip_radius[3],
                    self.is_foreground,
                );
            }
        }

        // Scale border width by the current transform's uniform scale (DPI + CSS transforms)
        let scaled_border_width = stroke.width * self.current_uniform_scale();

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
            color: [0.0, 0.0, 0.0, 0.0], // Transparent fill
            color2: [0.0, 0.0, 0.0, 0.0],
            border: [scaled_border_width, 0.0, 0.0, 0.0],
            border_color: color,
            shadow: [0.0; 4],
            shadow_color: [0.0; 4],
            clip_bounds,
            clip_radius,
            clip_corner_shape,
            gradient_params,
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
                PrimitiveType::Rect as u32,
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
