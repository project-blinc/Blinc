//! The path shader: tessellated vector paths as coloured triangles, with solid, linear and radial
//! brushes (multi-stop through a 1D gradient texture), an image brush, a glass effect over the
//! backdrop, and a per-vertex clip. `TIER` 0 reads the polygon clip vertices and ABI clips from a
//! storage buffer, 2 from a data texture; the path bind group has them at slot 7, which the
//! specialisations set.

russl::russl! {
    use super::aux::aux;
    use super::clip::abi_shape_clip_alpha;
    use super::common::{sd_circle, sd_ellipse, sd_rounded_rect};

    #[pipeline] const TIER: u32 = 0;
    #[pipeline] const AUX_SLOT: u32 = 5;

    pub struct PathUniforms {
        // viewport_size (vec2) + padding (vec2) = 16 bytes, offset 0
        pub viewport_size: Vec2,
        pub opacity: f32,
        pub _pad0: f32,
        // 3x3 transform stored as 3 vec4s (xyz used, w is padding) = 48 bytes, offset 16
        pub transform_row0: Vec4,
        pub transform_row1: Vec4,
        pub transform_row2: Vec4,
        // Clip parameters, offset 64: legacy, the clip is per vertex
        pub clip_bounds: Vec4,
        pub clip_radius: Vec4,
        // clip_type + flags = 16 bytes, offset 96
        pub clip_type: u32,
        // 0 uses vertex colours, 1 samples the gradient texture
        pub use_gradient_texture: u32,
        // 0 no image, 1 samples the image texture
        pub use_image_texture: u32,
        // 0 no glass, 1 glass effect on the path
        pub use_glass_effect: u32,
        // Image UV bounds (u_min, v_min, u_max, v_max), offset 112
        pub image_uv_bounds: Vec4,
        // Glass parameters (blur_radius, saturation, tint_strength, opacity), offset 128
        pub glass_params: Vec4,
        // Glass tint colour, offset 144
        pub glass_tint: Vec4,
    }

    #[group(0)] #[binding(0)] static PATH: Uniform<PathUniforms>;
    #[group(0)] #[binding(1)] static GRADIENT_TEXTURE: Texture1D<f32>;
    #[group(0)] #[binding(2)] static GRADIENT_SAMPLER: Sampler;
    #[group(0)] #[binding(3)] static IMAGE_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(4)] static IMAGE_SAMPLER: Sampler;
    // The backdrop and the auxiliary data take slots 5 and 7 between them: the path bind group has
    // the backdrop at 5 and the auxiliary data at 7, and the source built on its own (the auxiliary
    // data at its default, 5) swaps them.
    #if AUX_SLOT == 7
        #[group(0)] #[binding(5)] static BACKDROP_TEXTURE: Texture2D<f32>;
    #else
        #[group(0)] #[binding(7)] static BACKDROP_TEXTURE: Texture2D<f32>;
    #endif
    #[group(0)] #[binding(6)] static BACKDROP_SAMPLER: Sampler;

    struct VertexOutput {
        #[builtin(position)] position: Vec4,
        #[location(0)] color: Vec4,
        #[location(1)] end_color: Vec4,
        #[location(2)] uv: Vec2,
        #[location(3)] #[flat] gradient_params: Vec4,
        #[location(4)] #[flat] gradient_type: u32,
        #[location(5)] edge_distance: f32,
        // Screen position, for the clip
        #[location(6)] screen_pos: Vec2,
        #[location(7)] #[flat] v_clip_bounds: Vec4,
        #[location(8)] #[flat] v_clip_radius: Vec4,
        #[location(9)] #[flat] v_clip_type: u32,
        #[location(10)] #[flat] v_clip_shape: u32,
    }

    // Alpha of the clip, 1 inside. For the shaped clips `clip_bounds` is a rect scissor and
    // `clip_radius` holds the shape: 1 rect, 2 circle, 3 ellipse, 4 polygon.
    fn calculate_clip_alpha(p: Vec2, clip_bounds: Vec4, clip_radius: Vec4, clip_type: u32) -> f32 {
        if clip_type == 0u32 {
            return 1.0;
        }

        let aa_width = 0.75;
        let mut alpha = 1.0;

        match clip_type {
            1u32 => {
                let clip_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, clip_radius);
                alpha = 1.0 - smoothstep(-aa_width, aa_width, clip_d);
            }
            2u32 => {
                let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, Vec4::splat(0.0));
                let scissor_alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
                let clip_d = sd_circle(p, clip_radius.xy, clip_radius.z);
                let shape_alpha = 1.0 - smoothstep(-aa_width, aa_width, clip_d);
                alpha = scissor_alpha * shape_alpha;
            }
            3u32 => {
                let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, Vec4::splat(0.0));
                let scissor_alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
                let clip_d = sd_ellipse(p, clip_radius.xy, clip_radius.zw);
                let shape_alpha = 1.0 - smoothstep(-aa_width, aa_width, clip_d);
                alpha = scissor_alpha * shape_alpha;
            }
            4u32 => {
                let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, Vec4::splat(0.0));
                let scissor_alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
                let vertex_count = clip_radius.z as u32;
                let aux_offset = clip_radius.w as u32;
                let shape_alpha = calculate_polygon_clip_alpha(p, vertex_count, aux_offset);
                alpha = scissor_alpha * shape_alpha;
            }
            _ => {}
        }

        return alpha;
    }

    // Winding-number test against a polygon whose vertices are packed two to a vec4 in the
    // auxiliary data from vec4 `aux_offset`. The same test the SDF pipeline runs, so a polygon
    // clip cuts fills and shape primitives alike.
    fn calculate_polygon_clip_alpha(p: Vec2, vertex_count: u32, aux_offset: u32) -> f32 {
        if vertex_count < 3u32 {
            return 1.0;
        }

        let mut winding: i32 = 0;

        for i in 0u32..vertex_count {
            let data = aux(aux_offset + (i / 2u32));
            let mut vi: Vec2;
            if (i % 2u32) == 0u32 {
                vi = data.xy;
            } else {
                vi = data.zw;
            }

            let j = (i + 1u32) % vertex_count;
            let data_j = aux(aux_offset + (j / 2u32));
            let mut vj: Vec2;
            if (j % 2u32) == 0u32 {
                vj = data_j.xy;
            } else {
                vj = data_j.zw;
            }

            // A horizontal ray from `p` to the right: +1 or -1 at each crossing by edge direction.
            if vi.y <= p.y {
                if vj.y > p.y {
                    let cross = (vj.x - vi.x) * (p.y - vi.y) - (p.x - vi.x) * (vj.y - vi.y);
                    if cross > 0.0 {
                        winding = winding + 1;
                    }
                }
            } else {
                if vj.y <= p.y {
                    let cross = (vj.x - vi.x) * (p.y - vi.y) - (p.x - vi.x) * (vj.y - vi.y);
                    if cross < 0.0 {
                        winding = winding - 1;
                    }
                }
            }
        }

        if winding != 0 {
            return 1.0;
        } else {
            return 0.0;
        }
    }

    #[vertex]
    fn vs_main(
        #[location(0)] position: Vec2,
        // Start colour for gradients, the colour otherwise
        #[location(1)] color: Vec4,
        // End colour for gradients
        #[location(2)] end_color: Vec4,
        #[location(3)] uv: Vec2,
        // Linear (x1, y1, x2, y2); radial (cx, cy, rx, ry), in the path's bounding box space
        #[location(4)] gradient_params: Vec4,
        #[location(5)] gradient_type: u32,
        // Distance to the nearest edge
        #[location(6)] edge_distance: f32,
        // Per-vertex clip, so paths with different clips can share one draw
        #[location(7)] clip_bounds: Vec4,
        #[location(8)] clip_radius: Vec4,
        #[location(9)] clip_type: u32,
        #[location(10)] clip_shape: u32,
    ) -> VertexOutput {
        let mut out: VertexOutput;

        let p = Vec3::new(position.x, position.y, 1.0);
        let transformed = Vec3::new(
            dot(PATH.transform_row0.xyz, p),
            dot(PATH.transform_row1.xyz, p),
            dot(PATH.transform_row2.xyz, p)
        );

        out.screen_pos = transformed.xy;

        let clip_pos = Vec2::new(
            (transformed.x / PATH.viewport_size.x) * 2.0 - 1.0,
            1.0 - (transformed.y / PATH.viewport_size.y) * 2.0
        );

        out.position = Vec4::new(clip_pos.x, clip_pos.y, 0.0, 1.0);
        out.color = color;
        out.end_color = end_color;
        out.uv = uv;
        out.gradient_params = gradient_params;
        out.gradient_type = gradient_type;
        out.edge_distance = edge_distance;
        out.v_clip_bounds = clip_bounds;
        out.v_clip_radius = clip_radius;
        out.v_clip_type = clip_type;
        out.v_clip_shape = clip_shape;

        return out;
    }

    // A box blur of the backdrop around `uv`, for the glass effect. Level 0 explicitly: the glass
    // branch is chosen per vertex, which is not uniform control flow, where an implicit-derivative
    // sample is not allowed.
    fn sample_blur(uv: Vec2, blur_radius: f32, viewport_size: Vec2) -> Vec4 {
        let pixel_size = Vec2::splat(1.0) / viewport_size;
        let mut total = Vec4::splat(0.0);
        let mut samples = 0.0;

        let sample_radius = clamp(blur_radius * 0.1, 1.0, 3.0) as i32;
        for x in (-sample_radius)..(sample_radius + 1i32) {
            for y in (-sample_radius)..(sample_radius + 1i32) {
                let offset = Vec2::new(x as f32, y as f32) * pixel_size * blur_radius * 0.5;
                let sample_uv = clamp(uv + offset, Vec2::splat(0.0), Vec2::splat(1.0));
                total = total + textureSampleLevel(BACKDROP_TEXTURE, BACKDROP_SAMPLER, sample_uv, 0.0);
                samples = samples + 1.0;
            }
        }

        return total / samples;
    }

    fn adjust_saturation(color: Vec3, saturation: f32) -> Vec3 {
        let gray = dot(color, Vec3::new(0.299, 0.587, 0.114));
        return mix(Vec3::splat(gray), color, saturation);
    }

    // The gradient and image lookups use level 0 explicitly for the same reason: the brush is
    // chosen by a per-vertex value. Both textures have a single level.
    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        // The clip comes from the vertex, so several paths with different clips share one draw.
        let clip_alpha = calculate_clip_alpha(
            input.screen_pos,
            input.v_clip_bounds,
            input.v_clip_radius,
            input.v_clip_type
        ) * abi_shape_clip_alpha(input.screen_pos, input.v_clip_shape as f32);

        if clip_alpha < 0.001 {
            discard!();
        }

        let mut color: Vec4;

        if PATH.use_glass_effect == 1u32 {
            // Glass: the blurred backdrop, its saturation adjusted, tinted.
            let screen_uv = input.screen_pos / PATH.viewport_size;
            let blur_radius = PATH.glass_params.x;
            let saturation = PATH.glass_params.y;
            let tint_strength = PATH.glass_params.z;
            let glass_opacity = PATH.glass_params.w;

            let mut backdrop = sample_blur(screen_uv, blur_radius, PATH.viewport_size);
            let adjusted = adjust_saturation(backdrop.xyz, saturation);
            backdrop = Vec4::new(adjusted.x, adjusted.y, adjusted.z, backdrop.w);

            let tinted = mix(backdrop.xyz, PATH.glass_tint.xyz, tint_strength * PATH.glass_tint.w);
            color = Vec4::new(tinted.x, tinted.y, tinted.z, glass_opacity);
        } else if PATH.use_image_texture == 1u32 {
            // Image brush: the path's 0..1 UV mapped into the image's UV bounds, tinted.
            let uv_min = PATH.image_uv_bounds.xy;
            let uv_max = PATH.image_uv_bounds.zw;
            let image_uv = uv_min + input.uv * (uv_max - uv_min);
            color = textureSampleLevel(IMAGE_TEXTURE, IMAGE_SAMPLER, image_uv, 0.0);
            color = Vec4::new(color.x * input.color.x, color.y * input.color.y, color.z * input.color.z, color.w * input.color.w);
        } else if input.gradient_type == 0u32 {
            color = input.color;
        } else if input.gradient_type == 1u32 {
            // Linear gradient: the UV projected onto the line from start to end.
            let g_start = input.gradient_params.xy;
            let g_end = input.gradient_params.zw;
            let g_dir = g_end - g_start;
            let g_len_sq = dot(g_dir, g_dir);

            let mut t: f32;
            if g_len_sq > 0.0001 {
                let p_lin = input.uv - g_start;
                t = clamp(dot(p_lin, g_dir) / g_len_sq, 0.0, 1.0);
            } else {
                t = 0.0;
            }

            if PATH.use_gradient_texture == 1u32 {
                color = textureSampleLevel(GRADIENT_TEXTURE, GRADIENT_SAMPLER, t, 0.0);
            } else {
                color = mix(input.color, input.end_color, t);
            }
        } else {
            // Radial gradient: (cx, cy, rx, ry) in bounding box space, a radius per axis so a
            // circle stays a circle on a path whose box is not square.
            let center = input.gradient_params.xy;
            let radius_xy = max(input.gradient_params.zw, Vec2::splat(0.001));
            let d = (input.uv - center) / radius_xy;
            let t = clamp(length(d), 0.0, 1.0);

            if PATH.use_gradient_texture == 1u32 {
                color = textureSampleLevel(GRADIENT_TEXTURE, GRADIENT_SAMPLER, t, 0.0);
            } else {
                color = mix(input.color, input.end_color, t);
            }
        }

        // Opacity and clip. No edge-distance anti-aliasing: tessellated vertices lie on the edge,
        // where the distance is zero.
        color.w = color.w * PATH.opacity * clip_alpha;
        return color;
    }
}
