//! The shadow depth pass: positions transformed from a light's point of view. Only the vertex
//! stage runs, and depth is written by the pipeline.

russl::russl! {
    struct ShadowUniforms {
        light_view_proj: Mat4,
        model: Mat4,
    }

    #[group(0)] #[binding(0)] static SHADOW_UNIFORMS: Uniform<ShadowUniforms>;

    struct VertexInput {
        #[location(0)] position: Vec3,
    }

    #[vertex]
    #[builtin(position)]
    fn vs_main(input: VertexInput) -> Vec4 {
        let world_pos = SHADOW_UNIFORMS.model * Vec4::new(input.position, 1.0);
        return SHADOW_UNIFORMS.light_view_proj * world_pos;
    }
}
