// PBR mesh shader — Cook-Torrance BRDF with metallic/roughness/emissive/AO
// texture sampling, shadow mapping, normal mapping, parallax displacement,
// skeletal skinning.
//
// Data-texture (DT) variant: replaces the joint_matrices storage buffer
// (binding 8) with a RGBA32F texture_2d for WebGL2 compatibility.
// Each mat4x4<f32> occupies one row (y = joint index), four texels wide
// (x = 0..3, one texel per matrix row).
//
// Vertex format: position[3], normal[3], uv[2], color[4], tangent[4], joints[4], weights[4]
//
// The BRDF implementation follows glTF 2.0's metallic-roughness workflow:
//   - D  = Trowbridge-Reitz (GGX) normal distribution function
//   - G  = Smith's method with Schlick-GGX geometry term
//   - F  = Schlick's Fresnel approximation
//   - kd = (1 - F) * (1 - metallic) for energy-conserving diffuse
//
// Per-texel metallic/roughness/emissive/AO samples are multiplied against
// the scalar factors from `MaterialUniforms`. When a texture is absent the
// caller binds a 1x1 white default so the multiplication is identity —
// the shader never branches on `has_*` flags for the texture samples
// themselves, only for the scalar override fallback.

// Layout must match `Uniforms` in mesh.wgsl and `MeshUniforms` in mesh_pipeline.rs: both shader
// variants read the same uploaded buffer. This shader lights with the key light (`lights[0]`)
// only and has no morph targets.
struct DirLight {
    direction: vec3<f32>,
    intensity: f32,
    color: vec3<f32>,
    _pad: f32,
}

struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _pad: f32,
    lights: array<DirLight, 4>,
    light_count: u32,
    _pad_lc0: f32,
    _pad_lc1: f32,
    _pad_lc2: f32,
    viewport_size: vec2<f32>,
    has_texture: f32,
    has_normal_map: f32,
    shadow_enabled: f32,
    displacement_scale: f32,
    normal_scale: f32,
    has_skinning: f32,
    morph_target_count: u32,
    morph_vertex_count: u32,
    _pad_morph0: u32,
    _pad_morph1: u32,
}

struct MaterialUniforms {
    base_color: vec4<f32>,
    metallic_roughness: vec2<f32>,
    emissive: vec3<f32>,
    unlit: f32,
    // Texture presence flags: 1.0 = sample the texture and multiply,
    // 0.0 = skip the sample (the bound texture is a 1x1 default).
    has_metallic_roughness_texture: f32,
    has_emissive_texture: f32,
    has_occlusion_texture: f32,
    occlusion_strength: f32,
    // glTF alphaMode encoded as a float: 0 = Opaque, 1 = Mask, 2 = Blend.
    // Mirrors `mesh.wgsl` — the Rust side uploads the same struct
    // regardless of which shader variant is compiled.
    alpha_mode: f32,
    // Mask-mode alpha cutoff (glTF default 0.5). Mirrors the layout
    // in `mesh.wgsl` — Rust-side `MaterialGpu` uploads the same
    // struct regardless of which shader variant is compiled.
    alpha_cutoff: f32,
    _pad1: f32,
    _pad2: f32,
    // KHR_texture_transform (see mesh.wgsl for the full explanation).
    // Layout must match `MaterialGpu` in mesh_pipeline.rs exactly —
    // both shader variants share the same upload buffer.
    uv_transform_matrix: vec4<f32>,
    uv_transform_offset: vec2<f32>,
    _pad_uv: vec2<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<uniform> material: MaterialUniforms;
@group(0) @binding(2) var base_texture: texture_2d<f32>;
@group(0) @binding(3) var base_sampler: sampler;
@group(0) @binding(4) var normal_texture: texture_2d<f32>;
@group(0) @binding(5) var shadow_map: texture_depth_2d;
@group(0) @binding(6) var shadow_sampler: sampler_comparison;
@group(0) @binding(7) var displacement_texture: texture_2d<f32>;
@group(0) @binding(8) var joint_data: texture_2d<f32>;
@group(0) @binding(9) var metallic_roughness_texture: texture_2d<f32>;
@group(0) @binding(10) var emissive_texture: texture_2d<f32>;
@group(0) @binding(11) var occlusion_texture: texture_2d<f32>;
@group(0) @binding(12) var env_cubemap: texture_cube<f32>;
@group(0) @binding(13) var env_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) tangent: vec4<f32>,
    @location(5) joints: vec4<u32>,
    @location(6) weights: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) vertex_color: vec4<f32>,
    @location(4) world_tangent: vec3<f32>,
    @location(5) tangent_handedness: f32,
    @location(6) shadow_pos: vec4<f32>,
}

