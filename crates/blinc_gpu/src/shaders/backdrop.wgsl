// ABI backdrop geometry stays in logical coordinates; samples use target pixels.
struct Params {
    screen: vec4<f32>,
    source: vec4<f32>,
    taps: vec4<f32>,
    record: array<vec4<f32>, 28>,
}
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var rows: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var auxiliary: texture_2d<f32>;

fn load_aux(index: u32) -> vec4<f32> {
    return textureLoad(auxiliary, vec2<i32>(i32(index % 1024u), i32(index / 1024u)), 0);
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var corners = array<vec2<f32>, 6>(vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let m = params.record[15];
    let box = params.record[0];
    let grow = vec2<f32>(1.5) / max(vec2<f32>(length(m.xy), length(m.zw)), vec2<f32>(0.01));
    let local = corners[index] * (box.zw + grow * 2.0) - grow;
    let pixel = (box.xy + vec2<f32>(m.x * local.x + m.z * local.y, m.y * local.x + m.w * local.y)) * params.screen.z;
    return vec4<f32>(pixel.x / params.screen.x * 2.0 - 1.0, 1.0 - pixel.y / params.screen.y * 2.0, 0.0, 1.0);
}

fn box_distance(p: vec2<f32>) -> f32 {
    let corners = params.record[25];
    let top = params.record[26];
    let bottom = params.record[27];
    if dot(abs(corners), vec4<f32>(1.0)) + top.x + bottom.x > 0.0 {
        let kinds = select(vec4<f32>(0.0), vec4<f32>(1.0), corners < vec4<f32>(0.0));
        return sd_abi_notch(p, params.record[0].zw, abs(corners), kinds, top, bottom);
    }
    return sd_shaped_rect(p, vec2<f32>(0.0), params.record[0].zw, params.record[1], params.record[12]);
}

fn rim_lens(inner: f32, width: f32) -> f32 {
    let bevel = 1.0 - clamp(inner / width, 0.0, 1.0);
    return bevel * bevel;
}

fn rim_line(inner: f32, width: f32) -> f32 {
    return smoothstep(0.0, width * 0.3, inner) * (1.0 - smoothstep(width, width * 1.5, inner));
}

fn glass_environment(p: vec2<f32>) -> f32 {
    let direction = dot(p, vec2<f32>(0.55, 0.83));
    let broad = 0.5 + 0.5 * sin(direction * 7.0 + 0.8);
    let streak = 0.5 + 0.5 * sin(direction * 18.0 - 1.2);
    return 0.25 + broad * broad * 0.55 + pow(streak, 6.0) * 0.2;
}

fn grain_hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453123);
}

