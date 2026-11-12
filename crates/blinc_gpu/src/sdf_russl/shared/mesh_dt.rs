//! The reduced PBR mesh shader for devices without storage buffers: the same material, shadow and
//! environment shading as `mesh`, with the joint matrices in an `RGBA32F` texture (a row for each
//! joint, a texel for each matrix column), lit by the key light alone and without morph targets.
//! It reads the uniform buffer `mesh` reads.

russl::russl! {
    use super::mesh_common::{
        MESH, MATERIAL, BASE_TEXTURE, BASE_SAMPLER, NORMAL_TEXTURE, METALLIC_ROUGHNESS_TEXTURE,
        EMISSIVE_TEXTURE, OCCLUSION_TEXTURE, ENV_CUBEMAP, ENV_SAMPLER, VertexInput, VertexOutput,
        sample_shadow, d_ggx, g_smith, f_schlick, parallax_mapping,
    };

    #[group(0)] #[binding(8)] static JOINT_DATA: Texture2D<f32>;

    fn load_joint_matrix(index: u32) -> Mat4 {
        let y = index as i32;
        let r0 = JOINT_DATA.load(IVec2::new(0, y), 0);
        let r1 = JOINT_DATA.load(IVec2::new(1, y), 0);
        let r2 = JOINT_DATA.load(IVec2::new(2, y), 0);
        let r3 = JOINT_DATA.load(IVec2::new(3, y), 0);
        return Mat4::from_cols(r0, r1, r2, r3);
    }

    fn compute_skin_matrix(joints: UVec4, weights: Vec4) -> Mat4 {
        return load_joint_matrix(joints.x) * weights.x
             + load_joint_matrix(joints.y) * weights.y
             + load_joint_matrix(joints.z) * weights.z
             + load_joint_matrix(joints.w) * weights.w;
    }

    #[vertex]
    fn vs_main(input: VertexInput) -> VertexOutput {
        let mut out: VertexOutput;

        let mut position = Vec4::new(input.position, 1.0);
        let mut normal = Vec4::new(input.normal, 0.0);
        let mut tangent_dir = Vec4::new(input.tangent.xyz, 0.0);

        if MESH.has_skinning > 0.5 {
            let skin = compute_skin_matrix(input.joints, input.weights);
            position = skin * position;
            normal = skin * normal;
            tangent_dir = skin * tangent_dir;
        }

        let world_pos = MESH.model * position;
        out.clip_position = MESH.view_proj * world_pos;
        out.world_pos = world_pos.xyz;

        let normal_mat = Mat3::from_cols(
            MESH.model[0].xyz,
            MESH.model[1].xyz,
            MESH.model[2].xyz,
        );
        out.world_normal = normalize(normal_mat * normal.xyz);
        out.world_tangent = normalize(normal_mat * tangent_dir.xyz);
        out.tangent_handedness = input.tangent.w;
        out.uv = input.uv;
        out.vertex_color = input.color;

        out.shadow_pos = MESH.light_view_proj * world_pos;

        return out;
    }

    // The shading body: the premultiplied colour, with the surface alpha last. An unlit material
    // returns its raw base colour.
    fn shade_dt(input: VertexOutput) -> Vec4 {
        let m = MATERIAL.uv_transform_matrix;
        let mut uv = Vec2::new(
            m.x * input.uv.x + m.y * input.uv.y,
            m.z * input.uv.x + m.w * input.uv.y,
        ) + MATERIAL.uv_transform_offset;

        let N = normalize(input.world_normal);
        let T = normalize(input.world_tangent);
        let B = cross(N, T) * input.tangent_handedness;
        let TBN = Mat3::from_cols(T, B, N);
        let TBN_inv = transpose(TBN);

        if MESH.displacement_scale > 0.0 {
            let V_world = normalize(MESH.camera_pos - input.world_pos);
            let V_tangent = TBN_inv * V_world;
            uv = parallax_mapping(uv, V_tangent, MESH.displacement_scale);
            if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
                discard!();
            }
        }

        let mut base_color = MATERIAL.base_color * input.vertex_color;
        if MESH.has_texture > 0.5 {
            let tex_color = textureSample(BASE_TEXTURE, BASE_SAMPLER, uv);
            base_color = base_color * tex_color;
        }

        if MATERIAL.alpha_mode < 0.5 {
            base_color.w = 1.0;
        } else if MATERIAL.alpha_mode < 1.5 {
            if base_color.w < MATERIAL.alpha_cutoff { discard!(); }
            base_color.w = 1.0;
        }

        if MATERIAL.unlit > 0.5 {
            return base_color;
        }

        let mut shading_normal = N;
        if MESH.has_normal_map > 0.5 {
            let nm_rg = textureSample(NORMAL_TEXTURE, BASE_SAMPLER, uv).xy;
            let rg = nm_rg * 2.0 - 1.0;
            let b = sqrt(max(1.0 - dot(rg, rg), 0.0));
            let mut tangent_normal = Vec3::new(rg.x, rg.y, b);
            tangent_normal.x = tangent_normal.x * MESH.normal_scale;
            tangent_normal.y = tangent_normal.y * MESH.normal_scale;
            shading_normal = normalize(TBN * tangent_normal);
        }

        let mut metallic = MATERIAL.metallic_roughness.x;
        let mut roughness = MATERIAL.metallic_roughness.y;
        if MATERIAL.has_metallic_roughness_texture > 0.5 {
            let mr = textureSample(METALLIC_ROUGHNESS_TEXTURE, BASE_SAMPLER, uv);
            metallic = metallic * mr.z;
            roughness = roughness * mr.y;
        }
        roughness = clamp(roughness, 0.04, 1.0);

        let mut emissive_value = MATERIAL.emissive;
        if MATERIAL.has_emissive_texture > 0.5 {
            let em = textureSample(EMISSIVE_TEXTURE, BASE_SAMPLER, uv).xyz;
            emissive_value = emissive_value * em;
        }

        let mut ao = 1.0;
        if MATERIAL.has_occlusion_texture > 0.5 {
            let sampled_ao = textureSample(OCCLUSION_TEXTURE, BASE_SAMPLER, uv).x;
            ao = mix(1.0, sampled_ao, MATERIAL.occlusion_strength);
        }

        // The key light alone, its intensity but not its colour.
        let L = normalize(-MESH.lights[0].direction);
        let V = normalize(MESH.camera_pos - input.world_pos);
        let H = normalize(L + V);

        let Nsh = shading_normal;
        let n_dot_l = max(dot(Nsh, L), 0.0);
        let n_dot_v = max(dot(Nsh, V), 0.0001);
        let n_dot_h = max(dot(Nsh, H), 0.0);
        let v_dot_h = max(dot(V, H), 0.0);

        let f0 = mix(Vec3::splat(0.04), base_color.xyz, metallic);

        let d = d_ggx(n_dot_h, roughness);
        let g = g_smith(n_dot_v, n_dot_l, roughness);
        let f = f_schlick(v_dot_h, f0);

        let specular_brdf = (d * g * f) / (4.0 * n_dot_l * n_dot_v + 0.0001);

        let kd = (Vec3::splat(1.0) - f) * (1.0 - metallic);
        let diffuse_brdf = kd * base_color.xyz / 3.14159265;

        let direct_lighting = (diffuse_brdf + specular_brdf) * MESH.lights[0].intensity * n_dot_l;

        let mut shadow_factor = 1.0;
        if MESH.shadow_enabled > 0.5 {
            let shadow_ndc = input.shadow_pos.xyz / input.shadow_pos.w;
            let shadow_uv = Vec3::new(
                shadow_ndc.x * 0.5 + 0.5,
                -shadow_ndc.y * 0.5 + 0.5,
                shadow_ndc.z,
            );
            if shadow_uv.x >= 0.0 && shadow_uv.x <= 1.0 &&
               shadow_uv.y >= 0.0 && shadow_uv.y <= 1.0 &&
               shadow_uv.z >= 0.0 && shadow_uv.z <= 1.0 {
                shadow_factor = sample_shadow(shadow_uv);
            }
        }

        // The ambient diffuse weight here is the key light's, not the view's as in `mesh`.
        let max_mip = 7.0;
        let R = reflect(-V, Nsh);
        let irradiance = textureSampleLevel(ENV_CUBEMAP, ENV_SAMPLER, Nsh, max_mip).xyz;
        let prefiltered = textureSampleLevel(ENV_CUBEMAP, ENV_SAMPLER, R, roughness * max_mip).xyz;
        let env_fresnel = f_schlick(n_dot_v, f0);

        let ambient_diffuse = kd * base_color.xyz * irradiance;
        let ambient_specular = prefiltered * env_fresnel;
        let ambient = (ambient_diffuse + ambient_specular) * ao;

        let reflected = ambient + direct_lighting * shadow_factor;
        let final_rgb = reflected * base_color.w + emissive_value;
        return Vec4::new(final_rgb.x, final_rgb.y, final_rgb.z, base_color.w);
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        return shade_dt(input);
    }

    // The weighted blended OIT entry: see `mesh`.
    struct OitOutput {
        #[location(0)] accum: Vec4,
        #[location(1)] reveal: Vec4,
    }

    #[fragment]
    fn fs_main_oit(input: VertexOutput) -> OitOutput {
        let lit = shade_dt(input);
        let alpha = lit.w;
        if alpha <= 0.001 {
            discard!();
        }

        let view_z = 1.0 / max(input.clip_position.w, 1e-6);
        let scaled = view_z / 20.0;
        let w_depth = clamp(1.0 / (1e-5 + pow(scaled, 3.0)), 1e-2, 3e3);
        let w = clamp(alpha * w_depth, 1e-3, 3e3);

        let mut out: OitOutput;
        out.accum = Vec4::new(lit.x * w, lit.y * w, lit.z * w, alpha * w);
        out.reveal = Vec4::new(alpha, 0.0, 0.0, 0.0);
        return out;
    }
}
