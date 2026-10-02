// ============================================================================
// Vertex Buffer fallback variant — vs_main reads per-instance fields from
// an instance-stepped vertex buffer instead of indexing into the storage
// buffer. Used when the GPU adapter lacks VERTEX_STORAGE support.
// Blinc SDF Core Shader — Rect, Circle, Ellipse
// ============================================================================
// Handles prim_type 0 (Rect), 1 (Circle), 2 (Ellipse) with full
// anti-aliasing, borders, shadows-behind-shape, gradients, mask
// gradients, CSS filters, clip paths, and CSS perspective.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) instance_index: u32,
}

struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
}

// Enum values used throughout this shader. They're inlined as
// literals at every use site rather than declared as `const` at
// module scope so naga's WGSL→MSL backend doesn't emit orphaned
// `constant uint NAME = <value>;` declarations that trip the Metal
// shader compiler's `-Wunused-const-variable` pass at runtime.
// naga constant-folds every reference, so the const symbols would
// have no surviving users in the generated MSL source.
//
// Primitive types (field: type_info.x, variable: prim_type)
//   0u = RECT                 1u = CIRCLE              2u = ELLIPSE
//
// Fill types (field: type_info.y, variable: fill_type)
//   0u = SOLID                1u = LINEAR_GRADIENT     2u = RADIAL_GRADIENT
//
// Clip types (field: type_info.z, variable: clip_type)
//   0u = NONE                 1u = RECT                2u = CIRCLE
//   3u = ELLIPSE              4u = POLYGON

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
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> primitives: array<Primitive>;
// Glyph atlas textures for unified text rendering
@group(0) @binding(2) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(3) var glyph_sampler: sampler;
@group(0) @binding(4) var color_glyph_atlas: texture_2d<f32>;
// Auxiliary data buffer for variable-length per-primitive data
// (3D group shape descriptors, polygon clip vertices, etc.)
@group(0) @binding(5) var<storage, read> aux_data: array<vec4<f32>>;

