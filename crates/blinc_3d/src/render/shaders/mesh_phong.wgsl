// Blinn-Phong mesh shader

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
    diffuse: vec4<f32>,
    specular: vec4<f32>,
    emissive: vec4<f32>,
    shininess: f32,
    opacity: f32,
    alpha_test: f32,
    normal_scale: f32,
}

struct LightUniform {
    position_or_direction: vec4<f32>,
    color: vec4<f32>,
    params: vec4<f32>,
}

struct LightsUniform {
    ambient: vec4<f32>,
    num_lights: u32,
    _padding: vec3<u32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> model: ModelUniform;
@group(2) @binding(0) var<uniform> material: MaterialUniform;
@group(2) @binding(1) var diffuse_texture: texture_2d<f32>;
@group(2) @binding(2) var diffuse_sampler: sampler;
@group(2) @binding(3) var specular_texture: texture_2d<f32>;
@group(2) @binding(4) var specular_sampler: sampler;
@group(2) @binding(5) var normal_texture: texture_2d<f32>;
@group(2) @binding(6) var normal_sampler: sampler;
@group(3) @binding(0) var<uniform> lights: LightsUniform;
@group(3) @binding(1) var<storage, read> light_data: array<LightUniform>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let world_position = model.model * vec4<f32>(in.position, 1.0);
    out.world_position = world_position.xyz;
    out.clip_position = camera.view_projection * world_position;
    out.world_normal = normalize((model.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz);
    out.uv = in.uv;

    return out;
}

// Blinn-Phong lighting calculation
fn blinn_phong(
    N: vec3<f32>,
    V: vec3<f32>,
    L: vec3<f32>,
    diffuse: vec3<f32>,
    specular: vec3<f32>,
    shininess: f32
) -> vec3<f32> {
    // Diffuse
    let NdotL = max(dot(N, L), 0.0);
    let diffuse_color = diffuse * NdotL;

    // Specular (Blinn-Phong)
    let H = normalize(V + L);
    let NdotH = max(dot(N, H), 0.0);
    let spec_intensity = pow(NdotH, shininess);
    let specular_color = specular * spec_intensity;

    return diffuse_color + specular_color;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample textures
    let diffuse_sample = textureSample(diffuse_texture, diffuse_sampler, in.uv);
    let diffuse = diffuse_sample.rgb * material.diffuse.rgb;
    let alpha = diffuse_sample.a * material.opacity;

    // Alpha test
    if (alpha < material.alpha_test) {
        discard;
    }

    // Sample specular
    let specular_sample = textureSample(specular_texture, specular_sampler, in.uv);
    let specular = specular_sample.rgb * material.specular.rgb;

    let N = normalize(in.world_normal);
    let V = normalize(camera.position.xyz - in.world_position);

    // Accumulate lighting
    var color = vec3<f32>(0.0);

    // Process each light
    for (var i = 0u; i < lights.num_lights; i++) {
        let light = light_data[i];
        let light_type = u32(light.params.x);

        var L: vec3<f32>;
        var attenuation: f32 = 1.0;

        if (light_type == 0u) {
            // Directional light
            L = normalize(-light.position_or_direction.xyz);
        } else if (light_type == 1u) {
            // Point light
            let light_vec = light.position_or_direction.xyz - in.world_position;
            let distance = length(light_vec);
            L = normalize(light_vec);
            attenuation = 1.0 / (1.0 + light.params.z * distance * distance);
        } else {
            // Spot light
            let light_vec = light.position_or_direction.xyz - in.world_position;
            let distance = length(light_vec);
            L = normalize(light_vec);
            attenuation = 1.0 / (1.0 + light.params.z * distance * distance);
        }

        let light_color = light.color.rgb * light.color.a * attenuation;
        color += blinn_phong(N, V, L, diffuse, specular, material.shininess) * light_color;
    }

    // Ambient
    color += lights.ambient.rgb * diffuse;

    // Emissive
    color += material.emissive.rgb * material.emissive.a;

    // Gamma correction
    color = pow(color, vec3<f32>(1.0 / 2.2));

    return vec4<f32>(color, alpha);
}
