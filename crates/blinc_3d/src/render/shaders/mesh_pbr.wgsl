// PBR mesh shader (metallic-roughness workflow)

const PI: f32 = 3.14159265359;

struct CameraUniform {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    view_projection: mat4x4<f32>,
    inverse_view: mat4x4<f32>,
    position: vec4<f32>,
    direction: vec4<f32>,
    near_far: vec4<f32>,
}

struct ModelUniform {
    model: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
}

struct MaterialUniform {
    base_color: vec4<f32>,
    metallic: f32,
    roughness: f32,
    ao: f32,
    emissive_intensity: f32,
    emissive: vec3<f32>,
    opacity: f32,
    alpha_test: f32,
    normal_scale: f32,
    _padding: vec2<f32>,
}

struct LightUniform {
    position_or_direction: vec4<f32>,
    color: vec4<f32>,
    params: vec4<f32>,  // [type, distance, decay, shadow]
}

struct LightsUniform {
    ambient: vec4<f32>,
    num_lights: u32,
    _padding: vec3<u32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> model: ModelUniform;
@group(2) @binding(0) var<uniform> material: MaterialUniform;
@group(2) @binding(1) var albedo_texture: texture_2d<f32>;
@group(2) @binding(2) var albedo_sampler: sampler;
@group(2) @binding(3) var normal_texture: texture_2d<f32>;
@group(2) @binding(4) var normal_sampler: sampler;
@group(2) @binding(5) var metallic_roughness_texture: texture_2d<f32>;
@group(2) @binding(6) var metallic_roughness_sampler: sampler;
@group(3) @binding(0) var<uniform> lights: LightsUniform;
@group(3) @binding(1) var<storage, read> light_data: array<LightUniform>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) world_tangent: vec3<f32>,
    @location(4) world_bitangent: vec3<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let world_position = model.model * vec4<f32>(in.position, 1.0);
    out.world_position = world_position.xyz;
    out.clip_position = camera.view_projection * world_position;

    // Transform normal to world space
    out.world_normal = normalize((model.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz);

    // Transform tangent to world space
    out.world_tangent = normalize((model.model * vec4<f32>(in.tangent.xyz, 0.0)).xyz);
    out.world_bitangent = cross(out.world_normal, out.world_tangent) * in.tangent.w;

    out.uv = in.uv;

    return out;
}

// Fresnel-Schlick approximation
fn fresnel_schlick(cos_theta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

// GGX normal distribution function
fn distribution_ggx(N: vec3<f32>, H: vec3<f32>, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let NdotH = max(dot(N, H), 0.0);
    let NdotH2 = NdotH * NdotH;

    let num = a2;
    var denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return num / denom;
}

// Schlick-GGX geometry function
fn geometry_schlick_ggx(NdotV: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;

    let num = NdotV;
    let denom = NdotV * (1.0 - k) + k;

    return num / denom;
}

// Smith's geometry function
fn geometry_smith(N: vec3<f32>, V: vec3<f32>, L: vec3<f32>, roughness: f32) -> f32 {
    let NdotV = max(dot(N, V), 0.0);
    let NdotL = max(dot(N, L), 0.0);
    let ggx2 = geometry_schlick_ggx(NdotV, roughness);
    let ggx1 = geometry_schlick_ggx(NdotL, roughness);

    return ggx1 * ggx2;
}

// Cook-Torrance BRDF
fn cook_torrance_brdf(
    N: vec3<f32>,
    V: vec3<f32>,
    L: vec3<f32>,
    albedo: vec3<f32>,
    metallic: f32,
    roughness: f32
) -> vec3<f32> {
    let H = normalize(V + L);

    // Calculate reflectance at normal incidence
    var F0 = vec3<f32>(0.04);
    F0 = mix(F0, albedo, metallic);

    // Cook-Torrance BRDF
    let NDF = distribution_ggx(N, H, roughness);
    let G = geometry_smith(N, V, L, roughness);
    let F = fresnel_schlick(max(dot(H, V), 0.0), F0);

    let kS = F;
    var kD = vec3<f32>(1.0) - kS;
    kD *= 1.0 - metallic;  // Metals have no diffuse

    let numerator = NDF * G * F;
    let denominator = 4.0 * max(dot(N, V), 0.0) * max(dot(N, L), 0.0) + 0.0001;
    let specular = numerator / denominator;

    let NdotL = max(dot(N, L), 0.0);
    return (kD * albedo / PI + specular) * NdotL;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample textures
    let albedo_sample = textureSample(albedo_texture, albedo_sampler, in.uv);
    let albedo = albedo_sample.rgb * material.base_color.rgb;
    let alpha = albedo_sample.a * material.opacity;

    // Alpha test
    if (alpha < material.alpha_test) {
        discard;
    }

    // Sample metallic-roughness
    let mr_sample = textureSample(metallic_roughness_texture, metallic_roughness_sampler, in.uv);
    let metallic = mr_sample.b * material.metallic;
    let roughness = mr_sample.g * material.roughness;

    // Sample and construct normal from normal map
    let normal_sample = textureSample(normal_texture, normal_sampler, in.uv).xyz * 2.0 - 1.0;
    let scaled_normal = vec3<f32>(normal_sample.xy * material.normal_scale, normal_sample.z);
    let TBN = mat3x3<f32>(
        normalize(in.world_tangent),
        normalize(in.world_bitangent),
        normalize(in.world_normal)
    );
    let N = normalize(TBN * scaled_normal);

    // View direction
    let V = normalize(camera.position.xyz - in.world_position);

    // Accumulate lighting
    var Lo = vec3<f32>(0.0);

    // Process each light
    for (var i = 0u; i < lights.num_lights; i++) {
        let light = light_data[i];
        let light_type = u32(light.params.x);

        var L: vec3<f32>;
        var radiance: vec3<f32>;

        if (light_type == 0u) {
            // Directional light
            L = normalize(-light.position_or_direction.xyz);
            radiance = light.color.rgb * light.color.a;
        } else if (light_type == 1u) {
            // Point light
            let light_vec = light.position_or_direction.xyz - in.world_position;
            let distance = length(light_vec);
            L = normalize(light_vec);

            let attenuation = 1.0 / (1.0 + light.params.z * distance * distance);
            radiance = light.color.rgb * light.color.a * attenuation;
        } else {
            // Spot light
            let light_vec = light.position_or_direction.xyz - in.world_position;
            let distance = length(light_vec);
            L = normalize(light_vec);

            // TODO: Add spot cone attenuation
            let attenuation = 1.0 / (1.0 + light.params.z * distance * distance);
            radiance = light.color.rgb * light.color.a * attenuation;
        }

        Lo += cook_torrance_brdf(N, V, L, albedo, metallic, roughness) * radiance;
    }

    // Ambient lighting
    let ambient = lights.ambient.rgb * albedo * material.ao;

    // Emissive
    let emissive = material.emissive * material.emissive_intensity;

    // Final color
    var color = ambient + Lo + emissive;

    // HDR tone mapping (simple Reinhard)
    color = color / (color + vec3<f32>(1.0));

    // Gamma correction
    color = pow(color, vec3<f32>(1.0 / 2.2));

    return vec4<f32>(color, alpha);
}
