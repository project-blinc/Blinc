//! The liquid glass shader, Apple's vibrancy styles: a blurred, refracted backdrop with an edge
//! bevel, a light-based or solid border highlight, a tint and a drop shadow. `GLASS_SIMPLE` is the
//! frosted variant without the liquid effects. `TIER` 0 reads the primitives and the auxiliary
//! data from storage buffers, 2 from textures; the glass bind group has the auxiliary data at
//! slot 4, which the specialisations set.

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

    // Error function approximation, for the Gaussian shadow.
    fn erf(x: f32) -> f32 {
        let s = sign(x);
        let a = abs(x);
        let t = 1.0 / (1.0 + 0.3275911 * a);
        let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
        return s * y;
    }

    // Gaussian shadow of a rounded rect from its SDF, so it follows the corner radii.
    fn shadow_rounded_rect(p: Vec2, origin: Vec2, size: Vec2, radius: Vec4, sigma: f32) -> f32 {
        let d = sd_rounded_rect(p, origin, size, radius);

        if sigma < 0.001 {
            if d < 0.0 { return 1.0; } else { return 0.0; }
        }

        let blur_factor = 0.5 * sqrt(2.0) * sigma;
        return 0.5 * (1.0 - erf(d / blur_factor));
    }

    fn hash(p: Vec2) -> f32 {
        let h = dot(p, Vec2::new(127.1, 311.7));
        return fract(sin(h) * 43758.5453123);
    }

    fn noise(p: Vec2) -> f32 {
        let i = floor(p);
        let f = fract(p);
        let u = f * f * (3.0 - 2.0 * f);

        return mix(
            mix(hash(i + Vec2::new(0.0, 0.0)), hash(i + Vec2::new(1.0, 0.0)), u.x),
            mix(hash(i + Vec2::new(0.0, 1.0)), hash(i + Vec2::new(1.0, 1.0)), u.x),
            u.y
        );
    }

    fn gaussian_weight(x: f32, sigma: f32) -> f32 {
        return exp(-(x * x) / (2.0 * sigma * sigma));
    }

    // Alpha of the rectangular clip region of a scroll container: one pixel of soft edge.
    fn calculate_glass_clip_alpha(p: Vec2, clip_bounds: Vec4) -> f32 {
        if clip_bounds.x < -5000.0 {
            return 1.0;
        }

        let clip_min = clip_bounds.xy;
        let clip_max = clip_bounds.xy + clip_bounds.zw;

        let d_left = p.x - clip_min.x;
        let d_right = clip_max.x - p.x;
        let d_top = p.y - clip_min.y;
        let d_bottom = clip_max.y - p.y;

        let d = min(min(d_left, d_right), min(d_top, d_bottom));

        return clamp(d + 0.5, 0.0, 1.0);
    }

    // Blur of the backdrop around `uv` with samples on a golden-angle spiral, clamped to the clip.
    // The blur radius is the Gaussian's standard deviation, as in CSS. Level 0 explicitly: the
    // fragment stage discards shadow-only fragments first, which makes the control flow
    // non-uniform, where an implicit-derivative sample is not allowed.
    fn blur_backdrop_clipped(uv: Vec2, blur_radius: f32, clip_bounds: Vec4) -> Vec4 {
        let clip_min = clip_bounds.xy / GLASS_UNIFORMS.sample_size;
        let clip_max = (clip_bounds.xy + clip_bounds.zw) / GLASS_UNIFORMS.sample_size;
        let has_clip = clip_bounds.x > -5000.0;

        if blur_radius < 0.5 {
            let mut clamped_uv = uv;
            if has_clip { clamped_uv = clamp(uv, clip_min, clip_max); }
            return textureSampleLevel(BACKDROP_TEXTURE, BACKDROP_SAMPLER, clamped_uv, 0.0);
        }

        let texel_size = Vec2::splat(1.0) / GLASS_UNIFORMS.sample_size;
        let sigma = blur_radius;

        // The centre sample first, then 6 rings of 12 samples out to 2.5 sigma.
        let mut center_uv = uv;
        if has_clip { center_uv = clamp(uv, clip_min, clip_max); }
        let mut color = textureSampleLevel(BACKDROP_TEXTURE, BACKDROP_SAMPLER, center_uv, 0.0);
        let mut total_weight = 1.0;

        let golden_angle = 2.39996323;
        let sample_extent = blur_radius * 2.5;
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

    fn adjust_saturation(color: Vec3, saturation: f32) -> Vec3 {
        let luminance = dot(color, Vec3::new(0.299, 0.587, 0.114));
        return mix(Vec3::splat(luminance), color, saturation);
    }

    // The SDF's gradient: the unit normal pointing outward from the shape.
    fn sdf_gradient(p: Vec2, origin: Vec2, size: Vec2, radius: Vec4) -> Vec2 {
        let eps = 0.5;
        let d = sd_rounded_rect(p, origin, size, radius);
        let dx = sd_rounded_rect(p + Vec2::new(eps, 0.0), origin, size, radius) - d;
        let dy = sd_rounded_rect(p + Vec2::new(0.0, eps), origin, size, radius) - d;
        let g = Vec2::new(dx, dy);
        let len = length(g);
        if len < 0.001 {
            return Vec2::new(0.0, -1.0);
        }
        return g / len;
    }

    // Liquid glass: a smooth rounded bevel at the edge, where the backdrop bends along the edge
    // normal, over a flat frosted centre, with a highlight line along the rim.
    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        let prim = glass_primitive(input.instance_index);
        let p = input.uv;

        // The clip first: a fragment outside it is discarded.
        let mut clip_alpha = 1.0;
        if GLASS_UNIFORMS.clip_in_image < 0.5 {
            let mut shape = 0.0;
            if prim.clip_radius.w < 0.0 { shape = prim.clip_radius.x; }
            clip_alpha = calculate_glass_clip_alpha(p, prim.clip_bounds) * abi_shape_clip_alpha(p, shape);
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

        // A wide edge keeps the triangle seam from showing on Vulkan.
        let d = sd_rounded_rect(p, origin, size, prim.corner_radius);
        let aa = 1.5;

        let mask = (1.0 - smoothstep(-aa, aa, d)) * clip_alpha;

        // The drop shadow: a soft rectangle behind the glass, with no bevel or refraction.
        let has_shadow = shadow_opacity > 0.001 && shadow_blur > 0.001;
        let mut shadow_color_premult = Vec4::splat(0.0);

        if has_shadow {
            let shadow_origin = origin + Vec2::new(shadow_offset_x, shadow_offset_y);
            let shadow_alpha = shadow_rounded_rect(p, shadow_origin, size, prim.corner_radius, shadow_blur);
            shadow_color_premult = Vec4::new(0.0, 0.0, 0.0, shadow_alpha * shadow_opacity * clip_alpha);

            // Outside the glass panel only the shadow is drawn.
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
        let glass_type = prim.type_info.x;

        // Simple frosted glass (type 5): blur, tint, saturation and brightness, with no
        // refraction, bevel or reflection.
        if glass_type == 5u32 {
            let simple_backdrop = blur_backdrop_clipped(input.screen_uv, blur_radius, prim.clip_bounds);

            let mut result_rgb = adjust_saturation(simple_backdrop.xyz, saturation);
            result_rgb = result_rgb * brightness;

            // The tint is a light additive overlay, so the backdrop colours stay visible.
            let tint = prim.tint_color;
            if tint.w > 0.001 {
                result_rgb = result_rgb + tint.xyz * tint.w * 0.5;
            }

            if noise_amount > 0.0 {
                let n = noise(p * 0.3);
                result_rgb = result_rgb + Vec3::splat((n - 0.5) * noise_amount * 0.02);
            }

            result_rgb = clamp(result_rgb, Vec3::splat(0.0), Vec3::splat(1.0));

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

        // Distance from the edge, positive going inward.
        let inner_dist = max(0.0, -d);

        // Two layers: an edge bevel with strong light bending, and a flat undistorted centre.
        let edge_thickness = min(25.0, min(size.x, size.y) * 0.2);
        let edge_progress = clamp(inner_dist / edge_thickness, 0.0, 1.0);
        let bevel = 1.0 - edge_progress;

        // Refraction follows the edge normal, strongest at the outer edge, fading quadratically.
        let edge_normal = sdf_gradient(p, origin, size, prim.corner_radius);
        let refract_strength = bevel * bevel;

        // The refraction multiplier is the bits of `type_info.w`. Zero bits mean unset, which is
        // full refraction; a set sign bit marks an explicit value, taken as its magnitude.
        let refraction_mult = f32::from_bits(prim.type_info.w);
        let is_explicitly_set = (prim.type_info.w & 0x80000000u32) != 0u32;
        let explicit_value = abs(refraction_mult);
        let mut effective_refract_mult = 1.0;
        if is_explicitly_set { effective_refract_mult = explicit_value; }

        // The backdrop is sampled from outside the shape, so content looks pulled inward at the
        // rim: up to 60 pixels of displacement.
        let refract_pixels = refract_strength * 60.0 * effective_refract_mult;
        let refract_offset = edge_normal * refract_pixels;
        let refracted_uv = input.screen_uv + refract_offset / GLASS_UNIFORMS.sample_size;

        // The blur is the user's radius, clamped to the clip so a scroll container's content
        // outside it is not sampled.
        let effective_blur = blur_radius;
        let blurred = blur_backdrop_clipped(refracted_uv, effective_blur, prim.clip_bounds);
        let adjusted = adjust_saturation(blurred.xyz, saturation);
        let backdrop = adjusted * brightness;

        let mut result = backdrop;

        // The rim: a thin line, with the border colour when its alpha is set and otherwise a
        // highlight that depends on the angle to the light.
        let edge_line_width = prim.params2.x;
        let light_angle = prim.params2.y;
        let bc = prim.border_color;

        let edge_line = smoothstep(0.0, edge_line_width * 0.3, inner_dist) *
                        (1.0 - smoothstep(edge_line_width, edge_line_width * 1.5, inner_dist));

        let light_dir = Vec2::new(cos(light_angle), sin(light_angle));
        let facing_light = dot(edge_normal, -light_dir);
        let light_factor = 0.2 + 0.8 * max(0.0, facing_light);
        let highlight_strength = edge_line * 0.6 * light_factor * mask;

        if bc.w > 0.001 {
            result = mix(result, bc.xyz, edge_line * bc.w * mask);
        } else {
            result = result + Vec3::splat(highlight_strength);
        }

        // A very subtle shadow just inside the highlight.
        let shadow_start = edge_line_width * 2.5;
        let shadow_end = edge_line_width * 8.0;
        let inner_shadow = smoothstep(shadow_start, shadow_end, inner_dist) *
                           (1.0 - smoothstep(shadow_end, shadow_end * 3.0, inner_dist));
        result = result - Vec3::splat(inner_shadow * 0.04 * mask);

        // An almost invisible tint.
        let tint = prim.tint_color;
        let tint_strength = tint.w * 0.08;
        result = mix(result, tint.xyz, tint_strength);

        if noise_amount > 0.0 {
            let n = noise(p * 0.3);
            result = result + Vec3::splat((n - 0.5) * noise_amount * 0.005);
        }

        // The glass types adjust the highlight.
        match glass_type {
            0u32 => {
                // Ultra thin: even more transparent.
                result = mix(backdrop, result, 0.7);
            }
            3u32 => {
                // Thick: a stronger edge highlight.
                result = result + Vec3::splat(highlight_strength * 0.3);
            }
            4u32 => {
                // Chrome: a slight metallic tint.
                let chrome = Vec3::new(0.96, 0.97, 0.99);
                result = mix(result, chrome, 0.1);
            }
            _ => {}
        }

        result = clamp(result, Vec3::splat(0.0), Vec3::splat(1.0));

        // The glass over its shadow: standard alpha compositing.
        if has_shadow && shadow_color_premult.w > 0.001 {
            let shadow_contrib = shadow_color_premult.w * (1.0 - mask);
            let final_alpha = mask + shadow_contrib;
            if final_alpha > 0.001 {
                let final_rgb = (result * mask + shadow_color_premult.xyz * shadow_contrib) / final_alpha;
                return Vec4::new(final_rgb.x, final_rgb.y, final_rgb.z, final_alpha);
            }
        }

        return Vec4::new(result.x, result.y, result.z, mask);
    }
}