// ============================================================================
// Vertex Shader
// ============================================================================

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
    @location(0) vb_bounds: vec4<f32>,
    @location(1) vb_shadow: vec4<f32>,
    @location(2) vb_rotation: vec4<f32>,
    @location(3) vb_perspective: vec4<f32>,
    @location(4) vb_sdf_3d: vec4<f32>,
    @location(5) vb_local_affine: vec4<f32>,
    @location(6) vb_corner_radius: vec4<f32>,
    @location(7) vb_light: vec4<f32>,
) -> VertexOutput {
    var out: VertexOutput;

    // Expand bounds for shadow blur
    let blur_expand = vb_shadow.z * 3.0 + abs(vb_shadow.x) + abs(vb_shadow.y);

    // Check for rotation, skew, and 3D transforms.
    let sin_rz = vb_rotation.x;
    let cos_rz = vb_rotation.y;
    let sin_ry = vb_rotation.z;
    let cos_ry = vb_rotation.w;
    let sin_rx = vb_perspective.x;
    let cos_rx = vb_perspective.y;
    let persp_d = vb_perspective.z;
    let la = vb_local_affine; // [a, b, c, d] of normalized 2x2 affine
    let has_3d = abs(sin_ry) > 0.0001 || abs(sin_rx) > 0.0001 || persp_d > 0.001;
    // Check if local_affine is non-identity (rotation, skew, or non-uniform scale)
    let has_local_affine = abs(la.x - 1.0) > 0.0001 || abs(la.y) > 0.0001
                        || abs(la.z) > 0.0001 || abs(la.w - 1.0) > 0.0001;

    var bounds: vec4<f32>;
    if has_3d {
        // 3D perspective: project all 8 corners of the 3D bounding box to find AABB
        let ctr = vb_bounds.xy + vb_bounds.zw * 0.5;
        let half = vb_bounds.zw * 0.5;
        let half_d = vb_sdf_3d.x * 0.5; // half-depth
        let corners3d = array<vec3<f32>, 8>(
            vec3<f32>(-half.x, -half.y, -half_d),
            vec3<f32>( half.x, -half.y, -half_d),
            vec3<f32>( half.x,  half.y, -half_d),
            vec3<f32>(-half.x,  half.y, -half_d),
            vec3<f32>(-half.x, -half.y,  half_d),
            vec3<f32>( half.x, -half.y,  half_d),
            vec3<f32>( half.x,  half.y,  half_d),
            vec3<f32>(-half.x,  half.y,  half_d),
        );
        var min_p = vec2<f32>(1e10);
        var max_p = vec2<f32>(-1e10);
        let pd = select(800.0, persp_d, persp_d > 0.001);
        for (var i = 0u; i < 8u; i++) {
            let c = corners3d[i];
            // Apply rotateZ
            let rz_x = c.x * cos_rz - c.y * sin_rz;
            let rz_y = c.x * sin_rz + c.y * cos_rz;
            let rz_z = c.z;
            // Apply rotateX (tilt Y/Z)
            let rx_y = rz_y * cos_rx - rz_z * sin_rx;
            let rx_z = rz_y * sin_rx + rz_z * cos_rx;
            // Apply rotateY (tilt X/Z)
            let ry_x = rz_x * cos_ry + rx_z * sin_ry;
            let ry_z = -rz_x * sin_ry + rx_z * cos_ry;
            // Perspective divide
            let w = 1.0 - ry_z / pd;
            let proj = vec2<f32>(ry_x, rx_y) / max(w, 0.001);
            min_p = min(min_p, proj);
            max_p = max(max_p, proj);
        }
        min_p -= vec2<f32>(blur_expand + 2.0);
        max_p += vec2<f32>(blur_expand + 2.0);
        bounds = vec4<f32>(ctr + min_p, max_p - min_p);
    } else if has_local_affine {
        // General 2D affine (rotation, skew, non-uniform scale):
        // Transform the 4 corners of the local rect by the local_affine to find AABB
        let center = vb_bounds.xy + vb_bounds.zw * 0.5;
        let hw = vb_bounds.z * 0.5;
        let hh = vb_bounds.w * 0.5;
        // Transform corners: la * (±hw, ±hh)
        // new_x = la.x * cx + la.z * cy, new_y = la.y * cx + la.w * cy
        let c0x = la.x * hw + la.z * hh;
        let c0y = la.y * hw + la.w * hh;
        let c1x = -la.x * hw + la.z * hh;
        let c1y = -la.y * hw + la.w * hh;
        let aabb_hw = max(abs(c0x), abs(c1x)) + blur_expand;
        let aabb_hh = max(abs(c0y), abs(c1y)) + blur_expand;
        bounds = vec4<f32>(center.x - aabb_hw, center.y - aabb_hh, aabb_hw * 2.0, aabb_hh * 2.0);
    } else {
        bounds = vec4<f32>(
            vb_bounds.x - blur_expand,
            vb_bounds.y - blur_expand,
            vb_bounds.z + blur_expand * 2.0,
            vb_bounds.w + blur_expand * 2.0
        );
    }

    // Generate quad vertices (two triangles split along / diagonal)
    // 0--1
    // |\ |
    // | \|
    // 3--2
    // Triangle 1: 0 -> 1 -> 3 (TL -> TR -> BL) - upper-left triangle
    // Triangle 2: 1 -> 2 -> 3 (TR -> BR -> BL) - lower-right triangle
    // Shared edge: 1-3 (top-right to bottom-left = / diagonal)
    //
    // PowerVR Vulkan codegen bug workaround: dynamic indexing into a
    // `let array<...>(literal)` produces an `OpConstantComposite` +
    // `OpAccessChain` pattern that the Pixel 10 Pro / Tensor G5
    // PowerVR driver compiles incorrectly — vertex_index 0..2 work
    // but 3..5 silently produce degenerate output, collapsing the
    // second triangle to a point and leaving every primitive a
    // half-quad. Replacing the array-literal indexing with an explicit
    // `switch` forces naga to emit `OpSwitch`, which the driver
    // handles correctly. Confirmed on Android 16 / driver 25.1@6794074.
    var uv: vec2<f32>;
    switch vertex_index {
        case 0u: { uv = vec2<f32>(0.0, 0.0); } // 0 - top-left
        case 1u: { uv = vec2<f32>(1.0, 0.0); } // 1 - top-right
        case 2u: { uv = vec2<f32>(0.0, 1.0); } // 3 - bottom-left
        case 3u: { uv = vec2<f32>(1.0, 0.0); } // 1 - top-right
        case 4u: { uv = vec2<f32>(1.0, 1.0); } // 2 - bottom-right
        default: { uv = vec2<f32>(0.0, 1.0); } // 3 - bottom-left
    }
    let pos = vec2<f32>(
        bounds.x + uv.x * bounds.z,
        bounds.y + uv.y * bounds.w
    );

    // Convert to clip space (-1 to 1)
    let clip_pos = vec2<f32>(
        (pos.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (pos.y / uniforms.viewport_size.y) * 2.0
    );

    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.uv = pos; // Pass world position for SDF calculation
    out.instance_index = instance_index;

    return out;
}

// ============================================================================
// SDF Functions
// ============================================================================

// Rounded rectangle SDF
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

// Shaped rectangle SDF with per-corner superellipse parameter
// shape.xyzw = superellipse n for (top-left, top-right, bottom-right, bottom-left)
// n=1.0 = round (circle), n=0.0 = bevel, n=2.0 = squircle
// n>=100.0 = square, n<=-100.0 = notch, n<0 = concave (scoop)
// Distance to the superellipse |t|_p = 1, scaled by r.
//
// `(|t|_p - 1) * r` is a level set, not a distance: its gradient is
// sqrt(2) for p = 1 and drifts with p, which made the bevel's border
// ~30% thin and the AA width uneven around squircles. Dividing by the
// norm's own gradient length normalises it.
//
// The largest component is factored out because `t.x^p + t.y^p`
// underflows f32 once p reaches 8, which made f zero, the gradient term
// infinite and the distance collapse to -0.
//
// Callers pass a non-negative t.
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
    let p_exp = pow(2.0, min(abs(n), 5.0));
    let t = max(qa, vec2<f32>(0.0)) / rr;
    return max(box_d, superellipse_dist(t, p_exp, r));
}

// Circle SDF
fn sd_circle(p: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    return length(p - center) - radius;
}

// Ellipse SDF (approximation)
fn sd_ellipse(p: vec2<f32>, center: vec2<f32>, radii: vec2<f32>) -> f32 {
    let p_centered = p - center;
    let p_norm = p_centered / radii;
    let dist = length(p_norm);
    return (dist - 1.0) * min(radii.x, radii.y);
}

// Quarter ellipse SDF for inner corners with asymmetric borders (GPUI approach)
// This handles the case where adjacent border widths differ, creating an elliptical
// inner corner instead of circular. Returns negative inside, positive outside.
fn quarter_ellipse_sdf(point: vec2<f32>, radii: vec2<f32>) -> f32 {
    // Avoid division by zero
    let safe_radii = max(radii, vec2<f32>(0.001));
    // Map to unit circle space
    let circle_vec = point / safe_radii;
    let unit_circle_sdf = length(circle_vec) - 1.0;
    // Scale back using average radius for distance approximation
    return unit_circle_sdf * (safe_radii.x + safe_radii.y) * -0.5;
}

