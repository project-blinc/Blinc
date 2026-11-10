//! The SDF 3D shader for the tiers whose vertex stage has no storage buffer: it reads the
//! per-instance fields it needs from the vertex buffer.

russl::russl! {
    use super::three_d::{VertexOutput, three_d_vertex, three_d_fragment};

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
        #[location(0)] instance_bounds: Vec4,
        #[location(1)] instance_shadow: Vec4,
        #[location(2)] instance_rotation: Vec4,
        #[location(3)] instance_perspective: Vec4,
        #[location(4)] instance_sdf_3d: Vec4,
    ) -> VertexOutput {
        return three_d_vertex(vertex_index, instance_index, instance_bounds, instance_shadow, instance_rotation, instance_perspective, instance_sdf_3d);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        return three_d_fragment(input);
    }
}
