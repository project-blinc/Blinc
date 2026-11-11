//! Fullscreen ACES filmic tonemapping with the bloom composited in. Reads the HDR texture and the
//! half-resolution bloom texture and writes tonemapped linear colour; the sRGB framebuffer does
//! the gamma encoding.

russl::russl! {
    use super::fullscreen::{FullscreenOut, fullscreen_vertex};

    #[group(0)] #[binding(0)] static HDR_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(1)] static HDR_SAMPLER: Sampler;
    #[group(0)] #[binding(2)] static BLOOM_TEXTURE: Texture2D<f32>;

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vi: u32) -> FullscreenOut {
        return fullscreen_vertex(vi, 0.0);
    }

    // ACES filmic tone mapping (Narkowicz 2015 fit).
    fn aces_filmic(x: Vec3) -> Vec3 {
        let a = 2.51;
        let b = 0.03;
        let c = 2.43;
        let d = 0.59;
        let e = 0.14;
        return clamp((x * (a * x + b)) / (x * (c * x + d) + e), Vec3::splat(0.0), Vec3::splat(1.0));
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: FullscreenOut) -> Vec4 {
        let hdr = HDR_TEXTURE.sample(HDR_SAMPLER, input.uv).xyz;
        let bloom = BLOOM_TEXTURE.sample(HDR_SAMPLER, input.uv).xyz;
        let combined = hdr + bloom * 0.4;
        let mapped = aces_filmic(combined);
        return Vec4::new(mapped, 1.0);
    }
}