// Error function approximation for Gaussian blur
fn erf(x: f32) -> f32 {
    let s = sign(x);
    let a = abs(x);
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
    return s * y;
}

// Gaussian shadow for rounded rectangle - uses SDF for proper corner handling
fn shadow_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, corner_radius: vec4<f32>, sigma: f32) -> f32 {
    // Get signed distance to the rounded rectangle
    let sdf_dist = sd_rounded_rect(p, origin, size, corner_radius);

    if sigma < 0.001 {
        // No blur - use hard edge
        return select(0.0, 1.0, sdf_dist < 0.0);
    }

    // Gaussian falloff based on SDF distance
    // Same approach as shadow_circle: 1 inside, Gaussian falloff outside
    let d = 0.5 * sqrt(2.0) * sigma;
    return 0.5 * (1.0 + erf(-sdf_dist / d));
}

// Gaussian shadow for circle - radially symmetric blur
fn shadow_circle(p: vec2<f32>, center: vec2<f32>, radius: f32, sigma: f32) -> f32 {
    let dist = length(p - center);

    if sigma < 0.001 {
        // No blur - hard edge
        return select(0.0, 1.0, dist < radius);
    }

    // Gaussian falloff from circle edge
    // erf gives cumulative distribution, we want shadow inside and fading outside
    let d = 0.5 * sqrt(2.0) * sigma;
    return 0.5 * (1.0 + erf((radius - dist) / d));
}

// Calculate clip alpha (1.0 = inside clip, 0.0 = outside)
// For non-rect clips (circle, ellipse, polygon):
//   clip_bounds = rect scissor from parent clips [x, y, w, h]
//   clip_radius = shape-specific data
// The shader applies BOTH the rect scissor AND the shape clip.
// clip_fade = (top, right, bottom, left) overflow fade distances in pixels
fn calculate_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>, clip_radius: vec4<f32>, clip_corner_shape: vec4<f32>, clip_type: u32, clip_fade: vec4<f32>) -> f32 {
    var alpha: f32 = 1.0;

    if clip_type != 0u {
        let aa_width = 0.75;
        switch clip_type {
            case 1u /* CLIP_RECT */: {
                let clip_origin = clip_bounds.xy;
                let clip_size = clip_bounds.zw;
                let clip_d = sd_shaped_rect(p, clip_origin, clip_size, clip_radius, clip_corner_shape);
                alpha = 1.0 - smoothstep(-aa_width, aa_width, clip_d);
            }
            case 2u /* CLIP_CIRCLE */: {
                let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, vec4<f32>(0.0));
                let scissor_alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
                let center = clip_radius.xy;
                let radius = clip_radius.z;
                let clip_d = sd_circle(p, center, radius);
                let shape_alpha = 1.0 - smoothstep(-aa_width, aa_width, clip_d);
                alpha = scissor_alpha * shape_alpha;
            }
            case 3u /* CLIP_ELLIPSE */: {
                let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, vec4<f32>(0.0));
                let scissor_alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
                let center = clip_radius.xy;
                let radii = clip_radius.zw;
                let clip_d = sd_ellipse(p, center, radii);
                let shape_alpha = 1.0 - smoothstep(-aa_width, aa_width, clip_d);
                alpha = scissor_alpha * shape_alpha;
            }
            case 4u /* CLIP_POLYGON */: {
                // Scissor-only here; polygon shape test deferred to fs_main
                // after sp computation. See sdf_core.wgsl for rationale.
                let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, vec4<f32>(0.0));
                alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
            }
            default: {}
        }
    }

    // Apply overflow fade (smooth alpha ramp at clip edges)
    if clip_fade.x > 0.0 || clip_fade.y > 0.0 || clip_fade.z > 0.0 || clip_fade.w > 0.0 {
        let clip_min = clip_bounds.xy;
        let clip_max = clip_bounds.xy + clip_bounds.zw;
        if clip_fade.x > 0.0 { alpha *= saturate((p.y - clip_min.y) / clip_fade.x); }  // top
        if clip_fade.y > 0.0 { alpha *= saturate((clip_max.x - p.x) / clip_fade.y); }  // right
        if clip_fade.z > 0.0 { alpha *= saturate((clip_max.y - p.y) / clip_fade.z); }  // bottom
        if clip_fade.w > 0.0 { alpha *= saturate((p.x - clip_min.x) / clip_fade.w); }  // left
    }

    return alpha;
}

