//! The SDF shadow shader: Shadow, InnerShadow, CircleShadow and CircleInnerShadow primitives.

russl::russl! {
    use super::abi::{Uniforms, Primitive, UNIFORMS, PRIMITIVES, GLYPH_ATLAS, GLYPH_SAMPLER, COLOR_GLYPH_ATLAS};
    use super::clip::{calculate_clip_alpha, calculate_polygon_clip_alpha, abi_shape_clip_alpha, AUX_DATA};
    use super::common::{sd_circle, sd_shaped_rect, erf, shadow_circle, shadow_spread_radius};

    // ============================================================================
    // Blinc SDF Shadow Shader — Shadow, InnerShadow, CircleShadow, CircleInnerShadow
    // ============================================================================
    // Handles prim_type 3 (Shadow), 4 (InnerShadow), 5 (CircleShadow),
    // 6 (CircleInnerShadow). These are shadow-only primitives that render
    // shadows as separate elements.

    struct VertexOutput {
        #[builtin(position)] position: Vec4,
        #[location(0)] uv: Vec2,
        #[location(1)] #[flat] instance_index: u32,
    }

    // Enum values are inlined as literals at every use site rather than declared as `const`, so
    // naga's WGSL->MSL backend emits no orphaned `constant uint NAME = <value>;` declarations
    // that trip the Metal shader compiler's `-Wunused-const-variable` pass.
    //
    // Primitive types (field: type_info.x, variable: prim_type)
    //   3u32 = SHADOW               4u32 = INNER_SHADOW
    //   5u32 = CIRCLE_SHADOW        6u32 = CIRCLE_INNER_SHADOW
    //
    // Clip types (field: type_info.z, variable: clip_type)
    //   0u32 = NONE                 1u32 = RECT                2u32 = CIRCLE
    //   3u32 = ELLIPSE              4u32 = POLYGON

    // ============================================================================
    // Vertex Shader
    // ============================================================================

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
    ) -> VertexOutput {
        let mut out: VertexOutput;

        let prim = PRIMITIVES[instance_index];

        // Expand the quad over the blur, the offset and the spread; the extra
        // pixel is for the antialiased edge of a shadow with no blur.
        let blur_expand = prim.shadow.z * 3.0 + abs(prim.shadow.x) + abs(prim.shadow.y) + max(prim.shadow.w, 0.0) + 1.0;
        let bounds = Vec4::new(
            prim.bounds.x - blur_expand,
            prim.bounds.y - blur_expand,
            prim.bounds.z + blur_expand * 2.0,
            prim.bounds.w + blur_expand * 2.0
        );

        // Generate quad vertices (two triangles split along / diagonal)
        // 0--1
        // |\ |
        // | \|
        // 3--2
        // Triangle 1: 0 -> 1 -> 3 (TL -> TR -> BL) - upper-left triangle
        // Triangle 2: 1 -> 2 -> 3 (TR -> BR -> BL) - lower-right triangle
        // Shared edge: 1-3 (top-right to bottom-left = / diagonal)
        //
        // Indexing a literal array with a dynamic index miscompiles on a PowerVR Vulkan driver
        // (the second triangle collapses); an explicit `match` makes naga emit `OpSwitch`.
        let mut uv: Vec2;
        match vertex_index {
            0u32 => { uv = Vec2::new(0.0, 0.0); } // 0 - top-left
            1u32 => { uv = Vec2::new(1.0, 0.0); } // 1 - top-right
            2u32 => { uv = Vec2::new(0.0, 1.0); } // 3 - bottom-left
            3u32 => { uv = Vec2::new(1.0, 0.0); } // 1 - top-right
            4u32 => { uv = Vec2::new(1.0, 1.0); } // 2 - bottom-right
            _ => { uv = Vec2::new(0.0, 1.0); } // 3 - bottom-left
        }
        let pos = Vec2::new(
            bounds.x + uv.x * bounds.z,
            bounds.y + uv.y * bounds.w
        );

        // Convert to clip space (-1 to 1)
        let clip_pos = Vec2::new(
            (pos.x / UNIFORMS.viewport_size.x) * 2.0 - 1.0,
            1.0 - (pos.y / UNIFORMS.viewport_size.y) * 2.0
        );

        out.position = Vec4::new(clip_pos, 0.0, 1.0);
        out.uv = pos; // Pass world position for SDF calculation
        out.instance_index = instance_index;

        return out;
    }

    // ============================================================================
    // Fragment Shader
    // ============================================================================

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        let prim = PRIMITIVES[input.instance_index];
        let p = input.uv;

        let prim_type = prim.type_info.x;
        let clip_type = prim.type_info.z;

        // Early type filter - discard primitives handled by other split pipelines
        if prim_type < 3u32 || prim_type > 6u32 { discard!(); }

        // Early clip test - discard if completely outside clip region (screen space).
        // The polygon shape test waits until `sp` is known.
        let mut clip_alpha = calculate_clip_alpha(p, prim.clip_bounds, prim.clip_radius, prim.clip_corner_shape, clip_type, prim.clip_fade, prim.clip_fade_bounds);
        clip_alpha = clip_alpha * abi_shape_clip_alpha(p, prim.clip_shape.x);
        if clip_alpha < 0.001 {
            discard!();
        }

        let origin = prim.bounds.xy;
        let size = prim.bounds.zw;
        let center = origin + size * 0.5;

        // Shadow primitives use screen-space position directly (no perspective unprojection)
        let sp = p;

        // CLIP_POLYGON shape test in element-local coords (vertices stored in
        // AUX_DATA as user-supplied 0..size).
        if clip_type == 4u32 {
            let vertex_count = ((prim.clip_radius.z) as u32);
            let aux_offset = ((prim.clip_radius.w) as u32);
            let local_p = sp - prim.bounds.xy;
            let shape_alpha = calculate_polygon_clip_alpha(local_p, vertex_count, aux_offset, prim.clip_radius.x > 0.5);
            clip_alpha = clip_alpha * shape_alpha;
            if clip_alpha < 0.001 {
                discard!();
            }
        }

        let mut result = Vec4::splat(0.0);

        // Calculate shadow first (rendered behind) - but NOT for inner shadow primitives
        // InnerShadow primitives handle their own shadow rendering differently
        if prim_type != 4u32 {
            let shadow_offset = prim.shadow.xy;
            let blur = prim.shadow.z;
            let spread = prim.shadow.w;

            let shadow_origin = origin + shadow_offset - Vec2::splat(spread);
            let shadow_size = size + Vec2::splat(spread * 2.0);

            let shadow_radii = shadow_spread_radius(prim.corner_radius, spread);

            let shadow_sdf_dist = sd_shaped_rect(sp, shadow_origin, shadow_size, shadow_radii, prim.corner_shape);
            let mut shadow_alpha: f32;
            if blur < 0.001 {
                // No blur: the edge, antialiased over a pixel.
                shadow_alpha = clamp(0.5 - shadow_sdf_dist, 0.0, 1.0);
            } else {
                let sigma_d = 0.5 * sqrt(2.0) * blur;
                shadow_alpha = 0.5 * (1.0 + erf(-shadow_sdf_dist / sigma_d));
            }

            // Premultiply and blend
            result = prim.shadow_color * shadow_alpha;
        }

        // Main SDF switch - shadow-only primitives
        match prim_type {
            3u32 /* PRIM_SHADOW */ => {
                // Shadow-only primitive - mask out the shape interior
                // Shadow should be visible starting from the shape boundary (d >= 0)
                // Use constant AA width to avoid discontinuities at triangle seams on Vulkan
                let shape_d = sd_shaped_rect(sp, origin, size, prim.corner_radius, prim.corner_shape);
                let aa_width = 0.75;
                let shape_mask = smoothstep(-aa_width, aa_width, shape_d); // 0 inside, 1 outside, AA at edge
                result.a *= shape_mask;
                result.a *= clip_alpha;
                return result;
            }
            4u32 /* PRIM_INNER_SHADOW */ => {
                // Inner shadow - renders INSIDE the shape only
                let shape_d = sd_shaped_rect(sp, origin, size, prim.corner_radius, prim.corner_shape);

                // Hard clip at shape boundary - only render where d < 0 (inside)
                if shape_d > 0.0 {
                    discard!();
                }

                let blur = max(prim.shadow.z, 0.1);
                let spread = prim.shadow.w;
                let offset = prim.shadow.xy;

                // Inner shadow effect: shadow darkens from outer edge inward
                // Use distance from edge (negative shape_d = distance inside)
                let edge_dist = -shape_d;  // Positive value = how far inside the shape

                // Create shadow falloff from edge toward center
                // At edge (edge_dist ~ 0): full shadow
                // Further inside (edge_dist > blur + spread): no shadow
                let shadow_range = blur + spread;
                let mut shadow_alpha: f32;
                if prim.shadow.z < 0.001 {
                    // No blur: the edge of the spread, antialiased over a pixel.
                    shadow_alpha = clamp(0.5 + spread - edge_dist, 0.0, 1.0);
                } else {
                    shadow_alpha = 1.0 - smoothstep(0.0, shadow_range, edge_dist - spread);
                }

                // Apply offset by shifting the shadow calculation
                // Offset shifts which "edge" the shadow appears from
                let offset_effect = dot(normalize(offset + Vec2::splat(0.001)), sp - center);
                let offset_bias = clamp(offset_effect / (length(size) * 0.5), -1.0, 1.0) * length(offset);
                let biased_alpha = shadow_alpha * (1.0 + offset_bias * 0.5);

                let mut inner_result = prim.shadow_color;
                inner_result.a *= clamp(biased_alpha, 0.0, 1.0) * clip_alpha;
                return inner_result;
            }
            5u32 /* PRIM_CIRCLE_SHADOW */ => {
                // Circle shadow - radially symmetric Gaussian blur
                let radius = min(size.x, size.y) * 0.5;
                let blur = prim.shadow.z;
                let spread = prim.shadow.w;
                let shadow_offset = prim.shadow.xy;

                let shadow_center = center + shadow_offset;
                let shadow_radius = radius + spread;

                let shadow_alpha = shadow_circle(sp, shadow_center, shadow_radius, blur);

                // Mask out the circle area so shadow doesn't render under it
                // Use constant AA width to avoid discontinuities at triangle seams on Vulkan
                let circle_d = sd_circle(sp, center, radius);
                let aa_width = 0.75;
                let shape_mask = smoothstep(-aa_width, aa_width, circle_d); // 0 inside, 1 outside, AA at edge

                let mut circle_result = prim.shadow_color * shadow_alpha;
                circle_result.a *= shape_mask * clip_alpha;
                return circle_result;
            }
            6u32 /* PRIM_CIRCLE_INNER_SHADOW */ => {
                // Circle inner shadow - renders INSIDE the circle only
                let radius = min(size.x, size.y) * 0.5;
                let circle_d = sd_circle(sp, center, radius);

                // Hard clip at circle boundary
                if circle_d > 0.0 {
                    discard!();
                }

                let blur = max(prim.shadow.z, 0.1);
                let spread = prim.shadow.w;
                let offset = prim.shadow.xy;

                // Inner shadow effect: shadow darkens from outer edge inward
                let edge_dist = -circle_d;  // How far inside the circle

                // Create shadow falloff from edge toward center
                let shadow_range = blur + spread;
                let mut shadow_alpha: f32;
                if prim.shadow.z < 0.001 {
                    // No blur: the edge of the spread, antialiased over a pixel.
                    shadow_alpha = clamp(0.5 + spread - edge_dist, 0.0, 1.0);
                } else {
                    shadow_alpha = 1.0 - smoothstep(0.0, shadow_range, edge_dist - spread);
                }

                // Apply offset
                let offset_effect = dot(normalize(offset + Vec2::splat(0.001)), sp - center);
                let offset_bias = clamp(offset_effect / radius, -1.0, 1.0) * length(offset);
                let biased_alpha = shadow_alpha * (1.0 + offset_bias * 0.5);

                let mut inner_result = prim.shadow_color;
                inner_result.a *= clamp(biased_alpha, 0.0, 1.0) * clip_alpha;
                return inner_result;
            }
            _ => {
                discard!();
            }
        }

        return Vec4::splat(0.0);
    }
}
