//! The SDF 3D shader. Tier 0 reads the instance fields it needs from the storage buffer; the
//! other tiers have no storage buffer in the vertex stage and read them from the vertex buffer.

russl::russl! {
    use super::access::prim;
    use super::three_d::{VertexOutput, three_d_vertex, three_d_fragment};

    #[pipeline] const TIER: u32 = 0;

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
        #if TIER >= 1
            #[location(0)] instance_bounds: Vec4,
            #[location(1)] instance_shadow: Vec4,
            #[location(2)] instance_rotation: Vec4,
            #[location(3)] instance_perspective: Vec4,
            #[location(4)] instance_sdf_3d: Vec4,
        #endif
    ) -> VertexOutput {
        #if TIER == 0
            let p = prim(instance_index);
            let instance_bounds = p.bounds;
            let instance_shadow = p.shadow;
            let instance_rotation = p.rotation;
            let instance_perspective = p.perspective;
            let instance_sdf_3d = p.sdf_3d;
        #endif
        return three_d_vertex(vertex_index, instance_index, instance_bounds, instance_shadow, instance_rotation, instance_perspective, instance_sdf_3d);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        return three_d_fragment(input);
    }
}