// Polygon clip using winding number test with edge-distance anti-aliasing.
// Vertices packed in aux_data as vec4(x0, y0, x1, y1) — 2 vertices per vec4.
fn calculate_polygon_clip_alpha(p: vec2<f32>, vertex_count: u32, aux_offset: u32) -> f32 {
    if vertex_count < 3u {
        return 1.0;
    }

    var winding: i32 = 0;
    var min_edge_dist: f32 = 1e10;

    for (var i: u32 = 0u; i < vertex_count; i = i + 1u) {
        // Read vertex i: packed as (x0, y0, x1, y1) per vec4
        let vec_idx = aux_offset + (i / 2u);
        let data = aux_data[vec_idx];
        var vi: vec2<f32>;
        if (i % 2u) == 0u {
            vi = data.xy;
        } else {
            vi = data.zw;
        }

        // Read vertex j (next, wrapping)
        let j = (i + 1u) % vertex_count;
        let vec_idx_j = aux_offset + (j / 2u);
        let data_j = aux_data[vec_idx_j];
        var vj: vec2<f32>;
        if (j % 2u) == 0u {
            vj = data_j.xy;
        } else {
            vj = data_j.zw;
        }

        // Winding number contribution (crossing number test)
        let edge = vj - vi;
        if vi.y <= p.y {
            if vj.y > p.y {
                // Upward crossing
                let cross_val = edge.x * (p.y - vi.y) - edge.y * (p.x - vi.x);
                if cross_val > 0.0 {
                    winding = winding + 1;
                }
            }
        } else {
            if vj.y <= p.y {
                // Downward crossing
                let cross_val = edge.x * (p.y - vi.y) - edge.y * (p.x - vi.x);
                if cross_val < 0.0 {
                    winding = winding - 1;
                }
            }
        }

        // Minimum distance to this edge segment (for anti-aliasing)
        let ap = p - vi;
        let edge_len_sq = dot(edge, edge);
        var t: f32 = 0.0;
        if edge_len_sq > 0.0001 {
            t = clamp(dot(ap, edge) / edge_len_sq, 0.0, 1.0);
        }
        let closest = vi + edge * t;
        let dist = length(p - closest);
        min_edge_dist = min(min_edge_dist, dist);
    }

    // Inside if winding number is non-zero
    let is_inside = winding != 0;

    // Signed distance: negative inside, positive outside
    let signed_dist = select(min_edge_dist, -min_edge_dist, is_inside);

    // Anti-aliased edge
    let aa_width = 0.75;
    return 1.0 - smoothstep(-aa_width, aa_width, signed_dist);
}

// ============================================================================
// CSS Filter Functions
// ============================================================================

/// Apply CSS filter effects to a color.
/// filter_a = (grayscale, invert, sepia, hue_rotate_rad)
/// filter_b = (brightness, contrast, saturate, 0)
fn apply_css_filter(color: vec4<f32>, filter_a: vec4<f32>, filter_b: vec4<f32>) -> vec4<f32> {
    var rgb = color.rgb;

    // Grayscale: desaturate using luminance weights
    let grayscale = filter_a.x;
    if grayscale > 0.0 {
        let lum = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        rgb = mix(rgb, vec3<f32>(lum, lum, lum), grayscale);
    }

    // Sepia: apply sepia tone matrix
    let sepia = filter_a.z;
    if sepia > 0.0 {
        let sepia_r = dot(rgb, vec3<f32>(0.393, 0.769, 0.189));
        let sepia_g = dot(rgb, vec3<f32>(0.349, 0.686, 0.168));
        let sepia_b = dot(rgb, vec3<f32>(0.272, 0.534, 0.131));
        rgb = mix(rgb, vec3<f32>(sepia_r, sepia_g, sepia_b), sepia);
    }

    // Invert
    let invert = filter_a.y;
    if invert > 0.0 {
        rgb = mix(rgb, vec3<f32>(1.0) - rgb, invert);
    }

    // Hue-rotate: rotate in RGB space using rotation matrix
    let hue_rad = filter_a.w;
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
    let brightness = filter_b.x;
    rgb = rgb * brightness;

    // Contrast
    let contrast = filter_b.y;
    rgb = (rgb - vec3<f32>(0.5)) * contrast + vec3<f32>(0.5);

    // Saturate
    let saturate = filter_b.z;
    if abs(saturate - 1.0) > 0.001 {
        let lum = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        rgb = mix(vec3<f32>(lum, lum, lum), rgb, saturate);
    }

    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), color.a);
}

