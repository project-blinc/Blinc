//! The SDF shadow shader for the tiers whose vertex stage has no storage buffer: it reads the
//! per-instance fields it needs from the vertex buffer.

russl::russl! {
    use super::shadow::{VertexOutput, shadow_vertex, shadow_fragment};

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
        #[location(0)] instance_bounds: Vec4,
        #[location(1)] instance_shadow: Vec4,
    ) -> VertexOutput {
        return shadow_vertex(vertex_index, instance_index, instance_bounds, instance_shadow);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        return shadow_fragment(input);
    }
}
