// Binding-dependent helpers are composed after shader declarations by build.rs.
fn abi_clip_data(index: u32) -> vec4<f32> {
    return aux_data[index];
}

fn abi_clip_crossing(a: vec2<f32>, b: vec2<f32>, p: vec2<f32>) -> i32 {
    let side = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
    if a.y <= p.y {
        if b.y > p.y && side > 0.0 { return 1; }
    } else if b.y <= p.y && side < 0.0 { return -1; }
    return 0;
}

fn abi_clip_filled(winding: i32, even_odd: bool) -> i32 {
    return select(0, 1, select(winding != 0, (winding & 1) != 0, even_odd));
}

fn abi_clip_polygon(p: vec2<f32>, first: u32, count: u32, even_odd: bool, aa: f32) -> f32 {
    let s = aa * 0.7;
    var w0 = 0;
    var w1 = 0;
    var w2 = 0;
    var w3 = 0;
    var w4 = 0;
    var distance_sq = 1e20;
    var previous = vec2<f32>(0.0);
    // Contours are explicitly closed. A point at 1e30 separates adjacent rings.
    for (var i = 0u; i < count; i = i + 1u) {
        let data = abi_clip_data(first + i / 2u);
        let v = select(data.xy, data.zw, (i & 1u) != 0u);
        if i > 0u && v.x < 1e29 && previous.x < 1e29 {
            let edge = previous - v;
            let delta = p - v;
            let closest = delta - edge * clamp(dot(delta, edge) / max(dot(edge, edge), 1e-12), 0.0, 1.0);
            distance_sq = min(distance_sq, dot(closest, closest));
            w0 += abi_clip_crossing(previous, v, p);
            w1 += abi_clip_crossing(previous, v, p + vec2<f32>(s, s));
            w2 += abi_clip_crossing(previous, v, p + vec2<f32>(-s, s));
            w3 += abi_clip_crossing(previous, v, p + vec2<f32>(s, -s));
            w4 += abi_clip_crossing(previous, v, p + vec2<f32>(-s, -s));
        }
        previous = v;
    }
    let inside = abi_clip_filled(w1, even_odd) + abi_clip_filled(w2, even_odd)
        + abi_clip_filled(w3, even_odd) + abi_clip_filled(w4, even_odd);
    let distance = select(sqrt(distance_sq), -sqrt(distance_sq), abi_clip_filled(w0, even_odd) != 0);
    var alpha = 1.0 - smoothstep(-aa, aa, distance);
    if inside == 4 { alpha = 1.0; }
    if inside == 0 { alpha = 0.0; }
    return alpha;
}

fn abi_shape_clip_alpha(pixel: vec2<f32>, pointer: f32) -> f32 {
    if pointer < 0.5 { return 1.0; }
    let base = u32(pointer) - 1u;
    let frame = abi_clip_data(base);
    let info = abi_clip_data(base + 1u);
    let shape = abi_clip_data(base + 2u);
    let p = vec2<f32>(frame.x * pixel.x + frame.z * pixel.y, frame.y * pixel.x + frame.w * pixel.y) + info.xy;
    // Analytic halfPixel in the clip's frame, valid inside nonuniform control flow.
    let aa = max(0.25 * (length(frame.xy) + length(frame.zw)), 1e-6);
    if info.z > 2.5 {
        return abi_clip_polygon(p, base + u32(shape.x), u32(shape.y), shape.z > 0.5, aa);
    }
    if info.z > 1.5 {
        let distance = sd_rounded_rect(p, shape.xy, shape.zw, vec4<f32>(info.w));
        return 1.0 - smoothstep(-aa, aa, distance);
    }
    let radius = max(shape.zw, vec2<f32>(1e-4));
    let u = (p - shape.xy) / radius;
    let len = length(u);
    let distance = (len - 1.0) * len / max(length(u / radius), 1e-4);
    return 1.0 - smoothstep(-aa, aa, distance);
}