// ============================================================================
// Fragment Shader
// ============================================================================

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let prim = primitives[in.instance_index];
    let p = in.uv;

    // Screen-space derivative magnitude, computed up-front *outside*
    // any control flow that depends on per-instance data. WGSL strictly
    // requires `fwidth` / `dpdx` / `dpdy` to be called from uniform
    // control flow; computing it here on the continuously-interpolated
    // `in.uv` (which is uniform across every 2x2 pixel quad regardless
    // of which primitive a quad belongs to) satisfies the rule.
    //
    // For an axis-aligned screen-space SDF this is identical to
    // `fwidth(d)`: `d` is locally 1-Lipschitz in pixels, so
    // `length(vec2(fwidth(p.x), fwidth(p.y)))` returns ~1.0 for
    // axis-aligned edges and ~1.41 for 45° diagonals — the same range
    // the previous `fwidth(d)` produced. Native backends (Metal,
    // Vulkan, DX12) didn't enforce the uniformity rule so the old
    // `fwidth(d)` worked there; Dawn (Chrome's WebGPU validator)
    // rejects it because `d` is computed inside a switch that branches
    // on a non-uniform `prim_type`. See
    // https://www.w3.org/TR/WGSL/#uniformity for the spec rule.
    let d_fw_screen = length(vec2<f32>(fwidth(p.x), fwidth(p.y)));

    let prim_type = prim.type_info.x;
    let fill_type = prim.type_info.y;
    let clip_type = prim.type_info.z;

    // Early type filter — discard primitives handled by other split pipelines.
    // Allow prim_type 7 (text) — transformed text glyphs are rendered through
    // the SDF pipeline with css_affine, not the separate text pipeline.
    // Allow prim_type 9 (mesh triangle) — see `sdf_core.wgsl` for rationale.
    if prim_type > 2u && prim_type != 7u && prim_type != 9u { discard; }

    // Early clip test - discard if completely outside clip region (screen space).
    // Polygon shape test deferred to after sp computation — see sdf_core.wgsl.
    var clip_alpha = calculate_clip_alpha(p, prim.clip_bounds, prim.clip_radius, prim.clip_corner_shape, clip_type, prim.clip_fade);
    if clip_alpha < 0.001 {
        discard;
    }

    let origin = prim.bounds.xy;
    let size = prim.bounds.zw;
    let center = origin + size * 0.5;

    // Extract rotation and perspective parameters
    let sin_rz = prim.rotation.x;
    let cos_rz = prim.rotation.y;
    let sin_ry = prim.rotation.z;
    let cos_ry = prim.rotation.w;
    let sin_rx = prim.perspective.x;
    let cos_rx = prim.perspective.y;
    let persp_d = prim.perspective.z;
    let shape_type = u32(prim.perspective.w);
    let depth = prim.sdf_3d.x;

    let has_3d = abs(sin_ry) > 0.0001 || abs(sin_rx) > 0.0001 || persp_d > 0.001;

    // ── Perspective Unprojection (flat elements with 3D perspective) ──
    var sp = p;
    if has_3d {
        let pd = select(800.0, persp_d, persp_d > 0.001);
        let rel = p - center;

        // Inverse homography: map screen point back to element local coords
        let safe_cos_ry = max(abs(cos_ry), 0.0001) * sign(cos_ry + 0.0001);
        let safe_cos_rx = max(abs(cos_rx), 0.0001) * sign(cos_rx + 0.0001);
        let tan_ry = sin_ry / safe_cos_ry;
        let tan_rx = sin_rx / safe_cos_rx;

        let u = rel.x * cos_rz / safe_cos_ry + rel.y * (-cos_rz * tan_rx * tan_ry + sin_rz / safe_cos_rx);
        let v = rel.x * (-sin_rz) / safe_cos_ry + rel.y * (sin_rz * tan_rx * tan_ry + cos_rz / safe_cos_rx);
        let w = 1.0 - rel.x * tan_ry / pd + rel.y * tan_rx / (pd * safe_cos_ry);

        let safe_w = max(abs(w), 0.001) * sign(w + 0.001);
        sp = vec2<f32>(u / safe_w, v / safe_w) + center;
    } else {
        // 2D affine (rotation, skew, non-uniform scale) via inverse local_affine
        let la = prim.local_affine;
        let is_identity = abs(la.x - 1.0) < 0.0001 && abs(la.y) < 0.0001
                       && abs(la.z) < 0.0001 && abs(la.w - 1.0) < 0.0001;
        if !is_identity {
            let rel = p - center;
            // Compute inverse of 2x2 [a,b; c,d]: inv = [d,-b; -c,a] / det
            let det = la.x * la.w - la.y * la.z;
            let inv_det = select(-1.0, 1.0, det >= 0.0) / max(abs(det), 0.0001);
            let inv_a = la.w * inv_det;
            let inv_b = -la.y * inv_det;
            let inv_c = -la.z * inv_det;
            let inv_d = la.x * inv_det;
            sp = vec2<f32>(inv_a * rel.x + inv_c * rel.y, inv_b * rel.x + inv_d * rel.y) + center;
        }
    }

    // CLIP_POLYGON shape test in element-local coords (see sdf_core.wgsl).
    if clip_type == 4u {
        let vertex_count = u32(prim.clip_radius.z);
        let aux_offset = u32(prim.clip_radius.w);
        let local_p = sp - prim.bounds.xy;
        let shape_alpha = calculate_polygon_clip_alpha(local_p, vertex_count, aux_offset);
        clip_alpha = clip_alpha * shape_alpha;
        if clip_alpha < 0.001 {
            discard;
        }
    }

    var result = vec4<f32>(0.0);

    // Calculate shadow first (rendered behind)
    if prim.shadow.z > 0.0 || prim.shadow.w != 0.0 {
        let shadow_offset = prim.shadow.xy;
        let blur = prim.shadow.z;
        let spread = prim.shadow.w;

        let shadow_origin = origin + shadow_offset - vec2<f32>(spread);
        let shadow_size = size + vec2<f32>(spread * 2.0);

        // Adjust corner radii for spread (expand corners proportionally)
        let shadow_radii = prim.corner_radius + vec4<f32>(spread);

        var shadow_sdf_dist: f32;
        shadow_sdf_dist = sd_shaped_rect(sp, shadow_origin, shadow_size, shadow_radii, prim.corner_shape);
        var shadow_alpha: f32;
        if blur < 0.001 {
            shadow_alpha = select(0.0, 1.0, shadow_sdf_dist < 0.0);
        } else {
            let sigma_d = 0.5 * sqrt(2.0) * blur;
            shadow_alpha = 0.5 * (1.0 + erf(-shadow_sdf_dist / sigma_d));
        }

        let shadow_color = prim.shadow_color * shadow_alpha;

        // Premultiply and blend
        result = shadow_color;
    }

    // Calculate main shape SDF
    var d: f32;
    switch prim_type {
        case 0u /* PRIM_RECT */: {
            d = sd_shaped_rect(sp, origin, size, prim.corner_radius, prim.corner_shape);
        }
        case 1u /* PRIM_CIRCLE */: {
            let radius = min(size.x, size.y) * 0.5;
            d = sd_circle(sp, center, radius);
        }
        case 2u /* PRIM_ELLIPSE */: {
            d = sd_ellipse(sp, center, size * 0.5);
        }
        case 9u /* PRIM_MESH */: {
            // VB fallback path: the vertex stage can't reach
            // `aux_data` (no vertex attribute for it), so the quad
            // geometry covers the full triangle AABB. Fall back to
            // a per-pixel point-in-triangle test here — cheap
            // because each primitive holds exactly one triangle,
            // so this is one point-in-triangle per fragment, not a
            // per-pixel walk over every triangle in the mesh.
            let aux_off = u32(prim.border.z);
            let pack0 = aux_data[aux_off];
            let pack1 = aux_data[aux_off + 1u];
            let v0 = pack0.xy;
            let v1 = pack0.zw;
            let v2 = pack1.xy;
            let s0 = (v1.x - v0.x) * (sp.y - v0.y) - (v1.y - v0.y) * (sp.x - v0.x);
            let s1 = (v2.x - v1.x) * (sp.y - v1.y) - (v2.y - v1.y) * (sp.x - v1.x);
            let s2 = (v0.x - v2.x) * (sp.y - v2.y) - (v0.y - v2.y) * (sp.x - v2.x);
            let inside = (s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0)
                      || (s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0);
            d = select(1.0, -1.0, inside);
        }
        case 7u /* PRIM_TEXT */: {
            // `gradient_params` holds atlas PIXEL coords (px_min,
            // py_min, px_max, py_max). UV computed from CURRENT
            // atlas dimensions — see sdf_core.wgsl for the full
            // rationale.
            let pixel_bounds = prim.gradient_params;
            let is_color = fill_type == 1u;
            var atlas_size: vec2<f32>;
            if is_color {
                atlas_size = vec2<f32>(textureDimensions(color_glyph_atlas));
            } else {
                atlas_size = vec2<f32>(textureDimensions(glyph_atlas));
            }
            let uv_bounds = pixel_bounds / vec4<f32>(
                atlas_size.x, atlas_size.y, atlas_size.x, atlas_size.y
            );
            let local_uv = (sp - origin) / size;
            let atlas_uv = uv_bounds.xy + local_uv * (uv_bounds.zw - uv_bounds.xy);

            var text_result: vec4<f32>;
            if is_color {
                text_result = textureSampleLevel(color_glyph_atlas, glyph_sampler, atlas_uv, 0.0);
            } else {
                let coverage = textureSampleLevel(glyph_atlas, glyph_sampler, atlas_uv, 0.0).r;
                let gamma_coverage = pow(coverage, 0.7);
                text_result = vec4<f32>(prim.color.rgb, prim.color.a * gamma_coverage);
            }

            text_result.a *= clip_alpha;

            // Rect-edge AA from `clip_bounds`. Only apply when the
            // primitive actually has a rect clip — when `clip_type`
            // is None the bounds are padding / stale metadata and
            // this smoothstep would discard every fragment for a
            // degenerate (zero-width or zero-height) rectangle.
            if clip_type == 1u {
                let edge_aa = 1.0;
                let clip_edge_alpha = smoothstep(0.0, edge_aa, min(
                    min(p.x - prim.clip_bounds.x, prim.clip_bounds.x + prim.clip_bounds.z - p.x),
                    min(p.y - prim.clip_bounds.y, prim.clip_bounds.y + prim.clip_bounds.w - p.y)
                ));
                text_result.a *= clip_edge_alpha;
            }

            return text_result;
        }
        default: {
            d = sd_shaped_rect(sp, origin, size, prim.corner_radius, prim.corner_shape);
        }
    }

    // Anti-aliasing: smooth transition at edge over ~1 pixel total.
    //
    // `aa_width = 0.5` means the `smoothstep` range is `[-0.5, 0.5]` —
    // one pixel wide, centered on the shape boundary. The fragment
    // shader is evaluated at pixel CENTERS (integer + 0.5), so for an
    // axis-aligned edge at integer y, pixels at `y ± 0.5` end up at
    // `d = ∓0.5`, which `smoothstep(-0.5, 0.5, ±0.5)` saturates to 1
    // and 0 respectively. That matters for two reasons:
    //
    //   1. **No seams between adjacent same-colored elements.** When
    //      two fills meet at an integer-pixel line (e.g. menu bar
    //      bottom + notch dropdown top), both sides of the boundary
    //      now reach full alpha at their own pixel-center and don't
    //      double-composite with partial alpha — the old wider AA
    //      (`d_fw * 0.75 ≈ 1.06`) left them at `alpha ≈ 0.83`, which
    //      let ~14% of the background bleed through at the shared
    //      edge as a hairline seam.
    //
    //   2. **Pixel-accurate rect bounds.** A rect with integer pixel
    //      bounds now covers exactly the right pixels with alpha 1,
    //      matching what a non-AA rasterizer would produce. Previously
    //      the AA zone extended half a pixel into each neighboring
    //      row/column.
    //
    // The trade-off is that diagonal edges lose a fraction of a pixel
    // of AA softness — they still transition over ~1 pixel in the
    // gradient direction but the transition is a touch tighter than
    // the old 2.12-pixel zone. Subpixel accuracy is preserved because
    // the SDF is Euclidean (|∇d| = 1) so the 1-pixel smoothstep range
    // matches the true distance-to-edge for every orientation.
    //
    // `d_fw_screen` (=`length(fwidth(p))`) isn't used for the width
    // computation anymore — it's still computed at the top of fs_main
    // because some upstream branches (shadow blur, etc.) read it.
    _ = d_fw_screen;
    let aa_width = 0.5;
    let fill_alpha = 1.0 - smoothstep(-aa_width, aa_width, d);

    if fill_alpha < 0.001 {
        return result;
    }

    // Determine fill color
    var fill_color: vec4<f32>;
    // PRIM_MESH with `type_info.w == 1u` carries per-vertex colours
    // after the triangle positions in aux_data. The VB variant can't
    // use a `mesh_bary` varying (vertex stage doesn't reach
    // aux_data) — compute barycentric weights from the per-fragment
    // triangle position. See `sdf_core.wgsl` for the design
    // rationale.
    if prim.type_info.x == 9u && prim.type_info.w == 1u {
        let aux_off = u32(prim.border.z);
        let pack0 = aux_data[aux_off];
        let pack1 = aux_data[aux_off + 1u];
        let v0 = pack0.xy;
        let v1 = pack0.zw;
        let v2 = pack1.xy;
        let c0 = aux_data[aux_off + 2u];
        let c1 = aux_data[aux_off + 3u];
        let c2 = aux_data[aux_off + 4u];
        let denom = (v1.y - v2.y) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.y - v2.y);
        let safe_denom = select(denom, 1e-6, abs(denom) < 1e-6);
        let w0 = ((v1.y - v2.y) * (sp.x - v2.x) + (v2.x - v1.x) * (sp.y - v2.y)) / safe_denom;
        let w1 = ((v2.y - v0.y) * (sp.x - v2.x) + (v0.x - v2.x) * (sp.y - v2.y)) / safe_denom;
        let w2 = 1.0 - w0 - w1;
        fill_color = c0 * w0 + c1 * w1 + c2 * w2;
    } else {
        switch fill_type {
            case 0u /* FILL_SOLID */: {
                fill_color = prim.color;
            }
            case 1u /* FILL_LINEAR_GRADIENT */: {
                // Linear gradient using gradient_params (x1, y1, x2, y2) in user space
                let g_start = prim.gradient_params.xy;
                let g_end = prim.gradient_params.zw;
                let g_dir = g_end - g_start;
                let g_len_sq = dot(g_dir, g_dir);

                var t: f32;
                if (g_len_sq > 0.0001) {
                    // Project current position onto gradient line
                    let proj = sp - g_start;
                    t = clamp(dot(proj, g_dir) / g_len_sq, 0.0, 1.0);
                } else {
                    t = 0.0;
                }
                fill_color = mix(prim.color, prim.color2, t);
            }
            case 2u /* FILL_RADIAL_GRADIENT */: {
                // Radial gradient using gradient_params (cx, cy, radius, 0) in user space
                let g_center = prim.gradient_params.xy;
                let g_radius = prim.gradient_params.z;

                let dist = length(sp - g_center);
                let t = clamp(dist / max(g_radius, 0.001), 0.0, 1.0);
                fill_color = mix(prim.color, prim.color2, t);
            }
            default: {
                fill_color = prim.color;
            }
        }
    }

    // Handle border with proper inner corner radii (GPUI-style approach)
    // The border is the ring between the outer shape edge and an inner shape
    // For asymmetric borders, inner corners become elliptical, not circular
    // prim.border = [top, right, bottom, left] for per-side borders, or [uniform, 0, 0, 0] for uniform
    let border_top = prim.border.x;
    let border_right = prim.border.y;
    let border_bottom = prim.border.z;
    let border_left = prim.border.w;

    // Check if any border is present (using max of all sides)
    // Skip for PRIM_MESH — see `sdf_core.wgsl` rationale (mesh
    // reuses border[1]/border[2] for triangle_count + aux_offset).
    let max_border = max(max(border_top, border_right), max(border_bottom, border_left));
    if max_border > 0.0 && prim_type != 9u {
        // For uniform border (legacy: only .x set), use it for all sides
        let bt = select(border_top, border_top, border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0);
        let br = select(border_top, border_right, border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0);
        let bb = select(border_top, border_bottom, border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0);
        let bl = select(border_top, border_left, border_right > 0.0 || border_bottom > 0.0 || border_left > 0.0);

        let half_size = size * 0.5;
        let rel = sp - center;  // Position relative to center (signed, in unrotated space)

        // Use the same AA width as the outer edge smoothstep so the
        // border's inner transition matches the fill's outer transition
        // pixel-for-pixel. Currently 0.5 (tight 1-pixel AA — see the
        // longer rationale above `aa_width`).
        let border_aa = aa_width;

        // Select corner radius and corner shape based on quadrant
        var corner_radius: f32;
        var corner_n: f32;
        if rel.y < 0.0 {
            if rel.x > 0.0 { corner_radius = prim.corner_radius.y; corner_n = prim.corner_shape.y; }  // top-right
            else { corner_radius = prim.corner_radius.x; corner_n = prim.corner_shape.x; }           // top-left
        } else {
            if rel.x > 0.0 { corner_radius = prim.corner_radius.z; corner_n = prim.corner_shape.z; }  // bottom-right
            else { corner_radius = prim.corner_radius.w; corner_n = prim.corner_shape.w; }           // bottom-left
        }
        // Clamp radius to half the minimum dimension (CSS spec)
        corner_radius = min(corner_radius, min(half_size.x, half_size.y));

        // Select border widths for nearest edges based on quadrant (GPUI approach)
        let border = vec2<f32>(
            select(br, bl, rel.x < 0.0),  // horizontal: left or right
            select(bb, bt, rel.y < 0.0)   // vertical: top or bottom
        );

        // Handle zero-width borders (treat as negative for AA purposes)
        let reduced_border = vec2<f32>(
            select(border.x, -border_aa, border.x == 0.0),
            select(border.y, -border_aa, border.y == 0.0)
        );

        // Calculate position relative to corner
        let corner_to_point = abs(rel) - half_size;
        let corner_center_to_point = corner_to_point + corner_radius;

        // Determine if we're near a rounded corner
        let is_near_rounded_corner = corner_center_to_point.x >= 0.0 && corner_center_to_point.y >= 0.0;

        // Inner straight border edge
        let straight_border_inner = corner_to_point + reduced_border;

        // Check if we're clearly inside the inner area (not near border)
        let is_within_inner_straight = straight_border_inner.x < -border_aa &&
                                       straight_border_inner.y < -border_aa;

        // Fast path: clearly inside inner area, not near rounded corner.
        // A concave corner's edges lie inside the straight-inner box and
        // outside the corner square, which is exactly what the fast path
        // skips -- so a scoop's arc ends and a notch's step faces drew no
        // border at all. Send them to the SDF branch.
        let is_concave_corner = corner_n < 0.0;

        if is_within_inner_straight && !is_near_rounded_corner && !is_concave_corner {
            // No border here, keep fill_color as-is
        } else {
            // Calculate inner SDF based on context
            var inner_sdf: f32;

            if is_concave_corner {
                // The asymmetric branch below insets the corner RADIUS,
                // which only means something for a convex corner. Now
                // that `d` is a true distance, offsetting it is exact for
                // a uniform width; with differing widths take the smaller
                // so the border never exceeds what was asked for.
                inner_sdf = -(d + min(reduced_border.x, reduced_border.y));
            } else if abs(reduced_border.x - reduced_border.y) < 0.001 {
                // Uniform border — use exact SDF offset of the outer shape distance.
                // For rounded rects (Minkowski sum of box + circle), inset by a constant
                // produces another rounded rect. This is branchless, handles both straight
                // edges and corners, and is guaranteed continuous with the outer SDF.
                inner_sdf = -(d + reduced_border.x);
            } else {
                // Asymmetric borders — inner corners become elliptical
                if corner_center_to_point.x <= 0.0 || corner_center_to_point.y <= 0.0 {
                    // Not in corner region — straight edge distance
                    inner_sdf = -max(straight_border_inner.x, straight_border_inner.y);
                } else if abs(corner_n - 1.0) < 0.01 {
                    // Round corner — elliptical inner corner (GPUI approach)
                    let ellipse_radii = max(vec2<f32>(0.0), vec2<f32>(corner_radius) - reduced_border);
                    inner_sdf = quarter_ellipse_sdf(corner_center_to_point, ellipse_radii);
                } else {
                    // Superellipse with per-axis reduced radii
                    let inner_radii = max(vec2<f32>(0.0), vec2<f32>(corner_radius) - reduced_border);
                    let p_exp = pow(2.0, min(abs(corner_n), 5.0));
                    let min_inner_r = min(inner_radii.x, inner_radii.y);
                    if min_inner_r < 0.001 {
                        inner_sdf = -length(max(vec2<f32>(0.0), corner_center_to_point));
                    } else {
                        let inner_t = corner_center_to_point / inner_radii;
                        let inner_se = pow(max(inner_t.x, 0.0), p_exp) + pow(max(inner_t.y, 0.0), p_exp);
                        let inner_r_scale = sqrt(inner_radii.x * inner_radii.y);
                        inner_sdf = -((pow(inner_se, 1.0 / p_exp) - 1.0) * inner_r_scale);
                    }
                }
            }

            // Match the main fill's tight 1-pixel AA (see `aa_width`
            // above) so the inner border edge shares the same
            // transition width as the outer shape edge. Wider AA here
            // would create a visible gap between the border fill and
            // the main fill at pixel boundaries.
            let inner_aa = aa_width;
            let border_blend = smoothstep(-inner_aa, inner_aa, -inner_sdf);

            // Only apply border color where we're inside the shape
            fill_color = mix(fill_color, prim.border_color, border_blend * step(0.001, fill_alpha));
        }
    }

    // Apply clip alpha to shadow
    result.a *= clip_alpha;

    // Mask shadow strictly outside the shape boundary
    // Use the same aa_width as fill_alpha to prevent gaps at corners
    // The shadow should render only where d > 0 (outside the shape)
    if result.a > 0.0 {
        // Use matching AA width to ensure shadow and fill meet seamlessly
        let shadow_mask = smoothstep(-aa_width, aa_width, d);
        result.a *= shadow_mask;
    }

    // Blend fill over shadow at FULL opacity first (fill fully covers shadow)
    // This ensures no shadow bleeds through the shape regardless of edge AA
    let full_fill = vec4<f32>(fill_color.rgb, fill_color.a * clip_alpha);
    result = full_fill + result * (1.0 - full_fill.a);

    // NOW apply outer edge anti-aliasing to the combined result
    // This gives smooth edges against the background without shadow bleed
    result.a *= fill_alpha;

    // Apply mask gradient (mask-image: linear-gradient / radial-gradient)
    // mask_info.x: 0=none, 1=linear, 2=radial
    // mask_params are in OBB (0-1) space relative to element bounds
    let mask_type = prim.mask_info.x;
    if mask_type > 0.5 {
        // Compute normalized UV relative to element bounds
        let mask_uv = (sp - origin) / max(size, vec2<f32>(0.001));
        var mask_t: f32;
        if mask_type < 1.5 {
            // Linear mask gradient
            let m_start = prim.mask_params.xy;
            let m_end = prim.mask_params.zw;
            let m_dir = m_end - m_start;
            let m_len_sq = dot(m_dir, m_dir);
            if m_len_sq > 0.0001 {
                mask_t = clamp(dot(mask_uv - m_start, m_dir) / m_len_sq, 0.0, 1.0);
            } else {
                mask_t = 0.0;
            }
        } else {
            // Radial mask gradient
            let m_center = prim.mask_params.xy;
            let m_radius = prim.mask_params.z;
            mask_t = clamp(length(mask_uv - m_center) / max(m_radius, 0.001), 0.0, 1.0);
        }
        let mask_alpha = mix(prim.mask_info.y, prim.mask_info.z, mask_t);
        result = vec4<f32>(result.rgb * mask_alpha, result.a * mask_alpha);
    }

    // Apply CSS filters (grayscale, invert, sepia, hue-rotate, brightness, contrast, saturate)
    // Skip if all identity (filter_a all zero, filter_b = (1,1,1,0))
    let fa = prim.filter_a;
    let fb = prim.filter_b;
    if fa.x != 0.0 || fa.y != 0.0 || fa.z != 0.0 || abs(fa.w) > 0.001 || fb.x != 1.0 || fb.y != 1.0 || fb.z != 1.0 {
        result = apply_css_filter(result, fa, fb);
    }

    return result;
}
