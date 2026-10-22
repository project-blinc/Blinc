// ============================================================================
// Path Rendering Shader
// ============================================================================
//
// Renders tessellated vector paths as colored triangles.
// Supports solid colors and gradients via per-vertex UV coordinates.
// Supports multi-stop gradients via 1D texture lookup.
// Supports clipping via rect/circle/ellipse shapes.

// Clip type values (variable: clip_type)
//   0u = NONE    1u = RECT    2u = CIRCLE    3u = ELLIPSE    4u = POLYGON
//
// Inlined as literals rather than `const` to prevent naga from
// emitting orphaned `constant uint CLIP_* = ...;` declarations into
// the generated Metal source (which trip
// `-Wunused-const-variable` at runtime shader compile time).

struct Uniforms {
    // viewport_size (vec2) + padding (vec2) = 16 bytes, offset 0
    viewport_size: vec2<f32>,
    opacity: f32,
    _pad0: f32,
    // 3x3 transform stored as 3 vec4s (xyz used, w is padding) = 48 bytes, offset 16
    transform_row0: vec4<f32>,
    transform_row1: vec4<f32>,
    transform_row2: vec4<f32>,
    // Clip parameters = 32 bytes, offset 64
    clip_bounds: vec4<f32>,   // (x, y, width, height) or (cx, cy, rx, ry)
    clip_radius: vec4<f32>,   // corner radii or (rx, ry, 0, 0)
    // clip_type + flags = 16 bytes, offset 96
    clip_type: u32,
    use_gradient_texture: u32,  // 0=use vertex colors, 1=sample gradient texture
    use_image_texture: u32,     // 0=no image, 1=sample image texture
    use_glass_effect: u32,      // 0=no glass, 1=glass effect on path
    // Image UV bounds = 16 bytes, offset 112
    image_uv_bounds: vec4<f32>, // (u_min, v_min, u_max, v_max)
    // Glass parameters = 16 bytes, offset 128
    glass_params: vec4<f32>,    // (blur_radius, saturation, tint_strength, opacity)
    // Glass tint color = 16 bytes, offset 144
    glass_tint: vec4<f32>,      // RGBA tint color
}
// Total: 160 bytes

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var gradient_texture: texture_1d<f32>;
@group(0) @binding(2) var gradient_sampler: sampler;
@group(0) @binding(3) var image_texture: texture_2d<f32>;
@group(0) @binding(4) var image_sampler: sampler;
@group(0) @binding(5) var backdrop_texture: texture_2d<f32>;
@group(0) @binding(6) var backdrop_sampler: sampler;
// Shared with the SDF pipeline — holds polygon clip vertices packed as
// vec4(x0, y0, x1, y1). Exposed to the path pipeline so Lottie track
// mattes (rendered as `ClipShape::Polygon` by `blinc_lottie`) affect
// tessellated shape fills, not just SDF-drawn primitives.
@group(0) @binding(7) var<storage, read> aux_data: array<vec4<f32>>;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,           // start color for gradients, solid color otherwise
    @location(2) end_color: vec4<f32>,       // end color for gradients
    @location(3) uv: vec2<f32>,
    @location(4) gradient_params: vec4<f32>, // linear: (x1,y1,x2,y2); radial: (cx,cy,r,0)
    @location(5) gradient_type: u32,
    @location(6) edge_distance: f32,         // distance to nearest edge (for AA)
    // Per-vertex clip data — populated by push_path_with_brush_info so multiple
    // path submissions with different clips can coexist in one VBO/draw call.
    @location(7) clip_bounds: vec4<f32>,
    @location(8) clip_radius: vec4<f32>,
    @location(9) clip_type: u32,
    @location(10) clip_shape: u32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) end_color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) gradient_params: vec4<f32>,
    @location(4) @interpolate(flat) gradient_type: u32,
    @location(5) edge_distance: f32,
    @location(6) screen_pos: vec2<f32>,      // screen position for clip calculations
    @location(7) @interpolate(flat) v_clip_bounds: vec4<f32>,
    @location(8) @interpolate(flat) v_clip_radius: vec4<f32>,
    @location(9) @interpolate(flat) v_clip_type: u32,
    @location(10) @interpolate(flat) v_clip_shape: u32,
}

