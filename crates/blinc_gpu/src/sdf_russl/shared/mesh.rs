//! The PBR mesh shader: Cook-Torrance BRDF with metallic, roughness, emissive and occlusion
//! textures, directional lights, shadow mapping, normal mapping, parallax displacement, skeletal
//! skinning, morph targets and image-based ambient light. It has two fragment entries: `fs_main`
//! writes the colour, `fs_main_oit` the weighted blended order-independent transparency targets.
//!
//! Per-texel metallic, roughness, emissive and occlusion samples multiply the scalar factors of
//! the material. A missing texture is a 1x1 white default, so the multiplication is the identity.

russl::russl! {
    use super::mesh_common::{
        MESH, MATERIAL, BASE_TEXTURE, BASE_SAMPLER, NORMAL_TEXTURE, METALLIC_ROUGHNESS_TEXTURE,
        EMISSIVE_TEXTURE, OCCLUSION_TEXTURE, ENV_CUBEMAP, ENV_SAMPLER, VertexInput, VertexOutput,
        sample_shadow, d_ggx, g_smith, f_schlick, parallax_mapping,
    };

    #[group(0)] #[binding(8)] static JOINT_MATRICES: Storage<[Mat4]>;
    // Morph-target data, two vec4s for each (target, vertex): the position delta then the normal
    // delta, flattened as `(target * morph_vertex_count + vertex) * 2`. A target without authored
    // normals carries zero normal deltas.
    #[group(0)] #[binding(14)] static MORPH_DELTAS: Storage<[Vec4]>;
    // One weight for each morph target, updated every frame.
    #[group(0)] #[binding(15)] static MORPH_WEIGHTS: Storage<[f32]>;

    fn compute_skin_matrix(joints: UVec4, weights: Vec4) -> Mat4 {
        return JOINT_MATRICES[joints.x] * weights.x
             + JOINT_MATRICES[joints.y] * weights.y
             + JOINT_MATRICES[joints.z] * weights.z
             + JOINT_MATRICES[joints.w] * weights.w;
    }

    #[vertex]
    fn vs_main(input: VertexInput, #[builtin(vertex_index)] vertex_idx: u32) -> VertexOutput {
        let mut out: VertexOutput;

        let mut pos_local = input.position;
        let mut nrm_local = input.normal;

        // Morph targets blend against the rest pose, before skinning, as glTF says. A mesh without
        // morph data has a count of zero, and the loop runs no iterations.
        for t in 0u32..MESH.morph_target_count {
            let w = MORPH_WEIGHTS[t];
            if w == 0.0 {
                continue;
            }
            let base = (t * MESH.morph_vertex_count + vertex_idx) * 2u32;
            let dp = MORPH_DELTAS[base];
            let dn = MORPH_DELTAS[base + 1u32];
            pos_local = pos_local + w * dp.xyz;
            nrm_local = nrm_local + w * dn.xyz;
        }
        // Accumulated linear deltas do not keep a unit length. The tangent is left alone: no
        // tangent deltas are fed in.
        if MESH.morph_target_count > 0u32 {
            nrm_local = normalize(nrm_local);
        }

        let mut position = Vec4::new(pos_local, 1.0);
        let mut normal = Vec4::new(nrm_local, 0.0);
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

        // The position in the light's space, for shadow mapping.
        out.shadow_pos = MESH.light_view_proj * world_pos;

        return out;
    }

    // The result of the shared shading body: `rgb_premult` is the premultiplied colour (reflected
    // light times alpha, plus emissive), `alpha` the surface alpha after the mask and opaque
    // branch, and `emissive_rgb` the emissive light on its own, so the transparency path can keep
    // it un-attenuated through transparent layers.
    struct ShadedFragment {
        rgb_premult: Vec3,
        alpha: f32,
        emissive_rgb: Vec3,
        // The raw unlit base colour when `material.unlit > 0.5`, which only that branch uses.
        unlit_rgb: Vec3,
        // 1 for an unlit material, whose `unlit_rgb` the callers use in place of `rgb_premult`.
        is_unlit: f32,
    }

    // The lighting body both fragment entries call. A `discard` here leaves both of them.
    fn shade(input: VertexOutput) -> ShadedFragment {
        // KHR_texture_transform, the identity when the material has none. Every sample below
        // takes it, parallax occlusion included.
        let m = MATERIAL.uv_transform_matrix;
        let mut uv = Vec2::new(
            m.x * input.uv.x + m.y * input.uv.y,
            m.z * input.uv.x + m.w * input.uv.y,
        ) + MATERIAL.uv_transform_offset;

        // The tangent-space basis; its inverse is its transpose.
        let N = normalize(input.world_normal);
        let T = normalize(input.world_tangent);
        let B = cross(N, T) * input.tangent_handedness;
        let TBN = Mat3::from_cols(T, B, N);
        let TBN_inv = transpose(TBN);

        if MESH.displacement_scale > 0.0 {
            let V_world = normalize(MESH.camera_pos - input.world_pos);
            let V_tangent = TBN_inv * V_world;
            uv = parallax_mapping(uv, V_tangent, MESH.displacement_scale);
            // A UV displaced out of 0..1 is discarded.
            if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
                discard!();
            }
        }

        // The base colour: the material's factor times the vertex colour and, when a texture is
        // bound, the texel, as glTF and KHR_materials_unlit expect.
        let mut base_color = MATERIAL.base_color * input.vertex_color;
        if MESH.has_texture > 0.5 {
            let tex_color = textureSample(BASE_TEXTURE, BASE_SAMPLER, uv);
            base_color = base_color * tex_color;
        }

        // The alpha mode: mask discards below the cutoff, opaque and mask force the alpha to 1,
        // blend passes the sampled alpha through. Ahead of the unlit return, so unlit materials
        // get it too.
        if MATERIAL.alpha_mode < 0.5 {
            base_color.w = 1.0;
        } else if MATERIAL.alpha_mode < 1.5 {
            if base_color.w < MATERIAL.alpha_cutoff { discard!(); }
            base_color.w = 1.0;
        }

        if MATERIAL.unlit > 0.5 {
            let mut out: ShadedFragment;
            out.rgb_premult = base_color.xyz * base_color.w;
            out.alpha = base_color.w;
            out.emissive_rgb = Vec3::splat(0.0);
            out.unlit_rgb = base_color.xyz;
            out.is_unlit = 1.0;
            return out;
        }

        // Normal mapping from the red and green channels, with the blue reconstructed: the same
        // for RGBA8 maps and for two-channel BC5 maps. The max guards the square root against a
        // rounding error.
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

        // glTF metallic-roughness: blue is metallic, green roughness, and the occlusion
        // texture's red is the occlusion. The material's scalars multiply the texels.
        let mut metallic = MATERIAL.metallic_roughness.x;
        let mut roughness = MATERIAL.metallic_roughness.y;
        if MATERIAL.has_metallic_roughness_texture > 0.5 {
            let mr = textureSample(METALLIC_ROUGHNESS_TEXTURE, BASE_SAMPLER, uv);
            metallic = metallic * mr.z;
            roughness = roughness * mr.y;
        }
        // GGX explodes at zero roughness; 0.04 is glTF's minimum perceptual roughness.
        roughness = clamp(roughness, 0.04, 1.0);

        let mut emissive_value = MATERIAL.emissive;
        if MATERIAL.has_emissive_texture > 0.5 {
            let em = textureSample(EMISSIVE_TEXTURE, BASE_SAMPLER, uv).xyz;
            emissive_value = emissive_value * em;
        }

        let mut ao = 1.0;
        if MATERIAL.has_occlusion_texture > 0.5 {
            let sampled_ao = textureSample(OCCLUSION_TEXTURE, BASE_SAMPLER, uv).x;
            // glTF: `mix(color, color * ao, strength)`, so a strength of 1 applies the occlusion
            // fully and 0 bypasses it.
            ao = mix(1.0, sampled_ao, MATERIAL.occlusion_strength);
        }

        // Cook-Torrance over every active directional light. `N` above is the geometric normal
        // for the tangent basis; `Nsh` is the shading normal after the normal map.
        let Nsh = shading_normal;
        let V = normalize(MESH.camera_pos - input.world_pos);
        let n_dot_v = max(dot(Nsh, V), 0.0001);

        // The base reflectance, 4% for dielectrics and the base colour for metals.
        let f0 = mix(Vec3::splat(0.04), base_color.xyz, metallic);

        let mut direct_lighting = Vec3::splat(0.0);
        // Bounded by the array, so the loop can be statically bounded; the CPU keeps the count
        // within it and the break skips the rest.
        for li in 0u32..4u32 {
            if li >= MESH.light_count { break; }
            let light = MESH.lights[li];
            let L = normalize(-light.direction);
            let H = normalize(L + V);
            let n_dot_l = max(dot(Nsh, L), 0.0);
            if n_dot_l <= 0.0 { continue; }
            let n_dot_h = max(dot(Nsh, H), 0.0);
            let v_dot_h = max(dot(V, H), 0.0);

            let d = d_ggx(n_dot_h, roughness);
            let g = g_smith(n_dot_v, n_dot_l, roughness);
            let f = f_schlick(v_dot_h, f0);

            // The specular BRDF, and the Lambertian diffuse, which is zero for metals.
            let specular_brdf = (d * g * f) / (4.0 * n_dot_l * n_dot_v + 0.0001);
            let kd = (Vec3::splat(1.0) - f) * (1.0 - metallic);
            let diffuse_brdf = kd * base_color.xyz / 3.14159265;

            direct_lighting = direct_lighting
                + (diffuse_brdf + specular_brdf) * light.color * light.intensity * n_dot_l;
        }

        // The shadow, where the light's space holds the fragment.
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

        // Ambient light from the environment cubemap, whose mip levels blur it. The diffuse term
        // is the shading normal at the blurriest level; the specular term is the reflection
        // vector at a level that grows with roughness, weighted by Fresnel.
        let max_mip = 7.0;
        let R = reflect(-V, Nsh);
        let irradiance = textureSampleLevel(ENV_CUBEMAP, ENV_SAMPLER, Nsh, max_mip).xyz;
        let prefiltered = textureSampleLevel(ENV_CUBEMAP, ENV_SAMPLER, R, roughness * max_mip).xyz;
        let env_fresnel = f_schlick(n_dot_v, f0);

        // The ambient diffuse weight is taken against the view direction, not a light's.
        let kd_ambient = (Vec3::splat(1.0) - env_fresnel) * (1.0 - metallic);
        let ambient_diffuse = kd_ambient * base_color.xyz * irradiance;
        let ambient_specular = prefiltered * env_fresnel;
        let ambient = (ambient_diffuse + ambient_specular) * ao;

        // Premultiplied output. Reflected light (ambient and direct) is attenuated by the sampled
        // alpha, since a transparent surface shows what is behind it; emissive light is not, so a
        // glow shows through a low-alpha material.
        let reflected = ambient + direct_lighting * shadow_factor;
        let mut out: ShadedFragment;
        out.rgb_premult = reflected * base_color.w + emissive_value;
        out.alpha = base_color.w;
        out.emissive_rgb = emissive_value;
        out.unlit_rgb = Vec3::splat(0.0);
        out.is_unlit = 0.0;
        return out;
    }

    #[fragment]
    #[location(0)]
    fn fs_main(input: VertexOutput) -> Vec4 {
        let s = shade(input);
        if s.is_unlit > 0.5 {
            return Vec4::new(s.unlit_rgb.x, s.unlit_rgb.y, s.unlit_rgb.z, s.alpha);
        }
        return Vec4::new(s.rgb_premult.x, s.rgb_premult.y, s.rgb_premult.z, s.alpha);
    }

    // The weighted blended OIT entry (McGuire and Bavoil 2013), writing two targets: `accum`
    // (premultiplied colour times the weight, alpha times the weight) and `reveal` (the alpha,
    // combined by a zero / one-minus-source blend into the product of `1 - alpha` over the
    // overlapping fragments). `oit_composite` divides out the accumulated weight and blends over
    // the opaque image with `reveal` as the transmission. The depth test is on and the depth write
    // off, so opaque geometry occludes and blend fragments do not occlude each other.
    struct OitOutput {
        #[location(0)] accum: Vec4,
        #[location(1)] reveal: Vec4,
    }

    // The weight of a fragment from its linear view-space depth: with NDC depth most of the range
    // maps near the far plane and stacked layers at grazing angles cancel out. Depth is scaled by
    // 20, so a character-scale scene (5 to 10 metres) lands on the cubic falloff's knee: weights
    // near 3e3 at one metre, about 60 at five, 8 at ten and 1 at twenty.
    fn oit_weight(alpha: f32, view_z: f32) -> f32 {
        let scaled = view_z / 20.0;
        let w_depth = clamp(1.0 / (1e-5 + pow(scaled, 3.0)), 1e-2, 3e3);
        return clamp(alpha * w_depth, 1e-3, 3e3);
    }

    #[fragment]
    fn fs_main_oit(input: VertexOutput) -> OitOutput {
        let s = shade(input);

        // An unlit material keeps its raw colour in `unlit_rgb`; the accumulator still takes it
        // premultiplied, so the composite divide treats both paths alike.
        let mut color = s.rgb_premult;
        if s.is_unlit > 0.5 {
            color = s.unlit_rgb * s.alpha;
        }

        // A fragment with no alpha carries no signal and would only dilute the accumulator.
        if s.alpha <= 0.001 {
            discard!();
        }

        // `clip_position.w` is 1 / clip_w in the fragment stage, and clip_w is the view depth.
        let view_z = 1.0 / max(input.clip_position.w, 1e-6);
        let w = oit_weight(s.alpha, view_z);

        let mut out: OitOutput;
        out.accum = Vec4::new(color.x * w, color.y * w, color.z * w, s.alpha * w);
        out.reveal = Vec4::new(s.alpha, 0.0, 0.0, 0.0);
        return out;
    }
}
