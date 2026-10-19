// Image rendering shader
// Supports: texture sampling, UV cropping, tinting, rounded corners, opacity, clipping, CSS filters

struct Uniforms {
    screen_size: vec2<f32>,
    _padding: vec2<f32>,
}

struct ImageInstance {
    // Destination rectangle (x, y, width, height) in screen pixels
    @location(0) dst_rect: vec4<f32>,
    // Source UV rectangle (u_min, v_min, u_max, v_max)
    @location(1) src_uv: vec4<f32>,
    // Tint color (RGBA)
    @location(2) tint: vec4<f32>,
    // params: (border_radius, opacity, border_width, packed_border_color)
    @location(3) params: vec4<f32>,
    // Clip bounds (x, y, width, height) - set to large values for no clip
    @location(4) clip_bounds: vec4<f32>,
    // Clip corner radii (top-left, top-right, bottom-right, bottom-left)
    @location(5) clip_radius: vec4<f32>,
    // CSS filter A (grayscale, invert, sepia, hue_rotate_rad)
    @location(6) filter_a: vec4<f32>,
    // CSS filter B (brightness, contrast, saturate, unused)
    @location(7) filter_b: vec4<f32>,
    // 2x2 affine transform (a, b, c, d) applied around quad center
    // Identity = (1, 0, 0, 1). Supports rotation, scale, and skew.
    @location(8) transform: vec4<f32>,
    // Secondary clip bounds (x, y, width, height) - sharp rect for scroll boundary
    @location(9) clip2_bounds: vec4<f32>,
    // Mask gradient params: linear=(x1,y1,x2,y2), radial=(cx,cy,r,0) in OBB space
    @location(10) mask_params: vec4<f32>,
    // Mask info: [mask_type, start_alpha, end_alpha, 0] (0=none, 1=linear, 2=radial)
    @location(11) mask_info: vec4<f32>,
    @location(12) abi_clip: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) local_pos: vec2<f32>,
    @location(3) rect_size: vec2<f32>,
    // params: (border_radius, opacity, border_width, packed_border_color_as_f32)
    @location(4) params: vec4<f32>,
    @location(5) world_pos: vec2<f32>,
    @location(6) clip_bounds: vec4<f32>,
    @location(7) clip_radius: vec4<f32>,
    @location(8) filter_a: vec4<f32>,
    @location(9) filter_b: vec4<f32>,
    @location(10) clip2_bounds: vec4<f32>,
    @location(11) mask_params: vec4<f32>,
    @location(12) mask_info: vec4<f32>,
    @location(13) @interpolate(flat) abi_clip: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var image_texture: texture_2d<f32>;

@group(0) @binding(2)
var image_sampler: sampler;

@group(0) @binding(3)
var<storage, read> aux_data: array<vec4<f32>>;