// ============================================================================
// SDF Functions for Clipping
// ============================================================================

// Calculate clip alpha (1.0 = inside clip, 0.0 = outside)
// For non-rect clips: clip_bounds = rect scissor, clip_radius = shape data
fn calculate_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>, clip_radius: vec4<f32>, clip_type: u32) -> f32 {
    if clip_type == 0u {
        return 1.0;
    }

    let aa_width = 0.75;

    switch clip_type {
        case 1u /* CLIP_RECT */: {
            let clip_origin = clip_bounds.xy;
            let clip_size = clip_bounds.zw;
            let clip_d = sd_rounded_rect(p, clip_origin, clip_size, clip_radius);
            return 1.0 - smoothstep(-aa_width, aa_width, clip_d);
        }
        case 2u /* CLIP_CIRCLE */: {
            let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, vec4<f32>(0.0));
            let scissor_alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
            let center = clip_radius.xy;
            let radius = clip_radius.z;
            let clip_d = sd_circle(p, center, radius);
            let shape_alpha = 1.0 - smoothstep(-aa_width, aa_width, clip_d);
            return scissor_alpha * shape_alpha;
        }
        case 3u /* CLIP_ELLIPSE */: {
            let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, vec4<f32>(0.0));
            let scissor_alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
            let center = clip_radius.xy;
            let radii = clip_radius.zw;
            let clip_d = sd_ellipse(p, center, radii);
            let shape_alpha = 1.0 - smoothstep(-aa_width, aa_width, clip_d);
            return scissor_alpha * shape_alpha;
        }
        case 4u /* CLIP_POLYGON */: {
            let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, vec4<f32>(0.0));
            let scissor_alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
            let vertex_count = u32(clip_radius.z);
            let aux_offset = u32(clip_radius.w);
            let shape_alpha = calculate_polygon_clip_alpha(p, vertex_count, aux_offset);
            return scissor_alpha * shape_alpha;
        }
        default: {
            return 1.0;
        }
    }
}

// Polygon winding-number test. Vertices are packed 2-per-vec4 in the
// shared aux_data storage buffer; `aux_offset` is the starting vec4
// index and `vertex_count` is the polygon vertex count. Mirrors the
// implementation used by the SDF pipeline — kept in sync so a
// polygon clip pushed at the `DrawContext` level clips fills and
// shape primitives identically.
fn calculate_polygon_clip_alpha(p: vec2<f32>, vertex_count: u32, aux_offset: u32) -> f32 {
    if vertex_count < 3u {
        return 1.0;
    }

    var winding: i32 = 0;

    for (var i: u32 = 0u; i < vertex_count; i = i + 1u) {
        let vec_idx = aux_offset + (i / 2u);
        let data = abi_clip_data(vec_idx);
        var vi: vec2<f32>;
        if (i % 2u) == 0u {
            vi = data.xy;
        } else {
            vi = data.zw;
        }

        let j = (i + 1u) % vertex_count;
        let vec_idx_j = aux_offset + (j / 2u);
        let data_j = abi_clip_data(vec_idx_j);
        var vj: vec2<f32>;
        if (j % 2u) == 0u {
            vj = data_j.xy;
        } else {
            vj = data_j.zw;
        }

        // Cast a horizontal ray from `p` to the right; accumulate +1 / -1
        // at each crossing based on edge direction.
        if (vi.y <= p.y) {
            if (vj.y > p.y) {
                let cross = (vj.x - vi.x) * (p.y - vi.y) - (p.x - vi.x) * (vj.y - vi.y);
                if (cross > 0.0) {
                    winding = winding + 1;
                }
            }
        } else {
            if (vj.y <= p.y) {
                let cross = (vj.x - vi.x) * (p.y - vi.y) - (p.x - vi.x) * (vj.y - vi.y);
                if (cross < 0.0) {
                    winding = winding - 1;
                }
            }
        }
    }

    if winding != 0 {
        return 1.0;
    } else {
        return 0.0;
    }
}

