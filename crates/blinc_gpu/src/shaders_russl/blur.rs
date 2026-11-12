//! Layer effect: Kawase blur, a multi-pass approximation of a Gaussian that samples a plus
//! pattern at an offset growing with the pass, and a single-pass box blur for low quality. Both
//! have an alpha mode: 0 blurs the element (colour weighted by alpha), 1 blurs only the alpha for
//! a shadow shape, 2 (Kawase only) passes the centre sample through to restore alpha.

russl::russl! {
    use super::layer_quad::{LayerQuadOut, layer_quad_vertex};

    struct BlurUniforms {
        // Inverse texture size (1/width, 1/height)
        texel_size: Vec2,
        // Base blur radius
        radius: f32,
        // Current iteration (0, 1, 2, ...); it sets the sample offset
        iteration: u32,
        // Whether to blur alpha (1) or preserve the original alpha (0)
        blur_alpha: u32,
        _pad1: f32,
        _pad2: f32,
        _pad3: f32,
    }

    #[group(0)] #[binding(0)] static UNIFORMS: Uniform<BlurUniforms>;
    #[group(0)] #[binding(1)] static INPUT_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(2)] static INPUT_SAMPLER: Sampler;

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vertex_index: u32) -> LayerQuadOut {
        return layer_quad_vertex(vertex_index);
    }

    #[fragment]
    #[location(0)]
    fn fs_kawase_blur(input: LayerQuadOut) -> Vec4 {
        // Each pass samples at an offset scaled by the radius; more passes spread the blur further.
        let base_offset = (UNIFORMS.iteration as f32) + 0.5;
        let spread = max(UNIFORMS.radius * 0.4, 1.0);
        let offset = base_offset * spread;
        let pixel_offset = offset * UNIFORMS.texel_size;

        // Sample in a + pattern (up, down, left, right).
        let uv_up = clamp(input.uv + Vec2::new(0.0, -pixel_offset.y), Vec2::splat(0.0), Vec2::splat(1.0));
        let uv_down = clamp(input.uv + Vec2::new(0.0, pixel_offset.y), Vec2::splat(0.0), Vec2::splat(1.0));
        let uv_left = clamp(input.uv + Vec2::new(-pixel_offset.x, 0.0), Vec2::splat(0.0), Vec2::splat(1.0));
        let uv_right = clamp(input.uv + Vec2::new(pixel_offset.x, 0.0), Vec2::splat(0.0), Vec2::splat(1.0));

        let s0 = INPUT_TEXTURE.sample(INPUT_SAMPLER, input.uv);
        let s1 = INPUT_TEXTURE.sample(INPUT_SAMPLER, uv_up);
        let s2 = INPUT_TEXTURE.sample(INPUT_SAMPLER, uv_down);
        let s3 = INPUT_TEXTURE.sample(INPUT_SAMPLER, uv_left);
        let s4 = INPUT_TEXTURE.sample(INPUT_SAMPLER, uv_right);

        if UNIFORMS.blur_alpha == 0u32 {
            // CSS filter blur: all of RGBA, with the colour weighted by alpha so transparent edges
            // do not darken. The alpha-restore pass (mode 2) fixes the corners after the passes.
            let total_alpha = s0.w + s1.w + s2.w + s3.w + s4.w;
            let avg_alpha = total_alpha / 5.0;

            if avg_alpha < 0.001 {
                return Vec4::new(0.0, 0.0, 0.0, 0.0);
            }

            let weighted_rgb = s0.xyz * s0.w + s1.xyz * s1.w + s2.xyz * s2.w + s3.xyz * s3.w + s4.xyz * s4.w;
            let avg_rgb = weighted_rgb / total_alpha;

            return Vec4::new(avg_rgb, avg_alpha);
        } else if UNIFORMS.blur_alpha == 1u32 {
            // Shadow blur: only the alpha, with white RGB since the drop shadow pass colours it.
            let total_alpha = s0.w + s1.w + s2.w + s3.w + s4.w;
            let avg_alpha = total_alpha / 5.0;

            return Vec4::new(1.0, 1.0, 1.0, avg_alpha);
        } else {
            // Mode 2: the centre sample only, for the alpha-restore pass.
            return s0;
        }
    }

    // Single-pass box blur for low quality mode.
    #[fragment]
    #[location(0)]
    fn fs_box_blur(input: LayerQuadOut) -> Vec4 {
        let radius = (UNIFORMS.radius as i32);
        let center = INPUT_TEXTURE.sample(INPUT_SAMPLER, input.uv);

        if UNIFORMS.blur_alpha == 0u32 {
            // Element blur: preserve alpha, blur RGB weighted by alpha.
            let mut weighted_rgb = Vec3::splat(0.0);
            let mut total_alpha = 0.0;

            for x in -radius..(radius + 1) {
                for y in -radius..(radius + 1) {
                    let offset = Vec2::new(x as f32, y as f32) * UNIFORMS.texel_size;
                    let sample_uv = clamp(input.uv + offset, Vec2::splat(0.0), Vec2::splat(1.0));
                    let s = INPUT_TEXTURE.sample(INPUT_SAMPLER, sample_uv);
                    weighted_rgb += s.xyz * s.w;
                    total_alpha += s.w;
                }
            }

            if total_alpha < 0.001 {
                return Vec4::new(0.0, 0.0, 0.0, 0.0);
            }

            let avg_rgb = weighted_rgb / total_alpha;
            return Vec4::new(avg_rgb, center.w);
        } else {
            // Shadow blur: only the alpha, with white RGB.
            let mut total_alpha = 0.0;
            let mut samples = 0.0;

            for x in -radius..(radius + 1) {
                for y in -radius..(radius + 1) {
                    let offset = Vec2::new(x as f32, y as f32) * UNIFORMS.texel_size;
                    let sample_uv = clamp(input.uv + offset, Vec2::splat(0.0), Vec2::splat(1.0));
                    let s = INPUT_TEXTURE.sample(INPUT_SAMPLER, sample_uv);
                    total_alpha += s.w;
                    samples += 1.0;
                }
            }

            let avg_alpha = total_alpha / samples;
            return Vec4::new(1.0, 1.0, 1.0, avg_alpha);
        }
    }
}
