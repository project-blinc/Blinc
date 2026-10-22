// ============================================================================
// Blinc SDF Text Shader
// ============================================================================
// Supports grayscale text and color emoji via separate atlases

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_pos: vec2<f32>,
    @location(3) @interpolate(flat) clip_bounds: vec4<f32>,
    @location(4) @interpolate(flat) clip_fade: vec4<f32>,
    @location(5) @interpolate(flat) is_color: f32,
    @location(6) @interpolate(flat) clip_shape: f32,
}

struct TextUniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
}

struct GlyphInstance {
    // Position and size (x, y, width, height)
    bounds: vec4<f32>,
    // UV coordinates in atlas (u_min, v_min, u_max, v_max)
    uv_bounds: vec4<f32>,
    // Text color
    color: vec4<f32>,
    // Clip bounds (x, y, width, height) - set to large values for no clip
    clip_bounds: vec4<f32>,
    // Overflow fade distances (top, right, bottom, left) in pixels
    clip_fade: vec4<f32>,
    // Flags: [is_color, path_header, unused, unused]
    // is_color: 1.0 = color emoji (use color_atlas), 0.0 = grayscale (use glyph_atlas)
    flags: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: TextUniforms;
@group(0) @binding(1) var<storage, read> glyphs: array<GlyphInstance>;
@group(0) @binding(2) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(3) var glyph_sampler: sampler;
@group(0) @binding(4) var color_atlas: texture_2d<f32>;
@group(0) @binding(5) var<storage, read> aux_data: array<vec4<f32>>;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let glyph = glyphs[instance_index];

    // Generate quad vertices
    // Quad vertices split along / diagonal (1-3 shared edge)
    // PowerVR Vulkan codegen bug workaround — see SDF_SHADER vs_main
    // for the rationale. `let array<...>(literal)[runtime]` is broken
    // for indices 3..5 on the Pixel 10 Pro PowerVR driver; an explicit
    // `switch` keeps the same vertex layout and renders correctly.
    var local_uv: vec2<f32>;
    switch vertex_index {
        case 0u: { local_uv = vec2<f32>(0.0, 0.0); } // 0 - top-left
        case 1u: { local_uv = vec2<f32>(1.0, 0.0); } // 1 - top-right
        case 2u: { local_uv = vec2<f32>(0.0, 1.0); } // 3 - bottom-left
        case 3u: { local_uv = vec2<f32>(1.0, 0.0); } // 1 - top-right
        case 4u: { local_uv = vec2<f32>(1.0, 1.0); } // 2 - bottom-right
        default: { local_uv = vec2<f32>(0.0, 1.0); } // 3 - bottom-left
    }

    // Position in screen space
    let pos = vec2<f32>(
        glyph.bounds.x + local_uv.x * glyph.bounds.z,
        glyph.bounds.y + local_uv.y * glyph.bounds.w
    );

    // glyph.uv_bounds holds atlas PIXEL coords (px_min, py_min,
    // px_max, py_max). Interpolate in pixel space and defer the
    // divide to the fragment shader — `textureDimensions()` in
    // the vertex stage would require the atlas binding to be
    // Vertex-visible, but the Text Pipeline layout declares it
    // Fragment-only.
    let pixel_uv = vec2<f32>(
        glyph.uv_bounds.x + local_uv.x * (glyph.uv_bounds.z - glyph.uv_bounds.x),
        glyph.uv_bounds.y + local_uv.y * (glyph.uv_bounds.w - glyph.uv_bounds.y)
    );
    let uv = pixel_uv;

