//! The SDF notch shader: rounded rects with concave corners and edge modifiers, and the ABI's
//! notch shapes with their shadows. Tier 0 reads the instance fields the vertex stage needs from
//! the storage buffer; the other tiers have no storage buffer in the vertex stage and read them
//! from the vertex buffer.

russl::russl! {
    use super::access::{UNIFORMS, prim};
    use super::clip::{calculate_clip_alpha, calculate_polygon_clip_alpha, gradient_table_color, abi_shape_clip_alpha};
    use super::common::{erf, apply_css_filter, quarter_ellipse_sdf, sd_shaped_rect};
    use super::notch::{sd_notch, sd_abi_notch};

    #[pipeline] const TIER: u32 = 0;

    // ============================================================================
    // Blinc SDF Notch Primitive Shader
    //
    // Handles prim_type 8 (Notch) - concave corners with edge modifiers.
    // ============================================================================

    struct VertexOutput {
        #[builtin(position)] position: Vec4,
        #[location(0)] uv: Vec2,
        #[location(1)] #[flat] instance_index: u32,
    }

    // Enum values are inlined as literals at every use site rather than declared as `const`, so
    // naga's WGSL->MSL backend emits no orphaned `constant uint NAME = <value>;` declarations
    // that trip the Metal shader compiler's `-Wunused-const-variable` pass.
    //
    // Fill types (field: type_info.y, variable: fill_type)
    //   0u32 = SOLID   1u32 = LINEAR_GRADIENT   2u32 = RADIAL_GRADIENT   3u32 = CONIC_GRADIENT
    //
    // Clip types (field: type_info.z, variable: clip_type)
    //   0u32 = NONE   1u32 = RECT   2u32 = CIRCLE   3u32 = ELLIPSE   4u32 = POLYGON

    // ============================================================================
    // Vertex Shader
    // ============================================================================

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
        #if TIER >= 1
            #[location(0)] instance_bounds: Vec4,
            #[location(1)] instance_shadow: Vec4,
            #[location(3)] instance_perspective: Vec4,
            #[location(4)] instance_sdf_3d: Vec4,
            #[location(5)] instance_local_affine: Vec4,
            #[location(6)] instance_corner_radius: Vec4,
            #[location(7)] instance_light: Vec4,
            #[location(8)] instance_notch_info: Vec4,
        #endif
    ) -> VertexOutput {
        let mut out: VertexOutput;

        #if TIER == 0
            let p = prim(instance_index);
            let instance_bounds = p.bounds;
            let instance_shadow = p.shadow;
            let instance_perspective = p.perspective;
            let instance_sdf_3d = p.sdf_3d;
            let instance_local_affine = p.local_affine;
            let instance_corner_radius = p.corner_radius;
            let instance_light = p.light;
            let instance_notch_info = p.notch_info;
        #endif

        // Expand bounds for shadow blur
        let mut blur_expand = instance_shadow.z * 3.0 + abs(instance_shadow.x) + abs(instance_shadow.y);
        if instance_notch_info.x > 0.0 && instance_notch_info.y > 0.5 {
            let la = instance_local_affine;
            blur_expand = (instance_shadow.z * 3.0 + abs(instance_shadow.x) + abs(instance_shadow.y) + abs(instance_shadow.w))
                * max(abs(la.x) + abs(la.z), abs(la.y) + abs(la.w));
        }

        let la = instance_local_affine; // [a, b, c, d] of normalized 2x2 affine
        // Check if local_affine is non-identity (rotation, skew, or non-uniform scale)
        let has_local_affine = abs(la.x - 1.0) > 0.0001 || abs(la.y) > 0.0001
                            || abs(la.z) > 0.0001 || abs(la.w - 1.0) > 0.0001;

        let mut bounds: Vec4;
        if has_local_affine {
            // General 2D affine (rotation, skew, non-uniform scale):
            // Transform the 4 corners of the local rect by the local_affine to find AABB
            let center = instance_bounds.xy + instance_bounds.zw * 0.5;
            let hw = instance_bounds.z * 0.5;
            let hh = instance_bounds.w * 0.5;
            // Transform corners: la * (±hw, ±hh)
            // new_x = la.x * cx + la.z * cy, new_y = la.y * cx + la.w * cy
            let c0x = la.x * hw + la.z * hh;
            let c0y = la.y * hw + la.w * hh;
            let c1x = -la.x * hw + la.z * hh;
            let c1y = -la.y * hw + la.w * hh;
            let aabb_hw = max(abs(c0x), abs(c1x)) + blur_expand;
            let aabb_hh = max(abs(c0y), abs(c1y)) + blur_expand;
            bounds = Vec4::new(center.x - aabb_hw, center.y - aabb_hh, aabb_hw * 2.0, aabb_hh * 2.0);
        } else {
            // Notches can reach outside `prim.bounds` via concave corners,
            // top/bottom bulge, or top/bottom peak modifiers. The SDF in fs_main
            // returns negative for those exterior pixels, but the fragment shader
            // only runs on pixels covered by this vertex quad — so if we don't
            // expand the quad, the protrusion is invisible (every pixel outside
            // `prim.bounds` is never rasterized in the first place). Compute the
            // per-edge outward expansion from the notch parameters and fold it
            // into the quad size here. Scoop/cut modifiers go INWARD so they
            // don't need any expansion.
            let mut notch_left = 0.0;
            let mut notch_top = 0.0;
            let mut notch_right = 0.0;
            let mut notch_bottom = 0.0;

            let ct = instance_light; // (TL, TR, BR, BL) corner type flags
            let cr = instance_corner_radius;
            // Concave corners extend the shape outward by `radius` along the
            // two edges they touch.
            if ct.x > 0.5 { // TL concave
                notch_left = max(notch_left, cr.x);
                notch_top = max(notch_top, cr.x);
            }
            if ct.y > 0.5 { // TR concave
                notch_right = max(notch_right, cr.y);
                notch_top = max(notch_top, cr.y);
            }
            if ct.z > 0.5 { // BR concave
                notch_right = max(notch_right, cr.z);
                notch_bottom = max(notch_bottom, cr.z);
            }
            if ct.w > 0.5 { // BL concave
                notch_left = max(notch_left, cr.w);
                notch_bottom = max(notch_bottom, cr.w);
            }
            // Top / bottom modifiers: bulge (2) and peak (4) both extend the
            // shape by `height` past the base edge.
            let top_type = instance_perspective.x;
            let top_height = instance_perspective.z;
            if (top_type > 1.5 && top_type < 2.5) || (top_type > 3.5 && top_type < 4.5) {
                notch_top = max(notch_top, top_height);
            }
            let bot_type = instance_sdf_3d.x;
            let bot_height = instance_sdf_3d.z;
            if (bot_type > 1.5 && bot_type < 2.5) || (bot_type > 3.5 && bot_type < 4.5) {
                notch_bottom = max(notch_bottom, bot_height);
            }

            bounds = Vec4::new(
                instance_bounds.x - blur_expand - notch_left,
                instance_bounds.y - blur_expand - notch_top,
                instance_bounds.z + blur_expand * 2.0 + notch_left + notch_right,
                instance_bounds.w + blur_expand * 2.0 + notch_top + notch_bottom
            );
        }

        // Generate quad vertices (two triangles split along / diagonal)
        // 0--1
        // |\ |
        // | \|
        // 3--2
        // Triangle 1: 0 → 1 → 3 (TL → TR → BL) - upper-left triangle
        // Triangle 2: 1 → 2 → 3 (TR → BR → BL) - lower-right triangle
        // Shared edge: 1-3 (top-right to bottom-left = / diagonal)
        //
        // PowerVR Vulkan codegen bug workaround: dynamic indexing into a
        // `let array<...>(literal)` produces an `OpConstantComposite` +
        // `OpAccessChain` pattern that the Pixel 10 Pro / Tensor G5
        // PowerVR driver compiles incorrectly — vertex_index 0..2 work
        // but 3..5 silently produce degenerate output, collapsing the
        // second triangle to a point and leaving every primitive a
        // half-quad. Replacing the array-literal indexing with an explicit
        // `switch` forces naga to emit `OpSwitch`, which the driver
        // handles correctly. Confirmed on Android 16 / driver 25.1@6794074.
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
        let prim = prim(input.instance_index);
        let p = input.uv;

        // Screen-space derivative magnitude, computed up-front *outside*
        // any control flow that depends on per-instance data. WGSL strictly
        // requires `fwidth` / `dpdx` / `dpdy` to be called from uniform
        // control flow; computing it here on the continuously-interpolated
        // `input.uv` (which is uniform across every 2x2 pixel quad regardless
        // of which primitive a quad belongs to) satisfies the rule.

        let prim_type = prim.type_info.x;
        let fill_type = prim.type_info.y;
        let clip_type = prim.type_info.z;

        // Early type filter — discard primitives handled by other split pipelines
        if prim_type != 8u32 { discard!(); }

        // Early clip test - discard if completely outside clip region (screen space).
        // Polygon shape test deferred until sp is known — see sdf_core.wgsl.
        let mut clip_alpha = calculate_clip_alpha(p, prim.clip_bounds, prim.clip_radius, prim.clip_corner_shape, clip_type, prim.clip_fade, prim.clip_fade_bounds);
        clip_alpha = clip_alpha * abi_shape_clip_alpha(p, prim.clip_shape.x);
        if clip_alpha < 0.001 {
            discard!();
        }

        let origin = prim.bounds.xy;
        let size = prim.bounds.zw;
        let center = origin + size * 0.5;

        // 2D affine (rotation, skew, non-uniform scale) via inverse local_affine
        let mut sp = p;
        let la = prim.local_affine;
        let is_identity = abs(la.x - 1.0) < 0.0001 && abs(la.y) < 0.0001
                       && abs(la.z) < 0.0001 && abs(la.w - 1.0) < 0.0001;
        if !is_identity {
            let rel = p - center;
            // Compute inverse of 2x2 [a,b; c,d]: inv = [d,-b; -c,a] / det
            let det = la.x * la.w - la.y * la.z;
            let inv_det = if det >= 0.0 { 1.0 } else { -1.0 } / max(abs(det), 0.0001);
            let inv_a = la.w * inv_det;
            let inv_b = -la.y * inv_det;
            let inv_c = -la.z * inv_det;
            let inv_d = la.x * inv_det;
            sp = Vec2::new(inv_a * rel.x + inv_c * rel.y, inv_b * rel.x + inv_d * rel.y) + center;
        }

        // CLIP_POLYGON shape test in element-local coords. See sdf_core.wgsl.
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

        // A clip in the primitive's own frame. `sp` has been taken back through any
        // transform, so a turned parent clips its children to its turned, rounded box.
        if prim.local_clip_bounds.z > 0.0 {
            let local_d = sd_shaped_rect(
                sp - origin,
                prim.local_clip_bounds.xy,
                prim.local_clip_bounds.zw,
                prim.local_clip_radius,
                prim.clip_corner_shape
            );
            clip_alpha = clip_alpha * (1.0 - smoothstep(-0.75, 0.75, local_d));
            if clip_alpha < 0.001 {
                discard!();
            }
        }

        let abi_unit = max(prim.notch_info.x, 0.000001);
        let abi_point = (sp - origin) / abi_unit;
        let abi_size = size / abi_unit;
        let abi_radii = prim.corner_radius / abi_unit;
        let abi_top = Vec4::new(prim.perspective.x, prim.perspective.yzw / abi_unit);
        let abi_bottom = Vec4::new(prim.sdf_3d.x, prim.sdf_3d.yzw / abi_unit);
        if prim.notch_info.x > 0.0 && prim.notch_info.y > 0.5 {
            let d = sd_abi_notch(abi_point, abi_size, abi_radii, prim.light, abi_top, abi_bottom);
            let params = prim.shadow / abi_unit;
            let shifted = sd_abi_notch(abi_point - params.xy, abi_size, abi_radii, prim.light, abi_top, abi_bottom);
            let inset = prim.notch_info.y > 1.5;
            let distance = if inset { shifted + params.w } else { params.w - shifted };
            let mut shade = clamp(0.5 + distance, 0.0, 1.0);
            if params.z >= 0.001 { shade = 0.5 * (1.0 + erf(distance / (0.7071067811865476 * params.z))); }
            let outside = smoothstep(-0.75, 0.75, d);
            let mask = if inset { 1.0 - outside } else { outside };
            let color = prim.shadow_color * shade;
            return Vec4::new(color.rgb, color.a * mask * clip_alpha);
        }

        let mut result = Vec4::splat(0.0);

        // Calculate shadow first (rendered behind)
        if prim.shadow.z > 0.0 || prim.shadow.w != 0.0 {
            let shadow_offset = prim.shadow.xy;
            let blur = prim.shadow.z;
            let spread = prim.shadow.w;

            let shadow_origin = origin + shadow_offset - Vec2::splat(spread);
            let shadow_size = size + Vec2::splat(spread * 2.0);

            // Adjust corner radii for spread (expand corners proportionally)
            let shadow_radii = prim.corner_radius + Vec4::splat(spread);

            // For PRIM_NOTCH the shadow traces the actual notch outline
            // via `sd_notch` — so concave arcs, bulges, scoops, cuts and
            // peaks all cast shadows from their real visible edges instead
            // of from the rectangular bounding box.
            let mut shadow_sdf_dist: f32;
            shadow_sdf_dist = sd_notch(
                sp, shadow_origin, shadow_size,
                shadow_radii,
                prim.light,
                prim.perspective,
                prim.sdf_3d
            );
            let mut shadow_alpha: f32;
            if blur < 0.001 {
                shadow_alpha = if shadow_sdf_dist < 0.0 { 1.0 } else { 0.0 };
            } else {
                let sigma_d = 0.5 * sqrt(2.0) * blur;
                shadow_alpha = 0.5 * (1.0 + erf(-shadow_sdf_dist / sigma_d));
            }

            // Thin out the shadow at the "ending" of a concave arc. The
            // concave boundary has its outward normal pointing AWAY from
            // the shape (into the wedge region), and at the two points
            // where the arc touches the outer bounds the normal becomes
            // axis-aligned: at the `inner_top` end of a concave TOP arc
            // it points straight up, at `inner_bottom` it points straight
            // down, etc. Tracing `sd_notch` fully means those attachment
            // points cast a full-strength shadow outward, which reads as
            // a thick dark band bleeding past the concave edge.
            //
            // For each edge that has BOTH adjacent corners concave, fade
            // the shadow to zero as the pixel approaches the attachment
            // line on the "outside" side of the shape. The fade width is
            // the shadow blur so it tapers smoothly.
            if blur > 0.001 {
                let tl_c = prim.light.x > 0.5;
                let tr_c = prim.light.y > 0.5;
                let br_c = prim.light.z > 0.5;
                let bl_c = prim.light.w > 0.5;
                let fade_dist = blur;
                if tl_c && tr_c {
                    let top_off = max(shadow_radii.x, shadow_radii.y);
                    let inner_top = shadow_origin.y + top_off;
                    let top_fade = smoothstep(inner_top - fade_dist, inner_top, sp.y);
                    shadow_alpha *= top_fade;
                }
                if bl_c && br_c {
                    let bottom_off = max(shadow_radii.w, shadow_radii.z);
                    let inner_bottom = shadow_origin.y + shadow_size.y - bottom_off;
                    let bot_fade = smoothstep(inner_bottom + fade_dist, inner_bottom, sp.y);
                    shadow_alpha *= bot_fade;
                }
                if tl_c && bl_c {
                    let left_off = max(shadow_radii.x, shadow_radii.w);
                    let inner_left = shadow_origin.x + left_off;
                    let left_fade = smoothstep(inner_left - fade_dist, inner_left, sp.x);
                    shadow_alpha *= left_fade;
                }
                if tr_c && br_c {
                    let right_off = max(shadow_radii.y, shadow_radii.z);
                    let inner_right = shadow_origin.x + shadow_size.x - right_off;
                    let right_fade = smoothstep(inner_right + fade_dist, inner_right, sp.x);
                    shadow_alpha *= right_fade;
                }
            }

            let shadow_color = prim.shadow_color * shadow_alpha;

            // Premultiply and blend
            result = shadow_color;
        }

        // Calculate main shape SDF — always notch
        let mut d = sd_notch(
            sp, origin, size,
            prim.corner_radius,
            prim.light,       // per-corner type flags (TL, TR, BR, BL)
            prim.perspective, // top modifier    (type, width, height, corner_r)
            prim.sdf_3d       // bottom modifier (type, width, height, corner_r)
        );

        // Anti-aliasing: smooth transition at edge over ~1 pixel total.
        let mut aa_width = 0.5;
        if prim.notch_info.x > 0.0 {
            d = sd_abi_notch(abi_point, abi_size, abi_radii, prim.light, abi_top, abi_bottom);
            let det = abs(la.x * la.w - la.y * la.z);
            aa_width = 0.25 * (length(Vec2::new(la.w, -la.y)) + length(Vec2::new(-la.z, la.x))) / (det * abi_unit);
        }
        let fill_alpha = 1.0 - smoothstep(-aa_width, aa_width, d);

        if fill_alpha < 0.001 {
            return result;
        }

        // Determine fill color
        let mut fill_color: Vec4;
        match fill_type {
            0u32 /* FILL_SOLID */ => {
                fill_color = prim.color;
            }
            1u32 /* FILL_LINEAR_GRADIENT */ => {
                // Linear gradient using gradient_params (x1, y1, x2, y2) in user space
                let g_start = prim.gradient_params.xy;
                let g_end = prim.gradient_params.zw;
                let g_dir = g_end - g_start;
                let g_len_sq = dot(g_dir, g_dir);

                let mut t: f32;
                if g_len_sq > 0.0001 {
                    // Project current position onto gradient line
                    let proj = sp - g_start;
                    t = dot(proj, g_dir) / g_len_sq;
                } else {
                    t = 0.0;
                }
                if prim.gradient_table.x > 0.5 {
                    fill_color = gradient_table_color(t, ((prim.gradient_table.x) as u32) - 1u32);
                } else {
                    fill_color = mix(prim.color, prim.color2, clamp(t, 0.0, 1.0));
                }
            }
            2u32 /* FILL_RADIAL_GRADIENT */ => {
                // Radial gradient using gradient_params (cx, cy, radius, 0) in user space
                let g_center = prim.gradient_params.xy;
                let g_radius = prim.gradient_params.z;

                let dist = length(sp - g_center);
                let t = dist / max(g_radius, 0.001);
                if prim.gradient_table.x > 0.5 {
                    fill_color = gradient_table_color(t, ((prim.gradient_table.x) as u32) - 1u32);
                } else {
                    fill_color = mix(prim.color, prim.color2, clamp(t, 0.0, 1.0));
                }
            }
            3u32 /* FILL_CONIC_GRADIENT */ => {
                // Conic gradient using gradient_params (cx, cy, start angle, 0) in user space
                let delta = sp - prim.gradient_params.xy;
                let t = fract((atan2(delta.x, -delta.y) - prim.gradient_params.z) / 6.283185307179586);
                if prim.gradient_table.x > 0.5 {
                    fill_color = gradient_table_color(t, ((prim.gradient_table.x) as u32) - 1u32);
                } else {
                    fill_color = mix(prim.color, prim.color2, t);
                }
            }
            _ => {
                fill_color = prim.color;
            }
        }

        if prim.notch_info.x > 0.0 {
            if prim.border.x > 0.0 {
                let ring = smoothstep(-aa_width, aa_width, d + prim.border.x / abi_unit);
                fill_color = mix(fill_color, prim.border_color, ring * prim.border_color.a);
            }
            return Vec4::new(fill_color.rgb, fill_color.a * fill_alpha * clip_alpha);
        }

        // Handle border with proper inner corner radii (GPUI-style approach)
        // The border is the ring between the outer shape edge and an inner shape
        // For asymmetric borders, inner corners become elliptical, not circular
        // prim.border = [top, right, bottom, left] for per-side borders, or [uniform, 0, 0, 0] for uniform
        let border_top = prim.border.x;
        let border_right = prim.border.y;
        let border_bottom = prim.border.z;
        let border_left = prim.border.w;

        // Check if any border is present (using max of all sides)
        let max_border = max(max(border_top, border_right), max(border_bottom, border_left));
        if max_border > 0.0 {
            // For uniform border (legacy: only .x set), use it for all sides
            let bt = if border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0 { border_top } else { border_top };
            let br = if border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0 { border_right } else { border_top };
            let bb = if border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0 { border_bottom } else { border_top };
            let bl = if border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0 { border_left } else { border_top };

            let half_size = size * 0.5;
            let rel = sp - center;  // Position relative to center (signed, in unrotated space)

            // Use the same AA width as the outer edge smoothstep so the
            // border's inner transition matches the fill's outer transition
            // pixel-for-pixel. Currently 0.5 (tight 1-pixel AA — see the
            // longer rationale above `aa_width`).
            let border_aa = aa_width;

            // Select corner radius and corner shape based on quadrant
            let mut corner_radius: f32;
            let mut corner_n: f32;
            if rel.y < 0.0 {
                if rel.x > 0.0 { corner_radius = prim.corner_radius.y; corner_n = prim.corner_shape.y; }  // top-right
                else { corner_radius = prim.corner_radius.x; corner_n = prim.corner_shape.x; }           // top-left
            } else {
                if rel.x > 0.0 { corner_radius = prim.corner_radius.z; corner_n = prim.corner_shape.z; }  // bottom-right
                else { corner_radius = prim.corner_radius.w; corner_n = prim.corner_shape.w; }           // bottom-left
            }
            // Clamp radius to half the minimum dimension (CSS spec)
            corner_radius = min(corner_radius, min(half_size.x, half_size.y));

            // Select border widths for nearest edges based on quadrant (GPUI approach)
            let border = Vec2::new(
                if rel.x < 0.0 { bl } else { br },  // horizontal: left or right
                if rel.y < 0.0 { bt } else { bb }   // vertical: top or bottom
            );

            // Handle zero-width borders (treat as negative for AA purposes)
            let reduced_border = Vec2::new(
                if border.x == 0.0 { -border_aa } else { border.x },
                if border.y == 0.0 { -border_aa } else { border.y }
            );

            // Calculate position relative to corner
            let corner_to_point = abs(rel) - half_size;
            let corner_center_to_point = corner_to_point + corner_radius;

            // Determine if we're near a rounded corner
            let is_near_rounded_corner = corner_center_to_point.x >= 0.0 && corner_center_to_point.y >= 0.0;

            // Inner straight border edge
            let straight_border_inner = corner_to_point + reduced_border;

            // Check if we're clearly inside the inner area (not near border)
            let is_within_inner_straight = straight_border_inner.x < -border_aa &&
                                           straight_border_inner.y < -border_aa;

            // PRIM_NOTCH always goes through the SDF-based `inner_sdf` branch
            // because the quadrant-based geometry is derived from the rect
            // bounding box and doesn't account for concave corners / top-bottom
            // modifiers — a point deep inside the bbox can still be near a
            // concave flare or a bulge edge.
            {
                // Calculate inner SDF based on context
                let mut inner_sdf: f32;

                if abs(reduced_border.x - reduced_border.y) < 0.001 {
                    // Uniform border — use exact SDF offset of the outer shape distance.
                    inner_sdf = -(d + reduced_border.x);
                } else {
                    // Asymmetric borders — inner corners become elliptical
                    if corner_center_to_point.x <= 0.0 || corner_center_to_point.y <= 0.0 {
                        // Not in corner region — straight edge distance
                        inner_sdf = -max(straight_border_inner.x, straight_border_inner.y);
                    } else if abs(corner_n - 1.0) < 0.01 {
                        // Round corner — elliptical inner corner (GPUI approach)
                        let ellipse_radii = max(Vec2::splat(0.0), Vec2::splat(corner_radius) - reduced_border);
                        inner_sdf = quarter_ellipse_sdf(corner_center_to_point, ellipse_radii);
                    } else {
                        // Superellipse with per-axis reduced radii
                        let inner_radii = max(Vec2::splat(0.0), Vec2::splat(corner_radius) - reduced_border);
                        let p_exp = pow(2.0, min(abs(corner_n), 5.0));
                        let min_inner_r = min(inner_radii.x, inner_radii.y);
                        if min_inner_r < 0.001 {
                            inner_sdf = -length(max(Vec2::splat(0.0), corner_center_to_point));
                        } else {
                            let inner_t = corner_center_to_point / inner_radii;
                            let inner_se = pow(max(inner_t.x, 0.0), p_exp) + pow(max(inner_t.y, 0.0), p_exp);
                            let inner_r_scale = sqrt(inner_radii.x * inner_radii.y);
                            inner_sdf = -((pow(inner_se, 1.0 / p_exp) - 1.0) * inner_r_scale);
                        }
                    }
                }

                // Match the main fill's tight 1-pixel AA (see `aa_width`
                // above) so the inner border edge shares the same
                // transition width as the outer shape edge.
                let inner_aa = aa_width;
                let border_blend = smoothstep(-inner_aa, inner_aa, -inner_sdf);

                // Only apply border color where we're inside the shape
                fill_color = mix(fill_color, prim.border_color, border_blend * step(0.001, fill_alpha));
            }
        }

        // Apply clip alpha to shadow
        result.a *= clip_alpha;

        // Mask shadow strictly outside the shape boundary
        // Use the same aa_width as fill_alpha to prevent gaps at corners
        // The shadow should render only where d > 0 (outside the shape)
        if result.a > 0.0 {
            // Use matching AA width to ensure shadow and fill meet seamlessly
            let shadow_mask = smoothstep(-aa_width, aa_width, d);
            result.a *= shadow_mask;
        }

        // Blend fill over shadow at FULL opacity first (fill fully covers shadow)
        // This ensures no shadow bleeds through the shape regardless of edge AA
        let full_fill = Vec4::new(fill_color.rgb, fill_color.a * clip_alpha);
        result = full_fill + result * (1.0 - full_fill.a);

        // NOW apply outer edge anti-aliasing to the combined result
        // This gives smooth edges against the background without shadow bleed
        result.a *= fill_alpha;

        // Apply mask gradient (mask-image: linear-gradient / radial-gradient)
        // mask_info.x: 0=none, 1=linear, 2=radial
        // mask_params are in OBB (0-1) space relative to element bounds
        let mask_type = prim.mask_info.x;
        if mask_type > 0.5 {
            // Compute normalized UV relative to element bounds
            let mask_uv = (sp - origin) / max(size, Vec2::splat(0.001));
            let mut mask_t: f32;
            if mask_type < 1.5 {
                // Linear mask gradient
                let m_start = prim.mask_params.xy;
                let m_end = prim.mask_params.zw;
                let m_dir = m_end - m_start;
                let m_len_sq = dot(m_dir, m_dir);
                if m_len_sq > 0.0001 {
                    mask_t = clamp(dot(mask_uv - m_start, m_dir) / m_len_sq, 0.0, 1.0);
                } else {
                    mask_t = 0.0;
                }
            } else {
                // Radial mask gradient
                let m_center = prim.mask_params.xy;
                let m_radius = prim.mask_params.z;
                mask_t = clamp(length(mask_uv - m_center) / max(m_radius, 0.001), 0.0, 1.0);
            }
            let mask_alpha = mix(prim.mask_info.y, prim.mask_info.z, mask_t);
            result = Vec4::new(result.rgb * mask_alpha, result.a * mask_alpha);
        }

        // Apply CSS filters (grayscale, invert, sepia, hue-rotate, brightness, contrast, saturate)
        // Skip if all identity (filter_a all zero, filter_b = (1,1,1,0))
        let fa = prim.filter_a;
        let fb = prim.filter_b;
        if fa.x != 0.0 || fa.y != 0.0 || fa.z != 0.0 || abs(fa.w) > 0.001 || fb.x != 1.0 || fb.y != 1.0 || fb.z != 1.0 {
            result = apply_css_filter(result, fa, fb);
        }

        return result;
    }
}
