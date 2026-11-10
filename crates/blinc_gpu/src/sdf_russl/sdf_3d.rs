//! The SDF 3D shader: rects, circles and ellipses carrying a 3D shape (box, sphere, cylinder,
//! torus, capsule) or a group of them, raymarched per fragment.

russl::russl! {
    use super::abi::{Uniforms, Primitive, UNIFORMS, PRIMITIVES, GLYPH_ATLAS, GLYPH_SAMPLER, COLOR_GLYPH_ATLAS};
    use super::clip::{calculate_clip_alpha, abi_shape_clip_alpha, AUX_DATA};
    use super::shapes3d::{apply_boolean_op, ray_aabb_intersect, rotate_x_inv, rotate_y_inv, rotate_z_inv, sdf_3d_eval};

    // ============================================================================
    // Blinc SDF 3D Primitive Shader
    //
    // Handles primitives that use 3D SDF raymarching - when perspective.w
    // (shape_type) > 0. This includes individual 3D shapes (shape_type 1-5:
    // box, sphere, cylinder, torus, capsule) and 3D groups (shape_type 6).
    // ============================================================================

    struct VertexOutput {
        #[builtin(position)] position: Vec4,
        #[location(0)] uv: Vec2,
        #[location(1)] #[flat] instance_index: u32,
    }

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

        // Expand bounds for shadow blur
        let blur_expand = prim.shadow.z * 3.0 + abs(prim.shadow.x) + abs(prim.shadow.y);

        // Extract 3D rotation/perspective parameters
        let sin_rz = prim.rotation.x;
        let cos_rz = prim.rotation.y;
        let sin_ry = prim.rotation.z;
        let cos_ry = prim.rotation.w;
        let sin_rx = prim.perspective.x;
        let cos_rx = prim.perspective.y;
        let persp_d = prim.perspective.z;

        // 3D perspective: project all 8 corners of the 3D bounding box to find AABB
        let ctr = prim.bounds.xy + prim.bounds.zw * 0.5;
        let half = prim.bounds.zw * 0.5;
        let half_d = prim.sdf_3d.x * 0.5; // half-depth
        let corners3d = [
            Vec3::new(-half.x, -half.y, -half_d),
            Vec3::new( half.x, -half.y, -half_d),
            Vec3::new( half.x,  half.y, -half_d),
            Vec3::new(-half.x,  half.y, -half_d),
            Vec3::new(-half.x, -half.y,  half_d),
            Vec3::new( half.x, -half.y,  half_d),
            Vec3::new( half.x,  half.y,  half_d),
            Vec3::new(-half.x,  half.y,  half_d),
        ];
        let mut min_p = Vec2::splat(1e10);
        let mut max_p = Vec2::splat(-1e10);
        let pd = if persp_d > 0.001 { persp_d } else { 800.0 };
        for i in 0u32..8u32 {
            let c = corners3d[i];
            // Apply rotateZ
            let rz_x = c.x * cos_rz - c.y * sin_rz;
            let rz_y = c.x * sin_rz + c.y * cos_rz;
            let rz_z = c.z;
            // Apply rotateX (tilt Y/Z)
            let rx_y = rz_y * cos_rx - rz_z * sin_rx;
            let rx_z = rz_y * sin_rx + rz_z * cos_rx;
            // Apply rotateY (tilt X/Z)
            let ry_x = rz_x * cos_ry + rx_z * sin_ry;
            let ry_z = -rz_x * sin_ry + rx_z * cos_ry;
            // Perspective divide
            let w = 1.0 - ry_z / pd;
            let proj = Vec2::new(ry_x, rx_y) / max(w, 0.001);
            min_p = min(min_p, proj);
            max_p = max(max_p, proj);
        }
        min_p -= Vec2::splat(blur_expand + 2.0);
        max_p += Vec2::splat(blur_expand + 2.0);
        let bounds = Vec4::new(ctr + min_p, max_p - min_p);

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
    // Group SDF: shapes packed in AUX_DATA, four vec4 each
    // ============================================================================

    fn eval_group_sdf(p: Vec3, shape_count: u32, aux_offset: u32) -> f32 {
        let mut d = 1e10;
        for i in 0u32..shape_count {
            let base = aux_offset + i * 4u32;
            let s_offset = AUX_DATA[base];       // x, y, z, corner_radius
            let s_params = AUX_DATA[base + 1u32];  // shape_type, depth, op_type, blend
            let s_half = AUX_DATA[base + 2u32];    // half_w, half_h, half_d, 0

            let local_p = p - s_offset.xyz;
            let shape_d = sdf_3d_eval(local_p, ((s_params.x) as u32), s_half.xyz, s_offset.w);

            if i == 0u32 {
                d = shape_d;
            } else {
                d = apply_boolean_op(d, shape_d, ((s_params.z) as u32), s_params.w);
            }
        }
        return d;
    }

    // Compute group normal via central differences
    fn eval_group_normal(hp: Vec3, shape_count: u32, aux_offset: u32) -> Vec3 {
        let eps = 0.001;
        return normalize(Vec3::new(
            eval_group_sdf(hp + Vec3::new(eps, 0.0, 0.0), shape_count, aux_offset) -
            eval_group_sdf(hp - Vec3::new(eps, 0.0, 0.0), shape_count, aux_offset),
            eval_group_sdf(hp + Vec3::new(0.0, eps, 0.0), shape_count, aux_offset) -
            eval_group_sdf(hp - Vec3::new(0.0, eps, 0.0), shape_count, aux_offset),
            eval_group_sdf(hp + Vec3::new(0.0, 0.0, eps), shape_count, aux_offset) -
            eval_group_sdf(hp - Vec3::new(0.0, 0.0, eps), shape_count, aux_offset)
        ));
    }

    // Find which shape in the group is closest to the hit point (for per-shape coloring)
    fn eval_group_closest_shape_color(hp: Vec3, shape_count: u32, aux_offset: u32) -> Vec4 {
        let mut min_d = 1e10;
        let mut closest_color = Vec4::splat(1.0);
        for i in 0u32..shape_count {
            let base = aux_offset + i * 4u32;
            let s_offset = AUX_DATA[base];
            let s_params = AUX_DATA[base + 1u32];
            let s_half = AUX_DATA[base + 2u32];
            let s_color = AUX_DATA[base + 3u32];

            let local_p = hp - s_offset.xyz;
            let d = abs(sdf_3d_eval(local_p, ((s_params.x) as u32), s_half.xyz, s_offset.w));
            if d < min_d {
                min_d = d;
                closest_color = s_color;
            }
        }
        return closest_color;
    }

    // ============================================================================
    // Fragment Shader
    // ============================================================================

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        let prim = PRIMITIVES[input.instance_index];
        let p = input.uv;

        let fill_type = prim.type_info.y;
        let clip_type = prim.type_info.z;

        // Early clip test - discard if completely outside clip region (screen space)
        let mut clip_alpha = calculate_clip_alpha(p, prim.clip_bounds, prim.clip_radius, Vec4::splat(1.0), clip_type, prim.clip_fade, prim.clip_fade_bounds);
        clip_alpha = clip_alpha * abi_shape_clip_alpha(p, prim.clip_shape.x);
        if clip_alpha < 0.001 {
            discard!();
        }

        let origin = prim.bounds.xy;
        let size = prim.bounds.zw;
        let center = origin + size * 0.5;

        // Extract rotation and perspective parameters
        let sin_rz = prim.rotation.x;
        let cos_rz = prim.rotation.y;
        let sin_ry = prim.rotation.z;
        let cos_ry = prim.rotation.w;
        let sin_rx = prim.perspective.x;
        let cos_rx = prim.perspective.y;
        let persp_d = prim.perspective.z;
        let shape_type = ((prim.perspective.w) as u32);

        // Early type filter - discard non-3D primitives handled by other split pipelines
        if shape_type == 0u32 { discard!(); }

        let depth = prim.sdf_3d.x;

        // -- 3D SDF Raymarching Path (individual shapes) --
        if shape_type > 0u32 && shape_type != 6u32 && depth > 0.001 {
            let translate_z = prim.sdf_3d.w;
            let pd = if persp_d > 0.001 { persp_d } else { 800.0 };
            let rel = p - center;

            // Ray setup: camera at (0, 0, pd - translate_z), fragment at (rel.x, rel.y, 0)
            // Positive translate_z moves object toward viewer (closer = larger)
            let cam_vs = Vec3::new(0.0, 0.0, pd - translate_z);
            let frag_vs = Vec3::new(rel.x, rel.y, 0.0);
            let ray_dir_vs = normalize(frag_vs - cam_vs);

            // Transform ray to shape space (inverse of rotateZ -> rotateX -> rotateY)
            let mut ro = cam_vs;
            let mut rd = ray_dir_vs;
            ro = rotate_y_inv(ro, sin_ry, cos_ry);
            rd = rotate_y_inv(rd, sin_ry, cos_ry);
            ro = rotate_x_inv(ro, sin_rx, cos_rx);
            rd = rotate_x_inv(rd, sin_rx, cos_rx);
            ro = rotate_z_inv(ro, sin_rz, cos_rz);
            rd = rotate_z_inv(rd, sin_rz, cos_rz);

            // Shape bounds in shape space
            let half_3d = Vec3::new(size.x * 0.5, size.y * 0.5, depth * 0.5);
            let corner_r = min(min(prim.corner_radius.x, prim.corner_radius.y), depth * 0.5);

            // Analytical ray-AABB intersection for tight starting bound
            let aabb_t = ray_aabb_intersect(ro, rd, half_3d);
            if aabb_t.x > aabb_t.y || aabb_t.y < 0.0 {
                discard!();  // Ray misses bounding box entirely
            }

            // Raymarch (32 steps) starting from AABB entry point
            // Use AABB diagonal as max distance to allow rays to reach rounded corners
            // (at AABB corners, the SDF distance to rounded surface can be large)
            let mut t_rm = max(aabb_t.x - 0.01, 0.0);
            let t_max = aabb_t.x + length(half_3d) * 2.0 + 1.0;
            let mut hit = false;
            let mut min_d = 1e10;
            for i in 0u32..32u32 {
                let pos = ro + rd * t_rm;
                let d3 = sdf_3d_eval(pos, shape_type, half_3d, corner_r);
                min_d = min(min_d, d3);
                if d3 < 0.001 {
                    hit = true;
                    break;
                }
                t_rm += d3;
                if t_rm > t_max {
                    break;
                }
            }

            // Edge anti-aliasing: smooth alpha based on closest approach distance
            let pixel_size = max(t_rm / pd, 0.5);
            let mut edge_aa = 1.0;
            if !hit {
                if min_d > pixel_size * 2.0 {
                    discard!();
                }
                edge_aa = 1.0 - smoothstep(0.0, pixel_size * 1.5, min_d);
            }

            // Compute normal via central differences
            let hp = ro + rd * t_rm;
            let eps = 0.001;
            let normal = normalize(Vec3::new(
                sdf_3d_eval(hp + Vec3::new(eps, 0.0, 0.0), shape_type, half_3d, corner_r) -
                sdf_3d_eval(hp - Vec3::new(eps, 0.0, 0.0), shape_type, half_3d, corner_r),
                sdf_3d_eval(hp + Vec3::new(0.0, eps, 0.0), shape_type, half_3d, corner_r) -
                sdf_3d_eval(hp - Vec3::new(0.0, eps, 0.0), shape_type, half_3d, corner_r),
                sdf_3d_eval(hp + Vec3::new(0.0, 0.0, eps), shape_type, half_3d, corner_r) -
                sdf_3d_eval(hp - Vec3::new(0.0, 0.0, eps), shape_type, half_3d, corner_r)
            ));

            // Lighting
            let light_dir = normalize(prim.light.xyz);
            let n_dot_l = max(dot(normal, light_dir), 0.0);
            let ambient_3d = prim.sdf_3d.y;
            let diffuse_3d = n_dot_l * prim.light.w;

            // Specular (Blinn-Phong)
            let view_dir = normalize(-rd);
            let half_vec = normalize(light_dir + view_dir);
            let spec_3d = pow(max(dot(normal, half_vec), 0.0), prim.sdf_3d.z) * 0.5;

            let lighting = ambient_3d + diffuse_3d + spec_3d;

            // UV mapping: use screen-space position for gradient evaluation.
            // This gives smooth gradients across all visible faces of 3D shapes
            // (avoids face-based UV discontinuities on boxes).
            // Screen-space UV: fragment position relative to element bounds.
            let mut base_color_3d: Vec4;
            if fill_type == 1u32 {
                // Linear gradient: gradient_params are already in screen-space pixels
                let gp = prim.gradient_params;
                let gdir = gp.zw - gp.xy;
                let glen_sq = dot(gdir, gdir);
                if glen_sq > 0.000001 {
                    let t_g = dot(p - gp.xy, gdir) / glen_sq;
                    base_color_3d = mix(prim.color, prim.color2, clamp(t_g, 0.0, 1.0));
                } else {
                    base_color_3d = prim.color;
                }
            } else if fill_type == 2u32 {
                // Radial gradient: (cx, cy, radius, 0) in screen-space pixels
                let t_g = length(p - prim.gradient_params.xy) / max(prim.gradient_params.z, 0.001);
                base_color_3d = mix(prim.color, prim.color2, clamp(t_g, 0.0, 1.0));
            } else {
                base_color_3d = prim.color;
            }

            let mut result_3d = base_color_3d * Vec4::new(Vec3::splat(lighting), 1.0);
            result_3d.a *= clip_alpha * edge_aa;
            return result_3d;
        }

        // -- 3D Group SDF Raymarching Path --
        // border[1] = shape_count, border[2] = aux_data offset
        if shape_type == 6u32 && prim.border.y > 0.5 {
            let group_shape_count = ((prim.border.y) as u32);
            let group_aux_offset = ((prim.border.z) as u32);

            // Use max depth from child shapes via border[3] (set by paint context)
            let group_depth = max(prim.border.w, 1.0);
            let translate_z = prim.sdf_3d.w;
            let pd = if persp_d > 0.001 { persp_d } else { 800.0 };
            let rel = p - center;

            // Ray setup (same as individual shapes)
            let cam_vs = Vec3::new(0.0, 0.0, pd - translate_z);
            let frag_vs = Vec3::new(rel.x, rel.y, 0.0);
            let ray_dir_vs = normalize(frag_vs - cam_vs);

            // Transform ray to shape space
            let mut ro = cam_vs;
            let mut rd = ray_dir_vs;
            ro = rotate_y_inv(ro, sin_ry, cos_ry);
            rd = rotate_y_inv(rd, sin_ry, cos_ry);
            ro = rotate_x_inv(ro, sin_rx, cos_rx);
            rd = rotate_x_inv(rd, sin_rx, cos_rx);
            ro = rotate_z_inv(ro, sin_rz, cos_rz);
            rd = rotate_z_inv(rd, sin_rz, cos_rz);

            // AABB for the entire group
            let half_3d = Vec3::new(size.x * 0.5, size.y * 0.5, group_depth * 0.5);
            let aabb_t = ray_aabb_intersect(ro, rd, half_3d);
            if aabb_t.x > aabb_t.y || aabb_t.y < 0.0 {
                discard!();
            }

            // Raymarch the compound SDF (32 steps)
            let mut t_rm = max(aabb_t.x - 0.01, 0.0);
            let t_max = aabb_t.x + length(half_3d) * 2.0 + 1.0;
            let mut hit = false;
            let mut min_d = 1e10;
            for i in 0u32..32u32 {
                let pos = ro + rd * t_rm;
                let d3 = eval_group_sdf(pos, group_shape_count, group_aux_offset);
                min_d = min(min_d, d3);
                if d3 < 0.001 {
                    hit = true;
                    break;
                }
                t_rm += d3;
                if t_rm > t_max {
                    break;
                }
            }

            // Edge anti-aliasing
            let pixel_size = max(t_rm / pd, 0.5);
            let mut edge_aa = 1.0;
            if !hit {
                if min_d > pixel_size * 2.0 {
                    discard!();
                }
                edge_aa = 1.0 - smoothstep(0.0, pixel_size * 1.5, min_d);
            }

            // Compute normal via group SDF
            let hp = ro + rd * t_rm;
            let normal = eval_group_normal(hp, group_shape_count, group_aux_offset);

            // Lighting (same as individual shapes)
            let light_dir = normalize(prim.light.xyz);
            let n_dot_l = max(dot(normal, light_dir), 0.0);
            let ambient_3d = prim.sdf_3d.y;
            let diffuse_3d = n_dot_l * prim.light.w;
            let view_dir = normalize(-rd);
            let half_vec = normalize(light_dir + view_dir);
            let spec_3d = pow(max(dot(normal, half_vec), 0.0), prim.sdf_3d.z) * 0.5;
            let lighting = ambient_3d + diffuse_3d + spec_3d;

            // Per-shape coloring: find which child shape is closest to the hit point
            let base_color_3d = eval_group_closest_shape_color(hp, group_shape_count, group_aux_offset);

            let mut result_3d = base_color_3d * Vec4::new(Vec3::splat(lighting), 1.0);
            result_3d.a *= clip_alpha * edge_aa;
            return result_3d;
        }

        // Safety fallback: if neither 3D path triggered, discard
        discard!();
        return Vec4::splat(0.0);
    }
}
