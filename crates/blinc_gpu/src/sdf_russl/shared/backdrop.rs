//! The backdrop composite: the vertical pass of an ABI backdrop's blur and its glass shading. The
//! geometry stays in the element's logical coordinates and the samples are in target pixels. The
//! blur taps and the clip data come from an auxiliary texture at slot 3, which the specialisation
//! sets; `backdrop_rows` is the horizontal pass.

russl::russl! {
    use super::aux_data::aux;
    use super::clip::abi_shape_clip_alpha;
    use super::common::sd_shaped_rect;
    use super::notch::sd_abi_notch;

    #[pipeline] const TIER: u32 = 0;
    #[pipeline] const AUX_SLOT: u32 = 5;

    pub struct Params {
        pub screen: Vec4,
        pub source: Vec4,
        pub taps: Vec4,
        pub record: [Vec4; 28],
    }

    #[group(0)] #[binding(0)] static PARAMS: Uniform<Params>;
    #[group(0)] #[binding(1)] static ROWS: Texture2D<f32>;
    #[group(0)] #[binding(2)] static LINEAR_SAMPLER: Sampler;

    // The box's corners are widened by a pixel and a half of antialiasing fringe.
    #[vertex]
    #[builtin(position)]
    fn vs_main(#[builtin(vertex_index)] index: u32) -> Vec4 {
        let mut corners = [
            Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0),
        ];
        let m = PARAMS.record[15];
        let box_rect = PARAMS.record[0];
        let grow = Vec2::splat(1.5) / max(Vec2::new(length(m.xy), length(m.zw)), Vec2::splat(0.01));
        let local = corners[index] * (box_rect.zw + grow * 2.0) - grow;
        let pixel = (box_rect.xy + Vec2::new(m.x * local.x + m.z * local.y, m.y * local.x + m.w * local.y)) * PARAMS.screen.z;
        return Vec4::new(pixel.x / PARAMS.screen.x * 2.0 - 1.0, 1.0 - pixel.y / PARAMS.screen.y * 2.0, 0.0, 1.0);
    }

    // Distance to the box, in its own frame: a notched shape when the notch rows are set, a shaped
    // rect otherwise.
    fn box_distance(p: Vec2) -> f32 {
        let corners = PARAMS.record[25];
        let top = PARAMS.record[26];
        let bottom = PARAMS.record[27];
        if abs(corners.x) + abs(corners.y) + abs(corners.z) + abs(corners.w) + top.x + bottom.x > 0.0 {
            let kinds = Vec4::new(
                if corners.x < 0.0 { 1.0 } else { 0.0 },
                if corners.y < 0.0 { 1.0 } else { 0.0 },
                if corners.z < 0.0 { 1.0 } else { 0.0 },
                if corners.w < 0.0 { 1.0 } else { 0.0 },
            );
            return sd_abi_notch(p, PARAMS.record[0].zw, abs(corners), kinds, top, bottom);
        }
        return sd_shaped_rect(p, Vec2::splat(0.0), PARAMS.record[0].zw, PARAMS.record[1], PARAMS.record[12]);
    }

    fn rim_lens(inner: f32, width: f32) -> f32 {
        let bevel = 1.0 - clamp(inner / width, 0.0, 1.0);
        return bevel * bevel;
    }

    fn rim_line(inner: f32, width: f32) -> f32 {
        return smoothstep(0.0, width * 0.3, inner) * (1.0 - smoothstep(width, width * 1.5, inner));
    }

    fn glass_environment(p: Vec2) -> f32 {
        let direction = dot(p, Vec2::new(0.55, 0.83));
        let broad = 0.5 + 0.5 * sin(direction * 7.0 + 0.8);
        let streak = 0.5 + 0.5 * sin(direction * 18.0 - 1.2);
        return 0.25 + broad * broad * 0.55 + pow(streak, 6.0) * 0.2;
    }

    fn grain_hash(p: Vec2) -> f32 {
        return fract(sin(dot(p, Vec2::new(127.1, 311.7))) * 43758.5453123);
    }

    fn grain(p: Vec2) -> f32 {
        let i = floor(p);
        let f = fract(p);
        let u = f * f * (Vec2::splat(3.0) - 2.0 * f);
        return mix(mix(grain_hash(i), grain_hash(i + Vec2::new(1.0, 0.0)), u.x),
            mix(grain_hash(i + Vec2::new(0.0, 1.0)), grain_hash(i + Vec2::splat(1.0)), u.x), u.y);
    }

    // The rows texture's coordinate for a target pixel, clamped to the target.
    fn rows_at(pixel: Vec2) -> Vec2 {
        let p = clamp(pixel, Vec2::splat(0.5), PARAMS.screen.xy - Vec2::splat(0.5));
        return (p - PARAMS.source.xy) / ROWS.dimensions().as_vec2();
    }

    #[fragment]
    #[location(0)]
    fn fs_main(#[builtin(position)] position: Vec4) -> Vec4 {
        let pixel = position.xy / PARAMS.screen.z;
        let matrix = PARAMS.record[15];
        let det = matrix.x * matrix.w - matrix.y * matrix.z;
        let delta = pixel - PARAMS.record[0].xy;
        let local = Vec2::new(matrix.w * delta.x - matrix.z * delta.y, matrix.x * delta.y - matrix.y * delta.x) / det;
        let aa = max(0.25 * (length(Vec2::new(matrix.w, matrix.y)) + length(Vec2::new(matrix.z, matrix.x))) / (abs(det) * PARAMS.screen.z), 1e-6);
        let distance = box_distance(local);
        let types = PARAMS.record[11];
        let clip_n = Vec4::splat(types.w);
        let flags = types.z as u32;
        let mut cover = 1.0 - smoothstep(-aa, aa, distance);
        if (flags & 1u32) != 0u32 {
            let bounds = PARAMS.record[8];
            cover = cover * (1.0 - smoothstep(-0.75, 0.75, sd_shaped_rect(pixel, bounds.xy, bounds.zw, PARAMS.record[9], clip_n)));
        }
        if (flags & 2u32) != 0u32 {
            let bounds = PARAMS.record[6];
            let mut fringe = aa;
            if matrix.x == 1.0 && matrix.y == 0.0 && matrix.z == 0.0 && matrix.w == 1.0 {
                fringe = 0.75;
            }
            cover = cover * (1.0 - smoothstep(-fringe, fringe, sd_shaped_rect(local, bounds.xy, bounds.zw, PARAMS.record[7], clip_n)));
        }
        let fade = PARAMS.record[21];
        let fade_bounds = PARAMS.record[20];
        if fade.x > 0.0 { cover = cover * clamp((pixel.y - fade_bounds.y) / fade.x, 0.0, 1.0); }
        if fade.y > 0.0 { cover = cover * clamp((fade_bounds.x + fade_bounds.z - pixel.x) / fade.y, 0.0, 1.0); }
        if fade.z > 0.0 { cover = cover * clamp((fade_bounds.y + fade_bounds.w - pixel.y) / fade.z, 0.0, 1.0); }
        if fade.w > 0.0 { cover = cover * clamp((pixel.x - fade_bounds.x) / fade.w, 0.0, 1.0); }
        cover = cover * abi_shape_clip_alpha(position.xy, PARAMS.source.w);
        if cover < 0.001 { discard!(); }
        let box_size = PARAMS.record[0].zw;
        let liquid = PARAMS.record[10].x > 0.5;
        let inner = max(0.0, -distance);
        let bevel = clamp(abs(PARAMS.record[10].y), 0.0, 1.0);
        let rim = max(1e-4, min(25.0, min(box_size.x, box_size.y) * 0.2) * bevel);
        let mut normal = Vec2::splat(0.0);
        let mut local_normal = Vec2::splat(0.0);
        let mut offset = Vec2::splat(0.0);
        let mut lens = 0.0;
        if liquid {
            let gradient = Vec2::new(box_distance(local + Vec2::new(0.5, 0.0)) - box_distance(local - Vec2::new(0.5, 0.0)),
                box_distance(local + Vec2::new(0.0, 0.5)) - box_distance(local - Vec2::new(0.0, 0.5)));
            local_normal = gradient / max(length(gradient), 1e-4);
            let world = Vec2::new(matrix.x * gradient.x + matrix.z * gradient.y, matrix.y * gradient.x + matrix.w * gradient.y);
            normal = world / max(length(world), 1e-4);
            lens = rim_lens(inner, rim);
            offset = normal * lens * 60.0 * PARAMS.record[10].y * length(matrix.xy) * PARAMS.screen.z;
        }
        let aberration = PARAMS.record[14].x;
        let dispersion = 0.12 * aberration + 0.38 * aberration * aberration * aberration;
        let split = offset * (dispersion / max(bevel, 1e-4));
        let disperse = dot(split, split) > 1e-6;
        let mut sum = Vec4::splat(0.0);
        let mut red = Vec2::splat(0.0);
        let mut blue = Vec2::splat(0.0);
        for i in 0u32..(PARAMS.source.z as u32) {
            let tap = aux((PARAMS.taps.x as u32) + i).xy;
            let at = position.xy + offset + Vec2::new(0.0, tap.x);
            sum = sum + textureSampleLevel(ROWS, LINEAR_SAMPLER, rows_at(at), 0.0) * tap.y;
            if disperse {
                let r = textureSampleLevel(ROWS, LINEAR_SAMPLER, rows_at(at + split), 0.0);
                let b = textureSampleLevel(ROWS, LINEAR_SAMPLER, rows_at(at - split), 0.0);
                red = red + Vec2::new(r.x, r.w) * tap.y;
                blue = blue + Vec2::new(b.z, b.w) * tap.y;
            }
        }
        let mut texel = sum;
        if disperse { texel = Vec4::new(red.x, sum.y, blue.x, max(sum.w, max(red.y, blue.y))); }
        let mut rgb = Vec3::splat(0.0);
        if texel.w > 1e-4 { rgb = texel.xyz / texel.w; }
        let color = Vec4::new(rgb.x, rgb.y, rgb.z, 1.0);
        rgb = clamp(Vec3::new(dot(PARAMS.record[3], color), dot(PARAMS.record[4], color), dot(PARAMS.record[5], color)), Vec3::splat(0.0), Vec3::splat(1.0));
        let mut tint = clamp(Vec3::new(PARAMS.record[3].w, PARAMS.record[4].w, PARAMS.record[5].w), Vec3::splat(0.0), Vec3::splat(1.0));
        let mut tint_alpha = clamp(1.0 - (PARAMS.record[3].x + PARAMS.record[4].y + PARAMS.record[5].z) / 3.0, 0.0, 1.0);
        if liquid {
            let width = max(PARAMS.record[10].z, 1e-4);
            let line = rim_line(inner, width) * bevel;
            let light_shift = aberration * width * 0.75;
            let angle = PARAMS.record[10].w;
            let mut light_sign = 1.0;
            if PARAMS.record[10].y < 0.0 { light_sign = -1.0; }
            let light = Vec2::new(cos(angle), sin(angle)) * light_sign;
            let lit_rgb = Vec3::new(rim_line(inner + light_shift, width) * bevel, line, rim_line(inner - light_shift, width) * bevel)
                * (0.6 * (0.2 + 0.8 * max(0.0, dot(normal, -light))));
            let lit = max(lit_rgb.x, max(lit_rgb.y, lit_rgb.z));
            let edge = PARAMS.record[16];
            let shade = smoothstep(width * 2.5, width * 8.0, inner) * (1.0 - smoothstep(width * 8.0, width * 24.0, inner)) * 0.04 * bevel;
            let via = clamp(PARAMS.record[13].w, 0.0, 1.0);
            if edge.w > 0.001 {
                let k = line * edge.w;
                rgb = mix(rgb, edge.xyz, k);
                tint = tint * (1.0 - k) + edge.xyz * k;
                tint_alpha = k + tint_alpha * (1.0 - k);
            } else {
                tint = tint * (1.0 - lit) + lit_rgb;
                tint_alpha = lit + tint_alpha * (1.0 - lit);
            }
            rgb = clamp(mix(rgb - Vec3::splat(shade), PARAMS.record[13].xyz, via), Vec3::splat(0.0), Vec3::splat(1.0));
            if edge.w <= 0.001 { rgb = clamp(rgb * (1.0 - lit) + lit_rgb, Vec3::splat(0.0), Vec3::splat(1.0)); }
            tint = tint * (1.0 - shade) * (1.0 - via) + PARAMS.record[13].xyz * via;
            tint_alpha = via + (shade + tint_alpha * (1.0 - shade)) * (1.0 - via);
            if texel.w < 0.999 && lens > 0.0 {
                let span = max(min(box_size.x, box_size.y), 1.0);
                let bend = local_normal * lens * (60.0 / span) * PARAMS.record[10].y;
                let point = (local - box_size * 0.5) / span + bend;
                let separation = bend * (dispersion / max(bevel, 1e-4));
                let middle = glass_environment(point);
                let mut environment = Vec3::new(glass_environment(point + separation), middle, glass_environment(point - separation));
                environment = mix(Vec3::splat(middle), environment, 0.35);
                let strength = (lens * (1.0 - lens) * 4.0 * 0.18 + lens * 0.06) * bevel;
                let environment_alpha = max(environment.x, max(environment.y, environment.z)) * strength;
                tint = tint * (1.0 - environment_alpha) + environment * strength;
                tint_alpha = environment_alpha + tint_alpha * (1.0 - environment_alpha);
            }
            let band = lens * (1.0 - lens) * 4.0 * 0.12 * bevel;
            tint = tint * (1.0 - band);
            tint_alpha = band + tint_alpha * (1.0 - band);
            let spectrum = Vec3::new(rim_lens(inner + aberration * 0.6, rim), lens, rim_lens(inner - aberration * 0.6, rim));
            let reflected = 0.85 * max(0.0, dot(normal, -light)) + 0.35 * max(0.0, dot(normal, light));
            let neutral_spec = lens * lens * reflected;
            let spec_rgb = mix(Vec3::splat(neutral_spec), spectrum * spectrum * reflected, 0.35) * bevel;
            let spec = max(spec_rgb.x, max(spec_rgb.y, spec_rgb.z));
            tint = tint * (1.0 - spec) + spec_rgb;
            tint_alpha = max(spec + tint_alpha * (1.0 - spec), 0.04);
        }
        let alpha = texel.w + tint_alpha * (1.0 - texel.w);
        if alpha > 1e-4 { rgb = (rgb * texel.w + tint * (1.0 - texel.w)) / alpha; }
        if PARAMS.record[2].z > 0.0 {
            let mut grain_scale = 0.02;
            if liquid { grain_scale = 0.005; }
            rgb = clamp(rgb + Vec3::splat((grain(pixel * 0.3) - 0.5) * PARAMS.record[2].z * grain_scale), Vec3::splat(0.0), Vec3::splat(1.0));
        }
        return Vec4::new(rgb.x, rgb.y, rgb.z, alpha * cover * PARAMS.record[2].w);
    }
}
