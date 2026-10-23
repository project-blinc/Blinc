//! Patch cached motion-binding geometry and opacity.
use super::{LayoutNodeId, RenderContext, RenderTree, bounds_union_of_range};

impl RenderContext {
    /// Patch dynamic-owner binding ranges and their baselines. Return false when any
    /// affected stream requires a full paint.
    pub fn apply_binding_deltas(&mut self, tree: &blinc_layout::RenderTree, scale: f32) -> bool {
        self.last_binding_damage_rects.clear();
        // Binding ranges index the dynamic batch. Static-owned ranges require a repaint
        // before mutation.
        let batch = match self.cached_dynamic_batch.as_mut() {
            Some(b) => b,
            None => return false,
        };
        if batch.has_ordered_draws()
            || batch.has_foreground_content()
            || batch.has_foreground_layers()
        {
            return false;
        }
        let mut bindings = tree.composite_bindings_mut();
        if bindings.values().any(|meta| !meta.dynamic_batch) {
            return false;
        }
        if bindings.is_empty() {
            // An unchanged cache still needs dispatch to present a fresh frame.
            return true;
        }

        // Track active bindings so fast frames keep the redraw signal current.
        let motion_bindings = tree.motion_bindings_map();

        // Compute ancestor translation deltas before patching. Strictly containing
        // primitive ranges identify ancestors; their summed delta moves each child's
        // scale/rotation pivot into current world coordinates.
        let mut raw_translate_deltas: std::collections::HashMap<
            blinc_layout::tree::LayoutNodeId,
            (f32, f32),
        > = std::collections::HashMap::new();
        for (node, meta) in bindings.iter() {
            let Some(b) = motion_bindings.get(node) else {
                continue;
            };
            let tx = b
                .translate_x
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(0.0);
            let ty = b
                .translate_y
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(0.0);
            raw_translate_deltas.insert(
                *node,
                (tx - meta.last_translate.0, ty - meta.last_translate.1),
            );
        }
        let mut inherited_shifts: std::collections::HashMap<
            blinc_layout::tree::LayoutNodeId,
            (f32, f32),
        > = std::collections::HashMap::new();
        for (node, meta) in bindings.iter() {
            let mut shift = (0.0f32, 0.0f32);
            for (other_node, other_meta) in bindings.iter() {
                if other_node == node {
                    continue;
                }
                // Equal ranges share the same primitive set, so they need no separate
                // inherited pivot shift.
                let contains = other_meta.primitive_range.start <= meta.primitive_range.start
                    && other_meta.primitive_range.end >= meta.primitive_range.end
                    && (other_meta.primitive_range.start < meta.primitive_range.start
                        || other_meta.primitive_range.end > meta.primitive_range.end);
                if contains {
                    if let Some(&(dx, dy)) = raw_translate_deltas.get(other_node) {
                        shift.0 += dx;
                        shift.1 += dy;
                    }
                }
            }
            inherited_shifts.insert(*node, shift);
        }

        let mut any_binding_active = false;
        for (node, meta) in bindings.iter_mut() {
            let bindings_for_node = match motion_bindings.get(node) {
                Some(b) => b,
                None => continue,
            };
            // Snapshot the AABB before any patching this frame. The
            // damage rect for this binding = union of the AABB before
            // and after — whichever pixels the static cache might be
            // showing stale need re-painting.
            let aabb_before = meta.last_screen_aabb;
            let mut binding_moved = false;

            // Keep requesting frames for an active binding even before its first value
            // changes.
            if bindings_for_node.is_any_animating() {
                any_binding_active = true;
            }

            // ----------------------------------------------------------------
            // Translate: read new (tx, ty), compute pixel-space delta,
            // shift every primitive's `bounds.xy` by it.
            // ----------------------------------------------------------------
            let (binding_tx, binding_ty) = {
                let tx = bindings_for_node
                    .translate_x
                    .as_ref()
                    .and_then(|v| v.lock().ok().map(|g| g.get()))
                    .unwrap_or(0.0);
                let ty = bindings_for_node
                    .translate_y
                    .as_ref()
                    .and_then(|v| v.lock().ok().map(|g| g.get()))
                    .unwrap_or(0.0);
                (tx, ty)
            };
            let new_translate = (binding_tx, binding_ty);
            let dx_logical = new_translate.0 - meta.last_translate.0;
            let dy_logical = new_translate.1 - meta.last_translate.1;
            if dx_logical.abs() > f32::EPSILON || dy_logical.abs() > f32::EPSILON {
                let dx_phys = dx_logical * scale;
                let dy_phys = dy_logical * scale;
                if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                    for p in prims.iter_mut() {
                        p.bounds[0] += dx_phys;
                        p.bounds[1] += dy_phys;
                    }
                }
                meta.last_translate = new_translate;
                any_binding_active = true;
                binding_moved = true;
            }

            // Patch group opacity directly, or scale primitive alpha by the opacity ratio.
            // An ungrouped zero baseline requires repaint.
            let new_opacity = bindings_for_node
                .opacity
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(meta.last_opacity);
            if (new_opacity - meta.last_opacity).abs() > f32::EPSILON {
                if let Some(push_idx) = meta.layer_push_index {
                    // Grouped opacity is an absolute command update, so zero is a valid
                    // baseline.
                    use blinc_gpu::primitives::LayerCommand;
                    match batch.layer_commands.get_mut(push_idx) {
                        Some(entry) => {
                            if let LayerCommand::Push { config } = &mut entry.command {
                                config.opacity = new_opacity;
                            } else {
                                return false;
                            }
                        }
                        None => return false,
                    }
                    meta.last_opacity = new_opacity;
                    any_binding_active = true;
                } else {
                    if meta.last_opacity.abs() < f32::EPSILON {
                        return false;
                    }
                    let ratio = new_opacity / meta.last_opacity;
                    if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                        for p in prims.iter_mut() {
                            p.color[3] *= ratio;
                            p.border_color[3] *= ratio;
                            p.shadow_color[3] *= ratio;
                        }
                    }
                    meta.last_opacity = new_opacity;
                    any_binding_active = true;
                }
                binding_moved = true;
            }

