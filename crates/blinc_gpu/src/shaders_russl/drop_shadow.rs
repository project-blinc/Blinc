//! Layer effect: a drop shadow. It finds the distance from each (offset) pixel to the original
//! shape by sampling concentric rings, which keeps rounded corners, turns the distance into an
//! alpha with a smooth falloff, colours it and composites it behind the original.

russl::russl! {
    use super::layer_quad::{LayerQuadOut, layer_quad_vertex};

    struct DropShadowUniforms {
        // Shadow offset in pixels
        offset: Vec2,
        blur_radius: f32,
        // Spread: positive grows the shadow, negative shrinks it
        spread: f32,
        color: Vec4,
        texel_size: Vec2,
        _pad: Vec2,
    }

    #[group(0)] #[binding(0)] static UNIFORMS: Uniform<DropShadowUniforms>;
    #[group(0)] #[binding(1)] static INPUT_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(2)] static INPUT_SAMPLER: Sampler;
    // The original (unblurred) texture, for compositing.
    #[group(0)] #[binding(3)] static ORIGINAL_TEXTURE: Texture2D<f32>;

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vertex_index: u32) -> LayerQuadOut {
        return layer_quad_vertex(vertex_index);
    }

    // The minimum distance to an opaque pixel of the original within `radius`.
    fn sample_min_distance(uv: Vec2, radius: f32, texel_size: Vec2) -> f32 {
        // An opaque centre is at distance 0.
        let center = ORIGINAL_TEXTURE.sample_level(INPUT_SAMPLER, uv, 0.0);
        if center.w > 0.5 {
            return 0.0;
        }

        // Concentric rings, 8 by 8 samples at most, with an early exit.
        let mut min_dist = radius + 1.0;
        let num_angles = 8;
        let num_rings = 8;

        for ring in 1..(num_rings + 1) {
            let dist = ((ring as f32) / (num_rings as f32)) * radius;
            let pixel_dist = dist;

            for i in 0..num_angles {
                let angle = (i as f32) * 6.28318530718 / (num_angles as f32);
                let offset = Vec2::new(cos(angle), sin(angle)) * pixel_dist * texel_size;
                let sample_uv = clamp(uv + offset, Vec2::splat(0.0), Vec2::splat(1.0));
                let s = ORIGINAL_TEXTURE.sample_level(INPUT_SAMPLER, sample_uv, 0.0);

                if s.w > 0.5 {
                    min_dist = min(min_dist, dist);
                }
            }

            if min_dist <= dist {
                break;
            }
        }

        return min_dist;
    }

    #[fragment]
    #[location(0)]
    fn fs_drop_shadow(input: LayerQuadOut) -> Vec4 {
        let shadow_uv = clamp(input.uv - UNIFORMS.offset * UNIFORMS.texel_size, Vec2::splat(0.0), Vec2::splat(1.0));

        let dist = sample_min_distance(shadow_uv, UNIFORMS.blur_radius, UNIFORMS.texel_size);

        // Alpha 1 at distance 0, about 0 at `blur_radius`.
        let mut alpha = 1.0 - smoothstep(0.0, UNIFORMS.blur_radius, dist);

        if UNIFORMS.spread != 0.0 {
            let adjusted_dist = dist - UNIFORMS.spread;
            alpha = 1.0 - smoothstep(0.0, UNIFORMS.blur_radius, max(adjusted_dist, 0.0));
        }

        let shadow_a = UNIFORMS.color.w * alpha;
        let shadow_rgb = UNIFORMS.color.xyz;

        let original = ORIGINAL_TEXTURE.sample_level(INPUT_SAMPLER, input.uv, 0.0);

        // The shadow behind the original, Porter-Duff over for straight colours.
        let result_a = original.w + shadow_a * (1.0 - original.w);

        if result_a < 0.001 {
            return Vec4::new(0.0, 0.0, 0.0, 0.0);
        }

        let result_rgb = (original.xyz * original.w + shadow_rgb * shadow_a * (1.0 - original.w)) / result_a;

        return Vec4::new(result_rgb, result_a);
    }
}
