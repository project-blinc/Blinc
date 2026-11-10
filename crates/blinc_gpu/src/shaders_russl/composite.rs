//! The compositor's plain pass: a texture with its alpha scaled by an opacity. The blending with
//! the destination happens in the pipeline's blend state.

russl::russl! {
    use super::fullscreen::{FullscreenOut, fullscreen_vertex};

    struct CompositeUniforms {
        opacity: f32,
        blend_mode: u32,
        _padding: Vec2,
    }

    #[group(0)] #[binding(0)] static UNIFORMS: Uniform<CompositeUniforms>;
    #[group(0)] #[binding(1)] static SOURCE_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(2)] static SOURCE_SAMPLER: Sampler;

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vertex_index: u32) -> FullscreenOut {
        return fullscreen_vertex(vertex_index, 0.0);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: FullscreenOut) -> Vec4 {
        let color = SOURCE_TEXTURE.sample(SOURCE_SAMPLER, input.uv);

        let mut result = color;
        result.w *= UNIFORMS.opacity;

        return result;
    }
}