    // Convert to clip space
    let clip_pos = vec2<f32>(
        (pos.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (pos.y / uniforms.viewport_size.y) * 2.0
    );

    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.uv = uv;
    out.color = glyph.color;
    out.world_pos = pos;
    out.clip_bounds = glyph.clip_bounds;
    out.clip_fade = glyph.clip_fade;
    out.is_color = glyph.flags.x;
    out.clip_shape = glyph.flags.y;

    return out;
}

// Calculate clip alpha for rectangular clip region
fn calculate_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>, clip_fade: vec4<f32>) -> f32 {
    // Check if clipping is active (default bounds are very large negative values)
    if clip_bounds.x < -5000.0 {
        return 1.0;
    }

    // Clip bounds are (x, y, width, height)
    let clip_min = clip_bounds.xy;
    let clip_max = clip_bounds.xy + clip_bounds.zw;

    // Calculate signed distance to clip rect edges
    let d_left = p.x - clip_min.x;
    let d_right = clip_max.x - p.x;
    let d_top = p.y - clip_min.y;
    let d_bottom = clip_max.y - p.y;

    // Minimum distance to any edge (negative = outside)
    let d = min(min(d_left, d_right), min(d_top, d_bottom));

    // Soft anti-aliased edge (1 pixel transition)
    var alpha = clamp(d + 0.5, 0.0, 1.0);

    // Apply overflow fade
    if clip_fade.x > 0.0 { alpha *= saturate(d_top / clip_fade.x); }
    if clip_fade.y > 0.0 { alpha *= saturate(d_right / clip_fade.y); }
    if clip_fade.z > 0.0 { alpha *= saturate(d_bottom / clip_fade.z); }
    if clip_fade.w > 0.0 { alpha *= saturate(d_left / clip_fade.w); }

    return alpha;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Calculate clip alpha first - discard if completely outside
    let clip_alpha = calculate_clip_alpha(in.world_pos, in.clip_bounds, in.clip_fade) * abi_shape_clip_alpha(in.world_pos, in.clip_shape);
    if clip_alpha < 0.001 {
        discard;
    }

    // Use `textureSampleLevel(..., 0.0)` instead of `textureSample`
    // for both atlas paths. WGSL's uniform-control-flow rule applies
    // to *implicit-LOD* sampling — `textureSample` derives the mip
    // level from quad derivatives, which need all four pixels in the
    // 2x2 derivative quad to take the same code path. The `is_color`
    // flag is per-glyph (`@interpolate(flat)`) and can vary between
    // adjacent quads, so Dawn classifies the `if in.is_color > 0.5`
    // branch as non-uniform and rejects implicit-LOD samples inside.
    //
    // The glyph atlases are single-mip textures (R8Unorm and
    // Rgba8UnormSrgb, no mipmap chain), so LOD 0 is the only valid
    // level — `textureSampleLevel(..., 0.0)` is byte-identical to
    // what `textureSample` would have produced. Native backends
    // (Metal, Vulkan, DX12) accept both forms; Dawn requires the
    // explicit form here.
    // `in.uv` is interpolated PIXEL coords in atlas space (the
    // vertex shader interpolates pixel coords rather than UVs so
    // the divide-by-atlas-size happens here, where the atlas
    // texture is bind-group-visible). Live `textureDimensions()`
    // tracks atlas growth automatically.
    if in.is_color > 0.5 {
        // Color emoji: sample RGBA from color atlas, use texture color directly
        let atlas_size = vec2<f32>(textureDimensions(color_atlas));
        let uv = in.uv / atlas_size;
        let emoji_color = textureSampleLevel(color_atlas, glyph_sampler, uv, 0.0);
        return vec4<f32>(emoji_color.rgb, emoji_color.a * clip_alpha);
    } else {
        // Grayscale text: sample coverage from glyph atlas, apply tint color
        let atlas_size = vec2<f32>(textureDimensions(glyph_atlas));
        let uv = in.uv / atlas_size;
        let coverage = textureSampleLevel(glyph_atlas, glyph_sampler, uv, 0.0).r;

        // Use coverage directly with slight gamma correction for cleaner edges
        // pow(x, 0.7) brightens mid-tones, making strokes appear crisper
        let aa_alpha = pow(coverage, 0.7);
        return vec4<f32>(in.color.rgb, in.color.a * aa_alpha * clip_alpha);
    }
}