// --- Skinning (data-texture variant) -----------------------------------------

fn load_joint_matrix(index: u32) -> mat4x4<f32> {
    let y = i32(index);
    let r0 = textureLoad(joint_data, vec2<i32>(0, y), 0);
    let r1 = textureLoad(joint_data, vec2<i32>(1, y), 0);
    let r2 = textureLoad(joint_data, vec2<i32>(2, y), 0);
    let r3 = textureLoad(joint_data, vec2<i32>(3, y), 0);
    return mat4x4<f32>(r0, r1, r2, r3);
}

fn compute_skin_matrix(joints: vec4<u32>, weights: vec4<f32>) -> mat4x4<f32> {
    return load_joint_matrix(joints.x) * weights.x
         + load_joint_matrix(joints.y) * weights.y
         + load_joint_matrix(joints.z) * weights.z
         + load_joint_matrix(joints.w) * weights.w;
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Apply skeletal skinning if enabled
    var position = vec4(input.position, 1.0);
    var normal = vec4(input.normal, 0.0);
    var tangent_dir = vec4(input.tangent.xyz, 0.0);

    if uniforms.has_skinning > 0.5 {
        let skin = compute_skin_matrix(input.joints, input.weights);
        position = skin * position;
        normal = skin * normal;
        tangent_dir = skin * tangent_dir;
    }

    let world_pos = uniforms.model * position;
    out.clip_position = uniforms.view_proj * world_pos;
    out.world_pos = world_pos.xyz;

    let normal_mat = mat3x3(
        uniforms.model[0].xyz,
        uniforms.model[1].xyz,
        uniforms.model[2].xyz,
    );
    out.world_normal = normalize(normal_mat * normal.xyz);
    out.world_tangent = normalize(normal_mat * tangent_dir.xyz);
    out.tangent_handedness = input.tangent.w;
    out.uv = input.uv;
    out.vertex_color = input.color;

    // Light-space position for shadow mapping
    out.shadow_pos = uniforms.light_view_proj * world_pos;

    return out;
}

// --- Shadow sampling with PCF ------------------------------------------------

fn sample_shadow(shadow_coord: vec3<f32>) -> f32 {
    // 4-tap PCF for soft shadow edges. Uses textureSampleCompareLevel
    // (explicit LOD 0) instead of textureSampleCompare because this
    // function is called from non-uniform control flow (the shadow_uv
    // bounds check in fs_main). Chrome's WebGPU validator rejects
    // implicit-LOD texture sampling in non-uniform flow.
    let texel_size = 1.0 / 2048.0;
    var shadow = 0.0;
    let offsets = array<vec2<f32>, 4>(
        vec2(-texel_size, -texel_size),
        vec2( texel_size, -texel_size),
        vec2(-texel_size,  texel_size),
        vec2( texel_size,  texel_size),
    );
    for (var i = 0u; i < 4u; i++) {
        shadow += textureSampleCompareLevel(
            shadow_map, shadow_sampler,
            shadow_coord.xy + offsets[i],
            shadow_coord.z - 0.002
        );
    }
    return shadow * 0.25;
}

// --- Cook-Torrance BRDF components -------------------------------------------

/// GGX / Trowbridge-Reitz normal distribution function. Describes how
/// microfacet normals cluster around the macro-surface normal at a
/// given roughness. `roughness = 0` -> delta spike (mirror);
/// `roughness = 1` -> hemispherical lobe (matte).
fn d_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let n_dot_h2 = n_dot_h * n_dot_h;
    let denom = n_dot_h2 * (a2 - 1.0) + 1.0;
    return a2 / (3.14159265 * denom * denom);
}

