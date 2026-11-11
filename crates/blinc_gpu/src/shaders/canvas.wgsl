// The canvas pass: the record's frame, the clip, the fade and the shape clip around a callback's own
// shader, which the host splices in at run time. Self-contained, because the splice happens after
// the build: the shaped-rect and ABI clip helpers are copied here from the russl modules the other
// shaders use.
fn sd_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let center = origin + half_size;
    let rel = p - center;  // Relative position from center (signed)
    let q = abs(rel) - half_size;

    // Select corner radius based on quadrant
    // radius: (top-left, top-right, bottom-right, bottom-left)
    // In screen coords: Y increases downward, so rel.y < 0 means top half
    var r: f32;
    if rel.y < 0.0 {
        // Top half (y is above center)
        if rel.x > 0.0 {
            r = radius.y; // top-right
        } else {
            r = radius.x; // top-left
        }
    } else {
        // Bottom half (y is below center)
        if rel.x > 0.0 {
            r = radius.z; // bottom-right
        } else {
            r = radius.w; // bottom-left
        }
    }

    // Clamp radius to half the minimum dimension
    r = min(r, min(half_size.x, half_size.y));

    let q_adjusted = q + vec2<f32>(r);
    return length(max(q_adjusted, vec2<f32>(0.0))) + min(max(q_adjusted.x, q_adjusted.y), 0.0) - r;
}

fn superellipse_dist(t: vec2<f32>, p: f32, r: f32) -> f32 {
    let m = max(t.x, t.y);
    if m <= 1e-20 {
        // At the corner's inner centre the arc is exactly r away, and
        // the box bounds the result anyway.
        return -r;
    }
    let a = t.x / m;
    let b = t.y / m;
    let s = pow(a, p) + pow(b, p);
    let sp = pow(s, 1.0 / p);
    let f = m * sp;
    // grad |t|_p = ((t.x/f)^(p-1), (t.y/f)^(p-1)), written against the
    // factored form so nothing divides by an underflowed f. pow(0, 0) is
    // undefined in WGSL and p == 1 would reach it on the axes, so the
    // bevel takes its own exact branch below.
    let g = vec2<f32>(pow(a / sp, p - 1.0), pow(b / sp, p - 1.0));
    return (f - 1.0) * r / max(length(g), 1e-6);
}

fn sd_shaped_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>, shape: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let center = origin + half_size;
    let rel = p - center;
    let q = abs(rel) - half_size;

    // Select corner radius and shape based on quadrant
    var r: f32;
    var n: f32;
    if rel.y < 0.0 {
        if rel.x > 0.0 {
            r = radius.y; n = shape.y;  // top-right
        } else {
            r = radius.x; n = shape.x;  // top-left
        }
    } else {
        if rel.x > 0.0 {
            r = radius.z; n = shape.z;  // bottom-right
        } else {
            r = radius.w; n = shape.w;  // bottom-left
        }
    }

    r = min(r, min(half_size.x, half_size.y));

    // Sharp box, exact. Bounds every corner shape below, which is what
    // keeps the interior continuous without a region gate.
    let box_d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0);
    let qa = q + vec2<f32>(r);
    let rr = max(r, 0.001);

    // Notch: the box less the unbounded quadrant past the step, so its
    // only edges are the step's two faces. A union of two bars
    // misreported depth near the reentrant corner.
    if n <= -100.0 {
        let nq = -qa;
        let cut = length(max(nq, vec2<f32>(0.0))) + min(max(nq.x, nq.y), 0.0);
        return max(box_d, -cut);
    }

    // Square: the box itself.
    if n >= 100.0 {
        return box_d;
    }

    // Circle: already exact, and cheaper than the general form.
    if abs(n - 1.0) < 0.01 {
        return length(max(qa, vec2<f32>(0.0))) + min(max(qa.x, qa.y), 0.0) - r;
    }

    // Scoop: a concave arc centred on the corner's TIP, carved out of the
    // box across the whole quadrant. Negating the convex superellipse
    // about the inner centre instead left the tip filled and cut a disc
    // out of the interior.
    if n < 0.0 {
        let p_exp = pow(2.0, min(abs(n), 5.0));
        let t = abs(q) / rr;
        return max(box_d, -superellipse_dist(t, p_exp, r));
    }

    // Bevel: exact, and keeps p == 1 out of the general path.
    if abs(n) < 0.01 {
        return max(box_d, (qa.x + qa.y - r) / sqrt(2.0));
    }

    // Convex superellipse, clamped to the corner quadrant and bounded by
    // the box.
    //
    // Outside the corner square the curve does not apply, and the general form below reduces
    // to these values exactly, so the interior and straight edges (nearly every fragment of a
    // large rounded rect) skip its `pow` calls. Past both axes the arc is exactly r away.
    if qa.x <= 0.0 && qa.y <= 0.0 {
        return max(box_d, -r);
    }
    if qa.x <= 0.0 || qa.y <= 0.0 {
        return box_d;
    }
    let p_exp = pow(2.0, min(abs(n), 5.0));
    let t = max(qa, vec2<f32>(0.0)) / rr;
    return max(box_d, superellipse_dist(t, p_exp, r));
}

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

// Binding-dependent helpers are composed after shader declarations by build.rs.
fn abi_clip_data(index: u32) -> vec4<f32> {
    return load_aux(index);
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
