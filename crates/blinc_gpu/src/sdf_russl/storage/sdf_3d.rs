//! The SDF 3D shader for the tier with storage buffers in every stage.

russl::russl! {
    use super::access::prim;
    use super::three_d::{VertexOutput, three_d_vertex, three_d_fragment};

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
    ) -> VertexOutput {
        let p = prim(instance_index);
        return three_d_vertex(vertex_index, instance_index, p.bounds, p.shadow, p.rotation, p.perspective, p.sdf_3d);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        return three_d_fragment(input);
    }
}