            // Scale bounds around the recorded pivot using new_scale / last_scale. Reject
            // degenerate baselines and convert the logical pivot to physical pixels.
            let binding_scale = bindings_for_node
                .scale
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(1.0);
            let binding_scale_x = bindings_for_node
                .scale_x
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(1.0);
            let binding_scale_y = bindings_for_node
                .scale_y
                .as_ref()
                .and_then(|v| v.lock().ok().map(|g| g.get()))
                .unwrap_or(1.0);
            // Persist inherited translation on every frame so later scale changes use the
            // current pivot.
            if let Some(&(sx_shift, sy_shift)) = inherited_shifts.get(node) {
                if sx_shift.abs() > f32::EPSILON || sy_shift.abs() > f32::EPSILON {
                    meta.centre.0 += sx_shift;
                    meta.centre.1 += sy_shift;
                }
            }
            let new_sx = binding_scale_x * binding_scale;
            let new_sy = binding_scale_y * binding_scale;
            if (new_sx - meta.last_scale.0).abs() > f32::EPSILON
                || (new_sy - meta.last_scale.1).abs() > f32::EPSILON
            {
                if meta.last_scale.0.abs() < f32::EPSILON || meta.last_scale.1.abs() < f32::EPSILON
                {
                    return false;
                }
                let ratio_x = new_sx / meta.last_scale.0;
                let ratio_y = new_sy / meta.last_scale.1;
                let cx_phys = meta.centre.0 * scale;
                let cy_phys = meta.centre.1 * scale;
                // Scale corner radii by the geometric mean of the axis ratios.
                let ratio_radius = (ratio_x * ratio_y).sqrt();
                if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                    for p in prims.iter_mut() {
                        p.bounds[0] = cx_phys + (p.bounds[0] - cx_phys) * ratio_x;
                        p.bounds[1] = cy_phys + (p.bounds[1] - cy_phys) * ratio_y;
                        p.bounds[2] *= ratio_x;
                        p.bounds[3] *= ratio_y;
                        p.corner_radius[0] *= ratio_radius;
                        p.corner_radius[1] *= ratio_radius;
                        p.corner_radius[2] *= ratio_radius;
                        p.corner_radius[3] *= ratio_radius;
                    }
                }
                meta.last_scale = (new_sx, new_sy);
                any_binding_active = true;
                binding_moved = true;
            }

            // Apply the rotation delta around the binding pivot, preserving the existing
            // affine and ancestor transforms.
            let new_rotation_rad = bindings_for_node
                .get_rotation()
                .map(|deg| deg.to_radians())
                .unwrap_or(0.0);
            let drot = new_rotation_rad - meta.last_rotation_rad;
            if drot.abs() > f32::EPSILON {
                let (sin_d, cos_d) = drot.sin_cos();
                let cx_phys = meta.centre.0 * scale;
                let cy_phys = meta.centre.1 * scale;
                if let Some(prims) = batch.primitives.get_mut(meta.primitive_range.clone()) {
                    for p in prims.iter_mut() {
                        // Rotate the primitive's centre around the
                        // binding centre by `δ`. `p.bounds` is the
                        // axis-aligned AABB at the (DPI-scaled)
                        // centre; the GPU vertex shader rebuilds the
                        // post-rotation AABB at draw time off the
                        // rotation sin/cos fields, so we only need
                        // the centre to track the pivot.
                        let cx = p.bounds[0] + p.bounds[2] * 0.5;
                        let cy = p.bounds[1] + p.bounds[3] * 0.5;
                        let dx = cx - cx_phys;
                        let dy = cy - cy_phys;
                        let new_dx = dx * cos_d - dy * sin_d;
                        let new_dy = dx * sin_d + dy * cos_d;
                        let new_cx = cx_phys + new_dx;
                        let new_cy = cy_phys + new_dy;
                        p.bounds[0] = new_cx - p.bounds[2] * 0.5;
                        p.bounds[1] = new_cy - p.bounds[3] * 0.5;

                        // Compose δ onto the stored rotation
                        // `[sin α, cos α, sin_ry, cos_ry]`. Working
                        // in (sin, cos) avoids any atan2 round-trip.
                        // `α + δ` keeps the Y-rotation slots intact.
                        let sin_a = p.rotation[0];
                        let cos_a = p.rotation[1];
                        p.rotation[0] = sin_a * cos_d + cos_a * sin_d;
                        p.rotation[1] = cos_a * cos_d - sin_a * sin_d;

                        // Post-multiply the existing affine by R(delta), preserving the
                        // parent transform and updating the binding rotation.
                        let a = p.local_affine[0];
                        let b = p.local_affine[1];
                        let c = p.local_affine[2];
                        let d = p.local_affine[3];
                        p.local_affine[0] = a * cos_d + c * sin_d;
                        p.local_affine[1] = b * cos_d + d * sin_d;
                        p.local_affine[2] = -a * sin_d + c * cos_d;
                        p.local_affine[3] = -b * sin_d + d * cos_d;
                    }
                }
                meta.last_rotation_rad = new_rotation_rad;
                any_binding_active = true;
                binding_moved = true;
            }

            // Update the cached screen AABB even when we skip the
            // damage-rect push below — `meta.last_screen_aabb` is
            // also consumed by hit-test / scroll bookkeeping, not
            // just damage rects.
            if binding_moved {
                let aabb_after = bounds_union_of_range(&batch.primitives, &meta.primitive_range);
                meta.last_screen_aabb = aabb_after;
            }
            // Dynamic bindings leave static-cache pixels unchanged, so no static damage
            // repaint is needed.
            let _ = aabb_before;
        }
        self.last_binding_damage_rects.clear();
        // Replace the redraw signal each frame so settled bindings do not leave it latched.
        tree.set_visible_anim_active(any_binding_active);

        // Text and SVG caches are not patched with dynamic geometry. Repaint while their
        // bindings are active to keep the streams aligned.
        let has_motion_overlay_glyphs = self
            .cached_motion_subtree_text_prims
            .as_ref()
            .is_some_and(|v| !v.is_empty())
            || self
                .cached_motion_subtree_svgs
                .as_ref()
                .is_some_and(|v| !v.is_empty());
        !(any_binding_active && has_motion_overlay_glyphs)
    }
}
