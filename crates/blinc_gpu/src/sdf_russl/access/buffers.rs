//! The bind group of the tiers whose fragment stage reads storage buffers, and how they read a
//! primitive and the auxiliary data.

russl::russl! {
    use super::primitive::{Primitive, Uniforms};

    #[group(0)] #[binding(0)] pub static UNIFORMS: Uniform<Uniforms>;
    #[group(0)] #[binding(1)] pub static PRIMITIVES: Storage<[Primitive]>;
    // Glyph atlas textures for unified text rendering
    #[group(0)] #[binding(2)] pub static GLYPH_ATLAS: Texture2D<f32>;
    #[group(0)] #[binding(3)] pub static GLYPH_SAMPLER: Sampler;
    #[group(0)] #[binding(4)] pub static COLOR_GLYPH_ATLAS: Texture2D<f32>;
    // Variable-length per-primitive data: 3D group shape descriptors, polygon clip
    // vertices, gradient stop tables.
    #[group(0)] #[binding(5)] pub static AUX_DATA: Storage<[Vec4]>;

    // Primitive `index`.
    pub fn prim(index: u32) -> Primitive {
        return PRIMITIVES[index];
    }

    // Auxiliary data vec4 `index`.
    pub fn aux(index: u32) -> Vec4 {
        return AUX_DATA[index];
    }
}
