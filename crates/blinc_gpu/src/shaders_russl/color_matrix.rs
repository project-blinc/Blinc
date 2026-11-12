//! Layer effect: a 4x5 colour matrix, as four rows and an offset column.
//!
//! ```text
//! [R']   [m0  m1  m2  m3  m4 ]   [R]
//! [G'] = [m5  m6  m7  m8  m9 ] * [G]
//! [B']   [m10 m11 m12 m13 m14]   [B]
//! [A']   [m15 m16 m17 m18 m19]   [A]
//!                                [1]
//! ```

russl::russl! {
    use super::layer_quad::{LayerQuadOut, layer_quad_vertex};

    struct ColorMatrixUniforms {
        row0: Vec4,  // [m0,  m1,  m2,  m3 ]
        row1: Vec4,  // [m5,  m6,  m7,  m8 ]
        row2: Vec4,  // [m10, m11, m12, m13]
        row3: Vec4,  // [m15, m16, m17, m18]
        // Offset column (m4, m9, m14, m19)
        offset: Vec4,
    }

    #[group(0)] #[binding(0)] static UNIFORMS: Uniform<ColorMatrixUniforms>;
    #[group(0)] #[binding(1)] static INPUT_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(2)] static INPUT_SAMPLER: Sampler;

    #[vertex]
    fn vs_main(#[builtin(vertex_index)] vertex_index: u32) -> LayerQuadOut {
        return layer_quad_vertex(vertex_index);
    }

    #[fragment]
    #[location(0)]
    fn fs_color_matrix(input: LayerQuadOut) -> Vec4 {
        let src = INPUT_TEXTURE.sample(INPUT_SAMPLER, input.uv);

        let mut result: Vec4;
        result.x = dot(UNIFORMS.row0, src) + UNIFORMS.offset.x;
        result.y = dot(UNIFORMS.row1, src) + UNIFORMS.offset.y;
        result.z = dot(UNIFORMS.row2, src) + UNIFORMS.offset.z;
        result.w = dot(UNIFORMS.row3, src) + UNIFORMS.offset.w;

        // Clamp to valid range
        return clamp(result, Vec4::splat(0.0), Vec4::splat(1.0));
    }
}
