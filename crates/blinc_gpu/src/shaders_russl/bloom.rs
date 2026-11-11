//! Bloom: brightness threshold extraction and a Kawase blur, chosen by `uniforms.mode`. Mode 0
//! keeps the contribution of the pixels above `threshold`; mode 1 blurs with four diagonal taps
//! and a weighted centre, run for several iterations for a wider glow.

russl::russl! {
    use super::fullscreen::{FullscreenOut, fullscreen_vertex};

    struct BloomUniforms {
        texel_size: Vec2,
        threshold: f32,
        mode: f32,
    }

    #[group(0)] #[binding(0)] static UNIFORMS: Uniform<BloomUniforms>;
    #[group(0)] #[binding(1)] static SOURCE: Texture2D<f32>;
    #[group(0)] #[binding(2)] static SAMP: Sampler;

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vi: u32) -> FullscreenOut {
        return fullscreen_vertex(vi, 0.0);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: FullscreenOut) -> Vec4 {
        let uv = input.uv;

        if UNIFORMS.mode < 0.5 {
            // Mode 0: threshold extraction. Only bright pixels contribute to bloom; everything
            // below the threshold goes to black.
            let color = SOURCE.sample(SAMP, uv).xyz;
            let brightness = max(color.x, max(color.y, color.z));
            let soft = clamp(brightness - UNIFORMS.threshold, 0.0, 1.0);
            let contrib = soft / max(brightness, 0.001);
            return Vec4::new(color * contrib, 1.0);
        } else {
            // Mode 1: Kawase blur, four diagonal taps and a weighted centre.
            let ts = UNIFORMS.texel_size;
            let mut color = SOURCE.sample(SAMP, uv).xyz * 4.0;
            color += SOURCE.sample(SAMP, uv + Vec2::new(-ts.x, -ts.y)).xyz;
            color += SOURCE.sample(SAMP, uv + Vec2::new( ts.x, -ts.y)).xyz;
            color += SOURCE.sample(SAMP, uv + Vec2::new(-ts.x,  ts.y)).xyz;
            color += SOURCE.sample(SAMP, uv + Vec2::new( ts.x,  ts.y)).xyz;
            return Vec4::new(color / 8.0, 1.0);
        }
    }
}