fn grain(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (vec2<f32>(3.0) - 2.0 * f);
    return mix(mix(grain_hash(i), grain_hash(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(grain_hash(i + vec2<f32>(0.0, 1.0)), grain_hash(i + vec2<f32>(1.0)), u.x), u.y);
}

fn rows_at(pixel: vec2<f32>) -> vec2<f32> {
    let p = clamp(pixel, vec2<f32>(0.5), params.screen.xy - vec2<f32>(0.5));
    return (p - params.source.xy) / vec2<f32>(textureDimensions(rows));
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = position.xy / params.screen.z;
    let matrix = params.record[15];
    let det = matrix.x * matrix.w - matrix.y * matrix.z;
    let delta = pixel - params.record[0].xy;
    let local = vec2<f32>(matrix.w * delta.x - matrix.z * delta.y, matrix.x * delta.y - matrix.y * delta.x) / det;
    let aa = max(0.25 * (length(vec2<f32>(matrix.w, matrix.y)) + length(vec2<f32>(matrix.z, matrix.x))) / (abs(det) * params.screen.z), 1e-6);
    let distance = box_distance(local);
    let types = params.record[11];
    let clip_n = vec4<f32>(types.w);
    let flags = u32(types.z);
    var cover = 1.0 - smoothstep(-aa, aa, distance);
    if (flags & 1u) != 0u {
        let bounds = params.record[8];
        cover *= 1.0 - smoothstep(-0.75, 0.75, sd_shaped_rect(pixel, bounds.xy, bounds.zw, params.record[9], clip_n));
    }
    if (flags & 2u) != 0u {
        let bounds = params.record[6];
        let fringe = select(aa, 0.75, all(matrix == vec4<f32>(1.0, 0.0, 0.0, 1.0)));
        cover *= 1.0 - smoothstep(-fringe, fringe, sd_shaped_rect(local, bounds.xy, bounds.zw, params.record[7], clip_n));
    }
    let fade = params.record[21];
    let bounds = params.record[20];
    if fade.x > 0.0 { cover *= clamp((pixel.y - bounds.y) / fade.x, 0.0, 1.0); }
    if fade.y > 0.0 { cover *= clamp((bounds.x + bounds.z - pixel.x) / fade.y, 0.0, 1.0); }
    if fade.z > 0.0 { cover *= clamp((bounds.y + bounds.w - pixel.y) / fade.z, 0.0, 1.0); }
    if fade.w > 0.0 { cover *= clamp((pixel.x - bounds.x) / fade.w, 0.0, 1.0); }
    cover *= abi_shape_clip_alpha(position.xy, params.source.w);
    if cover < 0.001 { discard; }
    let box = params.record[0].zw;
    let liquid = params.record[10].x > 0.5;
    let inner = max(0.0, -distance);
    let bevel = clamp(abs(params.record[10].y), 0.0, 1.0);
    let rim = max(1e-4, min(25.0, min(box.x, box.y) * 0.2) * bevel);
    var normal = vec2<f32>(0.0);
    var local_normal = vec2<f32>(0.0);
    var offset = vec2<f32>(0.0);
    var lens = 0.0;
    if liquid {
        let gradient = vec2<f32>(box_distance(local + vec2<f32>(0.5, 0.0)) - box_distance(local - vec2<f32>(0.5, 0.0)),
            box_distance(local + vec2<f32>(0.0, 0.5)) - box_distance(local - vec2<f32>(0.0, 0.5)));
        local_normal = gradient / max(length(gradient), 1e-4);
        let world = vec2<f32>(matrix.x * gradient.x + matrix.z * gradient.y, matrix.y * gradient.x + matrix.w * gradient.y);
        normal = world / max(length(world), 1e-4);
        lens = rim_lens(inner, rim);
        offset = normal * lens * 60.0 * params.record[10].y * length(matrix.xy) * params.screen.z;
    }
    let aberration = params.record[14].x;
    let dispersion = 0.12 * aberration + 0.38 * aberration * aberration * aberration;
    let split = offset * (dispersion / max(bevel, 1e-4));
    let disperse = dot(split, split) > 1e-6;
    var sum = vec4<f32>(0.0);
    var red = vec2<f32>(0.0);
    var blue = vec2<f32>(0.0);
    for (var i = 0u; i < u32(params.source.z); i += 1u) {
        let tap = load_aux(u32(params.taps.x) + i).xy;
        let at = position.xy + offset + vec2<f32>(0.0, tap.x);
        sum += textureSampleLevel(rows, linear_sampler, rows_at(at), 0.0) * tap.y;
        if disperse {
            let r = textureSampleLevel(rows, linear_sampler, rows_at(at + split), 0.0);
            let b = textureSampleLevel(rows, linear_sampler, rows_at(at - split), 0.0);
            red += r.ra * tap.y;
            blue += b.ba * tap.y;
        }
    }
    var texel = sum;
    if disperse { texel = vec4<f32>(red.x, sum.g, blue.x, max(sum.a, max(red.y, blue.y))); }
    var rgb = vec3<f32>(0.0);
    if texel.a > 1e-4 { rgb = texel.rgb / texel.a; }
    let color = vec4<f32>(rgb, 1.0);
    rgb = clamp(vec3<f32>(dot(params.record[3], color), dot(params.record[4], color), dot(params.record[5], color)), vec3<f32>(0.0), vec3<f32>(1.0));
    var tint = clamp(vec3<f32>(params.record[3].w, params.record[4].w, params.record[5].w), vec3<f32>(0.0), vec3<f32>(1.0));
    var tint_alpha = clamp(1.0 - (params.record[3].x + params.record[4].y + params.record[5].z) / 3.0, 0.0, 1.0);
    if liquid {
        let width = max(params.record[10].z, 1e-4);
        let line = rim_line(inner, width) * bevel;
        let light_shift = aberration * width * 0.75;
        let angle = params.record[10].w;
        let light = vec2<f32>(cos(angle), sin(angle)) * select(1.0, -1.0, params.record[10].y < 0.0);
        let lit_rgb = vec3<f32>(rim_line(inner + light_shift, width) * bevel, line, rim_line(inner - light_shift, width) * bevel)
            * (0.6 * (0.2 + 0.8 * max(0.0, dot(normal, -light))));
        let lit = max(lit_rgb.r, max(lit_rgb.g, lit_rgb.b));
        let edge = params.record[16];
        let shade = smoothstep(width * 2.5, width * 8.0, inner) * (1.0 - smoothstep(width * 8.0, width * 24.0, inner)) * 0.04 * bevel;
        let via = clamp(params.record[13].a, 0.0, 1.0);
        if edge.a > 0.001 {
            let k = line * edge.a;
            rgb = mix(rgb, edge.rgb, k);
            tint = tint * (1.0 - k) + edge.rgb * k;
            tint_alpha = k + tint_alpha * (1.0 - k);
        } else {
            tint = tint * (1.0 - lit) + lit_rgb;
            tint_alpha = lit + tint_alpha * (1.0 - lit);
        }
        rgb = clamp(mix(rgb - vec3<f32>(shade), params.record[13].rgb, via), vec3<f32>(0.0), vec3<f32>(1.0));
        if edge.a <= 0.001 { rgb = clamp(rgb * (1.0 - lit) + lit_rgb, vec3<f32>(0.0), vec3<f32>(1.0)); }
        tint = tint * (1.0 - shade) * (1.0 - via) + params.record[13].rgb * via;
        tint_alpha = via + (shade + tint_alpha * (1.0 - shade)) * (1.0 - via);
        if texel.a < 0.999 && lens > 0.0 {
            let span = max(min(box.x, box.y), 1.0);
            let bend = local_normal * lens * (60.0 / span) * params.record[10].y;
            let point = (local - box * 0.5) / span + bend;
            let separation = bend * (dispersion / max(bevel, 1e-4));
            let middle = glass_environment(point);
            var environment = vec3<f32>(glass_environment(point + separation), middle, glass_environment(point - separation));
            environment = mix(vec3<f32>(middle), environment, 0.35);
            let strength = (lens * (1.0 - lens) * 4.0 * 0.18 + lens * 0.06) * bevel;
            let environment_alpha = max(environment.r, max(environment.g, environment.b)) * strength;
            tint = tint * (1.0 - environment_alpha) + environment * strength;
            tint_alpha = environment_alpha + tint_alpha * (1.0 - environment_alpha);
        }
        let band = lens * (1.0 - lens) * 4.0 * 0.12 * bevel;
        tint *= 1.0 - band;
        tint_alpha = band + tint_alpha * (1.0 - band);
        let spectrum = vec3<f32>(rim_lens(inner + aberration * 0.6, rim), lens, rim_lens(inner - aberration * 0.6, rim));
        let reflected = 0.85 * max(0.0, dot(normal, -light)) + 0.35 * max(0.0, dot(normal, light));
        let neutral_spec = lens * lens * reflected;
        let spec_rgb = mix(vec3<f32>(neutral_spec), spectrum * spectrum * reflected, 0.35) * bevel;
        let spec = max(spec_rgb.r, max(spec_rgb.g, spec_rgb.b));
        tint = tint * (1.0 - spec) + spec_rgb;
        tint_alpha = max(spec + tint_alpha * (1.0 - spec), 0.04);
    }
    let alpha = texel.a + tint_alpha * (1.0 - texel.a);
    if alpha > 1e-4 { rgb = (rgb * texel.a + tint * (1.0 - texel.a)) / alpha; }
    if params.record[2].z > 0.0 {
        rgb = clamp(rgb + vec3<f32>((grain(pixel * 0.3) - 0.5) * params.record[2].z * select(0.02, 0.005, liquid)), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return vec4<f32>(rgb, alpha * cover * params.record[2].w);
}
