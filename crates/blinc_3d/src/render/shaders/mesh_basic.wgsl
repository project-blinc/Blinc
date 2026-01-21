// Basic unlit mesh shader

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
    color: vec4<f32>,
    opacity: f32,
    alpha_test: f32,
    _padding: vec2<f32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> model: ModelUniform;
@group(2) @binding(0) var<uniform> material: MaterialUniform;
@group(2) @binding(1) var diffuse_texture: texture_2d<f32>;
@group(2) @binding(2) var diffuse_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let world_position = model.model * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_projection * world_position;
    out.uv = in.uv;
    out.color = material.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color;

    // Sample texture if available
    let tex_color = textureSample(diffuse_texture, diffuse_sampler, in.uv);
    color = color * tex_color;

    // Apply opacity
    color.a *= material.opacity;

    // Alpha test
    if (color.a < material.alpha_test) {
        discard;
    }

    return color;
}
