// ============================================================================
// Blinc SDF Simple Shader: plain Rect and Circle
// ============================================================================
// The common primitive, drawn with as little work per fragment as possible: a solid fill, a
// rounded rect (circular or convex superellipse corners) or a circle, an optional uniform
// border, and an optional rounded rect clip. Which primitives take it is `GpuPrimitive::is_plain_core`; everything
// else uses `sdf_core.wgsl`. For those primitives the pixels match the core shader's.
//
// The vertex stage reads the primitive once per vertex and passes what the fragment stage
// needs as flat values, so the fragment stage does no memory loads.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
    @location(2) @interpolate(flat) bounds: vec4<f32>,
    @location(3) @interpolate(flat) radius: vec4<f32>,
    @location(4) @interpolate(flat) border_color: vec4<f32>,
    @location(5) @interpolate(flat) clip_bounds: vec4<f32>,
    @location(6) @interpolate(flat) clip_radius: vec4<f32>,
    // x: border width, y: 1 for a circle, z: 1 when a rounded rect clip applies.
    @location(7) @interpolate(flat) misc: vec3<f32>,
    // The corner exponent of each corner: 1 is a circle, other positive values a superellipse.
    @location(8) @interpolate(flat) shape: vec4<f32>,
}

struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
}

// Same layout as every SDF shader; `tests/primitive_layout.rs` keeps the copies in step.
struct Primitive {
    // Bounds (x, y, width, height)
    bounds: vec4<f32>,
    // Corner radii (top-left, top-right, bottom-right, bottom-left)
    corner_radius: vec4<f32>,
    // Fill color (or gradient start color)
    color: vec4<f32>,
    // Gradient end color (for gradients)
    color2: vec4<f32>,
    // Border (width, 0, 0, 0)
    border: vec4<f32>,
    // Border color
    border_color: vec4<f32>,
    // Shadow (offset_x, offset_y, blur, spread)
    shadow: vec4<f32>,
    // Shadow color
    shadow_color: vec4<f32>,
    // Clip bounds (x, y, width, height) for rect clips, (cx, cy, rx, ry) for circle/ellipse
    clip_bounds: vec4<f32>,
    // Clip corner radii (for rounded rect) or (radius_x, radius_y, 0, 0) for ellipse
    clip_radius: vec4<f32>,
    // Clip corner shape (superellipse n per corner) for the rounded
    // rect clip — n=1.0 = round (default), 2.0 = squircle, 0.0 = bevel,
    // -1.0 = scoop. Lets overflow:clip on a squircle parent follow
    // the same curve as the parent fill instead of a circular cut.
    clip_corner_shape: vec4<f32>,
    // Gradient parameters: linear (x1, y1, x2, y2), radial (cx, cy, r, 0) in user space
    gradient_params: vec4<f32>,
    // Rotation (sin_rz, cos_rz, sin_ry, cos_ry) - for rotated SDF evaluation
    rotation: vec4<f32>,
    // Local 2x2 affine (a, b, c, d) - normalized (DPI removed).
    // Maps local rect space → screen space. Supports rotation, scale, AND skew.
    // Identity = (1, 0, 0, 1).
    local_affine: vec4<f32>,
    // Perspective (sin_rx, cos_rx, perspective_d, shape_3d_type)
    // shape_3d_type: 0=none, 1=box, 2=sphere, 3=cylinder, 4=torus, 5=capsule, 6=group
    perspective: vec4<f32>,
    // SDF 3D params (depth, ambient, specular_power, translate_z)
    sdf_3d: vec4<f32>,
    // Light params (dir_x, dir_y, dir_z, intensity)
    light: vec4<f32>,
    // CSS filter A (grayscale, invert, sepia, hue_rotate_rad)
    filter_a: vec4<f32>,
    // CSS filter B (brightness, contrast, saturate, 0)
    filter_b: vec4<f32>,
    // Mask gradient params: linear=(x1,y1,x2,y2), radial=(cx,cy,r,0) in OBB (0-1) space
    mask_params: vec4<f32>,
    // Mask info: (mask_type, start_alpha, end_alpha, 0)
    // mask_type: 0=none, 1=linear, 2=radial
    mask_info: vec4<f32>,
    // Corner shape (superellipse n parameter per corner)
    // n=1.0 = round (default), n=0.0 = bevel, n=2.0 = squircle, n=-1.0 = scoop
    corner_shape: vec4<f32>,
    // Overflow fade distances (top, right, bottom, left) in pixels
    clip_fade: vec4<f32>,
    // Type info (primitive_type, fill_type, clip_type, 0)
    type_info: vec4<u32>,
    // Border colour of each side when the sides differ; alphas that sum to
    // zero mean unset, and the border is `border_color` all round.
    side_color_top: vec4<f32>,
    side_color_right: vec4<f32>,
    side_color_bottom: vec4<f32>,
    side_color_left: vec4<f32>,
    // A gradient's stop table in aux_data: one more than its offset, zero for none.
    gradient_table: vec4<f32>,
    // A clip in the primitive's own frame, its width zero for none.
    local_clip_bounds: vec4<f32>,
    local_clip_radius: vec4<f32>,
    // Separate screen fade rectangle; zero width uses clip_bounds.
    clip_fade_bounds: vec4<f32>,
    // ABI notch scale and shadow mode; zero scale keeps native shading.
    notch_info: vec4<f32>,
    // One more than the aux_data header offset for an ABI shape clip, zero for none.
    clip_shape: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> primitives: array<Primitive>;
// Declared for the shared clip helpers the build appends; this shader does not read it.
@group(0) @binding(5) var<storage, read> aux_data: array<vec4<f32>>;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;
    let prim = primitives[instance_index];