// ============================================================================
// Vertex Shader
// ============================================================================

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Reconstruct transform matrix and apply
    let p = vec3<f32>(in.position, 1.0);
    let transformed = vec3<f32>(
        dot(uniforms.transform_row0.xyz, p),
        dot(uniforms.transform_row1.xyz, p),
        dot(uniforms.transform_row2.xyz, p)
    );

    // Store screen position for clip calculations
    out.screen_pos = transformed.xy;

    // Convert to clip space (-1 to 1)
    let clip_pos = vec2<f32>(
        (transformed.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (transformed.y / uniforms.viewport_size.y) * 2.0
    );

    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.color = in.color;
    out.end_color = in.end_color;
    out.uv = in.uv;
    out.gradient_params = in.gradient_params;
    out.gradient_type = in.gradient_type;
    out.edge_distance = in.edge_distance;
    out.v_clip_bounds = in.clip_bounds;
    out.v_clip_radius = in.clip_radius;
    out.v_clip_type = in.clip_type;
    out.v_clip_shape = in.clip_shape;

    return out;
}

// ============================================================================
// Fragment Shader
// ============================================================================

// Simple box blur for glass effect (samples backdrop in a small radius)
//
// Uses `textureSampleLevel` instead of `textureSample` so this is legal
// to call from non-uniform control flow. The path-pipeline fragment
// shader takes glass-effect branches based on per-vertex clip flags
// (`use_glass_effect`), which are not provably uniform across the
// rasterized triangle, so WGSL/WebGPU rejects implicit-derivative
// samples here.
fn sample_blur(uv: vec2<f32>, blur_radius: f32, viewport_size: vec2<f32>) -> vec4<f32> {
    let pixel_size = 1.0 / viewport_size;
    var total = vec4<f32>(0.0);
    var samples = 0.0;

    // Simple 5x5 box blur
    let sample_radius = i32(clamp(blur_radius * 0.1, 1.0, 3.0));
    for (var x = -sample_radius; x <= sample_radius; x++) {
        for (var y = -sample_radius; y <= sample_radius; y++) {
            let offset = vec2<f32>(f32(x), f32(y)) * pixel_size * blur_radius * 0.5;
            let sample_uv = clamp(uv + offset, vec2<f32>(0.0), vec2<f32>(1.0));
            total += textureSampleLevel(backdrop_texture, backdrop_sampler, sample_uv, 0.0);
            samples += 1.0;
        }
    }

    return total / samples;
}

