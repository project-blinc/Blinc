// SDF 3D text billboard shader

struct CameraUniform {
    view: mat4x4<f32>,
    projection: mat4x4<f32>,
    view_projection: mat4x4<f32>,
    inverse_view: mat4x4<f32>,
    position: vec4<f32>,
    direction: vec4<f32>,
    near_far: vec4<f32>,
}

struct TextUniform {
    color: vec4<f32>,
    outline_color: vec4<f32>,
    outline_width: f32,
    softness: f32,
    _padding: vec2<f32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<uniform> text: TextUniform;
@group(1) @binding(1) var sdf_atlas: texture_2d<f32>;
@group(1) @binding(2) var atlas_sampler: sampler;

struct VertexInput {
    // Per-vertex
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    // Per-instance
    @location(2) world_pos: vec3<f32>,
    @location(3) scale: f32,
    @location(4) color: vec4<f32>,
    @location(5) glyph_bounds: vec4<f32>,  // x, y, w, h in atlas UV space
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Extract camera right and up vectors from inverse view matrix
    let camera_right = camera.inverse_view[0].xyz;
    let camera_up = camera.inverse_view[1].xyz;

    // Billboard: face camera
    let offset = (in.position.x * camera_right + in.position.y * camera_up) * in.scale;
    let world_position = in.world_pos + offset;

    out.clip_position = camera.view_projection * vec4<f32>(world_position, 1.0);

    // Map local UV to atlas UV
    out.uv = in.glyph_bounds.xy + in.uv * in.glyph_bounds.zw;
    out.color = in.color;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample SDF texture
    let dist = textureSample(sdf_atlas, atlas_sampler, in.uv).r;

    // Calculate smoothstep range based on softness
    let edge = text.softness * 0.5;

    // Main text alpha
    let text_alpha = smoothstep(0.5 - edge, 0.5 + edge, dist);

    // Outline (if outline width > 0)
    var color = in.color * text.color;

    if (text.outline_width > 0.0) {
        let outline_outer = 0.5 - text.outline_width;
        let outline_alpha = smoothstep(outline_outer - edge, outline_outer + edge, dist);

        // Blend outline with text
        color = mix(
            text.outline_color,
            color,
            text_alpha
        );
        color.a *= outline_alpha;
    } else {
        color.a *= text_alpha;
    }

    // Discard fully transparent fragments
    if (color.a < 0.01) {
        discard;
    }

    return color;
}
