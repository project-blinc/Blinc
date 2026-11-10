//! Weighted blended order-independent transparency, the composite pass (McGuire and Bavoil 2013).
//! It reads the accumulation target (the sum of weighted premultiplied colour and alpha) and the
//! reveal target (the product of `1 - alpha`) and outputs the average colour with the coverage as
//! alpha, which a SRC_ALPHA / ONE_MINUS_SRC_ALPHA blend lays over the HDR content:
//! `final = avg_color * (1 - reveal) + hdr * reveal`.

russl::russl! {
    use super::fullscreen::{FullscreenOut, fullscreen_vertex};

    #[group(0)] #[binding(0)] static ACCUM_TEX: Texture2D<f32>;
    #[group(0)] #[binding(1)] static REVEAL_TEX: Texture2D<f32>;
    #[group(0)] #[binding(2)] static COMPOSITE_SAMPLER: Sampler;

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vi: u32) -> FullscreenOut {
        return fullscreen_vertex(vi, 0.0);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: FullscreenOut) -> Vec4 {
        let accum = ACCUM_TEX.sample(COMPOSITE_SAMPLER, input.uv);
        let reveal = REVEAL_TEX.sample(COMPOSITE_SAMPLER, input.uv).x;

        // `reveal` starts at 1.0 and only fragments that passed the alpha test reduce it, so no
        // coverage means no layer touched this pixel: skip it rather than blend a zero colour.
        let coverage = 1.0 - reveal;
        if coverage < 1e-4 {
            discard!();
        }

        let avg_color = accum.xyz / max(accum.w, 1e-4);
        return Vec4::new(avg_color, coverage);
    }
}
