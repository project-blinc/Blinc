//! The particle render shader: a billboard quad for each particle, a soft circle in the fragment
//! stage. `TIER` 0 reads the particles from a storage buffer, 2 from an `RGBA32F` texture with a
//! row for each particle and a texel for each of its four vec4 fields.

russl::russl! {
    use super::particle_data::Particle;

    #[pipeline] const TIER: u32 = 0;

    pub struct RenderUniforms {
        pub view_proj: Mat4,
        pub camera_pos_fov: Vec4,
        pub camera_right_aspect: Vec4,
        pub camera_up: Vec4,
        pub viewport_config: Vec4,
    }

    struct VertexOutput {
        #[builtin(position)] position: Vec4,
        #[location(0)] uv: Vec2,
        #[location(1)] color: Vec4,
    }

    #[group(0)] #[binding(1)] static UNIFORMS: Uniform<RenderUniforms>;

    #if TIER == 2
        #[group(0)] #[binding(0)] static PARTICLE_DATA: Texture2D<f32>;

        fn particle(index: u32) -> Particle {
            let y = index as i32;
            let mut p: Particle;
            p.position_life = PARTICLE_DATA.load(IVec2::new(0, y), 0);
            p.velocity_max_life = PARTICLE_DATA.load(IVec2::new(1, y), 0);
            p.color = PARTICLE_DATA.load(IVec2::new(2, y), 0);
            p.size_rotation = PARTICLE_DATA.load(IVec2::new(3, y), 0);
            return p;
        }
    #else
        #[group(0)] #[binding(0)] static PARTICLES: Storage<[Particle]>;

        fn particle(index: u32) -> Particle {
            return PARTICLES[index];
        }
    #endif

    #[vertex]
    fn vs_main(
        #[builtin(vertex_index)] vertex_index: u32,
        #[builtin(instance_index)] instance_index: u32,
    ) -> VertexOutput {
        let mut out: VertexOutput;

        let p = particle(instance_index);

        // A dead particle goes far away in clip space.
        if p.position_life.w <= 0.0 {
            out.position = Vec4::new(0.0, 0.0, 1000.0, 1.0);
            out.uv = Vec2::splat(0.0);
            out.color = Vec4::splat(0.0);
            return out;
        }

        // The billboard quad.
        let mut quad_verts = [
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(-1.0, 1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(-1.0, 1.0),
        ];
        let mut quad_uvs = [
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 0.0),
        ];

        let local_pos = quad_verts[vertex_index];
        let size = p.size_rotation.x;

        let camera_right = UNIFORMS.camera_right_aspect.xyz;
        let camera_up = UNIFORMS.camera_up.xyz;

        // The world position with the billboard offset.
        let world_pos = p.position_life.xyz +
                        camera_right * local_pos.x * size +
                        camera_up * local_pos.y * size;

        out.position = UNIFORMS.view_proj * Vec4::new(world_pos, 1.0);
        out.uv = quad_uvs[vertex_index];
        out.color = p.color;

        return out;
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        // A circle with soft edges.
        let center = Vec2::splat(0.5);
        let dist = length(input.uv - center) * 2.0;

        let alpha = 1.0 - smoothstep(0.8, 1.0, dist);

        if alpha < 0.01 {
            discard!();
        }

        return Vec4::new(input.color.x, input.color.y, input.color.z, input.color.w * alpha);
    }
}