/// Schlick-GGX geometry term for a single direction. Combined with
/// itself for `G = G1(V) * G1(L)` (Smith's method, height-uncorrelated).
fn g_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    // Direct-lighting remap (Disney): k = (r + 1)^2 / 8
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_v / (n_dot_v * (1.0 - k) + k);
}

/// Smith's method geometry term combining view and light attenuation.
fn g_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return g_schlick_ggx(n_dot_v, roughness) * g_schlick_ggx(n_dot_l, roughness);
}

/// Schlick's Fresnel approximation. `F0` is the base reflectance at
/// normal incidence — `vec3(0.04)` for dielectrics, `base_color` for
/// pure metals. Returns the reflectance at the given half-vector angle.
fn f_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// --- Parallax occlusion mapping ----------------------------------------------

fn parallax_mapping(uv: vec2<f32>, view_dir_ts: vec3<f32>, scale: f32) -> vec2<f32> {
    let num_layers = 16.0;
    let layer_depth = 1.0 / num_layers;
    let delta_uv = view_dir_ts.xy * scale / (view_dir_ts.z * num_layers);

    var current_uv = uv;
    var current_depth = 0.0;
    // Use textureSampleLevel (explicit LOD 0) instead of textureSample
    // inside the loop — textureSample requires implicit derivatives
    // which aren't available in non-uniform control flow (the break
    // condition depends on the texture value). WebGPU validators
    // reject textureSample here; native drivers silently accept it
    // but produce undefined LOD selection.
    var current_height = textureSampleLevel(displacement_texture, base_sampler, current_uv, 0.0).r;

    for (var i = 0u; i < 16u; i++) {
        if current_depth >= current_height {
            break;
        }
        current_uv -= delta_uv;
        current_height = textureSampleLevel(displacement_texture, base_sampler, current_uv, 0.0).r;
        current_depth += layer_depth;
    }

    // Interpolate between last two layers for smoother result
    let prev_uv = current_uv + delta_uv;
    let after_depth = current_height - current_depth;
    let before_depth = textureSampleLevel(displacement_texture, base_sampler, prev_uv, 0.0).r
                       - current_depth + layer_depth;
    let weight = after_depth / (after_depth - before_depth);
    return mix(current_uv, prev_uv, weight);
}

