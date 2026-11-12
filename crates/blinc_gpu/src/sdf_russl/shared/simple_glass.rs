//! The simple frosted glass shader: a blurred backdrop with a tint, saturation, brightness, noise
//! and an optional border, and a drop shadow, with no refraction or liquid effects. Cheaper than
//! `glass` and meant for subtle backgrounds. `TIER` 0 reads the primitives and the auxiliary data
//! from storage buffers, 2 from textures; the bind group has the auxiliary data at slot 4, which
//! the specialisations set.

russl::russl! {
    use super::clip::abi_shape_clip_alpha;
    use super::common::sd_rounded_rect;
    use super::glass_data::{GLASS_UNIFORMS, BACKDROP_TEXTURE, BACKDROP_SAMPLER, glass_primitive};

    #[pipeline] const TIER: u32 = 0;
    #[pipeline] const AUX_SLOT: u32 = 5;

    struct VertexOutput {
        #[builtin(position)] position: Vec4,
        #[location(0)] uv: Vec2,
        #[location(1)] screen_uv: Vec2,
        #[location(2)] #[flat] instance_index: u32,
    }

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
    ) -> VertexOutput {
        let mut out: VertexOutput;

        let prim = glass_primitive(instance_index);

        // The quad grows to hold the shadow's blur and offset.
        let shadow_blur = prim.params2.z;
        let shadow_offset_x = f32::from_bits(prim.type_info.y);
        let shadow_offset_y = f32::from_bits(prim.type_info.z);
        let shadow_expand = shadow_blur * 3.0 + abs(shadow_offset_x) + abs(shadow_offset_y);

        let bounds = Vec4::new(
            prim.bounds.x - shadow_expand,
            prim.bounds.y - shadow_expand,
            prim.bounds.z + shadow_expand * 2.0,
            prim.bounds.w + shadow_expand * 2.0
        );

        // Two triangles over the bounds. A match rather than an indexed array: the PowerVR Vulkan
        // driver compiles indexing a constant array wrongly (see `sdf_core`).
        let mut local_uv: Vec2;
        match vertex_index {
            0u32 => { local_uv = Vec2::new(0.0, 0.0); }
            1u32 => { local_uv = Vec2::new(1.0, 0.0); }
            2u32 => { local_uv = Vec2::new(0.0, 1.0); }
            3u32 => { local_uv = Vec2::new(1.0, 0.0); }
            4u32 => { local_uv = Vec2::new(1.0, 1.0); }
            _ => { local_uv = Vec2::new(0.0, 1.0); }
        }
        let pos = Vec2::new(
            bounds.x + local_uv.x * bounds.z,
            bounds.y + local_uv.y * bounds.w
        );

        let clip_pos = Vec2::new(
            ((pos.x - GLASS_UNIFORMS.origin.x) / GLASS_UNIFORMS.viewport_size.x) * 2.0 - 1.0,
            1.0 - ((pos.y - GLASS_UNIFORMS.origin.y) / GLASS_UNIFORMS.viewport_size.y) * 2.0
        );

        out.position = Vec4::new(clip_pos.x, clip_pos.y, 0.0, 1.0);
        out.uv = pos;
        out.screen_uv = pos / GLASS_UNIFORMS.sample_size;
        out.instance_index = instance_index;

        return out;
    }

    fn shadow_rounded_rect(p: Vec2, origin: Vec2, size: Vec2, radius: Vec4, blur: f32) -> f32 {
        let d = sd_rounded_rect(p, origin, size, radius);
        let sigma = blur * 0.5;
        return 1.0 - smoothstep(-sigma * 2.0, sigma * 2.0, d);
    }

    fn calculate_clip_alpha(p: Vec2, clip_bounds: Vec4) -> f32 {
        let clip_min = clip_bounds.xy;
        let clip_max = clip_bounds.xy + clip_bounds.zw;
        let edge_dist = min(
            min(p.x - clip_min.x, clip_max.x - p.x),
            min(p.y - clip_min.y, clip_max.y - p.y)
        );
        return smoothstep(-0.5, 0.5, edge_dist);
    }

    fn adjust_saturation(color: Vec3, saturation: f32) -> Vec3 {
        let luminance = dot(color, Vec3::new(0.299, 0.587, 0.114));
        return mix(Vec3::splat(luminance), color, saturation);
    }

    fn gaussian_weight(x: f32, sigma: f32) -> f32 {
        return exp(-(x * x) / (2.0 * sigma * sigma));
    }

    // Blur of the backdrop around `uv` with samples on a golden-angle spiral, clamped to the clip.
    // The radius is the Gaussian's standard deviation, as in CSS. Level 0 explicitly: the fragment
    // stage discards shadow-only fragments first, which makes the control flow non-uniform, where
    // an implicit-derivative sample is not allowed.
    fn blur_backdrop(uv: Vec2, radius: f32, clip_bounds: Vec4) -> Vec4 {
        if radius < 0.5 {
            return textureSampleLevel(BACKDROP_TEXTURE, BACKDROP_SAMPLER, uv, 0.0);
        }

        let texel_size = Vec2::splat(1.0) / GLASS_UNIFORMS.sample_size;
        let sigma = radius;

        let clip_min = clip_bounds.xy / GLASS_UNIFORMS.sample_size;
        let clip_max = (clip_bounds.xy + clip_bounds.zw) / GLASS_UNIFORMS.sample_size;
        let has_clip = clip_bounds.x > -5000.0;

        // The centre sample first, then 6 rings of 12 samples out to 2.5 sigma.
        let mut center_uv = uv;
        if has_clip { center_uv = clamp(uv, clip_min, clip_max); }
        let mut color = textureSampleLevel(BACKDROP_TEXTURE, BACKDROP_SAMPLER, center_uv, 0.0);
        let mut total_weight = 1.0;

        let golden_angle = 2.39996323;
        let sample_extent = radius * 2.5;
        let num_rings = 6i32;
        let samples_per_ring = 12i32;

        for ring in 1i32..(num_rings + 1i32) {
            let ring_t = (ring as f32) / (num_rings as f32);
            let ring_radius = sample_extent * ring_t;
            let ring_offset = ring_radius * texel_size;

            for i in 0i32..samples_per_ring {
                let angle = (i as f32) * (6.283185 / (samples_per_ring as f32)) + (ring as f32) * golden_angle;
                let offset = Vec2::new(cos(angle), sin(angle)) * ring_offset;

                let mut sample_pos = uv + offset;
                if has_clip { sample_pos = clamp(sample_pos, clip_min, clip_max); }

                let weight = gaussian_weight(ring_radius, sigma);
                color = color + textureSampleLevel(BACKDROP_TEXTURE, BACKDROP_SAMPLER, sample_pos, 0.0) * weight;
                total_weight = total_weight + weight;
            }
        }

        return color / total_weight;
    }

    // Value noise for the frosted texture.
    fn noise(p: Vec2) -> f32 {
        let i = floor(p);
        let f = fract(p);
        let u = f * f * (3.0 - 2.0 * f);

        let a = fract(sin(dot(i, Vec2::new(127.1, 311.7))) * 43758.5453);
        let b = fract(sin(dot(i + Vec2::new(1.0, 0.0), Vec2::new(127.1, 311.7))) * 43758.5453);
        let c = fract(sin(dot(i + Vec2::new(0.0, 1.0), Vec2::new(127.1, 311.7))) * 43758.5453);
        let d = fract(sin(dot(i + Vec2::new(1.0, 1.0), Vec2::new(127.1, 311.7))) * 43758.5453);

        return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        let prim = glass_primitive(input.instance_index);
        let p = input.uv;

        let mut clip_alpha = 1.0;
        if GLASS_UNIFORMS.clip_in_image < 0.5 {
            let mut shape = 0.0;
            if prim.clip_radius.w < 0.0 { shape = prim.clip_radius.x; }
            clip_alpha = calculate_clip_alpha(p, prim.clip_bounds) * abi_shape_clip_alpha(p, shape);
        }
        if clip_alpha < 0.001 {
            discard!();
        }

        let origin = prim.bounds.xy;
        let size = prim.bounds.zw;

        let shadow_blur = prim.params2.z;
        let shadow_opacity = prim.params2.w;
        let shadow_offset_x = f32::from_bits(prim.type_info.y);
        let shadow_offset_y = f32::from_bits(prim.type_info.z);

        // A constant edge width keeps the triangle seam from showing on Vulkan.
        let d = sd_rounded_rect(p, origin, size, prim.corner_radius);
        let aa = 1.5;
        let mask = (1.0 - smoothstep(-aa, aa, d)) * clip_alpha;

        // The drop shadow.
        let has_shadow = shadow_opacity > 0.001 && shadow_blur > 0.001;
        let mut shadow_color_premult = Vec4::splat(0.0);

        if has_shadow {
            let shadow_origin = origin + Vec2::new(shadow_offset_x, shadow_offset_y);
            let shadow_alpha = shadow_rounded_rect(p, shadow_origin, size, prim.corner_radius, shadow_blur);
            shadow_color_premult = Vec4::new(0.0, 0.0, 0.0, shadow_alpha * shadow_opacity * clip_alpha);

            if mask < 0.001 {
                if shadow_alpha > 0.001 && clip_alpha > 0.001 {
                    return shadow_color_premult;
                }
                discard!();
            }
        } else {
            if mask < 0.001 {
                discard!();
            }
        }

        let blur_radius = prim.params.x;
        let saturation = prim.params.y;
        let brightness = prim.params.z;
        let noise_amount = prim.params.w;

        // The backdrop blurred where it is: no refraction, no distortion.
        let backdrop = blur_backdrop(input.screen_uv, blur_radius, prim.clip_bounds);

        let mut result_rgb = adjust_saturation(backdrop.xyz, saturation);
        result_rgb = result_rgb * brightness;

        // The tint is a light additive overlay.
        let tint = prim.tint_color;
        if tint.w > 0.001 {
            result_rgb = result_rgb + tint.xyz * tint.w * 0.5;
        }

        if noise_amount > 0.0 {
            let n = noise(p * 0.3);
            result_rgb = result_rgb + Vec3::splat((n - 0.5) * noise_amount * 0.02);
        }

        // A solid border colour along the rim.
        let bc = prim.border_color;
        if bc.w > 0.001 {
            let border_width = prim.params2.x;
            let inner_dist = -d;
            let edge_band = smoothstep(0.0, border_width * 0.3, inner_dist) *
                            (1.0 - smoothstep(border_width, border_width * 1.5, inner_dist));
            result_rgb = mix(result_rgb, bc.xyz, edge_band * bc.w * mask);
        }

        result_rgb = clamp(result_rgb, Vec3::splat(0.0), Vec3::splat(1.0));

        // The glass over its shadow.
        if has_shadow && shadow_color_premult.w > 0.001 {
            let shadow_contrib = shadow_color_premult.w * (1.0 - mask);
            let final_alpha = mask + shadow_contrib;
            if final_alpha > 0.001 {
                let final_rgb = (result_rgb * mask + shadow_color_premult.xyz * shadow_contrib) / final_alpha;
                return Vec4::new(final_rgb.x, final_rgb.y, final_rgb.z, final_alpha);
            }
        }

        return Vec4::new(result_rgb.x, result_rgb.y, result_rgb.z, mask);
    }
}
