//! Layer effect: an outer glow. It finds the distance to the nearest opaque pixel by sampling
//! concentric rings, gives full intensity inside `range` and a Gaussian-like falloff over `blur`
//! beyond it, and composites the glow behind the original content.

russl::russl! {
    use super::layer_quad::{LayerQuadOut, layer_quad_vertex};

    struct GlowUniforms {
        color: Vec4,
        // Blur softness (the falloff's smoothness)
        blur: f32,
        // How far the glow extends at full intensity
        range: f32,
        opacity: f32,
        _pad0: f32,
        texel_size: Vec2,
        _pad1: Vec2,
    }

    #[group(0)] #[binding(0)] static UNIFORMS: Uniform<GlowUniforms>;
    #[group(0)] #[binding(1)] static SOURCE_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(2)] static SOURCE_SAMPLER: Sampler;

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vertex_index: u32) -> LayerQuadOut {
        return layer_quad_vertex(vertex_index);
    }

    // The minimum distance to an opaque pixel within `search_radius`.
    fn find_edge_distance(uv: Vec2, search_radius: f32, texel_size: Vec2) -> f32 {
        // An opaque centre is at distance 0.
        let center = SOURCE_TEXTURE.sample_level(SOURCE_SAMPLER, uv, 0.0);
        if center.w > 0.5 {
            return 0.0;
        }

        // Concentric rings, 8 by 8 samples at most, with an early exit.
        let mut min_dist = search_radius + 1.0;
        let num_angles = 8;
        let num_rings = 8;

        for ring in 1..(num_rings + 1) {
            let dist = ((ring as f32) / (num_rings as f32)) * search_radius;
            let pixel_dist = dist;

            for i in 0..num_angles {
                let angle = (i as f32) * 6.28318530718 / (num_angles as f32);
                let offset = Vec2::new(cos(angle), sin(angle)) * pixel_dist * texel_size;
                let sample_uv = clamp(uv + offset, Vec2::splat(0.0), Vec2::splat(1.0));
                let s = SOURCE_TEXTURE.sample_level(SOURCE_SAMPLER, sample_uv, 0.0);

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
    fn fs_glow(input: LayerQuadOut) -> Vec4 {
        // Total search distance = blur + range
        let search_radius = UNIFORMS.blur + UNIFORMS.range;

        let dist = find_edge_distance(input.uv, search_radius, UNIFORMS.texel_size);

        // Distance 0 is inside the shape (no glow); up to `range` is full intensity; beyond it the
        // glow fades over `blur`.
        let mut glow_alpha = 0.0;

        if dist > 0.0 && dist <= search_radius {
            if dist <= UNIFORMS.range {
                glow_alpha = 1.0;
            } else {
                let fade_dist = dist - UNIFORMS.range;
                let sigma = UNIFORMS.blur * 0.5;
                glow_alpha = exp(-(fade_dist * fade_dist) / (2.0 * sigma * sigma));
            }
        }

        glow_alpha *= UNIFORMS.opacity * UNIFORMS.color.w;

        let original = SOURCE_TEXTURE.sample_level(SOURCE_SAMPLER, input.uv, 0.0);

        // The glow behind the original, Porter-Duff over: `original + glow * (1 - original.a)`.
        let glow_rgb = UNIFORMS.color.xyz;
        let result_a = original.w + glow_alpha * (1.0 - original.w);

        if result_a < 0.001 {
            return Vec4::new(0.0, 0.0, 0.0, 0.0);
        }

        let result_rgb = (original.xyz * original.w + glow_rgb * glow_alpha * (1.0 - original.w)) / result_a;

        return Vec4::new(result_rgb, result_a);
    }
}
