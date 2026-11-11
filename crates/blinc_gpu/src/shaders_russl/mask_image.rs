//! Layer effect: multiply a layer by a mask image, `output.a = input.a * mask_value`. Mask mode 0
//! takes the mask's alpha, mode 1 its luminance.

russl::russl! {
    use super::layer_quad::{LayerQuadOut, layer_quad_vertex};

    struct MaskUniforms {
        // 0 = alpha, 1 = luminance
        mask_mode: u32,
        _pad: Vec3,
    }

    #[group(0)] #[binding(0)] static UNIFORMS: Uniform<MaskUniforms>;
    #[group(0)] #[binding(1)] static INPUT_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(2)] static INPUT_SAMPLER: Sampler;
    #[group(0)] #[binding(3)] static MASK_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(4)] static MASK_SAMPLER: Sampler;

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vertex_index: u32) -> LayerQuadOut {
        return layer_quad_vertex(vertex_index);
    }

    #[fragment]
    #[location(0)]
    fn fs_mask(input: LayerQuadOut) -> Vec4 {
        let src = INPUT_TEXTURE.sample(INPUT_SAMPLER, input.uv);
        let mask = MASK_TEXTURE.sample(MASK_SAMPLER, input.uv);

        let mut mask_alpha: f32;
        if UNIFORMS.mask_mode == 1u32 {
            // Luminance mode: weighted RGB as alpha
            mask_alpha = dot(mask.xyz, Vec3::new(0.2126, 0.7152, 0.0722)) * mask.w;
        } else {
            // Alpha mode: the mask's alpha channel directly
            mask_alpha = mask.w;
        }

        // Multiply source by the mask value (premultiplied alpha)
        return Vec4::new(src.xyz * mask_alpha, src.w * mask_alpha);
    }
}
