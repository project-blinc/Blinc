struct Params {
    screen: vec4<f32>,
    source: vec4<f32>,
    taps: vec4<f32>,
    record: array<vec4<f32>, 28>,
}
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var auxiliary: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 6>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    return vec4<f32>(corners[index], 0.0, 1.0);
}

@fragment
fn fs_rows(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(source));
    var sum = vec4<f32>(0.0);
    for (var i = 0u; i < u32(params.source.z); i += 1u) {
        let at = u32(params.taps.x) + i;
        let tap = textureLoad(auxiliary, vec2<i32>(i32(at % 1024u), i32(at / 1024u)), 0).xy;
        sum += textureSampleLevel(source, linear_sampler, (position.xy + vec2<f32>(tap.x, 0.0)) / size, 0.0) * tap.y;
    }
    return sum;
}

// A raw premultiplied copy, including transparent pixels.
@fragment
fn fs_copy(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(position.xy), 0);
}