// Vertex indices for a quad (two triangles)
var<private> QUAD_INDICES: array<u32, 6> = array<u32, 6>(0u, 1u, 2u, 2u, 3u, 0u);
var<private> QUAD_POSITIONS: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 0.0), // Top-left
    vec2<f32>(1.0, 0.0), // Top-right
    vec2<f32>(1.0, 1.0), // Bottom-right
    vec2<f32>(0.0, 1.0), // Bottom-left
);

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
    instance: ImageInstance,
) -> VertexOutput {
    let quad_index = QUAD_INDICES[vertex_index];
    let local_pos = QUAD_POSITIONS[quad_index];

    // Calculate screen position
    var x = instance.dst_rect.x + local_pos.x * instance.dst_rect.z;
    var y = instance.dst_rect.y + local_pos.y * instance.dst_rect.w;

    // Apply 2x2 affine transform around quad center if non-identity
    // transform = (a, b, c, d) where identity = (1, 0, 0, 1)
    let ta = instance.transform.x;
    let tb = instance.transform.y;
    let tc = instance.transform.z;
    let td = instance.transform.w;
    let has_transform = abs(ta - 1.0) > 0.0001 || abs(tb) > 0.0001 || abs(tc) > 0.0001 || abs(td - 1.0) > 0.0001;
    if has_transform {
        let cx = instance.dst_rect.x + instance.dst_rect.z * 0.5;
        let cy = instance.dst_rect.y + instance.dst_rect.w * 0.5;
        let dx = x - cx;
        let dy = y - cy;
        x = cx + ta * dx + tc * dy;
        y = cy + tb * dx + td * dy;
    }

    // Convert to NDC
    let ndc_x = (x / uniforms.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (y / uniforms.screen_size.y) * 2.0;

    // Interpolate UV coordinates
    let uv = vec2<f32>(
        mix(instance.src_uv.x, instance.src_uv.z, local_pos.x),
        mix(instance.src_uv.y, instance.src_uv.w, local_pos.y),
    );

    var output: VertexOutput;
    output.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    output.uv = uv;
    output.tint = instance.tint;
    output.local_pos = local_pos * vec2<f32>(instance.dst_rect.z, instance.dst_rect.w);
    output.rect_size = vec2<f32>(instance.dst_rect.z, instance.dst_rect.w);
    output.params = instance.params;
    output.world_pos = vec2<f32>(x, y);
    output.clip_bounds = instance.clip_bounds;
    output.clip_radius = instance.clip_radius;
    output.filter_a = instance.filter_a;
    output.filter_b = instance.filter_b;
    output.clip2_bounds = instance.clip2_bounds;
    output.mask_params = instance.mask_params;
    output.mask_info = instance.mask_info;
    output.abi_clip = instance.abi_clip.x;

    return output;
}

// SDF for rounded rectangle (uniform radius)
fn rounded_rect_sdf(pos: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    let half_size = size * 0.5;
    let center_pos = pos - half_size;
    let r = min(radius, min(half_size.x, half_size.y));
    let q = abs(center_pos) - half_size + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

// SDF for rounded rectangle with per-corner radii
fn rounded_rect_sdf_corners(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let center = origin + half_size;
    let rel = p - center;
    let q = abs(rel) - half_size;

    // Select corner radius based on quadrant
    // radius: (top-left, top-right, bottom-right, bottom-left)
    var r: f32;
    if rel.y < 0.0 {
        if rel.x > 0.0 {
            r = radius.y; // top-right
        } else {
            r = radius.x; // top-left
        }
    } else {
        if rel.x > 0.0 {
            r = radius.z; // bottom-right
        } else {
            r = radius.w; // bottom-left
        }
    }

    r = min(r, min(half_size.x, half_size.y));
    let q_adjusted = q + vec2<f32>(r);
    return length(max(q_adjusted, vec2<f32>(0.0))) + min(max(q_adjusted.x, q_adjusted.y), 0.0) - r;
}

// Calculate clip alpha (1.0 = inside clip, 0.0 = outside)
fn calculate_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>, clip_radius: vec4<f32>) -> f32 {
    // Check if clip is effectively disabled (large bounds)
    if clip_bounds.x < -9000.0 {
        return 1.0;
    }

    let clip_d = rounded_rect_sdf_corners(p, clip_bounds.xy, clip_bounds.zw, clip_radius);

    // Anti-aliased clip edge using a fixed pixel-width AA band.
    // We avoid `fwidth(clip_d)` here because Chrome/Dawn's WebGPU WGSL
    // validator rejects derivative functions (`fwidth`/`dpdx`/`dpdy`)
    // inside non-uniform control flow (the early `discard` in fs_main).
    // A constant 0.75 px half-width is visually identical for images
    // rendered at screen resolution.
    let aa_width = 0.75;
    return 1.0 - smoothstep(-aa_width, aa_width, clip_d);
}

/// Apply CSS filter effects to a color.
/// filter_a = (grayscale, invert, sepia, hue_rotate_rad)
/// filter_b = (brightness, contrast, saturate, 0)
/// Values are in sRGB space (texture uses Rgba8Unorm, no hardware sRGB decode).
fn apply_image_filter(color: vec4<f32>, fa: vec4<f32>, fb: vec4<f32>) -> vec4<f32> {
    var rgb = color.rgb;

    // Grayscale: desaturate using luminance weights
    let grayscale = fa.x;
    if grayscale > 0.0 {
        let lum = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        rgb = mix(rgb, vec3<f32>(lum, lum, lum), grayscale);
    }

    // Sepia: apply sepia tone matrix
    let sepia = fa.z;
    if sepia > 0.0 {
        let sepia_r = dot(rgb, vec3<f32>(0.393, 0.769, 0.189));
        let sepia_g = dot(rgb, vec3<f32>(0.349, 0.686, 0.168));
        let sepia_b = dot(rgb, vec3<f32>(0.272, 0.534, 0.131));
        rgb = mix(rgb, vec3<f32>(sepia_r, sepia_g, sepia_b), sepia);
    }

    // Invert
    let invert = fa.y;
    if invert > 0.0 {
        rgb = mix(rgb, vec3<f32>(1.0) - rgb, invert);
    }

    // Hue-rotate: rotate in RGB space using rotation matrix
    let hue_rad = fa.w;
    if abs(hue_rad) > 0.001 {
        let cos_h = cos(hue_rad);
        let sin_h = sin(hue_rad);
        let w = vec3<f32>(0.2126, 0.7152, 0.0722);
        // Rodrigues-style hue rotation matrix
        let r = vec3<f32>(
            cos_h + (1.0 - cos_h) * w.x,
            (1.0 - cos_h) * w.x * w.y - sin_h * w.z,
            (1.0 - cos_h) * w.x * w.z + sin_h * w.y
        );
        let g = vec3<f32>(
            (1.0 - cos_h) * w.x * w.y + sin_h * w.z,
            cos_h + (1.0 - cos_h) * w.y,
            (1.0 - cos_h) * w.y * w.z - sin_h * w.x
        );
        let b = vec3<f32>(
            (1.0 - cos_h) * w.x * w.z - sin_h * w.y,
            (1.0 - cos_h) * w.y * w.z + sin_h * w.x,
            cos_h + (1.0 - cos_h) * w.z
        );
        rgb = vec3<f32>(dot(rgb, r), dot(rgb, g), dot(rgb, b));
    }

    // Brightness
    let brightness = fb.x;
    rgb = rgb * brightness;

    // Contrast
    let contrast = fb.y;
    rgb = (rgb - vec3<f32>(0.5)) * contrast + vec3<f32>(0.5);

    // Saturate
    let saturate = fb.z;
    if abs(saturate - 1.0) > 0.001 {
        let lum = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        rgb = mix(vec3<f32>(lum, lum, lum), rgb, saturate);
    }

    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), color.a);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Early clip test - discard if outside either clip region
    var clip_alpha = calculate_clip_alpha(input.world_pos, input.clip_bounds, input.clip_radius);
    if input.abi_clip > 0.5 {
        let base = u32(input.abi_clip) - 1u;
        let info = abi_clip_data(base + 4u);
        if input.clip_bounds.x >= -9000.0 {
            let distance = sd_shaped_rect(input.world_pos, input.clip_bounds.xy, input.clip_bounds.zw, input.clip_radius, vec4<f32>(info.x));
            clip_alpha = 1.0 - smoothstep(-0.75, 0.75, distance);
        }
        let local = abi_clip_data(base);
        if local.z > 0.0 {
            let distance = sd_shaped_rect(input.local_pos, local.xy, local.zw, abi_clip_data(base + 1u), vec4<f32>(info.x));
            clip_alpha *= 1.0 - smoothstep(-info.y, info.y, distance);
        }
        let bounds = abi_clip_data(base + 2u);
        let fade = abi_clip_data(base + 3u);
        if fade.x > 0.0 { clip_alpha *= clamp((input.world_pos.y - bounds.y) / fade.x, 0.0, 1.0); }
        if fade.y > 0.0 { clip_alpha *= clamp((bounds.x + bounds.z - input.world_pos.x) / fade.y, 0.0, 1.0); }
        if fade.z > 0.0 { clip_alpha *= clamp((bounds.y + bounds.w - input.world_pos.y) / fade.z, 0.0, 1.0); }
        if fade.w > 0.0 { clip_alpha *= clamp((input.world_pos.x - bounds.x) / fade.w, 0.0, 1.0); }
        clip_alpha *= abi_shape_clip_alpha(input.world_pos, info.w);
    }
    if clip_alpha < 0.001 {
        discard;
    }
    // Secondary clip: sharp rect (scroll boundary), no corner radius
    let clip2_alpha = calculate_clip_alpha(input.world_pos, input.clip2_bounds, vec4<f32>(0.0));
    if clip2_alpha < 0.001 {
        discard;
    }

    // Unpack params
    let border_radius = input.params.x;
    let opacity = input.params.y;
    let border_width = input.params.z;

    // Sample the texture.
    // Uses `textureSampleLevel` (explicit LOD 0) instead of
    // `textureSample` because the `discard` statements above for
    // clip rejection make control flow non-uniform, and WGSL
    // forbids implicit-derivative sampling (`textureSample`) in
    // non-uniform flow. The image texture has no mipmaps so LOD 0
    // is identical to the implicit form.
    var color = textureSampleLevel(image_texture, image_sampler, input.uv, 0.0);

    // Apply tint
    color = color * input.tint;

    // Apply CSS filters (grayscale, invert, sepia, hue-rotate, brightness, contrast, saturate)
    let fa = input.filter_a;
    let fb = input.filter_b;
    if fa.x != 0.0 || fa.y != 0.0 || fa.z != 0.0 || abs(fa.w) > 0.001 || fb.x != 1.0 || fb.y != 1.0 || fb.z != 1.0 {
        color = apply_image_filter(color, fa, fb);
    }

    // Apply opacity
    color.a *= opacity;

    // Apply rounded corners and optional border
    if border_radius > 0.0 || border_width > 0.0 {
        let sdf = rounded_rect_sdf(input.local_pos, input.rect_size, border_radius);
        // Anti-aliased outer edge
        let outer_aa = 1.0 - smoothstep(-1.0, 1.0, sdf);

        if border_width > 0.0 {
            // Unpack border color from packed u32 in params.w (RGBA, 8 bits each)
            let packed = bitcast<u32>(input.params.w);
            let bc = vec4<f32>(
                f32((packed >> 24u) & 0xFFu) / 255.0,
                f32((packed >> 16u) & 0xFFu) / 255.0,
                f32((packed >> 8u) & 0xFFu) / 255.0,
                f32(packed & 0xFFu) / 255.0,
            );
            // Border factor: 0 deep inside image, 1 in border region
            let border_factor = smoothstep(-border_width - 1.0, -border_width + 1.0, sdf);
            // Blend image → border color at inner edge
            color = vec4<f32>(
                mix(color.rgb, bc.rgb, border_factor),
                mix(color.a, bc.a, border_factor),
            );
        }

        color.a *= outer_aa;
    }

    // Apply both clip alphas
    color.a *= clip_alpha * clip2_alpha;

    // Mask gradient evaluation
    let mask_type = input.mask_info.x;
    if mask_type > 0.5 {
        // Compute normalized UV within the image quad (0-1)
        let mask_uv = input.local_pos / max(input.rect_size, vec2<f32>(0.001));
        var mask_t: f32;
        if mask_type < 1.5 {
            // Linear mask gradient
            let m_start = input.mask_params.xy;
            let m_end = input.mask_params.zw;
            let m_dir = m_end - m_start;
            let m_len_sq = dot(m_dir, m_dir);
            if m_len_sq > 0.0001 {
                mask_t = clamp(dot(mask_uv - m_start, m_dir) / m_len_sq, 0.0, 1.0);
            } else {
                mask_t = 0.0;
            }
        } else {
            // Radial mask gradient
            let m_center = input.mask_params.xy;
            let m_radius = input.mask_params.z;
            mask_t = clamp(length(mask_uv - m_center) / max(m_radius, 0.001), 0.0, 1.0);
        }
        let mask_alpha = mix(input.mask_info.y, input.mask_info.z, mask_t);
        color = vec4<f32>(color.rgb * mask_alpha, color.a * mask_alpha);
    }

    // Output premultiplied alpha for correct blending
    // (blend state uses src_factor: One, dst_factor: OneMinusSrcAlpha)
    color = vec4<f32>(color.rgb * color.a, color.a);

    return color;
}
