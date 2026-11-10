//! The SDF shadow shader. Tier 0 reads the instance fields it needs from the storage buffer; the
//! other tiers have no storage buffer in the vertex stage and read them from the vertex buffer.

russl::russl! {
    use super::access::prim;
    use super::shadow::{VertexOutput, shadow_vertex, shadow_fragment};

    #[pipeline] const TIER: u32 = 0;

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
        #if TIER >= 1
            #[location(0)] instance_bounds: Vec4,
            #[location(1)] instance_shadow: Vec4,
        #endif
    ) -> VertexOutput {
        #if TIER == 0
            let p = prim(instance_index);
            let instance_bounds = p.bounds;
            let instance_shadow = p.shadow;
        #endif
        return shadow_vertex(vertex_index, instance_index, instance_bounds, instance_shadow);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        return shadow_fragment(input);
    }
}