// Adjust saturation of a color
fn adjust_saturation(color: vec3<f32>, saturation: f32) -> vec3<f32> {
    let gray = dot(color, vec3<f32>(0.299, 0.587, 0.114));
    return mix(vec3<f32>(gray), color, saturation);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Calculate clip alpha from per-vertex clip data so multiple paths with
    // different clips can share a single VBO/draw without clobbering each
    // other. The uniform clip_* fields are kept for legacy callers but no
    // longer used here.
    let clip_alpha = calculate_clip_alpha(
        in.screen_pos,
        in.v_clip_bounds,
        in.v_clip_radius,
        in.v_clip_type
    ) * abi_shape_clip_alpha(in.screen_pos, f32(in.v_clip_shape));

    // Early out if fully clipped
    if clip_alpha < 0.001 {
        discard;
    }

    // Use `textureSampleLevel(..., 0.0)` instead of `textureSample`
    // for the gradient and image lookups. WGSL's uniform-control-flow
    // rule applies to *implicit-LOD* sampling — `textureSample`
    // derives mip level from quad derivatives, which need all four
    // pixels in the 2x2 derivative quad to take the same code path.
    // The brush type is selected by `in.gradient_type` (per-vertex)
    // and `uniforms.use_image_texture` / `use_gradient_texture` /
    // `use_glass_effect` (uniform), but Dawn classifies the per-vertex
    // value as non-uniform — so any implicit-LOD sample inside a
    // branch keyed on it fails validation.
    //
    // Both the gradient texture (1D ramp) and the image brush texture
    // are created with `mip_level_count: 1` (see
    // `gradient_texture.rs:222` and `image.rs:39`), so LOD 0 is the
    // only valid level. `textureSampleLevel(t, s, uv, 0.0)` is
    // byte-identical to what `textureSample` would have produced.
    // Native backends (Metal, Vulkan, DX12) accept both forms; Dawn
    // requires the explicit form here.

    var color: vec4<f32>;

    // Check for glass effect first
    if (uniforms.use_glass_effect == 1u) {
        // Glass effect: sample and blur backdrop, apply tint
        let screen_uv = in.screen_pos / uniforms.viewport_size;
        let blur_radius = uniforms.glass_params.x;
        let saturation = uniforms.glass_params.y;
        let tint_strength = uniforms.glass_params.z;
        let glass_opacity = uniforms.glass_params.w;

        // Sample blurred backdrop
        var backdrop = sample_blur(screen_uv, blur_radius, uniforms.viewport_size);

        // Adjust saturation
        backdrop = vec4<f32>(adjust_saturation(backdrop.rgb, saturation), backdrop.a);

        // Apply tint
        let tinted = mix(backdrop.rgb, uniforms.glass_tint.rgb, tint_strength * uniforms.glass_tint.a);

        // Final color with glass opacity
        color = vec4<f32>(tinted, glass_opacity);
    } else if (uniforms.use_image_texture == 1u) {
        // Image brush: sample from image texture using UV coordinates
        // Map the path UV (0-1 in bounding box) to image UV bounds
        let uv_min = uniforms.image_uv_bounds.xy;
        let uv_max = uniforms.image_uv_bounds.zw;
        let image_uv = uv_min + in.uv * (uv_max - uv_min);
        color = textureSampleLevel(image_texture, image_sampler, image_uv, 0.0);
        // Apply tint from vertex color (multiply)
        color = vec4<f32>(color.rgb * in.color.rgb, color.a * in.color.a);
    } else if (in.gradient_type == 0u) {
        // Solid color
        color = in.color;
    } else if (in.gradient_type == 1u) {
        // Linear gradient - use gradient_params for direction
        // params: (x1, y1, x2, y2) in ObjectBoundingBox space (0-1)
        let g_start = in.gradient_params.xy;
        let g_end = in.gradient_params.zw;
        let g_dir = g_end - g_start;
        let g_len_sq = dot(g_dir, g_dir);

        // Project UV onto gradient line
        var t: f32;
        if (g_len_sq > 0.0001) {
            let p_lin = in.uv - g_start;
            t = clamp(dot(p_lin, g_dir) / g_len_sq, 0.0, 1.0);
        } else {
            t = 0.0;
        }

        // Sample from gradient texture or mix vertex colors
        if (uniforms.use_gradient_texture == 1u) {
            // Multi-stop gradient: sample from 1D texture
            color = textureSampleLevel(gradient_texture, gradient_sampler, t, 0.0);
        } else {
            // 2-stop fast path: mix vertex colors
            color = mix(in.color, in.end_color, t);
        }
    } else {
        // Radial gradient - params: (cx, cy, rx, ry) all in
        // ObjectBoundingBox space. `rx` = radius / bounds_width, `ry` =
        // radius / bounds_height. The per-axis radius lets a circle in
        // path-space stay a circle when the bounding box isn't square —
        // a single scalar radius would stretch the gradient to match
        // the OBB aspect ratio, making the Flair halo read as an
        // ellipse on wide paths.
        let center = in.gradient_params.xy;
        let radius_xy = max(in.gradient_params.zw, vec2<f32>(0.001));
        let d = (in.uv - center) / radius_xy;
        let t = clamp(length(d), 0.0, 1.0);

        // Sample from gradient texture or mix vertex colors
        if (uniforms.use_gradient_texture == 1u) {
            // Multi-stop gradient: sample from 1D texture
            color = textureSampleLevel(gradient_texture, gradient_sampler, t, 0.0);
        } else {
            // 2-stop fast path: mix vertex colors
            color = mix(in.color, in.end_color, t);
        }
    }

    // Apply opacity and clip alpha
    // Note: edge-distance AA disabled - tessellated geometry has vertices ON path edges
    // (edge_distance = 0), which causes entire shape to fade. Need different AA approach.
    color.a *= uniforms.opacity * clip_alpha;
    return color;
}
