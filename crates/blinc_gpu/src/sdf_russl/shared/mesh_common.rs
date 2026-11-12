//! What the mesh shaders share: the uniforms, the material, the vertex format, the bindings that do
//! not depend on the device tier, and the Cook-Torrance BRDF, shadow and parallax helpers.
//!
//! The BRDF follows glTF 2.0's metallic-roughness workflow: D is Trowbridge-Reitz (GGX), G is
//! Smith's method with the Schlick-GGX term, F is Schlick's Fresnel approximation, and the
//! diffuse term is `(1 - F) * (1 - metallic)` for energy conservation.

russl::russl! {
    pub struct DirLight {
        pub direction: Vec3,
        pub intensity: f32,
        pub color: Vec3,
        pub _pad: f32,
    }

    pub struct MeshUniforms {
        pub view_proj: Mat4,
        pub model: Mat4,
        pub light_view_proj: Mat4,
        pub camera_pos: Vec3,
        pub _pad: f32,
        // Up to 4 directional lights: `light_count` of them are active, and the shadow pass uses
        // the first (the key) alone.
        pub lights: [DirLight; 4],
        pub light_count: u32,
        pub _pad_lc0: f32,
        pub _pad_lc1: f32,
        pub _pad_lc2: f32,
        pub viewport_size: Vec2,
        pub has_texture: f32,
        pub has_normal_map: f32,
        pub shadow_enabled: f32,
        pub displacement_scale: f32,
        pub normal_scale: f32,
        pub has_skinning: f32,
        // Morph targets of this mesh: zero means none, and the vertex loop runs no iterations.
        // `morph_vertex_count` is the base mesh's vertex count, to index the flattened deltas.
        pub morph_target_count: u32,
        pub morph_vertex_count: u32,
        pub _pad_morph0: u32,
        pub _pad_morph1: u32,
    }

    pub struct MaterialUniforms {
        pub base_color: Vec4,
        pub metallic_roughness: Vec2,
        pub emissive: Vec3,
        pub unlit: f32,
        // Texture presence flags: 1 samples the texture and multiplies, 0 skips the sample (the
        // bound texture is a 1x1 default).
        pub has_metallic_roughness_texture: f32,
        pub has_emissive_texture: f32,
        pub has_occlusion_texture: f32,
        pub occlusion_strength: f32,
        // glTF alphaMode as a float: 0 opaque, 1 mask, 2 blend. Opaque and mask force the alpha to
        // 1; blend passes the sampled alpha through.
        pub alpha_mode: f32,
        // The mask mode's alpha cutoff: a sampled alpha below it is discarded.
        pub alpha_cutoff: f32,
        pub _pad1: f32,
        pub _pad2: f32,
        // KHR_texture_transform flattened to a 2x2 matrix and an offset:
        // `uv_out = M * uv_in + offset` with M = [[xx, xy], [yx, yy]] stored as (xx, xy, yx, yy).
        pub uv_transform_matrix: Vec4,
        pub uv_transform_offset: Vec2,
        pub _pad_uv: Vec2,
    }

    #[group(0)] #[binding(0)] pub static MESH: Uniform<MeshUniforms>;
    #[group(0)] #[binding(1)] pub static MATERIAL: Uniform<MaterialUniforms>;
    #[group(0)] #[binding(2)] pub static BASE_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(3)] pub static BASE_SAMPLER: Sampler;
    #[group(0)] #[binding(4)] pub static NORMAL_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(5)] pub static SHADOW_MAP: TextureDepth2D;
    #[group(0)] #[binding(6)] pub static SHADOW_SAMPLER: SamplerComparison;
    #[group(0)] #[binding(7)] pub static DISPLACEMENT_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(9)] pub static METALLIC_ROUGHNESS_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(10)] pub static EMISSIVE_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(11)] pub static OCCLUSION_TEXTURE: Texture2D<f32>;
    #[group(0)] #[binding(12)] pub static ENV_CUBEMAP: TextureCube<f32>;
    #[group(0)] #[binding(13)] pub static ENV_SAMPLER: Sampler;

    // Vertex format: position[3], normal[3], uv[2], color[4], tangent[4], joints[4], weights[4].
    pub struct VertexInput {
        #[location(0)] pub position: Vec3,
        #[location(1)] pub normal: Vec3,
        #[location(2)] pub uv: Vec2,
        #[location(3)] pub color: Vec4,
        #[location(4)] pub tangent: Vec4,
        #[location(5)] pub joints: UVec4,
        #[location(6)] pub weights: Vec4,
    }

    pub struct VertexOutput {
        #[builtin(position)] pub clip_position: Vec4,
        #[location(0)] pub world_normal: Vec3,
        #[location(1)] pub world_pos: Vec3,
        #[location(2)] pub uv: Vec2,
        #[location(3)] pub vertex_color: Vec4,
        #[location(4)] pub world_tangent: Vec3,
        #[location(5)] pub tangent_handedness: f32,
        #[location(6)] pub shadow_pos: Vec4,
    }

    // 4-tap PCF for soft shadow edges, at an explicit level: this is called from non-uniform
    // control flow, where an implicit-level sample is not allowed.
    pub fn sample_shadow(shadow_coord: Vec3) -> f32 {
        let texel_size = 1.0 / 2048.0;
        let mut shadow = 0.0;
        let mut offsets = [
            Vec2::new(-texel_size, -texel_size),
            Vec2::new(texel_size, -texel_size),
            Vec2::new(-texel_size, texel_size),
            Vec2::new(texel_size, texel_size),
        ];
        for i in 0u32..4u32 {
            shadow = shadow + textureSampleCompareLevel(
                SHADOW_MAP, SHADOW_SAMPLER,
                shadow_coord.xy + offsets[i],
                shadow_coord.z - 0.002
            );
        }
        return shadow * 0.25;
    }

    // GGX normal distribution: how microfacet normals cluster around the surface normal. Roughness
    // 0 is a mirror and 1 a matte hemispherical lobe.
    pub fn d_ggx(n_dot_h: f32, roughness: f32) -> f32 {
        let a = roughness * roughness;
        let a2 = a * a;
        let n_dot_h2 = n_dot_h * n_dot_h;
        let denom = n_dot_h2 * (a2 - 1.0) + 1.0;
        return a2 / (3.14159265 * denom * denom);
    }

    // The Schlick-GGX geometry term for one direction, with the direct-lighting remap
    // `k = (r + 1)^2 / 8`.
    pub fn g_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
        let r = roughness + 1.0;
        let k = (r * r) / 8.0;
        return n_dot_v / (n_dot_v * (1.0 - k) + k);
    }

    // Smith's method: the view and light attenuation together.
    pub fn g_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
        return g_schlick_ggx(n_dot_v, roughness) * g_schlick_ggx(n_dot_l, roughness);
    }

    // Schlick's Fresnel approximation: `f0` is the reflectance at normal incidence, 0.04 for
    // dielectrics and the base colour for metals.
    pub fn f_schlick(cos_theta: f32, f0: Vec3) -> Vec3 {
        return f0 + (Vec3::splat(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
    }

    // Parallax occlusion mapping over 16 layers. Level 0 explicitly, in the loop: the break
    // depends on the texture value, so the control flow is not uniform.
    pub fn parallax_mapping(uv: Vec2, view_dir_ts: Vec3, scale: f32) -> Vec2 {
        let num_layers = 16.0;
        let layer_depth = 1.0 / num_layers;
        let delta_uv = view_dir_ts.xy * scale / (view_dir_ts.z * num_layers);

        let mut current_uv = uv;
        let mut current_depth = 0.0;
        let mut current_height = textureSampleLevel(DISPLACEMENT_TEXTURE, BASE_SAMPLER, current_uv, 0.0).x;

        for i in 0u32..16u32 {
            if current_depth >= current_height {
                break;
            }
            current_uv = current_uv - delta_uv;
            current_height = textureSampleLevel(DISPLACEMENT_TEXTURE, BASE_SAMPLER, current_uv, 0.0).x;
            current_depth = current_depth + layer_depth;
        }

        // Interpolate between the last two layers.
        let prev_uv = current_uv + delta_uv;
        let after_depth = current_height - current_depth;
        let before_depth = textureSampleLevel(DISPLACEMENT_TEXTURE, BASE_SAMPLER, prev_uv, 0.0).x
                           - current_depth + layer_depth;
        let weight = after_depth / (after_depth - before_depth);
        return mix(current_uv, prev_uv, weight);
    }
}