// Shared shading body — returns the same premultiplied vec4 the
// original `fs_main` used to return. Factored out so `fs_main_oit`
// can reuse it (WGSL / naga forbid calling one entry point from
// another; helpers must be plain functions).
fn shade_dt(input: VertexOutput) -> vec4<f32> {
    // Apply KHR_texture_transform — see mesh.wgsl for the rationale.
    // Every downstream `textureSample(... , uv)` picks this up.
    let m = material.uv_transform_matrix;
    var uv = vec2<f32>(
        m.x * input.uv.x + m.y * input.uv.y,
        m.z * input.uv.x + m.w * input.uv.y,
    ) + material.uv_transform_offset;

    // Build TBN matrix for tangent-space transforms
    let N = normalize(input.world_normal);
    let T = normalize(input.world_tangent);
    let B = cross(N, T) * input.tangent_handedness;
    let TBN = mat3x3(T, B, N);
    let TBN_inv = transpose(TBN);  // inverse of orthonormal basis = transpose

    // Parallax displacement
    if uniforms.displacement_scale > 0.0 {
        let V_world = normalize(uniforms.camera_pos - input.world_pos);
        let V_tangent = TBN_inv * V_world;
        uv = parallax_mapping(uv, V_tangent, uniforms.displacement_scale);
        // Discard if UV went out of [0,1] range
        if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
            discard;
        }
    }

    // Sample base color. See `mesh.wgsl` for the full rationale — we
    // multiply factor × vertex_color × tex_color so `KHR_materials_*`
    // factors compose correctly on top of authored textures.
    var base_color = material.base_color * input.vertex_color;
    if uniforms.has_texture > 0.5 {
        let tex_color = textureSample(base_texture, base_sampler, uv);
        base_color = base_color * tex_color;
    }

    // Alpha-mode branch (0 = Opaque, 1 = Mask, 2 = Blend). Must match
    // `mesh.wgsl`.
    if material.alpha_mode < 0.5 {
        base_color.a = 1.0;
    } else if material.alpha_mode < 1.5 {
        if base_color.a < material.alpha_cutoff { discard; }
        base_color.a = 1.0;
    }

    if material.unlit > 0.5 {
        return base_color;
    }

    // Normal mapping
    var shading_normal = N;
    if uniforms.has_normal_map > 0.5 {
        // Sample RG only and reconstruct Z in-shader — see mesh.wgsl
        // for the rationale. Works for both RGBA8 normal maps and
        // BC5RgUnorm uploads.
        let nm_rg = textureSample(normal_texture, base_sampler, uv).rg;
        let rg = nm_rg * 2.0 - 1.0;
        let b = sqrt(max(1.0 - dot(rg, rg), 0.0));
        var tangent_normal = vec3<f32>(rg.x, rg.y, b);
        tangent_normal.x *= uniforms.normal_scale;
        tangent_normal.y *= uniforms.normal_scale;
        shading_normal = normalize(TBN * tangent_normal);
    }

    // --- Sample per-texel PBR inputs -----------------------------------------
    //
    // glTF 2.0 metallic-roughness convention:
    //   metallicRoughnessTexture.b = metallic
    //   metallicRoughnessTexture.g = roughness
    //   occlusionTexture.r         = AO
    //
    // The scalar factors from `MaterialUniforms` multiply the per-texel
    // samples, so `metallic: 1.0, roughness: 1.0` preserves the texture
    // values unchanged while smaller scalars attenuate them globally.
    // When `has_*` is 0.0 the caller has bound a 1x1 white default so
    // the multiply is effectively a no-op and we skip the
    // `textureSample` call for clarity.
    var metallic = material.metallic_roughness.x;
    var roughness = material.metallic_roughness.y;
    if material.has_metallic_roughness_texture > 0.5 {
        let mr = textureSample(metallic_roughness_texture, base_sampler, uv);
        metallic = metallic * mr.b;
        roughness = roughness * mr.g;
    }
    // Clamp roughness away from 0 — GGX's 1/alpha^2 term explodes at
    // zero, producing NaNs and blown-out highlights. 0.04 matches the
    // glTF spec's minimum perceptual roughness.
    roughness = clamp(roughness, 0.04, 1.0);

    var emissive_value = material.emissive;
    if material.has_emissive_texture > 0.5 {
        let em = textureSample(emissive_texture, base_sampler, uv).rgb;
        emissive_value = emissive_value * em;
    }

    var ao = 1.0;
    if material.has_occlusion_texture > 0.5 {
        let sampled_ao = textureSample(occlusion_texture, base_sampler, uv).r;
        // glTF semantic: `color = mix(color, color * ao, strength)` — a
        // strength of 1 fully applies AO, 0 bypasses it.
        ao = mix(1.0, sampled_ao, material.occlusion_strength);
    }

    // --- Cook-Torrance BRDF --------------------------------------------------
    //
    // Direct lighting from the single directional sun light. IBL
    // (environment-based indirect lighting) lands in Stage 4 and
    // adds a second contribution alongside this one.
    let L = normalize(-uniforms.lights[0].direction);
    let V = normalize(uniforms.camera_pos - input.world_pos);
    let H = normalize(L + V);

    // `Nsh` is the shading normal (post-normal-map); `N` earlier in
    // the function is the raw geometric normal used to build the TBN
    // matrix. Use a distinct name here to avoid shadowing that one.
    let Nsh = shading_normal;
    let n_dot_l = max(dot(Nsh, L), 0.0);
    let n_dot_v = max(dot(Nsh, V), 0.0001);
    let n_dot_h = max(dot(Nsh, H), 0.0);
    let v_dot_h = max(dot(V, H), 0.0);

    // Base reflectance: 4% for dielectrics, full base color for metals.
    // glTF convention: F0 = mix(0.04, baseColor, metallic).
    let f0 = mix(vec3(0.04), base_color.rgb, metallic);

    // Cook-Torrance numerator: D * G * F
    let d = d_ggx(n_dot_h, roughness);
    let g = g_smith(n_dot_v, n_dot_l, roughness);
    let f = f_schlick(v_dot_h, f0);

    // Specular BRDF: (D * G * F) / (4 * NdotL * NdotV)
    // The 0.0001 clamp on n_dot_v above protects the denominator.
    let specular_brdf = (d * g * f) / (4.0 * n_dot_l * n_dot_v + 0.0001);

    // Diffuse BRDF: energy-conserving Lambertian.
    // kd = (1 - F) attenuates diffuse where specular reflects, and
    // multiplying by (1 - metallic) zeroes diffuse for pure metals —
    // metals absorb all refracted light.
    let kd = (vec3(1.0) - f) * (1.0 - metallic);
    let diffuse_brdf = kd * base_color.rgb / 3.14159265;

    // Combined direct radiance. `n_dot_l` factor is the Lambert term
    // from the rendering equation; `lights[0].intensity` is the
    // sun's irradiance in arbitrary units tuned by the caller.
    let direct_lighting =
        (diffuse_brdf + specular_brdf) * uniforms.lights[0].intensity * n_dot_l;

    // --- Shadow --------------------------------------------------------------
    var shadow_factor = 1.0;
    if uniforms.shadow_enabled > 0.5 {
        let shadow_ndc = input.shadow_pos.xyz / input.shadow_pos.w;
        let shadow_uv = vec3(
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

    // --- IBL ambient (environment cubemap reflection) ------------------------
    //
    // Sample the procedural environment cubemap for both diffuse and
    // specular indirect lighting. The cubemap has mipmaps generated at
    // decreasing resolution; sampling at `roughness * max_mip` blurs
    // the reflection proportionally — smooth glass gets a sharp
    // horizon reflection, rough metal gets a soft ambient tint.
    //
    // Diffuse: sample at the shading normal direction at the highest
    // mip (fully blurred = irradiance-like) and attenuate by kd.
    //
    // Specular: sample at the reflection vector at a mip proportional
    // to roughness, weighted by Fresnel.
    let max_mip = 7.0;
    let R = reflect(-V, Nsh);
    let irradiance = textureSampleLevel(env_cubemap, env_sampler, Nsh, max_mip).rgb;
    let prefiltered = textureSampleLevel(env_cubemap, env_sampler, R, roughness * max_mip).rgb;
    let env_fresnel = f_schlick(n_dot_v, f0);

    let ambient_diffuse = kd * base_color.rgb * irradiance;
    let ambient_specular = prefiltered * env_fresnel;
    let ambient = (ambient_diffuse + ambient_specular) * ao;

    // Premultiplied-alpha output — see `mesh.wgsl` for rationale.
    // Emissive is self-emitted light; it must survive transparency.
    let reflected = ambient + direct_lighting * shadow_factor;
    let final_rgb = reflected * base_color.a + emissive_value;
    return vec4(final_rgb, base_color.a);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return shade_dt(input);
}

// Weighted Blended OIT entry — see mesh.wgsl for the full rationale.
// Delegates lighting to `shade_dt` and reweights; the dual-MRT
// contract matches the storage-buffer path.
struct OitOutput {
    @location(0) accum: vec4<f32>,
    @location(1) reveal: vec4<f32>,
}

@fragment
fn fs_main_oit(input: VertexOutput) -> OitOutput {
    let lit = shade_dt(input);
    let alpha = lit.a;
    if alpha <= 0.001 {
        discard;
    }
    // See mesh.wgsl `oit_weight` for the tuning rationale — linear
    // view-space z (≈ `1 / clip_w`) scaled by 20 so typical
    // character-scale depths land on the cubic knee.
    let view_z = 1.0 / max(input.clip_position.w, 1e-6);
    let scaled = view_z / 20.0;
    let w_depth = clamp(1.0 / (1e-5 + pow(scaled, 3.0)), 1e-2, 3e3);
    let w = clamp(alpha * w_depth, 1e-3, 3e3);
    var out: OitOutput;
    // `lit.rgb` is already premultiplied (lit = reflected * α + emissive);
    // feeding it into accum * w gives Σ(c*α*w) as required by the WBOIT
    // composite (see oit_composite.wgsl).
    out.accum = vec4(lit.rgb * w, alpha * w);
    out.reveal = vec4(alpha, 0.0, 0.0, 0.0);
    return out;
}
