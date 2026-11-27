//! The SDF shadow shader's stages, without their entry points: Shadow, InnerShadow, CircleShadow
//! and CircleInnerShadow primitives. Each tier's entry points call these with the fields they read.

russl::russl! {
    use super::access::{UNIFORMS, prim, path_clip_alpha};
    use super::clip::{calculate_clip_alpha, calculate_polygon_clip_alpha, abi_shape_clip_alpha};
    use super::common::{sd_circle, sd_shaped_rect, erf, shadow_circle, shadow_spread_radius};

    // ============================================================================
    // Blinc SDF Shadow Shader — Shadow, InnerShadow, CircleShadow, CircleInnerShadow
    // ============================================================================
    // Handles prim_type 3 (Shadow), 4 (InnerShadow), 5 (CircleShadow),
    // 6 (CircleInnerShadow). These are shadow-only primitives that render
    // shadows as separate elements.

    pub struct VertexOutput {
        #[builtin(position)] pub position: Vec4,
        #[location(0)] pub uv: Vec2,
        #[location(1)] #[flat] pub instance_index: u32,
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

    // The quad of one primitive: the instance's bounds, shadow and local affine, which the tier's
    // entry point reads from wherever it keeps them.
    pub fn shadow_vertex(
        vertex_index: u32,
        instance_index: u32,
        instance_bounds: Vec4,
        instance_shadow: Vec4,
        instance_local_affine: Vec4,
    ) -> VertexOutput {
        let mut out: VertexOutput;

        // Expand the quad over the blur, the offset and the spread; the extra
        // pixel is for the antialiased edge of a shadow with no blur.
        let blur_expand = instance_shadow.z * 3.0 + abs(instance_shadow.x) + abs(instance_shadow.y) + max(instance_shadow.w, 0.0) + 1.0;

        // A turned or skewed element covers the box of its corners under the local affine. The
        // offset turns with it, and its length is within `abs(x) + abs(y)`.
        let la = instance_local_affine;
        let has_local_affine = abs(la.x - 1.0) > 0.0001 || abs(la.y) > 0.0001
                            || abs(la.z) > 0.0001 || abs(la.w - 1.0) > 0.0001;
        let mut bounds: Vec4;
        if has_local_affine {
            let center = instance_bounds.xy + instance_bounds.zw * 0.5;
            let hw = instance_bounds.z * 0.5;
            let hh = instance_bounds.w * 0.5;
            let c0x = la.x * hw + la.z * hh;
            let c0y = la.y * hw + la.w * hh;
            let c1x = -la.x * hw + la.z * hh;
            let c1y = -la.y * hw + la.w * hh;
            let aabb_hw = max(abs(c0x), abs(c1x)) + blur_expand;
            let aabb_hh = max(abs(c0y), abs(c1y)) + blur_expand;
            bounds = Vec4::new(center.x - aabb_hw, center.y - aabb_hh, aabb_hw * 2.0, aabb_hh * 2.0);
        } else {
            bounds = Vec4::new(
                instance_bounds.x - blur_expand,
                instance_bounds.y - blur_expand,
                instance_bounds.z + blur_expand * 2.0,
                instance_bounds.w + blur_expand * 2.0
            );
        }

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
    // Inset shadow coverage
    // ============================================================================

    // The share of a Gaussian of deviation `sigma` centred on `at` that lies between `from` and
    // `to`.
    pub fn gaussian_span(at: f32, from: f32, to: f32, sigma: f32) -> f32 {
        let k = sqrt(0.5) / sigma;
        return 0.5 * (erf((to - at) * k) - erf((from - at) * k));
    }

    // How far a corner of radius `r` and shape `n` pulls a row in from the side, for a row
    // `into` pixels past where the corner begins, towards the end of the rect. The shapes are
    // those of `sd_shaped_rect`.
    pub fn corner_row_inset(r: f32, n: f32, into: f32) -> f32 {
        if into <= 0.0 || r <= 0.0 || n >= 100.0 {
            return 0.0;
        }
        if n <= -100.0 {
            return r;
        }
        let t = clamp(into / r, 0.000001, 1.0);
        let p = pow(2.0, min(abs(n), 5.0));
        if n < 0.0 {
            // A scoop: an arc about the corner's tip.
            return r * pow(max(1.0 - pow(max(1.0 - t, 0.000001), p), 0.000001), 1.0 / p);
        }
        return r * (1.0 - pow(max(1.0 - pow(t, p), 0.000001), 1.0 / p));
    }

    // How much of a Gaussian of deviation `sigma` centred on `p` a shaped rect covers, which
    // is the rect blurred, at `p`. Exact along each row; the rows are summed in slices, each
    // weighted by its share of the Gaussian.
    pub fn blurred_rect_coverage(p: Vec2, origin: Vec2, size: Vec2, radius: Vec4, shape: Vec4, sigma: f32) -> f32 {
        let end = origin + size;
        let radii = min(radius, Vec4::splat(min(size.x, size.y) * 0.5));
        if radii.x + radii.y + radii.z + radii.w <= 0.0 {
            return gaussian_span(p.x, origin.x, end.x, sigma) * gaussian_span(p.y, origin.y, end.y, sigma);
        }
        // Rows past three and a half deviations add less than one part in two thousand.
        let top = max(origin.y, p.y - 3.5 * sigma);
        let bottom = min(end.y, p.y + 3.5 * sigma);
        if bottom <= top {
            return 0.0;
        }
        let step = (bottom - top) / 12.0;
        let mut covered = 0.0;
        for i in 0u32..12u32 {
            let from = top + step * (i as f32);
            let row = from + step * 0.5;
            let mut left = 0.0;
            if row < origin.y + radii.x {
                left = corner_row_inset(radii.x, shape.x, origin.y + radii.x - row);
            } else if row > end.y - radii.w {
                left = corner_row_inset(radii.w, shape.w, row - (end.y - radii.w));
            }
            let mut right = 0.0;
            if row < origin.y + radii.y {
                right = corner_row_inset(radii.y, shape.y, origin.y + radii.y - row);
            } else if row > end.y - radii.z {
                right = corner_row_inset(radii.z, shape.z, row - (end.y - radii.z));
            }
            covered += gaussian_span(p.y, from, from + step, sigma)
                * gaussian_span(p.x, origin.x + left, end.x - right, sigma);
        }
        return covered;
    }

    // An inset shadow's strength at `p`, as CSS paints it: one less the blurred coverage of a
    // hole, which is the padding box moved by the offset and shrunk by the spread. `shadow` is
    // the offset, the blur radius and the spread; the Gaussian's deviation is half the blur.
    pub fn inset_shadow_alpha(p: Vec2, origin: Vec2, size: Vec2, radius: Vec4, shape: Vec4, shadow: Vec4) -> f32 {
        let spread = shadow.w;
        let hole_origin = origin + shadow.xy + Vec2::splat(spread);
        let hole_size = size - Vec2::splat(spread * 2.0);
        if hole_size.x <= 0.0 || hole_size.y <= 0.0 {
            return 1.0;
        }
        let hole_radii = shadow_spread_radius(radius, -spread);
        let mut covered: f32;
        if shadow.z < 0.001 {
            // No blur: the edge of the hole, antialiased over a pixel.
            covered = clamp(0.5 - sd_shaped_rect(p, hole_origin, hole_size, hole_radii, shape), 0.0, 1.0);
        } else {
            covered = blurred_rect_coverage(p, hole_origin, hole_size, hole_radii, shape, shadow.z * 0.5);
        }
        return clamp(1.0 - covered, 0.0, 1.0);
    }

    // ============================================================================
    // Fragment Shader
    // ============================================================================

    pub fn shadow_fragment(input: VertexOutput) -> Vec4 {
        let prim = prim(input.instance_index);
        let p = input.uv;

        let prim_type = prim.type_info.x;
        let clip_type = prim.type_info.z;

        // Early type filter - discard primitives handled by other split pipelines
        if prim_type < 3u32 || prim_type > 6u32 { discard!(); }

        // Early clip test - discard if completely outside clip region (screen space).
        // The polygon shape test waits until `sp` is known.
        let mut clip_alpha = calculate_clip_alpha(p, prim.clip_bounds, prim.clip_radius, prim.clip_corner_shape, clip_type, prim.clip_fade, prim.clip_fade_bounds);
        clip_alpha = clip_alpha * path_clip_alpha(prim, p);
        if clip_alpha < 0.001 {
            discard!();
        }

        let origin = prim.bounds.xy;
        let size = prim.bounds.zw;
        let center = origin + size * 0.5;

        // The point in the element's own frame: the screen point taken back through the local
        // affine about the centre. Shadow primitives take no perspective.
        let mut sp = p;
        let la = prim.local_affine;
        let is_identity = abs(la.x - 1.0) < 0.0001 && abs(la.y) < 0.0001
                       && abs(la.z) < 0.0001 && abs(la.w - 1.0) < 0.0001;
        if !is_identity {
            let rel = p - center;
            let det = la.x * la.w - la.y * la.z;
            let inv_det = if det >= 0.0 { 1.0 } else { -1.0 } / max(abs(det), 0.0001);
            let inv_a = la.w * inv_det;
            let inv_b = -la.y * inv_det;
            let inv_c = -la.z * inv_det;
            let inv_d = la.x * inv_det;
            sp = Vec2::new(inv_a * rel.x + inv_c * rel.y, inv_b * rel.x + inv_d * rel.y) + center;
        }

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

            // The pipeline blends straight alpha: only the alpha carries the falloff.
            result = Vec4::new(prim.shadow_color.rgb, prim.shadow_color.a * shadow_alpha);
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
                // Inside the shape only. The bounds and radii are the padding box's.
                if sd_shaped_rect(sp, origin, size, prim.corner_radius, prim.corner_shape) > 0.0 {
                    discard!();
                }
                let strength = inset_shadow_alpha(sp, origin, size, prim.corner_radius, prim.corner_shape, prim.shadow);
                return Vec4::new(prim.shadow_color.rgb, prim.shadow_color.a * strength * clip_alpha);
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

                return Vec4::new(
                    prim.shadow_color.rgb,
                    prim.shadow_color.a * shadow_alpha * shape_mask * clip_alpha
                );
            }
            6u32 /* PRIM_CIRCLE_INNER_SHADOW */ => {
                // Inside the circle only.
                let radius = min(size.x, size.y) * 0.5;
                if sd_circle(sp, center, radius) > 0.0 {
                    discard!();
                }
                // A circle is a square with fully round corners.
                let square = center - Vec2::splat(radius);
                let strength = inset_shadow_alpha(sp, square, Vec2::splat(radius * 2.0), Vec4::splat(radius), Vec4::splat(1.0), prim.shadow);
                return Vec4::new(prim.shadow_color.rgb, prim.shadow_color.a * strength * clip_alpha);
            }
            _ => {
                // The return stays inside the arm: FXC takes a case that ends in `discard` for a
                // fall-through.
                discard!();
                return Vec4::splat(0.0);
            }
        }

        return Vec4::splat(0.0);
    }
}
