// ABI notch distances stay in logical pixels, including the fixed join depths.
// Native notches keep their existing distance and shadow functions.

fn abi_notch_edge(p: vec2<f32>, distance: f32, cx: f32, base: f32, m: vec4<f32>, dir: f32, into: f32) -> f32 {
    if m.x <= 0.5 || m.y <= 0.001 || m.z <= 0.001 { return distance; }
    let half_w = m.y * 0.5;
    let q = vec2<f32>(p.x, (p.y - base) * dir);
    if m.x < 1.5 {
        let radius = min(half_w, m.z);
        let cy = m.z - radius;
        let disk = max(length(q - vec2<f32>(cx, cy)) - radius, cy - q.y);
        var hollow = disk;
        if cy > 0.001 {
            hollow = min(sd_rounded_rect(q, vec2<f32>(cx - half_w, 0.0), vec2<f32>(m.y, cy), vec4<f32>(0.0)), disk);
        }
        return smax(distance, -hollow, max(m.w, 0.001));
    }
    if m.x < 2.5 {
        let radius = (half_w * half_w + m.z * m.z) / max(2.0 * m.z, 0.001);
        var side = abs(q.x - cx) - half_w;
        if q.y < 0.0 { side = -1e5; }
        let cap = max(max(length(q - vec2<f32>(cx, radius - m.z)) - radius, q.y - into), side);
        return smin(distance, cap, max(m.w, 0.001));
    }
    if m.x < 3.5 {
        return smax(distance, -sd_triangle(q, vec2<f32>(cx - half_w, 0.0), vec2<f32>(cx, m.z), vec2<f32>(cx + half_w, 0.0)), 1.5);
    }
    let spread = half_w * (m.z + into) / m.z;
    let peak = max(sd_triangle(q, vec2<f32>(cx - spread, into), vec2<f32>(cx, -m.z), vec2<f32>(cx + spread, into)), abs(q.x - cx) - half_w);
    return smin(distance, peak, 1.5);
}

fn abi_notch_flare(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, centre: vec2<f32>, radii: vec2<f32>) -> f32 {
    let ellipse = (length((p - centre) / max(radii, vec2<f32>(0.001))) - 1.0) * min(radii.x, radii.y);
    return max(sd_rounded_rect(p, origin, size, vec4<f32>(0.0)), -ellipse);
}

fn sd_abi_notch(p: vec2<f32>, size: vec2<f32>, r: vec4<f32>, types: vec4<f32>, top: vec4<f32>, bottom: vec4<f32>) -> f32 {
    let concave = types > vec4<f32>(0.5);
    let offsets = select(vec4<f32>(0.0), r, concave);
    let left = max(offsets.x, offsets.w);
    let right = max(offsets.y, offsets.z);
    let top_h = select(0.0, top.z, (top.x > 1.5 && top.x < 2.5) || (top.x > 3.5 && top.x < 4.5));
    let bottom_h = select(0.0, bottom.z, (bottom.x > 1.5 && bottom.x < 2.5) || (bottom.x > 3.5 && bottom.x < 4.5));
    let top_offset = max(max(offsets.x, offsets.y), top_h);
    let bottom_offset = max(max(offsets.w, offsets.z), bottom_h);
    let origin = vec2<f32>(left, top_offset);
    let body = max(size - vec2<f32>(left + right, top_offset + bottom_offset), vec2<f32>(0.001));
    let radii = select(r, vec4<f32>(0.0), concave);
    var d = sd_rounded_rect(p, origin, body, radii);
    let end = origin + body;
    let into = min(body.x, body.y) * 0.5;
    let join = min(8.0, into);
    let room = max(size.y - top_offset - bottom_offset, 0.0);
    if concave.x {
        let ry = min(r.x, room);
        d = min(d, abi_notch_flare(p, vec2<f32>(0.0, origin.y), vec2<f32>(left + join, ry), vec2<f32>(0.0, origin.y + ry), vec2<f32>(left, ry)));
    }
    if concave.y {
        let ry = min(r.y, room);
        let width = size.x - end.x;
        d = min(d, abi_notch_flare(p, vec2<f32>(end.x - join, origin.y), vec2<f32>(width + join, ry), vec2<f32>(size.x, origin.y + ry), vec2<f32>(width, ry)));
    }
    if concave.z {
        let ry = min(r.z, room);
        let width = size.x - end.x;
        d = min(d, abi_notch_flare(p, vec2<f32>(end.x - join, end.y - ry), vec2<f32>(width + join, ry), vec2<f32>(size.x, end.y - ry), vec2<f32>(width, ry)));
    }
    if concave.w {
        let ry = min(r.w, room);
        d = min(d, abi_notch_flare(p, vec2<f32>(0.0, end.y - ry), vec2<f32>(left + join, ry), vec2<f32>(0.0, end.y - ry), vec2<f32>(left, ry)));
    }
    d = abi_notch_edge(p, d, size.x * 0.5, origin.y, top, 1.0, into);
    return abi_notch_edge(p, d, size.x * 0.5, end.y, bottom, -1.0, into);
}