    // Two triangles over the bounds. A switch rather than an indexed array: the PowerVR
    // Vulkan driver compiles indexing a constant array wrongly (see sdf_core.wgsl).
    var uv: vec2<f32>;
    switch vertex_index {
        case 0u: { uv = vec2<f32>(0.0, 0.0); }
        case 1u: { uv = vec2<f32>(1.0, 0.0); }
        case 2u: { uv = vec2<f32>(0.0, 1.0); }
        case 3u: { uv = vec2<f32>(1.0, 0.0); }
        case 4u: { uv = vec2<f32>(1.0, 1.0); }
        default: { uv = vec2<f32>(0.0, 1.0); }
    }
    let pos = prim.bounds.xy + uv * prim.bounds.zw;
    out.position = vec4<f32>(
        (pos.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (pos.y / uniforms.viewport_size.y) * 2.0,
        0.0,
        1.0
    );
    out.uv = pos;
    out.color = prim.color;
    out.bounds = prim.bounds;
    out.radius = prim.corner_radius;
    out.border_color = prim.border_color;
    out.clip_bounds = prim.clip_bounds;
    out.clip_radius = prim.clip_radius;
    out.shape = prim.corner_shape;
    // A border given as one width in `border.x`, or the same width on all four sides.
    out.misc = vec3<f32>(
        prim.border.x,
        select(0.0, 1.0, prim.type_info.x == 1u),
        select(0.0, 1.0, prim.type_info.z == 1u),
    );
    return out;
}

// `sd_shaped_rect` for the corner exponents this shader takes (positive and below 100): a circle
// where the exponent is 1, otherwise a convex superellipse. Outside the corner squares, nearly
// every fragment of a large rect, it skips the superellipse's `pow` calls and returns what the
// general form returns there.
fn sd_convex_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>, shape: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let rel = p - (origin + half_size);
    let q = abs(rel) - half_size;
    var r: f32;
    var n: f32;
    if rel.y < 0.0 {
        if rel.x > 0.0 {
            r = radius.y; n = shape.y;
        } else {
            r = radius.x; n = shape.x;
        }
    } else {
        if rel.x > 0.0 {
            r = radius.z; n = shape.z;
        } else {
            r = radius.w; n = shape.w;
        }
    }
    r = min(r, min(half_size.x, half_size.y));
    let qa = q + vec2<f32>(r);
    if abs(n - 1.0) < 0.01 {
        return length(max(qa, vec2<f32>(0.0))) + min(max(qa.x, qa.y), 0.0) - r;
    }
    let box_d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0);
    if qa.x <= 0.0 && qa.y <= 0.0 {
        return max(box_d, -r);
    }
    if qa.x <= 0.0 || qa.y <= 0.0 {
        return box_d;
    }
    let p_exp = pow(2.0, min(n, 5.0));
    let t = qa / max(r, 0.001);
    return max(box_d, superellipse_dist(t, p_exp, r));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv;

    // Rounded rect clip, with the core shader's edge width.
    var clip_alpha = 1.0;
    if in.misc.z > 0.5 {
        let clip_d = sd_rounded_rect(p, in.clip_bounds.xy, in.clip_bounds.zw, in.clip_radius);
        clip_alpha = 1.0 - smoothstep(-0.75, 0.75, clip_d);
        if clip_alpha < 0.001 {
            discard;
        }
    }

    // The shape's distance, then one pixel of edge.
    var d: f32;
    if in.misc.y > 0.5 {
        d = length(p - (in.bounds.xy + in.bounds.zw * 0.5)) - min(in.bounds.z, in.bounds.w) * 0.5;
    } else {
        d = sd_convex_rect(p, in.bounds.xy, in.bounds.zw, in.radius, in.shape);
    }
    let fill_alpha = 1.0 - smoothstep(-0.5, 0.5, d);
    if fill_alpha < 0.001 {
        discard;
    }

    var fill = in.color;

    // A uniform border is the ring between the shape and the shape inset by its width. Inside
    // the inset the fill is untouched, so only the edge pays for the blend.
    let border = in.misc.x;
    if border > 0.0 && d > -border - 0.5 {
        let border_blend = smoothstep(-0.5, 0.5, d + border);
        let border_a = in.border_color.a * border_blend;
        let out_a = border_a + fill.a * (1.0 - border_a);
        let premult_rgb = in.border_color.rgb * border_a + fill.rgb * fill.a * (1.0 - border_a);
        let rgb = select(premult_rgb / max(out_a, 0.0001), vec3<f32>(0.0), out_a < 0.0001);
        fill = vec4<f32>(rgb, out_a);
    }

    return vec4<f32>(fill.rgb, fill.a * clip_alpha * fill_alpha);
}
