// Group zero is borrowed for one synchronous canvas callback.
struct CanvasParams {
    screen: vec4<f32>,
    record: array<vec4<f32>, 28>,
}
@group(0) @binding(0) var<uniform> canvas: CanvasParams;
@group(0) @binding(1) var canvas_aux: texture_2d<f32>;
fn load_aux(index: u32) -> vec4<f32> {
    return textureLoad(canvas_aux, vec2<i32>(i32(index % 1024u), i32(index / 1024u)), 0);
}

fn canvas_position(local: vec2<f32>) -> vec4<f32> {
    let m = canvas.record[15];
    let pixel = (canvas.record[0].xy + vec2<f32>(m.x * local.x + m.z * local.y, m.y * local.x + m.w * local.y)) * canvas.screen.z;
    return vec4<f32>(pixel.x / canvas.screen.x * 2.0 - 1.0, 1.0 - pixel.y / canvas.screen.y * 2.0, 0.0, 1.0);
}

fn canvas_local(pixel: vec2<f32>) -> vec2<f32> {
    let m = canvas.record[15];
    let delta = pixel / canvas.screen.z - canvas.record[0].xy;
    return vec2<f32>(m.w * delta.x - m.z * delta.y, m.x * delta.y - m.y * delta.x) / (m.x * m.w - m.y * m.z);
}

@vertex
fn canvas_vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 6>(vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    return canvas_position(corners[index] * canvas.record[0].zw);
}

// CLIP_HELPERS

// The callback supplies straight colour; this helper applies record opacity once.
fn canvas_clip(pixel: vec2<f32>, color: vec4<f32>) -> vec4<f32> {
    let local = canvas_local(pixel);
    if any(local < vec2<f32>(0.0)) || any(local >= canvas.record[0].zw) { discard; }
    let m = canvas.record[15];
    let det = abs(m.x * m.w - m.y * m.z);
    let aa = max(0.25 * (length(vec2<f32>(m.w, m.y)) + length(vec2<f32>(m.z, m.x))) / (det * canvas.screen.z), 1e-6);
    let flags = u32(canvas.record[11].z);
    let n = vec4<f32>(canvas.record[11].w);
    let logical = pixel / canvas.screen.z;
    var cover = 1.0;
    if (flags & 1u) != 0u {
        let b = canvas.record[8];
        cover *= 1.0 - smoothstep(-0.75, 0.75, sd_shaped_rect(logical, b.xy, b.zw, canvas.record[9], n));
    }
    if (flags & 2u) != 0u {
        let b = canvas.record[6];
        cover *= 1.0 - smoothstep(-aa, aa, sd_shaped_rect(local, b.xy, b.zw, canvas.record[7], n));
    }
    let fade = canvas.record[21];
    let b = canvas.record[20];
    if fade.x > 0.0 { cover *= clamp((logical.y - b.y) / fade.x, 0.0, 1.0); }
    if fade.y > 0.0 { cover *= clamp((b.x + b.z - logical.x) / fade.y, 0.0, 1.0); }
    if fade.z > 0.0 { cover *= clamp((b.y + b.w - logical.y) / fade.z, 0.0, 1.0); }
    if fade.w > 0.0 { cover *= clamp((logical.x - b.x) / fade.w, 0.0, 1.0); }
    cover *= abi_shape_clip_alpha(pixel, canvas.screen.w);
    if cover < 0.001 { discard; }
    return vec4<f32>(color.rgb, color.a * cover * canvas.record[3].a);
}
