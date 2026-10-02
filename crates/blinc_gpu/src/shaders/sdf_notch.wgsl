// ============================================================================
// Blinc SDF Notch Primitive Shader
//
// Handles prim_type 8 (Notch) — concave corners with edge modifiers.
// ============================================================================

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
//   3u = SHADOW               4u = INNER_SHADOW        5u = CIRCLE_SHADOW
//   6u = CIRCLE_INNER_SHADOW  7u = TEXT (glyph atlas)
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
) -> VertexOutput {
    var out: VertexOutput;

    let prim = primitives[instance_index];

    // Expand bounds for shadow blur
    let blur_expand = prim.shadow.z * 3.0 + abs(prim.shadow.x) + abs(prim.shadow.y);

    let la = prim.local_affine; // [a, b, c, d] of normalized 2x2 affine
    // Check if local_affine is non-identity (rotation, skew, or non-uniform scale)
    let has_local_affine = abs(la.x - 1.0) > 0.0001 || abs(la.y) > 0.0001
                        || abs(la.z) > 0.0001 || abs(la.w - 1.0) > 0.0001;

    var bounds: vec4<f32>;
    if has_local_affine {
        // General 2D affine (rotation, skew, non-uniform scale):
        // Transform the 4 corners of the local rect by the local_affine to find AABB
        let center = prim.bounds.xy + prim.bounds.zw * 0.5;
        let hw = prim.bounds.z * 0.5;
        let hh = prim.bounds.w * 0.5;
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
        // Notches can reach outside `prim.bounds` via concave corners,
        // top/bottom bulge, or top/bottom peak modifiers. The SDF in fs_main
        // returns negative for those exterior pixels, but the fragment shader
        // only runs on pixels covered by this vertex quad — so if we don't
        // expand the quad, the protrusion is invisible (every pixel outside
        // `prim.bounds` is never rasterized in the first place). Compute the
        // per-edge outward expansion from the notch parameters and fold it
        // into the quad size here. Scoop/cut modifiers go INWARD so they
        // don't need any expansion.
        var notch_left = 0.0;
        var notch_top = 0.0;
        var notch_right = 0.0;
        var notch_bottom = 0.0;

        let ct = prim.light; // (TL, TR, BR, BL) corner type flags
        let cr = prim.corner_radius;
        // Concave corners extend the shape outward by `radius` along the
        // two edges they touch.
        if ct.x > 0.5 { // TL concave
            notch_left = max(notch_left, cr.x);
            notch_top = max(notch_top, cr.x);
        }
        if ct.y > 0.5 { // TR concave
            notch_right = max(notch_right, cr.y);
            notch_top = max(notch_top, cr.y);
        }
        if ct.z > 0.5 { // BR concave
            notch_right = max(notch_right, cr.z);
            notch_bottom = max(notch_bottom, cr.z);
        }
        if ct.w > 0.5 { // BL concave
            notch_left = max(notch_left, cr.w);
            notch_bottom = max(notch_bottom, cr.w);
        }
        // Top / bottom modifiers: bulge (2) and peak (4) both extend the
        // shape by `height` past the base edge.
        let top_type = prim.perspective.x;
        let top_height = prim.perspective.z;
        if (top_type > 1.5 && top_type < 2.5) || (top_type > 3.5 && top_type < 4.5) {
            notch_top = max(notch_top, top_height);
        }
        let bot_type = prim.sdf_3d.x;
        let bot_height = prim.sdf_3d.z;
        if (bot_type > 1.5 && bot_type < 2.5) || (bot_type > 3.5 && bot_type < 4.5) {
            notch_bottom = max(notch_bottom, bot_height);
        }

        bounds = vec4<f32>(
            prim.bounds.x - blur_expand - notch_left,
            prim.bounds.y - blur_expand - notch_top,
            prim.bounds.z + blur_expand * 2.0 + notch_left + notch_right,
            prim.bounds.w + blur_expand * 2.0 + notch_top + notch_bottom
        );
    }

    // Generate quad vertices (two triangles split along / diagonal)
    // 0--1
    // |\ |
    // | \|
    // 3--2
    // Triangle 1: 0 → 1 → 3 (TL → TR → BL) - upper-left triangle
    // Triangle 2: 1 → 2 → 3 (TR → BR → BL) - lower-right triangle
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

// ============================================================================
// Notch SDF helpers
//
// Used by `case 8u /* PRIM_NOTCH */` in fs_main to compose rounded rects with
// concave corners and optional top/bottom edge modifiers (scoop, bulge,
// v-cut, v-peak). The goal is to approximate blinc_layout's path-based
// `build_shape_path` output well enough that the notch_demo matches its
// tessellated counterpart visually, while keeping every notch on the main
// SDF pipeline (free AA, layer-clip, transforms, shadows).
//
// Coordinate convention throughout: `p` is the fragment's position in
// shader-space pixels; `origin`/`size` describe the outer bounds rect
// (x, y, width, height) in the same coordinate space.
// ============================================================================

// Polynomial smooth min (Inigo Quilez). Blends two SDFs over a `k`-pixel
// transition zone so their union doesn't produce a visible crease at the
// point where both SDFs are zero. At that point, `smin(0, 0, k) = -k/4`
// — the crease is pushed into the interior far enough that the pipeline's
// AA pass (which smoothsteps at `|d| < aa_width`) sees the surface as
// "solidly inside" there and no hairline surfaces.
//
// Returns the min for points far from both surfaces (`|a - b| ≥ k`), so
// points well inside or well outside either shape are unaffected — only
// the transition zone near the crease is altered.
fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

// Polynomial smooth max — the dual of `smin`. Used for smooth subtraction
// (`smax(d, -carve, k)`), which keeps the union of edges where a carve
// meets the base shape from developing a visible crease line.
fn smax(a: f32, b: f32, k: f32) -> f32 {
    return -smin(-a, -b, k);
}

// 2D triangle SDF (Inigo Quilez's standard formulation).
// `a`, `b`, `c` are the three vertices; winding doesn't matter — the helper
// normalizes via `sign(e0 × e2)` so the result is always negative inside.
fn sd_triangle(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> f32 {
    let e0 = b - a; let e1 = c - b; let e2 = a - c;
    let v0 = p - a; let v1 = p - b; let v2 = p - c;
    let pq0 = v0 - e0 * clamp(dot(v0, e0) / max(dot(e0, e0), 1e-6), 0.0, 1.0);
    let pq1 = v1 - e1 * clamp(dot(v1, e1) / max(dot(e1, e1), 1e-6), 0.0, 1.0);
    let pq2 = v2 - e2 * clamp(dot(v2, e2) / max(dot(e2, e2), 1e-6), 0.0, 1.0);
    let s = sign(e0.x * e2.y - e0.y * e2.x);
    let d = min(
        min(
            vec2<f32>(dot(pq0, pq0), s * (v0.x * e0.y - v0.y * e0.x)),
            vec2<f32>(dot(pq1, pq1), s * (v1.x * e1.y - v1.y * e1.x))
        ),
        vec2<f32>(dot(pq2, pq2), s * (v2.x * e2.y - v2.y * e2.x))
    );
    return -sqrt(d.x) * sign(d.y);
}

// Corner type codes (stored as f32 in `prim.light` for PRIM_NOTCH).
//   0.0 = sharp or convex (distinguished by corner_radius magnitude)
//   1.0 = concave
//
// Modifier type codes (stored as f32 in `prim.perspective.x` / `prim.sdf_3d.x`).
//   0.0 = none   1.0 = scoop   2.0 = bulge   3.0 = cut   4.0 = peak
//
// All notch geometry is composed via SDF union (`min`) and subtraction
// (`max(d, -d_sub)`); no CPU tessellation is involved at any point.

// Complete notch SDF — one call, all geometry in the shader.
//
// Mirrors `blinc_layout::notch::build_shape_path`: starts from an inner
// rounded rect that's inset from the outer bounds by the maximum of
// (concave corner radius, top/bottom modifier height) on each edge, then
// composes the concave corner quarter-discs and the top/bottom edge
// modifiers on top of it. The result is a single signed distance — negative
// inside the shape, positive outside — that the main SDF pipeline can treat
// exactly like any other primitive for shading, AA, shadows, borders, and
// layer composition.
//
// Parameter pack:
//   outer_origin, outer_size — the caller's bounds (what the element reserved)
//   radii                    — per-corner magnitudes (TL, TR, BR, BL)
//   corner_types             — per-corner flags (0 = sharp/convex, 1 = concave)
//   top_mod                  — (type, width, height, corner_radius) for the
//                              top-edge modifier; type 0 = none
//   bottom_mod               — same layout for the bottom edge
fn sd_notch(
    p: vec2<f32>,
    outer_origin: vec2<f32>,
    outer_size: vec2<f32>,
    radii: vec4<f32>,
    corner_types: vec4<f32>,
    top_mod: vec4<f32>,
    bottom_mod: vec4<f32>
) -> f32 {
    let tl_concave = corner_types.x > 0.5;
    let tr_concave = corner_types.y > 0.5;
    let br_concave = corner_types.z > 0.5;
    let bl_concave = corner_types.w > 0.5;

    let tl_r = radii.x;
    let tr_r = radii.y;
    let br_r = radii.z;
    let bl_r = radii.w;

    // Modifier height on each edge (only bulge and peak protrude outward;
    // scoop and cut go inward so they don't reserve space).
    let top_type = top_mod.x;
    let top_protrudes = (top_type > 1.5 && top_type < 2.5) || (top_type > 3.5 && top_type < 4.5);
    let top_mod_h = select(0.0, top_mod.z, top_protrudes);
    let bot_type = bottom_mod.x;
    let bot_protrudes = (bot_type > 1.5 && bot_type < 2.5) || (bot_type > 3.5 && bot_type < 4.5);
    let bot_mod_h = select(0.0, bottom_mod.z, bot_protrudes);

    // Edge offsets — the inner rect is inset on each edge by the max of
    // (concave radius on adjacent corners, outward modifier height on that
    // edge). Matches `build_shape_path`'s left/right/top/bottom_offset math.
    let left_offset = select(
        0.0,
        max(select(0.0, tl_r, tl_concave), select(0.0, bl_r, bl_concave)),
        tl_concave || bl_concave
    );
    let right_offset = select(
        0.0,
        max(select(0.0, tr_r, tr_concave), select(0.0, br_r, br_concave)),
        tr_concave || br_concave
    );
    let top_offset = max(
        select(
            0.0,
            max(select(0.0, tl_r, tl_concave), select(0.0, tr_r, tr_concave)),
            tl_concave || tr_concave
        ),
        top_mod_h
    );
    let bottom_offset = max(
        select(
            0.0,
            max(select(0.0, bl_r, bl_concave), select(0.0, br_r, br_concave)),
            bl_concave || br_concave
        ),
        bot_mod_h
    );

    // Inner body rect: inset from outer bounds by `left_offset` /
    // `right_offset` / `top_offset` / `bottom_offset`. For the
    // `notch_demo` dropdown (`.w(340).concave_top(32).rounded_bottom(16)`),
    // `left_offset = right_offset = max(tl_r=32, bl_r=16) = 32` and
    // `top_offset = 32`, so the inner body is `(32, 32)` to `(308, h)` —
    // 276 wide, matching `build_shape_path`'s shape BELOW the concave
    // region.
    //
    // Concave corners get radius 0 on the inner rect: their curvature
    // lives in the flare regions added below. Convex and sharp corners
    // keep their radii so the main body has proper rounded bottom
    // corners etc.
    let inner_origin = outer_origin + vec2<f32>(left_offset, top_offset);
    let inner_size = vec2<f32>(
        max(outer_size.x - left_offset - right_offset, 0.001),
        max(outer_size.y - top_offset - bottom_offset, 0.001)
    );
    let inner_radii = vec4<f32>(
        select(tl_r, 0.0, tl_concave),
        select(tr_r, 0.0, tr_concave),
        select(br_r, 0.0, br_concave),
        select(bl_r, 0.0, bl_concave)
    );

    var d = sd_rounded_rect(p, inner_origin, inner_size, inner_radii);

    // ------------------------------------------------------------------
    // Concave corner flares.
    //
    // Each flare is the region "inside the concave corner box AND OUTSIDE
    // the concave arc disc". The corner box is an axis-aligned rectangle
    // that spans from the outer canvas edge to the inner body edge:
    //
    //   TL box: (outer.x,           inner_top)    → (inner_left,  inner_top + tl_r)
    //   TR box: (inner_right,       inner_top)    → (outer.x+w,   inner_top + tr_r)
    //   BR box: (inner_right,       inner_bottom - br_r) → (outer.x+w, inner_bottom)
    //   BL box: (outer.x,           inner_bottom - bl_r) → (inner_left, inner_bottom)
    //
    // The concave arc is a quarter circle whose center sits on the outer
    // canvas edge, aligned so the arc meets the top edge with a horizontal
    // tangent and the inner body's side edge with a vertical tangent. For
    // the top-left corner that center is `(outer.x, inner_top + tl_r)`,
    // radius `tl_r`. The arc carves the concave "bite" out of the flare
    // box — the flare FILLED region is "box AND NOT disc".
    //
    // SDF: `flare = max(box_sd, -disc_sd)` gives negative when a point is
    // inside the box AND outside the disc. The flare is then `min`-unioned
    // into the overall distance so the inner body and flares combine into
    // one shape.
    //
    // The shape at each y is therefore:
    //   y < inner_top                      → empty (above top edge)
    //   y = inner_top                      → only the top edge point of the
    //                                        flare is tangent; the shape
    //                                        spans x ∈ [outer.x, outer.x+w]
    //                                        (full canvas width) as the AA
    //                                        kernel rounds off the corner
    //   inner_top < y < inner_top + tl_r   → taper from full canvas width
    //                                        to inner body width
    //   y ≥ inner_top + tl_r                → inner body width
    // ------------------------------------------------------------------
    let inner_right = inner_origin.x + inner_size.x;
    let inner_bottom = inner_origin.y + inner_size.y;

    // `k` controls how wide the `smin` blend zone is. Keep it below `2 *
    // aa_width + 1` (~2 px) — too large and smin's blend region inflates
    // pixels that are actually just outside both sub-shapes, producing a
    // visible "halo" along the crease where the concave flare meets the
    // inner body's side edge. At k=1.5 the crease value drops to -0.375
    // (≈ 96% alpha after smoothstep, imperceptible seam) while points
    // one pixel outside both shapes stay on the outside.
    let smin_k = 1.5;

    // Effective vertical radius for each concave corner. Scales down when
    // the canvas can't fit the user-requested `tl_r` (or `tr_r`, etc.) on
    // top of the body — i.e. when the element is mid-animation growing
    // out from under a parent. At low heights the corner collapses to
    // just the available space so it "follows" the element's growth
    // naturally instead of staying at full size and clipping against the
    // canvas edge.
    //
    // Horizontal radius (left_offset / right_offset) stays at the
    // user's value — the concave curve becomes an ellipse rather than
    // a quarter circle. At `eff_vertical_r == tl_r` the ellipse
    // degenerates back to a circle, which is the steady-state shape.
    let tb_available = max(outer_size.y - top_offset - bottom_offset, 0.0);
    let eff_tl_ry = select(tl_r, min(tl_r, tb_available), tl_concave);
    let eff_tr_ry = select(tr_r, min(tr_r, tb_available), tr_concave);
    let eff_br_ry = select(br_r, min(br_r, tb_available), br_concave);
    let eff_bl_ry = select(bl_r, min(bl_r, tb_available), bl_concave);

    if tl_concave {
        let box_origin = vec2<f32>(outer_origin.x, inner_origin.y);
        let box_size = vec2<f32>(left_offset, eff_tl_ry);
        let box_sd = sd_rounded_rect(p, box_origin, box_size, vec4<f32>(0.0));
        // Elliptical arc: center on outer canvas edge, horizontal radius
        // stays at `left_offset`, vertical radius scales with available
        // height. At `eff_tl_ry == tl_r` this is the same circle as
        // before; at smaller `eff_tl_ry` it squishes vertically so the
        // arc still meets the top edge and inner body tangentially but
        // over a shorter vertical span.
        let c = vec2<f32>(outer_origin.x, inner_origin.y + eff_tl_ry);
        let ell_sd = sd_ellipse(p, c, vec2<f32>(left_offset, eff_tl_ry));
        let flare = max(box_sd, -ell_sd);
        d = smin(d, flare, smin_k);
    }
    if tr_concave {
        let right_width = outer_origin.x + outer_size.x - inner_right;
        let box_origin = vec2<f32>(inner_right, inner_origin.y);
        let box_size = vec2<f32>(right_width, eff_tr_ry);
        let box_sd = sd_rounded_rect(p, box_origin, box_size, vec4<f32>(0.0));
        let c = vec2<f32>(outer_origin.x + outer_size.x, inner_origin.y + eff_tr_ry);
        let ell_sd = sd_ellipse(p, c, vec2<f32>(right_width, eff_tr_ry));
        let flare = max(box_sd, -ell_sd);
        d = smin(d, flare, smin_k);
    }
    if br_concave {
        let right_width = outer_origin.x + outer_size.x - inner_right;
        let box_origin = vec2<f32>(inner_right, inner_bottom - eff_br_ry);
        let box_size = vec2<f32>(right_width, eff_br_ry);
        let box_sd = sd_rounded_rect(p, box_origin, box_size, vec4<f32>(0.0));
        let c = vec2<f32>(outer_origin.x + outer_size.x, inner_bottom - eff_br_ry);
        let ell_sd = sd_ellipse(p, c, vec2<f32>(right_width, eff_br_ry));
        let flare = max(box_sd, -ell_sd);
        d = smin(d, flare, smin_k);
    }
    if bl_concave {
        let box_origin = vec2<f32>(outer_origin.x, inner_bottom - eff_bl_ry);
        let box_size = vec2<f32>(left_offset, eff_bl_ry);
        let box_sd = sd_rounded_rect(p, box_origin, box_size, vec4<f32>(0.0));
        let c = vec2<f32>(outer_origin.x, inner_bottom - eff_bl_ry);
        let ell_sd = sd_ellipse(p, c, vec2<f32>(left_offset, eff_bl_ry));
        let flare = max(box_sd, -ell_sd);
        d = smin(d, flare, smin_k);
    }

    // ------------------------------------------------------------------
    // Top-edge modifier.
    //
    // Base line is `inner_origin.y` — the inner rect's top edge, which is
    // where `build_shape_path` anchors the modifier. Scoop and cut carve
    // INTO the rect (subtraction); bulge and peak protrude UPWARD out of
    // the rect (union). The base of bulge/peak sits on the inner edge and
    // their apex reaches up to `inner_origin.y - height`, which is exactly
    // `outer_origin.y + top_offset - top_mod_h` — by construction, ≥ the
    // outer top edge, so the protrusion never leaks outside the caller's
    // bounds (and outside the canvas clip).
    // ------------------------------------------------------------------
    // Scoop / bulge modifiers.
    //
    // Bulge uses a (1 − u²)^1.5 dome curve — like the legacy cubic bezier
    // it has zero slope at u=±1 and u=0 (horizontal tangents at both the
    // baseline endpoints and the apex), so the "gentle wrap" join is
    // intentional. Apex curvature radius rx²/(3·h) is noticeably rounder
    // than a pure cosine, so a shallow bulge reads as a dome rather than
    // a curvy triangle.
    //
    // Scoop uses a half-ellipse bowl (radii half_w × depth) subtracted
    // from the body via `smax(−ell, k)`. The ellipse has a *vertical*
    // tangent where it meets the baseline (the 90° corner at the entry),
    // and the smooth-max rounds that corner into the Dynamic-Island-style
    // "ears" — fillet size is driven by the user's `corner_radius` param
    // (`top_mod.w`), so `.center_scoop_top_rounded(w, depth, cr)` behaves
    // like the legacy path renderer.
    //
    // All params come from the user's
    // `.center_bulge_top(w,h)` / `.center_scoop_top_rounded(w,depth,cr)`
    // call via `top_mod.{y,z,w}`.
    let top_w = top_mod.y;
    let top_h = top_mod.z;
    let top_cr = top_mod.w;
    if top_type > 0.5 && top_w > 0.001 && top_h > 0.001 {
        let cx = outer_origin.x + outer_size.x * 0.5;
        let base_y = inner_origin.y;
        let half_w = top_w * 0.5;
        let rel_x = p.x - cx;
        let u = clamp(rel_x / half_w, -1.0, 1.0);
        let dx_col = abs(rel_x) - half_w;
        let one_minus_u_sq = max(1.0 - u * u, 0.0);
        let dome = one_minus_u_sq * sqrt(one_minus_u_sq); // (1 − u²)^1.5
        if top_type < 1.5 { // scoop — rect + half-disk, smooth-max ears
            // The hollow is a thin rect from the baseline down to the top
            // of a half-disk, unioned with the half-disk itself. The
            // half-disk's radius is `min(half_w, depth)` — for depth ≥
            // half_w we get a true semicircular floor (no flat section),
            // for shallower scoops the disk shrinks and a residual rect
            // fills the remaining height.
            //
            // The rect has SHARP top corners so the body's 90° convex
            // corner at the scoop entry gets rounded OUTWARD by
            // `smax(−hollow, k=cr)` — that produces the Dynamic-Island
            // "ears" (body edge dipping smoothly from the baseline into
            // the vertical scoop wall). Hard `max()` would leave visible
            // 90° corners poking inward; smax's fillet bows outward,
            // matching the legacy cubic-bezier ear shape.
            let disk_r = min(half_w, top_h);
            let disk_cy = base_y + top_h - disk_r;
            let disk_sd = length(p - vec2<f32>(cx, disk_cy)) - disk_r;
            let disk_lower = max(disk_sd, disk_cy - p.y);
            let rect_origin = vec2<f32>(cx - half_w, base_y);
            let rect_h = max(disk_cy - base_y, 0.001);
            let rect_sd = sd_rounded_rect(p, rect_origin, vec2<f32>(top_w, rect_h), vec4<f32>(0.0));
            let hollow_sd = min(rect_sd, disk_lower);
            d = smax(d, -hollow_sd, max(top_cr, 0.001));
        } else if top_type < 2.5 { // bulge — circular arc cap with smooth ears
            // The cap is the segment of a disk passing through
            // (cx ± half_w, base_y) and (cx, base_y − top_h). Formula:
            //   r = (half_w² + h²) / (2·h)
            //   y_c = base_y − h + r   (center below the apex)
            // The circle meets the baseline at a nonzero angle (not a
            // horizontal tangent), so the union with the body has a
            // concave notch on the outside at each endpoint. `smin` adds
            // a fillet into the notch whose size is the user's
            // `corner_radius`, producing Dynamic-Island-style ears at
            // the bulge base.
            let r_bulge = (half_w * half_w + top_h * top_h) / max(2.0 * top_h, 0.001);
            let y_c = base_y - top_h + r_bulge;
            let disk_sd = length(p - vec2<f32>(cx, y_c)) - r_bulge;
            let bulge_sd = max(disk_sd, p.y - base_y);
            d = smin(d, bulge_sd, max(top_cr, 0.001));
        } else if top_type < 3.5 { // cut — subtract a V-triangle
            d = smax(d, -sd_triangle(
                p,
                vec2<f32>(cx - top_w * 0.5, base_y),
                vec2<f32>(cx, base_y + top_h),
                vec2<f32>(cx + top_w * 0.5, base_y)
            ), smin_k);
        } else { // peak — union a V-triangle protrusion
            d = smin(d, sd_triangle(
                p,
                vec2<f32>(cx - top_w * 0.5, base_y),
                vec2<f32>(cx, base_y - top_h),
                vec2<f32>(cx + top_w * 0.5, base_y)
            ), smin_k);
        }
    }

    // Bottom-edge modifier — mirror of top, anchored at
    // `inner_origin.y + inner_size.y`.
    let bot_w = bottom_mod.y;
    let bot_h = bottom_mod.z;
    let bot_cr = bottom_mod.w;
    if bot_type > 0.5 && bot_w > 0.001 && bot_h > 0.001 {
        let cx = outer_origin.x + outer_size.x * 0.5;
        let base_y = inner_origin.y + inner_size.y;
        let half_w = bot_w * 0.5;
        let rel_x = p.x - cx;
        let u = clamp(rel_x / half_w, -1.0, 1.0);
        let dx_col = abs(rel_x) - half_w;
        let one_minus_u_sq = max(1.0 - u * u, 0.0);
        let dome = one_minus_u_sq * sqrt(one_minus_u_sq);
        if bot_type < 1.5 { // scoop — mirror of top: rect + half-disk above bottom baseline
            let disk_r = min(half_w, bot_h);
            let disk_cy = base_y - bot_h + disk_r;
            let disk_sd = length(p - vec2<f32>(cx, disk_cy)) - disk_r;
            let disk_upper = max(disk_sd, p.y - disk_cy);
            let rect_h = max(base_y - disk_cy, 0.001);
            let rect_origin = vec2<f32>(cx - half_w, disk_cy);
            let rect_sd = sd_rounded_rect(p, rect_origin, vec2<f32>(bot_w, rect_h), vec4<f32>(0.0));
            let hollow_sd = min(rect_sd, disk_upper);
            d = smax(d, -hollow_sd, max(bot_cr, 0.001));
        } else if bot_type < 2.5 { // bulge — circular arc cap with smooth ears
            let r_bulge = (half_w * half_w + bot_h * bot_h) / max(2.0 * bot_h, 0.001);
            let y_c = base_y + bot_h - r_bulge;
            let disk_sd = length(p - vec2<f32>(cx, y_c)) - r_bulge;
            let bulge_sd = max(disk_sd, base_y - p.y);
            d = smin(d, bulge_sd, max(bot_cr, 0.001));
        } else if bot_type < 3.5 { // cut
            d = smax(d, -sd_triangle(
                p,
                vec2<f32>(cx - bot_w * 0.5, base_y),
                vec2<f32>(cx, base_y - bot_h),
                vec2<f32>(cx + bot_w * 0.5, base_y)
            ), smin_k);
        } else { // peak
            d = smin(d, sd_triangle(
                p,
                vec2<f32>(cx - bot_w * 0.5, base_y),
                vec2<f32>(cx, base_y + bot_h),
                vec2<f32>(cx + bot_w * 0.5, base_y)
            ), smin_k);
        }
    }

    return d;
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
                // Scissor-only; polygon shape test deferred. See sdf_core.wgsl.
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
    let d_fw_screen = length(vec2<f32>(fwidth(p.x), fwidth(p.y)));

    let prim_type = prim.type_info.x;
    let fill_type = prim.type_info.y;
    let clip_type = prim.type_info.z;

    // Early type filter — discard primitives handled by other split pipelines
    if prim_type != 8u { discard; }

    // Early clip test - discard if completely outside clip region (screen space).
    // Polygon shape test deferred until sp is known — see sdf_core.wgsl.
    var clip_alpha = calculate_clip_alpha(p, prim.clip_bounds, prim.clip_radius, prim.clip_corner_shape, clip_type, prim.clip_fade);
    if clip_alpha < 0.001 {
        discard;
    }

    let origin = prim.bounds.xy;
    let size = prim.bounds.zw;
    let center = origin + size * 0.5;

    // Notch never uses 3D — it aliases perspective fields for notch parameters
    let has_3d = false;

    // 2D affine (rotation, skew, non-uniform scale) via inverse local_affine
    var sp = p;
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

    // CLIP_POLYGON shape test in element-local coords. See sdf_core.wgsl.
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
    if (prim.shadow.z > 0.0 || prim.shadow.w != 0.0) {
        let shadow_offset = prim.shadow.xy;
        let blur = prim.shadow.z;
        let spread = prim.shadow.w;

        let shadow_origin = origin + shadow_offset - vec2<f32>(spread);
        let shadow_size = size + vec2<f32>(spread * 2.0);

        // Adjust corner radii for spread (expand corners proportionally)
        let shadow_radii = prim.corner_radius + vec4<f32>(spread);

        // For PRIM_NOTCH the shadow traces the actual notch outline
        // via `sd_notch` — so concave arcs, bulges, scoops, cuts and
        // peaks all cast shadows from their real visible edges instead
        // of from the rectangular bounding box.
        var shadow_sdf_dist: f32;
        shadow_sdf_dist = sd_notch(
            sp, shadow_origin, shadow_size,
            shadow_radii,
            prim.light,
            prim.perspective,
            prim.sdf_3d
        );
        var shadow_alpha: f32;
        if blur < 0.001 {
            shadow_alpha = select(0.0, 1.0, shadow_sdf_dist < 0.0);
        } else {
            let sigma_d = 0.5 * sqrt(2.0) * blur;
            shadow_alpha = 0.5 * (1.0 + erf(-shadow_sdf_dist / sigma_d));
        }

        // Thin out the shadow at the "ending" of a concave arc. The
        // concave boundary has its outward normal pointing AWAY from
        // the shape (into the wedge region), and at the two points
        // where the arc touches the outer bounds the normal becomes
        // axis-aligned: at the `inner_top` end of a concave TOP arc
        // it points straight up, at `inner_bottom` it points straight
        // down, etc. Tracing `sd_notch` fully means those attachment
        // points cast a full-strength shadow outward, which reads as
        // a thick dark band bleeding past the concave edge.
        //
        // For each edge that has BOTH adjacent corners concave, fade
        // the shadow to zero as the pixel approaches the attachment
        // line on the "outside" side of the shape. The fade width is
        // the shadow blur so it tapers smoothly.
        if blur > 0.001 {
            let tl_c = prim.light.x > 0.5;
            let tr_c = prim.light.y > 0.5;
            let br_c = prim.light.z > 0.5;
            let bl_c = prim.light.w > 0.5;
            let fade_dist = blur;
            if tl_c && tr_c {
                let top_off = max(shadow_radii.x, shadow_radii.y);
                let inner_top = shadow_origin.y + top_off;
                let top_fade = smoothstep(inner_top - fade_dist, inner_top, sp.y);
                shadow_alpha *= top_fade;
            }
            if bl_c && br_c {
                let bottom_off = max(shadow_radii.w, shadow_radii.z);
                let inner_bottom = shadow_origin.y + shadow_size.y - bottom_off;
                let bot_fade = smoothstep(inner_bottom + fade_dist, inner_bottom, sp.y);
                shadow_alpha *= bot_fade;
            }
            if tl_c && bl_c {
                let left_off = max(shadow_radii.x, shadow_radii.w);
                let inner_left = shadow_origin.x + left_off;
                let left_fade = smoothstep(inner_left - fade_dist, inner_left, sp.x);
                shadow_alpha *= left_fade;
            }
            if tr_c && br_c {
                let right_off = max(shadow_radii.y, shadow_radii.z);
                let inner_right = shadow_origin.x + shadow_size.x - right_off;
                let right_fade = smoothstep(inner_right + fade_dist, inner_right, sp.x);
                shadow_alpha *= right_fade;
            }
        }

        let shadow_color = prim.shadow_color * shadow_alpha;

        // Premultiply and blend
        result = shadow_color;
    }

    // Calculate main shape SDF — always notch
    let d = sd_notch(
        sp, origin, size,
        prim.corner_radius,
        prim.light,       // per-corner type flags (TL, TR, BR, BL)
        prim.perspective, // top modifier    (type, width, height, corner_r)
        prim.sdf_3d       // bottom modifier (type, width, height, corner_r)
    );

    // Anti-aliasing: smooth transition at edge over ~1 pixel total.
    _ = d_fw_screen;
    let aa_width = 0.5;
    let fill_alpha = 1.0 - smoothstep(-aa_width, aa_width, d);

    if fill_alpha < 0.001 {
        return result;
    }

    // Determine fill color
    var fill_color: vec4<f32>;
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

    // Handle border with proper inner corner radii (GPUI-style approach)
    // The border is the ring between the outer shape edge and an inner shape
    // For asymmetric borders, inner corners become elliptical, not circular
    // prim.border = [top, right, bottom, left] for per-side borders, or [uniform, 0, 0, 0] for uniform
    let border_top = prim.border.x;
    let border_right = prim.border.y;
    let border_bottom = prim.border.z;
    let border_left = prim.border.w;

    // Check if any border is present (using max of all sides)
    let max_border = max(max(border_top, border_right), max(border_bottom, border_left));
    if max_border > 0.0 {
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

        // PRIM_NOTCH always goes through the SDF-based `inner_sdf` branch
        // because the quadrant-based geometry is derived from the rect
        // bounding box and doesn't account for concave corners / top-bottom
        // modifiers — a point deep inside the bbox can still be near a
        // concave flare or a bulge edge.
        {
            // Calculate inner SDF based on context
            var inner_sdf: f32;

            if abs(reduced_border.x - reduced_border.y) < 0.001 {
                // Uniform border — use exact SDF offset of the outer shape distance.
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
            // transition width as the outer shape edge.
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
