//! The SDF shadow shader for the tier with storage buffers in every stage.

russl::russl! {
    use super::access::prim;
    use super::shadow::{VertexOutput, shadow_vertex, shadow_fragment};

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
    ) -> VertexOutput {
        let p = prim(instance_index);
        return shadow_vertex(vertex_index, instance_index, p.bounds, p.shadow);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        return shadow_fragment(input);
    }
}
