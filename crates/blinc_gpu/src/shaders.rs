//! GPU shaders for SDF primitives
//!
//! These shaders render:
//! - Rounded rectangles with borders
//! - Circles and ellipses
//! - Gaussian blur shadows (via error function approximation)
//! - Gradients (linear, radial, conic)
//! - Glass/vibrancy effects (backdrop blur, tint)

/// Main SDF primitive shader
///
/// Renders all basic UI primitives using signed distance fields:
/// - Rounded rectangles with per-corner radius
/// - Circles and ellipses
/// - Shadows with Gaussian blur
/// - Solid colors and gradients
pub const SDF_SHADER: &str = r#"
// ============================================================================
// Blinc SDF Primitive Shader
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

    // Check for rotation, skew, and 3D transforms. PRIM_NOTCH repurposes
    // `prim.perspective` / `prim.sdf_3d` / `prim.light` for 2D notch
    // parameters, so extracting 3D values from those slots would alias the
    // notch modifier type as `sin_rx`, flip `has_3d` on, and expand the
    // vertex bounding rect via the 3D projection loop below — producing a
    // quad that doesn't cover the notch. Treat notches as strictly 2D.
    let is_notch_vs = prim.type_info.x == 8u;
    let sin_rz = prim.rotation.x;
    let cos_rz = prim.rotation.y;
    let sin_ry = prim.rotation.z;
    let cos_ry = prim.rotation.w;
    let sin_rx = select(prim.perspective.x, 0.0, is_notch_vs);
    let cos_rx = select(prim.perspective.y, 1.0, is_notch_vs);
    let persp_d = select(prim.perspective.z, 0.0, is_notch_vs);
    let la = prim.local_affine; // [a, b, c, d] of normalized 2x2 affine
    let has_3d = !is_notch_vs
        && (abs(sin_ry) > 0.0001 || abs(sin_rx) > 0.0001 || persp_d > 0.001);
    // Check if local_affine is non-identity (rotation, skew, or non-uniform scale)
    let has_local_affine = abs(la.x - 1.0) > 0.0001 || abs(la.y) > 0.0001
                        || abs(la.z) > 0.0001 || abs(la.w - 1.0) > 0.0001;

    var bounds: vec4<f32>;
    if has_3d {
        // 3D perspective: project all 8 corners of the 3D bounding box to find AABB
        let ctr = prim.bounds.xy + prim.bounds.zw * 0.5;
        let half = prim.bounds.zw * 0.5;
        let half_d = prim.sdf_3d.x * 0.5; // half-depth
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
        if is_notch_vs {
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

    // Notch: rectangular step cut at each corner (before guard)
    // Shape = union of horizontal bar (full width, height-2r) and vertical bar (width-2r, full height)
    if n <= -100.0 {
        let d_h = max(q.x, q.y + r);  // horizontal bar SDF
        let d_v = max(q.x + r, q.y);  // vertical bar SDF
        return min(d_h, d_v);          // union
    }

    let q_adj = q + vec2<f32>(r);

    // Fast path: n ~ 1.0 -> standard circular
    if abs(n - 1.0) < 0.01 {
        return length(max(q_adj, vec2<f32>(0.0))) + min(max(q_adj.x, q_adj.y), 0.0) - r;
    }

    // Outside corner region -> flat edge
    if q_adj.x <= 0.0 || q_adj.y <= 0.0 {
        return max(q.x, q.y);
    }

    // Square: sharp corner (L-infinity convex)
    if n >= 100.0 {
        return max(q_adj.x, q_adj.y) - r;
    }

    // Superellipse: p_exp = 2^|n|, clamped to avoid overflow
    let t = q_adj / max(r, 0.001);
    let p_exp = pow(2.0, min(abs(n), 5.0));
    let se = pow(t.x, p_exp) + pow(t.y, p_exp);
    let se_dist = (pow(se, 1.0 / p_exp) - 1.0) * r;

    if n < 0.0 {
        return -se_dist;  // concave (scoop)
    }
    return se_dist;  // convex
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

// Gaussian shadow for rectangle (without corner radii - legacy)
fn shadow_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, sigma: f32) -> f32 {
    if sigma < 0.001 {
        // No blur - use hard edge
        let d = sd_rounded_rect(p, origin, size, vec4<f32>(0.0));
        return select(0.0, 1.0, d < 0.0);
    }

    let d = 0.5 * sqrt(2.0) * sigma;
    let half = size * 0.5;
    let center = origin + half;
    let rel = p - center;

    let x = 0.5 * (erf((half.x - rel.x) / d) + erf((half.x + rel.x) / d));
    let y = 0.5 * (erf((half.y - rel.y) / d) + erf((half.y + rel.y) / d));

    return x * y;
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
fn calculate_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>, clip_radius: vec4<f32>, clip_type: u32, clip_fade: vec4<f32>) -> f32 {
    var alpha: f32 = 1.0;

    if clip_type != 0u {
        let aa_width = 0.75;
        switch clip_type {
            case 1u /* CLIP_RECT */: {
                let clip_origin = clip_bounds.xy;
                let clip_size = clip_bounds.zw;
                let clip_d = sd_rounded_rect(p, clip_origin, clip_size, clip_radius);
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
                let scissor_d = sd_rounded_rect(p, clip_bounds.xy, clip_bounds.zw, vec4<f32>(0.0));
                let scissor_alpha = 1.0 - smoothstep(-aa_width, aa_width, scissor_d);
                let vertex_count = u32(clip_radius.z);
                let aux_offset = u32(clip_radius.w);
                let shape_alpha = calculate_polygon_clip_alpha(p, vertex_count, aux_offset);
                alpha = scissor_alpha * shape_alpha;
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
// 3D SDF Functions
// ============================================================================

// 3D shape types (field: perspective.w, variable: shape_type)
//   0u = NONE    1u = BOX    2u = SPHERE    3u = CYLINDER
//   4u = TORUS   5u = CAPSULE    6u = GROUP
//
// Inlined as literals rather than `const` at module scope so the
// generated Metal source doesn't carry orphaned `constant uint`
// declarations — see the longer comment next to the 2D enum block
// above for why.

fn sd_box_3d(p: vec3<f32>, half_ext: vec3<f32>, r: f32) -> f32 {
    let q = abs(p) - half_ext + vec3<f32>(r);
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0) - r;
}

fn sd_sphere_3d(p: vec3<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn sd_cylinder_3d(p: vec3<f32>, h: f32, r: f32) -> f32 {
    let d = vec2<f32>(length(p.xz) - r, abs(p.y) - h);
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0)));
}

fn sd_torus_3d(p: vec3<f32>, major_r: f32, minor_r: f32) -> f32 {
    let q = vec2<f32>(length(p.xz) - major_r, p.y);
    return length(q) - minor_r;
}

fn sd_capsule_3d(p: vec3<f32>, h: f32, r: f32) -> f32 {
    let py = p.y - clamp(p.y, -h, h);
    return length(vec3<f32>(p.x, py, p.z)) - r;
}

fn sdf_3d_eval(p: vec3<f32>, shape_type: u32, half_ext: vec3<f32>, corner_r: f32) -> f32 {
    // Use X-Y dimensions for shape sizing (not Z/depth which may be smaller)
    let min_xy = min(half_ext.x, half_ext.y);
    switch shape_type {
        case 1u: { return sd_box_3d(p, half_ext, corner_r); }
        case 2u: { return sd_sphere_3d(p, min_xy); }
        case 3u: { return sd_cylinder_3d(p, half_ext.y, half_ext.x); }
        case 4u: {
            // Torus: minor + major = min_xy so outer edge fills element
            let minor = min(min_xy / 3.0, half_ext.y);
            let major = min_xy - minor;
            return sd_torus_3d(p, major, minor);
        }
        case 5u: {
            // Capsule: inscribe in X-Y bounding box
            let r = min(half_ext.x, half_ext.y * 0.5);
            let h = max(half_ext.y - r, 0.0);
            return sd_capsule_3d(p, h, r);
        }
        default: { return 1e10; }
    }
}

// ============================================================================
// 3D Boolean Operations
// ============================================================================

fn op_union(d1: f32, d2: f32) -> f32 { return min(d1, d2); }
fn op_subtract(d1: f32, d2: f32) -> f32 { return max(d1, -d2); }
fn op_intersect(d1: f32, d2: f32) -> f32 { return max(d1, d2); }
fn op_smooth_union(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) - k * h * (1.0 - h);
}
fn op_smooth_subtract(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (d2 + d1) / k, 0.0, 1.0);
    return mix(d1, -d2, h) + k * h * (1.0 - h);
}
fn op_smooth_intersect(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) + k * h * (1.0 - h);
}

fn apply_boolean_op(d_accum: f32, d_new: f32, op_type: u32, blend: f32) -> f32 {
    switch op_type {
        case 0u: { return op_union(d_accum, d_new); }
        case 1u: { return op_subtract(d_accum, d_new); }
        case 2u: { return op_intersect(d_accum, d_new); }
        case 3u: { return op_smooth_union(d_accum, d_new, max(blend, 0.001)); }
        case 4u: { return op_smooth_subtract(d_accum, d_new, max(blend, 0.001)); }
        case 5u: { return op_smooth_intersect(d_accum, d_new, max(blend, 0.001)); }
        default: { return op_union(d_accum, d_new); }
    }
}

// ============================================================================
// 3D Group SDF Evaluation
// ============================================================================

fn eval_group_sdf(p: vec3<f32>, shape_count: u32, aux_offset: u32) -> f32 {
    var d = 1e10;
    for (var i = 0u; i < shape_count; i++) {
        let base = aux_offset + i * 4u;
        let s_offset = aux_data[base];       // x, y, z, corner_radius
        let s_params = aux_data[base + 1u];  // shape_type, depth, op_type, blend
        let s_half = aux_data[base + 2u];    // half_w, half_h, half_d, 0

        let local_p = p - s_offset.xyz;
        let shape_d = sdf_3d_eval(local_p, u32(s_params.x), s_half.xyz, s_offset.w);

        if i == 0u {
            d = shape_d;
        } else {
            d = apply_boolean_op(d, shape_d, u32(s_params.z), s_params.w);
        }
    }
    return d;
}

// Compute group normal via central differences
fn eval_group_normal(hp: vec3<f32>, shape_count: u32, aux_offset: u32) -> vec3<f32> {
    let eps = 0.001;
    return normalize(vec3<f32>(
        eval_group_sdf(hp + vec3<f32>(eps, 0.0, 0.0), shape_count, aux_offset) -
        eval_group_sdf(hp - vec3<f32>(eps, 0.0, 0.0), shape_count, aux_offset),
        eval_group_sdf(hp + vec3<f32>(0.0, eps, 0.0), shape_count, aux_offset) -
        eval_group_sdf(hp - vec3<f32>(0.0, eps, 0.0), shape_count, aux_offset),
        eval_group_sdf(hp + vec3<f32>(0.0, 0.0, eps), shape_count, aux_offset) -
        eval_group_sdf(hp - vec3<f32>(0.0, 0.0, eps), shape_count, aux_offset)
    ));
}

// Find which shape in the group is closest to the hit point (for per-shape coloring)
fn eval_group_closest_shape_color(hp: vec3<f32>, shape_count: u32, aux_offset: u32) -> vec4<f32> {
    var min_d = 1e10;
    var closest_color = vec4<f32>(1.0);
    for (var i = 0u; i < shape_count; i++) {
        let base = aux_offset + i * 4u;
        let s_offset = aux_data[base];
        let s_params = aux_data[base + 1u];
        let s_half = aux_data[base + 2u];
        let s_color = aux_data[base + 3u];

        let local_p = hp - s_offset.xyz;
        let d = abs(sdf_3d_eval(local_p, u32(s_params.x), s_half.xyz, s_offset.w));
        if d < min_d {
            min_d = d;
            closest_color = s_color;
        }
    }
    return closest_color;
}

// ============================================================================
// UV Mapping for 3D Shapes
// ============================================================================

fn compute_uv_box(hp: vec3<f32>, half: vec3<f32>) -> vec2<f32> {
    let abs_hp = abs(hp);
    let safe_half = max(abs(half), vec3<f32>(0.001));
    // Project onto dominant face
    if abs_hp.z >= safe_half.z - 0.01 {
        // Front/back face
        return vec2<f32>((hp.x / safe_half.x + 1.0) * 0.5, (hp.y / safe_half.y + 1.0) * 0.5);
    } else if abs_hp.y >= safe_half.y - 0.01 {
        // Top/bottom face
        return vec2<f32>((hp.x / safe_half.x + 1.0) * 0.5, (hp.z / safe_half.z + 1.0) * 0.5);
    } else {
        // Left/right face
        return vec2<f32>((hp.z / safe_half.z + 1.0) * 0.5, (hp.y / safe_half.y + 1.0) * 0.5);
    }
}

fn compute_uv_sphere(hp: vec3<f32>) -> vec2<f32> {
    let n = normalize(hp + vec3<f32>(0.0001));
    let u = atan2(n.z, n.x) / (2.0 * 3.14159) + 0.5;
    let v = asin(clamp(n.y, -1.0, 1.0)) / 3.14159 + 0.5;
    return vec2<f32>(u, v);
}

fn compute_uv_cylinder(hp: vec3<f32>, half_h: f32) -> vec2<f32> {
    let u = atan2(hp.z, hp.x) / (2.0 * 3.14159) + 0.5;
    let v = (hp.y / max(half_h, 0.001) + 1.0) * 0.5;
    return vec2<f32>(u, v);
}

fn compute_uv_3d(hp: vec3<f32>, shape_type: u32, half: vec3<f32>) -> vec2<f32> {
    switch shape_type {
        case 1u: { return compute_uv_box(hp, half); }
        case 2u: { return compute_uv_sphere(hp); }
        case 3u: { return compute_uv_cylinder(hp, half.y); }
        case 4u: { return compute_uv_cylinder(hp, half.y); } // torus uses cylindrical
        case 5u: { return compute_uv_cylinder(hp, half.y); } // capsule uses cylindrical
        default: { return vec2<f32>(0.5, 0.5); }
    }
}

// Analytical ray-AABB intersection (slab method)
// Returns vec2(t_enter, t_exit). If t_enter > t_exit, the ray misses.
fn ray_aabb_intersect(ro: vec3<f32>, rd: vec3<f32>, half: vec3<f32>) -> vec2<f32> {
    let inv_rd = vec3<f32>(
        select(1.0 / rd.x, 1e10, abs(rd.x) < 1e-8),
        select(1.0 / rd.y, 1e10, abs(rd.y) < 1e-8),
        select(1.0 / rd.z, 1e10, abs(rd.z) < 1e-8),
    );
    let t1 = (-half - ro) * inv_rd;
    let t2 = (half - ro) * inv_rd;
    let tmin = min(t1, t2);
    let tmax = max(t1, t2);
    let t_enter = max(max(tmin.x, tmin.y), tmin.z);
    let t_exit = min(min(tmax.x, tmax.y), tmax.z);
    return vec2<f32>(t_enter, t_exit);
}

// Inverse rotation helpers (transpose of forward rotation)
fn rotate_y_inv(p: vec3<f32>, s: f32, c: f32) -> vec3<f32> {
    return vec3<f32>(c * p.x - s * p.z, p.y, s * p.x + c * p.z);
}
fn rotate_x_inv(p: vec3<f32>, s: f32, c: f32) -> vec3<f32> {
    return vec3<f32>(p.x, c * p.y + s * p.z, -s * p.y + c * p.z);
}
fn rotate_z_inv(p: vec3<f32>, s: f32, c: f32) -> vec3<f32> {
    return vec3<f32>(c * p.x + s * p.y, -s * p.x + c * p.y, p.z);
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

    // Early clip test - discard if completely outside clip region (screen space)
    let clip_alpha = calculate_clip_alpha(p, prim.clip_bounds, prim.clip_radius, clip_type, prim.clip_fade);
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
    // PRIM_NOTCH repurposes `prim.perspective`, `prim.sdf_3d`, and `prim.light`
    // for 2D notch parameters (see the PRIM_NOTCH case below and the
    // `PrimitiveType::Notch` doc comment in blinc_gpu::primitives). Extracting
    // them as 3D data here would alias the modifier type code as `sin_rx` and
    // flip `has_3d` on, which then runs the 3D perspective-unprojection branch
    // and corrupts `sp` for every notch fragment. Force-zero those slots when
    // prim_type == Notch so the 3D path never fires.
    let is_notch = prim_type == 8u;
    let sin_rx = select(prim.perspective.x, 0.0, is_notch);
    let cos_rx = select(prim.perspective.y, 1.0, is_notch);
    let persp_d = select(prim.perspective.z, 0.0, is_notch);
    let shape_type = select(u32(prim.perspective.w), 0u, is_notch);
    let depth = select(prim.sdf_3d.x, 0.0, is_notch);

    // Notches never go through the 3D perspective-unprojection branch below
    // — their `sp` must be whatever the 2D local_affine path produces so the
    // SDF evaluates against fragment positions directly.
    let has_3d = !is_notch && (abs(sin_ry) > 0.0001 || abs(sin_rx) > 0.0001 || persp_d > 0.001);

    // ── 3D SDF Raymarching Path ──
    if shape_type > 0u && shape_type != 6u && depth > 0.001 {
        let translate_z = prim.sdf_3d.w;
        let pd = select(800.0, persp_d, persp_d > 0.001);
        let rel = p - center;

        // Ray setup: camera at (0, 0, pd - translate_z), fragment at (rel.x, rel.y, 0)
        // Positive translate_z moves object toward viewer (closer = larger)
        let cam_vs = vec3<f32>(0.0, 0.0, pd - translate_z);
        let frag_vs = vec3<f32>(rel.x, rel.y, 0.0);
        let ray_dir_vs = normalize(frag_vs - cam_vs);

        // Transform ray to shape space (inverse of rotateZ → rotateX → rotateY)
        var ro = cam_vs;
        var rd = ray_dir_vs;
        ro = rotate_y_inv(ro, sin_ry, cos_ry);
        rd = rotate_y_inv(rd, sin_ry, cos_ry);
        ro = rotate_x_inv(ro, sin_rx, cos_rx);
        rd = rotate_x_inv(rd, sin_rx, cos_rx);
        ro = rotate_z_inv(ro, sin_rz, cos_rz);
        rd = rotate_z_inv(rd, sin_rz, cos_rz);

        // Shape bounds in shape space
        let half_3d = vec3<f32>(size.x * 0.5, size.y * 0.5, depth * 0.5);
        let corner_r = min(min(prim.corner_radius.x, prim.corner_radius.y), depth * 0.5);

        // Analytical ray-AABB intersection for tight starting bound
        let aabb_t = ray_aabb_intersect(ro, rd, half_3d);
        if aabb_t.x > aabb_t.y || aabb_t.y < 0.0 {
            discard;  // Ray misses bounding box entirely
        }

        // Raymarch (32 steps) starting from AABB entry point
        // Use AABB diagonal as max distance to allow rays to reach rounded corners
        // (at AABB corners, the SDF distance to rounded surface can be large)
        var t_rm = max(aabb_t.x - 0.01, 0.0);
        let t_max = aabb_t.x + length(half_3d) * 2.0 + 1.0;
        var hit = false;
        var min_d = 1e10;
        for (var i = 0u; i < 32u; i++) {
            let pos = ro + rd * t_rm;
            let d3 = sdf_3d_eval(pos, shape_type, half_3d, corner_r);
            min_d = min(min_d, d3);
            if d3 < 0.001 {
                hit = true;
                break;
            }
            t_rm += d3;
            if t_rm > t_max {
                break;
            }
        }

        // Edge anti-aliasing: smooth alpha based on closest approach distance
        let pixel_size = max(t_rm / pd, 0.5);
        var edge_aa = 1.0;
        if !hit {
            if min_d > pixel_size * 2.0 {
                discard;
            }
            edge_aa = 1.0 - smoothstep(0.0, pixel_size * 1.5, min_d);
        }

        // Compute normal via central differences
        let hp = ro + rd * t_rm;
        let eps = 0.001;
        let normal = normalize(vec3<f32>(
            sdf_3d_eval(hp + vec3<f32>(eps, 0.0, 0.0), shape_type, half_3d, corner_r) -
            sdf_3d_eval(hp - vec3<f32>(eps, 0.0, 0.0), shape_type, half_3d, corner_r),
            sdf_3d_eval(hp + vec3<f32>(0.0, eps, 0.0), shape_type, half_3d, corner_r) -
            sdf_3d_eval(hp - vec3<f32>(0.0, eps, 0.0), shape_type, half_3d, corner_r),
            sdf_3d_eval(hp + vec3<f32>(0.0, 0.0, eps), shape_type, half_3d, corner_r) -
            sdf_3d_eval(hp - vec3<f32>(0.0, 0.0, eps), shape_type, half_3d, corner_r)
        ));

        // Lighting
        let light_dir = normalize(prim.light.xyz);
        let n_dot_l = max(dot(normal, light_dir), 0.0);
        let ambient_3d = prim.sdf_3d.y;
        let diffuse_3d = n_dot_l * prim.light.w;

        // Specular (Blinn-Phong)
        let view_dir = normalize(-rd);
        let half_vec = normalize(light_dir + view_dir);
        let spec_3d = pow(max(dot(normal, half_vec), 0.0), prim.sdf_3d.z) * 0.5;

        let lighting = ambient_3d + diffuse_3d + spec_3d;

        // UV mapping: use screen-space position for gradient evaluation.
        // This gives smooth gradients across all visible faces of 3D shapes
        // (avoids face-based UV discontinuities on boxes).
        // Screen-space UV: fragment position relative to element bounds.
        var base_color_3d: vec4<f32>;
        if fill_type == 1u {
            // Linear gradient: gradient_params are already in screen-space pixels
            let gp = prim.gradient_params;
            let gdir = gp.zw - gp.xy;
            let glen_sq = dot(gdir, gdir);
            if glen_sq > 0.000001 {
                let t_g = dot(p - gp.xy, gdir) / glen_sq;
                base_color_3d = mix(prim.color, prim.color2, clamp(t_g, 0.0, 1.0));
            } else {
                base_color_3d = prim.color;
            }
        } else if fill_type == 2u {
            // Radial gradient: (cx, cy, radius, 0) in screen-space pixels
            let t_g = length(p - prim.gradient_params.xy) / max(prim.gradient_params.z, 0.001);
            base_color_3d = mix(prim.color, prim.color2, clamp(t_g, 0.0, 1.0));
        } else {
            base_color_3d = prim.color;
        }

        var result_3d = base_color_3d * vec4<f32>(vec3<f32>(lighting), 1.0);
        result_3d.a *= clip_alpha * edge_aa;
        return result_3d;
    }

    // ── 3D Group SDF Raymarching Path ──
    // border[1] = shape_count, border[2] = aux_data offset
    if shape_type == 6u && prim.border.y > 0.5 {
        let group_shape_count = u32(prim.border.y);
        let group_aux_offset = u32(prim.border.z);

        // Use max depth from child shapes via border[3] (set by paint context)
        let group_depth = max(prim.border.w, 1.0);
        let translate_z = prim.sdf_3d.w;
        let pd = select(800.0, persp_d, persp_d > 0.001);
        let rel = p - center;

        // Ray setup (same as individual shapes)
        let cam_vs = vec3<f32>(0.0, 0.0, pd - translate_z);
        let frag_vs = vec3<f32>(rel.x, rel.y, 0.0);
        let ray_dir_vs = normalize(frag_vs - cam_vs);

        // Transform ray to shape space
        var ro = cam_vs;
        var rd = ray_dir_vs;
        ro = rotate_y_inv(ro, sin_ry, cos_ry);
        rd = rotate_y_inv(rd, sin_ry, cos_ry);
        ro = rotate_x_inv(ro, sin_rx, cos_rx);
        rd = rotate_x_inv(rd, sin_rx, cos_rx);
        ro = rotate_z_inv(ro, sin_rz, cos_rz);
        rd = rotate_z_inv(rd, sin_rz, cos_rz);

        // AABB for the entire group
        let half_3d = vec3<f32>(size.x * 0.5, size.y * 0.5, group_depth * 0.5);
        let aabb_t = ray_aabb_intersect(ro, rd, half_3d);
        if aabb_t.x > aabb_t.y || aabb_t.y < 0.0 {
            discard;
        }

        // Raymarch the compound SDF (32 steps)
        var t_rm = max(aabb_t.x - 0.01, 0.0);
        let t_max = aabb_t.x + length(half_3d) * 2.0 + 1.0;
        var hit = false;
        var min_d = 1e10;
        for (var i = 0u; i < 32u; i++) {
            let pos = ro + rd * t_rm;
            let d3 = eval_group_sdf(pos, group_shape_count, group_aux_offset);
            min_d = min(min_d, d3);
            if d3 < 0.001 {
                hit = true;
                break;
            }
            t_rm += d3;
            if t_rm > t_max {
                break;
            }
        }

        // Edge anti-aliasing
        let pixel_size = max(t_rm / pd, 0.5);
        var edge_aa = 1.0;
        if !hit {
            if min_d > pixel_size * 2.0 {
                discard;
            }
            edge_aa = 1.0 - smoothstep(0.0, pixel_size * 1.5, min_d);
        }

        // Compute normal via group SDF
        let hp = ro + rd * t_rm;
        let normal = eval_group_normal(hp, group_shape_count, group_aux_offset);

        // Lighting (same as individual shapes)
        let light_dir = normalize(prim.light.xyz);
        let n_dot_l = max(dot(normal, light_dir), 0.0);
        let ambient_3d = prim.sdf_3d.y;
        let diffuse_3d = n_dot_l * prim.light.w;
        let view_dir = normalize(-rd);
        let half_vec = normalize(light_dir + view_dir);
        let spec_3d = pow(max(dot(normal, half_vec), 0.0), prim.sdf_3d.z) * 0.5;
        let lighting = ambient_3d + diffuse_3d + spec_3d;

        // Per-shape coloring: find which child shape is closest to the hit point
        let base_color_3d = eval_group_closest_shape_color(hp, group_shape_count, group_aux_offset);

        var result_3d = base_color_3d * vec4<f32>(vec3<f32>(lighting), 1.0);
        result_3d.a *= clip_alpha * edge_aa;
        return result_3d;
    }

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

    var result = vec4<f32>(0.0);

    // Calculate shadow first (rendered behind) - but NOT for inner shadow primitives
    // InnerShadow primitives handle their own shadow rendering differently
    if (prim.shadow.z > 0.0 || prim.shadow.w != 0.0) && prim_type != 4u {
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
        // of from the rectangular bounding box. Other primitives use
        // the rounded-rect shadow path.
        var shadow_sdf_dist: f32;
        if prim_type == 8u /* PRIM_NOTCH */ {
            shadow_sdf_dist = sd_notch(
                sp, shadow_origin, shadow_size,
                shadow_radii,
                prim.light,
                prim.perspective,
                prim.sdf_3d
            );
        } else {
            shadow_sdf_dist = sd_shaped_rect(sp, shadow_origin, shadow_size, shadow_radii, prim.corner_shape);
        }
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
        if prim_type == 8u /* PRIM_NOTCH */ && blur > 0.001 {
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
        case 3u /* PRIM_SHADOW */: {
            // Shadow-only primitive - mask out the shape interior
            // Shadow should be visible starting from the shape boundary (d >= 0)
            // Use constant AA width to avoid discontinuities at triangle seams on Vulkan
            let shape_d = sd_shaped_rect(sp, origin, size, prim.corner_radius, prim.corner_shape);
            let aa_width = 0.75;
            let shape_mask = smoothstep(-aa_width, aa_width, shape_d); // 0 inside, 1 outside, AA at edge
            result.a *= shape_mask;
            result.a *= clip_alpha;
            return result;
        }
        case 4u /* PRIM_INNER_SHADOW */: {
            // Inner shadow - renders INSIDE the shape only
            let shape_d = sd_shaped_rect(sp, origin, size, prim.corner_radius, prim.corner_shape);

            // Hard clip at shape boundary - only render where d < 0 (inside)
            if shape_d > 0.0 {
                discard;
            }

            let blur = max(prim.shadow.z, 0.1);
            let spread = prim.shadow.w;
            let offset = prim.shadow.xy;

            // Inner shadow effect: shadow darkens from outer edge inward
            // Use distance from edge (negative shape_d = distance inside)
            let edge_dist = -shape_d;  // Positive value = how far inside the shape

            // Create shadow falloff from edge toward center
            // At edge (edge_dist ≈ 0): full shadow
            // Further inside (edge_dist > blur + spread): no shadow
            let shadow_range = blur + spread;
            let shadow_alpha = 1.0 - smoothstep(0.0, shadow_range, edge_dist - spread);

            // Apply offset by shifting the shadow calculation
            // Offset shifts which "edge" the shadow appears from
            let offset_effect = dot(normalize(offset + vec2<f32>(0.001)), sp - center);
            let offset_bias = clamp(offset_effect / (length(size) * 0.5), -1.0, 1.0) * length(offset);
            let biased_alpha = shadow_alpha * (1.0 + offset_bias * 0.5);

            var inner_result = prim.shadow_color;
            inner_result.a *= clamp(biased_alpha, 0.0, 1.0) * clip_alpha;
            return inner_result;
        }
        case 5u /* PRIM_CIRCLE_SHADOW */: {
            // Circle shadow - radially symmetric Gaussian blur
            let radius = min(size.x, size.y) * 0.5;
            let blur = prim.shadow.z;
            let spread = prim.shadow.w;
            let shadow_offset = prim.shadow.xy;

            let shadow_center = center + shadow_offset;
            let shadow_radius = radius + spread;

            let shadow_alpha = shadow_circle(sp, shadow_center, shadow_radius, blur);

            // Mask out the circle area so shadow doesn't render under it
            // Use constant AA width to avoid discontinuities at triangle seams on Vulkan
            let circle_d = sd_circle(sp, center, radius);
            let aa_width = 0.75;
            let shape_mask = smoothstep(-aa_width, aa_width, circle_d); // 0 inside, 1 outside, AA at edge

            var circle_result = prim.shadow_color * shadow_alpha;
            circle_result.a *= shape_mask * clip_alpha;
            return circle_result;
        }
        case 6u /* PRIM_CIRCLE_INNER_SHADOW */: {
            // Circle inner shadow - renders INSIDE the circle only
            let radius = min(size.x, size.y) * 0.5;
            let circle_d = sd_circle(sp, center, radius);

            // Hard clip at circle boundary
            if circle_d > 0.0 {
                discard;
            }

            let blur = max(prim.shadow.z, 0.1);
            let spread = prim.shadow.w;
            let offset = prim.shadow.xy;

            // Inner shadow effect: shadow darkens from outer edge inward
            let edge_dist = -circle_d;  // How far inside the circle

            // Create shadow falloff from edge toward center
            let shadow_range = blur + spread;
            let shadow_alpha = 1.0 - smoothstep(0.0, shadow_range, edge_dist - spread);

            // Apply offset
            let offset_effect = dot(normalize(offset + vec2<f32>(0.001)), sp - center);
            let offset_bias = clamp(offset_effect / radius, -1.0, 1.0) * length(offset);
            let biased_alpha = shadow_alpha * (1.0 + offset_bias * 0.5);

            var inner_result = prim.shadow_color;
            inner_result.a *= clamp(biased_alpha, 0.0, 1.0) * clip_alpha;
            return inner_result;
        }
        case 7u /* PRIM_TEXT */: {
            // Text glyph - sample from glyph atlas.
            //
            // `gradient_params` carries atlas PIXEL coords for the
            // glyph quad: (px_min, py_min, px_max, py_max). UVs are
            // derived from the CURRENT atlas dimensions via
            // `textureDimensions()` so atlas growth (which doubles
            // dims to fit fresh glyphs) automatically produces
            // correct sampling — pre-fix the CPU baked UVs at
            // `prepare_text` time and any later growth made them
            // reference the wrong sub-rect of the resized texture,
            // producing garbled glyphs on the next paint.
            //
            // fill_type stores is_color flag (1 = color emoji, 0 = grayscale).
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

            // Calculate UV within the glyph quad
            // Use sp (inverse-transformed point) so rotated/skewed text samples correctly
            let local_uv = (sp - origin) / size;

            // Map to atlas UV coordinates
            let atlas_uv = uv_bounds.xy + local_uv * (uv_bounds.zw - uv_bounds.xy);

            // Use `textureSampleLevel(..., 0.0)` instead of `textureSample`
            // here. WGSL's uniform-control-flow rule applies to
            // *implicit-LOD* sampling — `textureSample` derives mip
            // level from quad derivatives, which require all four
            // pixels in the 2x2 derivative quad to take the same code
            // path. We're inside `switch prim_type { case 7u /* PRIM_TEXT */ }`
            // and `prim_type` comes from a per-instance buffer lookup
            // (`primitives[in.instance_index]`), which Dawn classifies
            // as non-uniform — so any implicit-LOD sample inside ANY
            // arm of this switch fails validation.
            //
            // `textureSampleLevel(t, s, uv, 0.0)` takes the LOD as an
            // explicit parameter, so no derivatives are needed and the
            // uniformity rule doesn't apply. The glyph atlas is a
            // single-mip texture (R8Unorm / Rgba8UnormSrgb, no mipmap
            // chain), so LOD 0 is the only valid level anyway —
            // sampling at LOD 0 is byte-identical to what
            // `textureSample` would have produced. Native backends
            // (Metal, Vulkan, DX12) accept both forms; Dawn (Chrome's
            // WebGPU validator) requires the explicit form here.
            var text_result: vec4<f32>;
            if is_color {
                // Color emoji - sample RGBA directly from color atlas
                text_result = textureSampleLevel(color_glyph_atlas, glyph_sampler, atlas_uv, 0.0);
            } else {
                // Grayscale text - sample coverage from R channel,
                // apply gamma correction, tint with primitive color
                let coverage = textureSampleLevel(glyph_atlas, glyph_sampler, atlas_uv, 0.0).r;
                let gamma_coverage = pow(coverage, 0.7);
                text_result = vec4<f32>(prim.color.rgb, prim.color.a * gamma_coverage);
            }

            // Apply clip alpha
            text_result.a *= clip_alpha;

            // Soft anti-aliased clipping at the rect-clip edges —
            // only when the primitive actually has a rect clip.
            // When `clip_type` is None the bounds are padding /
            // stale metadata and this smoothstep would discard
            // every fragment for a degenerate (zero-width or
            // zero-height) rectangle that the layout pipeline can
            // produce mid-frame (e.g. a scroll container whose
            // inner clip hasn't resolved a dimension yet).
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
        case 8u /* PRIM_NOTCH */: {
            // Rounded rect with optional concave corners + optional top/bottom
            // edge modifiers (scoop/bulge/cut/peak) — everything is SDF-
            // composed in `sd_notch`, no CPU tessellation. See the
            // `PrimitiveType::Notch` doc comment in `blinc_gpu::primitives`
            // for which GpuPrimitive slots carry the notch parameters.
            d = sd_notch(
                sp, origin, size,
                prim.corner_radius,
                prim.light,       // per-corner type flags (TL, TR, BR, BL)
                prim.perspective, // top modifier    (type, width, height, corner_r)
                prim.sdf_3d       // bottom modifier (type, width, height, corner_r)
            );
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

        // Fast path: clearly inside inner area, not near rounded corner.
        // Disabled for PRIM_NOTCH because the quadrant-based geometry
        // above is derived from the rect bounding box and doesn't
        // account for concave corners / top-bottom modifiers — a point
        // deep inside the bbox can still be near a concave flare or a
        // bulge edge and must go through the SDF-based `inner_sdf`
        // branch below to pick up the border ring correctly.
        if is_within_inner_straight && !is_near_rounded_corner && prim_type != 8u {
            // No border here, keep fill_color as-is
        } else {
            // Calculate inner SDF based on context
            var inner_sdf: f32;

            if abs(reduced_border.x - reduced_border.y) < 0.001 {
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
"#;

/// Compositor v2 damage-rect scissored-clear shader.
///
/// Draws a fullscreen triangle that outputs `(0, 0, 0, 0)`. Combined
/// with a REPLACE blend pipeline and an active `set_scissor_rect`,
/// it zeros the damaged region of the static cache so the
/// subsequent SDF dispatch can re-paint without ghosting on
/// anti-aliased / semi-transparent edges.
pub const CLEAR_QUAD_SHADER: &str = include_str!("shaders/clear_quad.wgsl");

/// Split SDF shader: Core shapes (Rect, Circle, Ellipse)
///
/// Handles prim_type 0-2 with full features: borders, gradients,
/// CSS filters, mask gradients, perspective, clip regions.
/// Part of the multi-pipeline SDF split for driver compatibility.
pub const SDF_CORE_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_core.wgsl"));

/// Split SDF shader: Shadow primitives
///
/// Handles prim_type 3-6 (Shadow, InnerShadow, CircleShadow,
/// CircleInnerShadow). Each case is self-contained with early return.
pub const SDF_SHADOW_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_shadow.wgsl"));

/// Split SDF shader: 3D raymarched shapes
///
/// Handles primitives with shape_type > 0 (3D box, sphere, cylinder,
/// torus, capsule, group). 32-step raymarching with Blinn-Phong lighting.
pub const SDF_3D_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_3d.wgsl"));

/// Split SDF shader: Notch primitives (prim_type 8)
///
/// Handles concave corners with edge modifiers (scoop, bulge, cut, peak).
/// Full border, gradient, filter, and mask support.
pub const SDF_NOTCH_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_notch.wgsl"));

/// Vertex-buffer fallback: Core shapes (no VERTEX_STORAGE needed)
///
/// Same as SDF_CORE_SHADER but vs_main reads instance attributes
/// from a vertex buffer instead of the storage buffer.
pub const SDF_CORE_VB_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_core_vb.wgsl"));

/// Vertex-buffer fallback: Shadow primitives
pub const SDF_SHADOW_VB_SHADER: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sdf_shadow_vb.wgsl"));

/// Vertex-buffer fallback: 3D raymarched shapes
pub const SDF_3D_VB_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_3d_vb.wgsl"));

/// Vertex-buffer fallback: Notch primitives
pub const SDF_NOTCH_VB_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_notch_vb.wgsl"));

/// Data-texture fallback: Core shapes (no storage buffers needed — WebGL2)
///
/// Replaces storage buffer reads with textureLoad from RGBA32F data textures.
/// Also uses vertex buffer instance attributes (inherits from VB variant).
pub const SDF_CORE_DT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_core_dt.wgsl"));

/// Data-texture fallback: Shadow primitives
pub const SDF_SHADOW_DT_SHADER: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sdf_shadow_dt.wgsl"));

/// Data-texture fallback: 3D raymarched shapes
pub const SDF_3D_DT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_3d_dt.wgsl"));

/// Data-texture fallback: Notch primitives
pub const SDF_NOTCH_DT_SHADER: &str = include_str!(concat!(env!("OUT_DIR"), "/sdf_notch_dt.wgsl"));

/// Data-texture fallback: Text rendering (no storage buffers — WebGL2)
///
/// Glyph data packed into Rgba32Float texture (width=6, height=max_glyphs).
/// Same visual output as TEXT_SHADER.
pub const TEXT_DT_SHADER: &str = include_str!("shaders/text_dt.wgsl");

/// Data-texture fallback: Glass effects (no storage buffers — WebGL2)
pub const GLASS_DT_SHADER: &str = include_str!("shaders/glass_dt.wgsl");

/// Data-texture fallback: Simple glass effects (no storage buffers — WebGL2)
pub const SIMPLE_GLASS_DT_SHADER: &str = include_str!("shaders/simple_glass_dt.wgsl");

/// Data-texture fallback: Mesh rendering with joint matrices as texture (WebGL2)
pub const MESH_DT_SHADER: &str = include_str!("shaders/mesh_dt.wgsl");

/// Shader for text rendering with SDF glyphs
///
/// Supports both grayscale text glyphs and color emoji:
/// - Grayscale: samples R channel from glyph_atlas, multiplies with color
/// - Color emoji: samples RGBA from color_atlas, uses texture color directly
pub const TEXT_SHADER: &str = r#"
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
    // Flags: [is_color, unused, unused, unused]
    // is_color: 1.0 = color emoji (use color_atlas), 0.0 = grayscale (use glyph_atlas)
    flags: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: TextUniforms;
@group(0) @binding(1) var<storage, read> glyphs: array<GlyphInstance>;
@group(0) @binding(2) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(3) var glyph_sampler: sampler;
@group(0) @binding(4) var color_atlas: texture_2d<f32>;

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
    let clip_alpha = calculate_clip_alpha(in.world_pos, in.clip_bounds, in.clip_fade);
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
"#;

/// Shader for glass/vibrancy effects (Apple Glass UI style)
///
/// This shader creates frosted glass effects by:
/// 1. Sampling and blurring the backdrop
/// 2. Applying a tint color
/// 3. Adding optional noise for texture
/// 4. Compositing with the shape mask
pub const GLASS_SHADER: &str = r#"
// ============================================================================
// Blinc Glass/Vibrancy Shader
// ============================================================================
// Creates frosted glass effects similar to Apple's vibrancy system

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) screen_uv: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
}

struct GlassUniforms {
    viewport_size: vec2<f32>,
    time: f32,
    _padding: f32,
}

// Glass material types (matching Apple's vibrancy styles)
const GLASS_ULTRA_THIN: u32 = 0u;
const GLASS_THIN: u32 = 1u;
const GLASS_REGULAR: u32 = 2u;
const GLASS_THICK: u32 = 3u;
const GLASS_CHROME: u32 = 4u;
const GLASS_SIMPLE: u32 = 5u;  // Simple frosted glass - no liquid effects

struct GlassPrimitive {
    // Bounds (x, y, width, height)
    bounds: vec4<f32>,
    // Corner radii (top-left, top-right, bottom-right, bottom-left)
    corner_radius: vec4<f32>,
    // Tint color (RGBA)
    tint_color: vec4<f32>,
    // Glass parameters (blur_radius, saturation, brightness, noise_amount)
    params: vec4<f32>,
    // Glass parameters 2 (border_thickness, light_angle, shadow_blur, shadow_opacity)
    params2: vec4<f32>,
    // Type info (glass_type, shadow_offset_x_bits, shadow_offset_y_bits, 0)
    type_info: vec4<u32>,
    // Clip bounds (x, y, width, height) for clamping blur samples
    clip_bounds: vec4<f32>,
    // Clip corner radii (for rounded rect clips)
    clip_radius: vec4<f32>,
    // Border color (RGBA) - when alpha > 0, renders a solid border instead of light-based highlights
    border_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: GlassUniforms;
@group(0) @binding(1) var<storage, read> primitives: array<GlassPrimitive>;
@group(0) @binding(2) var backdrop_texture: texture_2d<f32>;
@group(0) @binding(3) var backdrop_sampler: sampler;

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
    let shadow_blur = prim.params2.z;
    let shadow_offset_x = bitcast<f32>(prim.type_info.y);
    let shadow_offset_y = bitcast<f32>(prim.type_info.z);
    let shadow_expand = shadow_blur * 3.0 + abs(shadow_offset_x) + abs(shadow_offset_y);

    let bounds = vec4<f32>(
        prim.bounds.x - shadow_expand,
        prim.bounds.y - shadow_expand,
        prim.bounds.z + shadow_expand * 2.0,
        prim.bounds.w + shadow_expand * 2.0
    );

    // Generate quad vertices split along / diagonal (1-3 shared edge)
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
    let pos = vec2<f32>(
        bounds.x + local_uv.x * bounds.z,
        bounds.y + local_uv.y * bounds.w
    );

    // Convert to clip space
    let clip_pos = vec2<f32>(
        (pos.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (pos.y / uniforms.viewport_size.y) * 2.0
    );

    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.uv = pos;
    out.screen_uv = pos / uniforms.viewport_size;
    out.instance_index = instance_index;

    return out;
}

// ============================================================================
// SDF and Blur Functions
// ============================================================================

// Error function approximation for Gaussian blur
fn erf(x: f32) -> f32 {
    let s = sign(x);
    let a = abs(x);
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
    return s * y;
}

// Gaussian shadow for rounded rectangle using SDF
// This properly respects corner radii for accurate rounded rect shadows
fn shadow_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>, sigma: f32) -> f32 {
    // Get SDF distance (negative inside, positive outside)
    let d = sd_rounded_rect(p, origin, size, radius);

    if sigma < 0.001 {
        // No blur - hard edge
        return select(0.0, 1.0, d < 0.0);
    }

    // Use SDF for Gaussian-like falloff
    // erf-based smooth transition from inside to outside
    // This creates a proper soft shadow that follows the rounded rect shape
    let blur_factor = 0.5 * sqrt(2.0) * sigma;
    return 0.5 * (1.0 - erf(d / blur_factor));
}

fn sd_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let center = origin + half_size;
    let rel = p - center;
    let q = abs(rel) - half_size;

    // Select corner radius based on quadrant
    // radius: (top-left, top-right, bottom-right, bottom-left)
    // In screen coords: Y increases downward, so rel.y < 0 means top half
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

// Hash function for noise
fn hash(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

// Smooth noise
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    return mix(
        mix(hash(i + vec2<f32>(0.0, 0.0)), hash(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y
    );
}

// Gaussian weight function
fn gaussian_weight(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma));
}

// Calculate clip alpha for rectangular clip region (for scroll containers)
fn calculate_glass_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>) -> f32 {
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
    return clamp(d + 0.5, 0.0, 1.0);
}

// High quality blur using golden-angle spiral sampling
// CSS spec: blur(Npx) means standard deviation = N pixels
//
// Uses `textureSampleLevel(.., .., uv, 0.0)` instead of `textureSample` so
// the call is legal from non-uniform control flow. The fragment shader
// `discard`s for shadow-only fragments before reaching the blur path,
// which makes flow non-uniform across the quad — and WGSL/WebGPU bans
// implicit-derivative samples (`textureSample`) in that case. The
// backdrop has `mip_level_count: 1`, so explicitly sampling LOD 0 is
// functionally identical to the implicit form.
fn blur_backdrop(uv: vec2<f32>, blur_radius: f32) -> vec4<f32> {
    if blur_radius < 0.5 {
        return textureSampleLevel(backdrop_texture, backdrop_sampler, uv, 0.0);
    }

    let texel_size = 1.0 / uniforms.viewport_size;
    let sigma = blur_radius; // CSS spec: blur radius IS the standard deviation

    // Start with center sample (highest weight)
    var color = textureSampleLevel(backdrop_texture, backdrop_sampler, uv, 0.0);
    var total_weight = 1.0;

    let golden_angle = 2.39996323; // 137.5 degrees in radians

    // Sample out to 2.5 sigma for proper Gaussian coverage
    let sample_extent = blur_radius * 2.5;

    // 6 rings with 12 samples each = 72 samples, linear spacing
    let num_rings = 6;
    let samples_per_ring = 12;

    for (var ring = 1; ring <= num_rings; ring++) {
        let ring_t = f32(ring) / f32(num_rings);
        let ring_radius = sample_extent * ring_t; // Linear spacing
        let ring_offset = ring_radius * texel_size;

        for (var i = 0; i < samples_per_ring; i++) {
            let angle = f32(i) * (6.283185 / f32(samples_per_ring)) + f32(ring) * golden_angle;
            let offset = vec2<f32>(cos(angle), sin(angle)) * ring_offset;

            let sample_pos = uv + offset;
            let weight = gaussian_weight(ring_radius, sigma);

            color += textureSampleLevel(backdrop_texture, backdrop_sampler, sample_pos, 0.0) * weight;
            total_weight += weight;
        }
    }

    return color / total_weight;
}

// High quality blur with clip bounds for scroll containers
// CSS spec: blur(Npx) means standard deviation = N pixels
//
// Uses `textureSampleLevel` for the same uniform-control-flow reason
// documented on `blur_backdrop` above.
fn blur_backdrop_clipped(uv: vec2<f32>, blur_radius: f32, clip_bounds: vec4<f32>) -> vec4<f32> {
    let clip_min = clip_bounds.xy / uniforms.viewport_size;
    let clip_max = (clip_bounds.xy + clip_bounds.zw) / uniforms.viewport_size;
    let has_clip = clip_bounds.x > -5000.0;

    if blur_radius < 0.5 {
        let clamped_uv = select(uv, clamp(uv, clip_min, clip_max), has_clip);
        return textureSampleLevel(backdrop_texture, backdrop_sampler, clamped_uv, 0.0);
    }

    let texel_size = 1.0 / uniforms.viewport_size;
    let sigma = blur_radius; // CSS spec: blur radius IS the standard deviation

    // Start with center sample (highest weight)
    let center_uv = select(uv, clamp(uv, clip_min, clip_max), has_clip);
    var color = textureSampleLevel(backdrop_texture, backdrop_sampler, center_uv, 0.0);
    var total_weight = 1.0;

    let golden_angle = 2.39996323; // 137.5 degrees in radians

    // Sample out to 2.5 sigma for proper Gaussian coverage
    let sample_extent = blur_radius * 2.5;

    // 6 rings with 12 samples each = 72 samples, linear spacing
    let num_rings = 6;
    let samples_per_ring = 12;

    for (var ring = 1; ring <= num_rings; ring++) {
        let ring_t = f32(ring) / f32(num_rings);
        let ring_radius = sample_extent * ring_t; // Linear spacing
        let ring_offset = ring_radius * texel_size;

        for (var i = 0; i < samples_per_ring; i++) {
            let angle = f32(i) * (6.283185 / f32(samples_per_ring)) + f32(ring) * golden_angle;
            let offset = vec2<f32>(cos(angle), sin(angle)) * ring_offset;

            var sample_pos = uv + offset;
            sample_pos = select(sample_pos, clamp(sample_pos, clip_min, clip_max), has_clip);

            let weight = gaussian_weight(ring_radius, sigma);
            color += textureSampleLevel(backdrop_texture, backdrop_sampler, sample_pos, 0.0) * weight;
            total_weight += weight;
        }
    }

    return color / total_weight;
}

// Apply saturation adjustment
fn adjust_saturation(color: vec3<f32>, saturation: f32) -> vec3<f32> {
    let luminance = dot(color, vec3<f32>(0.299, 0.587, 0.114));
    return mix(vec3<f32>(luminance), color, saturation);
}

// Calculate SDF gradient (normal direction pointing outward from shape)
fn sdf_gradient(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> vec2<f32> {
    let eps = 0.5;
    let d = sd_rounded_rect(p, origin, size, radius);
    let dx = sd_rounded_rect(p + vec2<f32>(eps, 0.0), origin, size, radius) - d;
    let dy = sd_rounded_rect(p + vec2<f32>(0.0, eps), origin, size, radius) - d;
    let g = vec2<f32>(dx, dy);
    let len = length(g);
    if len < 0.001 {
        return vec2<f32>(0.0, -1.0);
    }
    return g / len;
}

// ============================================================================
// Fragment Shader - iOS 26 Liquid Glass Effect
// ============================================================================
// Liquid glass = smooth rounded bevel, NOT hard edge lines
// The "liquid" feel comes from wide, gentle transitions that look organic

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let prim = primitives[in.instance_index];
    let p = in.uv;

    // Calculate clip alpha first - discard if completely outside clip bounds
    let clip_alpha = calculate_glass_clip_alpha(p, prim.clip_bounds);
    if clip_alpha < 0.001 {
        discard;
    }

    let origin = prim.bounds.xy;
    let size = prim.bounds.zw;

    // Shadow parameters
    let shadow_blur = prim.params2.z;
    let shadow_opacity = prim.params2.w;
    let shadow_offset_x = bitcast<f32>(prim.type_info.y);
    let shadow_offset_y = bitcast<f32>(prim.type_info.z);

    // Calculate SDF with smooth anti-aliasing
    let d = sd_rounded_rect(p, origin, size, prim.corner_radius);
    let aa = 1.5; // Wide AA for smooth edges (constant to avoid Vulkan triangle seam artifacts)

    // Smooth mask - combine with clip alpha
    let mask = (1.0 - smoothstep(-aa, aa, d)) * clip_alpha;

    // ========================================================================
    // DROP SHADOW (rendered as pure shadow, no glass effects)
    // ========================================================================
    // Shadow is a simple soft rectangle behind the glass - no bevel, no refraction
    let has_shadow = shadow_opacity > 0.001 && shadow_blur > 0.001;
    var shadow_color_premult = vec4<f32>(0.0);

    if has_shadow {
        let shadow_origin = origin + vec2<f32>(shadow_offset_x, shadow_offset_y);
        let shadow_alpha = shadow_rounded_rect(p, shadow_origin, size, prim.corner_radius, shadow_blur);
        // Apply clip alpha to shadow as well
        shadow_color_premult = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha * shadow_opacity * clip_alpha);

        // If we're completely outside the glass panel, just render the shadow
        if mask < 0.001 {
            if shadow_alpha > 0.001 && clip_alpha > 0.001 {
                return shadow_color_premult;
            }
            discard;
        }
    } else {
        // No shadow - discard if outside glass
        if mask < 0.001 {
            discard;
        }
    }

    // Glass parameters
    let blur_radius = prim.params.x;
    let saturation = prim.params.y;
    let brightness = prim.params.z;
    let noise_amount = prim.params.w;
    let glass_type = prim.type_info.x;

    // ========================================================================
    // SIMPLE FROSTED GLASS (no liquid effects)
    // ========================================================================
    // Pure frosted glass: blur + tint + saturation/brightness
    // No refraction, no edge bevels, no light reflections
    if glass_type == GLASS_SIMPLE {
        // Sample and blur the backdrop directly at screen UV (no refraction)
        var simple_backdrop = blur_backdrop_clipped(in.screen_uv, blur_radius, prim.clip_bounds);

        // Apply saturation and brightness adjustments
        var result_rgb = adjust_saturation(simple_backdrop.rgb, saturation);
        result_rgb = result_rgb * brightness;

        // Apply tint as a subtle additive overlay (not heavy mixing)
        // This keeps the backdrop colors visible while adding a light tint
        let tint = prim.tint_color;
        if tint.a > 0.001 {
            // Soft light blend: backdrop + tint * tint_alpha (additive overlay)
            result_rgb = result_rgb + tint.rgb * tint.a * 0.5;
        }

        // Optional noise for frosted texture
        if noise_amount > 0.0 {
            let n = noise(p * 0.3);
            result_rgb = result_rgb + vec3<f32>((n - 0.5) * noise_amount * 0.02);
        }

        result_rgb = clamp(result_rgb, vec3<f32>(0.0), vec3<f32>(1.0));

        // Blend shadow underneath the glass
        if has_shadow && shadow_color_premult.a > 0.001 {
            let shadow_contrib = shadow_color_premult.a * (1.0 - mask);
            let final_alpha = mask + shadow_contrib;
            if final_alpha > 0.001 {
                let final_rgb = (result_rgb * mask + shadow_color_premult.rgb * shadow_contrib) / final_alpha;
                return vec4<f32>(final_rgb, final_alpha);
            }
        }

        return vec4<f32>(result_rgb, mask);
    }

    // Distance from edge (0 at edge, positive going inward)
    let inner_dist = max(0.0, -d);

    // ========================================================================
    // TWO-LAYER LIQUID GLASS (Apple-style)
    // ========================================================================
    // Layer 1: EDGE BEVEL - wider rim with strong light bending for liquid effect
    // Layer 2: FLAT CENTER - undistorted frosted glass surface
    // The edge seamlessly connects to the flat center.

    // Edge bevel thickness - concentrated near edge for sharp liquid bevel
    let edge_thickness = min(25.0, min(size.x, size.y) * 0.2);

    // Progress through edge zone: 0 = at glass edge, 1 = into flat center
    let edge_progress = clamp(inner_dist / edge_thickness, 0.0, 1.0);

    // For depth shading (used later)
    let bevel = 1.0 - edge_progress;

    // ========================================================================
    // EDGE BEVEL REFRACTION - Liquid Glass Effect
    // ========================================================================
    // The refraction follows the edge NORMAL direction, not radial from center.
    // This creates proper glass rim bending where light bends perpendicular to the edge.

    // Get SDF gradient (points outward from shape - this IS the edge normal)
    let edge_normal = sdf_gradient(p, origin, size, prim.corner_radius);

    // Refraction strength: strongest at outer edge, fades smoothly to center
    // Using quadratic falloff concentrated at edge for visible bevel effect
    let refract_strength = bevel * bevel;

    // Refraction multiplier from type_info.w (0.0 = no refraction, 1.0 = full refraction)
    // We use a sentinel value: if type_info.w == 0 (unset), default to 1.0 (full refraction)
    // To disable refraction, set type_info.w to the bits of a small negative number like -1.0
    // This way 0 (unset) = full refraction, any other value = that value's refraction
    let refraction_mult = bitcast<f32>(prim.type_info.w);
    // Check if explicitly set (non-zero bits) - if unset (0), use 1.0 for backwards compat
    // If set to 0.0f (which has bits 0x00000000), we need a different sentinel
    // Solution: use -1.0 as "use explicit value" flag in the sign bit
    let is_explicitly_set = (prim.type_info.w & 0x80000000u) != 0u; // Check sign bit
    let explicit_value = abs(refraction_mult); // Remove sign to get actual value
    let effective_refract_mult = select(1.0, explicit_value, is_explicitly_set);

    // Offset UV along edge normal - sample backdrop from OUTSIDE the shape
    // This creates the "looking through curved glass rim" effect where
    // content appears pulled inward at the bevel
    // The offset is in PIXELS, then converted to UV space
    // Strong distortion for clearly visible bevel curve
    let refract_pixels = refract_strength * 60.0 * effective_refract_mult; // Up to 60 pixels of displacement at edge
    let refract_offset = edge_normal * refract_pixels;

    // Apply refraction - ADD offset to sample from outside (pulls content inward visually)
    let refracted_uv = in.screen_uv + refract_offset / uniforms.viewport_size;

    // ========================================================================
    // APPLE LIQUID GLASS EFFECT (WWDC25 Style)
    // ========================================================================
    // Key characteristics from reference:
    // 1. Nearly transparent interior - minimal blur/frost
    // 2. Crisp bright edge highlight line along perimeter
    // 3. Subtle edge shadow just inside the highlight
    // 4. Very subtle refraction - background barely distorted
    // 5. Optional chromatic aberration at edges

    // ========================================================================
    // BACKDROP - Blur based on blur_radius parameter
    // ========================================================================
    // Use blur_radius directly - user controls the blur amount
    // The blur is applied to the interior, edges remain clear due to refraction
    let effective_blur = blur_radius; // Direct control - user sets exact blur amount
    // Use clipped blur to prevent sampling outside scroll containers
    var backdrop = blur_backdrop_clipped(refracted_uv, effective_blur, prim.clip_bounds);
    backdrop = vec4<f32>(adjust_saturation(backdrop.rgb, saturation), 1.0);
    backdrop = vec4<f32>(backdrop.rgb * brightness, 1.0);

    var result = backdrop.rgb;

    // ========================================================================
    // EDGE HIGHLIGHT / BORDER - Configurable thin line with angle-based light reflection
    // ========================================================================
    // When border_color.a > 0, use the explicit border color.
    // Otherwise, use the signature light-based edge highlight.
    let edge_line_width = prim.params2.x; // User-configurable border thickness
    let light_angle = prim.params2.y;     // Light source angle in radians
    let bc = prim.border_color;

    let edge_line = smoothstep(0.0, edge_line_width * 0.3, inner_dist) *
                    (1.0 - smoothstep(edge_line_width, edge_line_width * 1.5, inner_dist));

    // Compute light-based highlight strength (used by glass type variants too)
    let light_dir = vec2<f32>(cos(light_angle), sin(light_angle));
    let facing_light = dot(edge_normal, -light_dir);
    let light_factor = 0.2 + 0.8 * max(0.0, facing_light);
    let highlight_strength = edge_line * 0.6 * light_factor * mask;

    if bc.a > 0.001 {
        // Solid border color mode: blend the user-specified color at the edge
        result = mix(result, bc.rgb, edge_line * bc.a * mask);
    } else {
        // Light-based highlight mode (default liquid glass look)
        result = result + vec3<f32>(highlight_strength);
    }

    // ========================================================================
    // INNER EDGE SHADOW - Very subtle depth
    // ========================================================================
    let shadow_start = edge_line_width * 2.5;
    let shadow_end = edge_line_width * 8.0;
    let inner_shadow = smoothstep(shadow_start, shadow_end, inner_dist) *
                       (1.0 - smoothstep(shadow_end, shadow_end * 3.0, inner_dist));
    result = result - vec3<f32>(inner_shadow * 0.04 * mask); // More subtle, masked

    // ========================================================================
    // VERY SUBTLE TINT - Almost invisible
    // ========================================================================
    let tint = prim.tint_color;
    let tint_strength = tint.a * 0.08; // Even more subtle
    result = mix(result, tint.rgb, tint_strength);

    // Optional subtle noise
    if noise_amount > 0.0 {
        let n = noise(p * 0.3);
        result = result + vec3<f32>((n - 0.5) * noise_amount * 0.005);
    }

    // Glass type variants - adjust edge highlight intensity
    switch glass_type {
        case GLASS_ULTRA_THIN: {
            // Even more transparent
            result = mix(backdrop.rgb, result, 0.7);
        }
        case GLASS_THIN: {
            // Slightly more visible
        }
        case GLASS_REGULAR: {
            // Default - as designed above
        }
        case GLASS_THICK: {
            // Stronger edge highlight
            result = result + vec3<f32>(highlight_strength * 0.3);
        }
        case GLASS_CHROME: {
            // Add slight metallic tint
            let chrome = vec3<f32>(0.96, 0.97, 0.99);
            result = mix(result, chrome, 0.1);
        }
        default: {}
    }

    result = clamp(result, vec3<f32>(0.0), vec3<f32>(1.0));

    // Blend shadow underneath the glass
    // Glass is rendered on top of shadow using standard alpha compositing
    // Final = glass_color * glass_alpha + shadow_color * shadow_alpha * (1 - glass_alpha)
    if has_shadow && shadow_color_premult.a > 0.001 {
        let glass_color = vec4<f32>(result, mask);
        let shadow_contrib = shadow_color_premult.a * (1.0 - mask);
        let final_alpha = mask + shadow_contrib;
        if final_alpha > 0.001 {
            let final_rgb = (result * mask + shadow_color_premult.rgb * shadow_contrib) / final_alpha;
            return vec4<f32>(final_rgb, final_alpha);
        }
    }

    return vec4<f32>(result, mask);
}
"#;

/// Simple frosted glass shader - pure backdrop blur without liquid glass effects
///
/// This shader provides:
/// - Backdrop blur (Gaussian approximation)
/// - Saturation/brightness adjustment
/// - Subtle tint overlay
/// - Drop shadows
///
/// Unlike GLASS_SHADER, this does NOT include:
/// - Edge bevels or refraction
/// - Light reflections
/// - Liquid glass distortion
pub const SIMPLE_GLASS_SHADER: &str = r#"
// ============================================================================
// Simple Frosted Glass Shader
// ============================================================================
//
// Pure backdrop blur without liquid glass effects.
// More performant and suitable for subtle UI backgrounds.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) screen_uv: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
}

struct SimpleGlassUniforms {
    viewport_size: vec2<f32>,
    time: f32,
    _padding: f32,
}

struct SimpleGlassPrimitive {
    bounds: vec4<f32>,
    corner_radius: vec4<f32>,
    tint_color: vec4<f32>,
    params: vec4<f32>,      // blur, saturation, brightness, noise
    params2: vec4<f32>,     // border_thickness, light_angle, shadow_blur, shadow_opacity
    type_info: vec4<u32>,   // glass_type, shadow_offset_x_bits, shadow_offset_y_bits, clip_type
    clip_bounds: vec4<f32>,
    clip_radius: vec4<f32>,
    border_color: vec4<f32>, // Border color (RGBA) - when alpha > 0, renders solid border
}

@group(0) @binding(0) var<uniform> uniforms: SimpleGlassUniforms;
@group(0) @binding(1) var<storage, read> primitives: array<SimpleGlassPrimitive>;
@group(0) @binding(2) var backdrop_texture: texture_2d<f32>;
@group(0) @binding(3) var backdrop_sampler: sampler;

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
    let shadow_blur = prim.params2.z;
    let shadow_offset_x = bitcast<f32>(prim.type_info.y);
    let shadow_offset_y = bitcast<f32>(prim.type_info.z);
    let shadow_expand = shadow_blur * 3.0 + abs(shadow_offset_x) + abs(shadow_offset_y);

    let bounds = vec4<f32>(
        prim.bounds.x - shadow_expand,
        prim.bounds.y - shadow_expand,
        prim.bounds.z + shadow_expand * 2.0,
        prim.bounds.w + shadow_expand * 2.0
    );

    // Generate quad vertices split along / diagonal (1-3 shared edge)
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
    let pos = vec2<f32>(
        bounds.x + local_uv.x * bounds.z,
        bounds.y + local_uv.y * bounds.w
    );

    // Convert to clip space
    let clip_pos = vec2<f32>(
        (pos.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - (pos.y / uniforms.viewport_size.y) * 2.0
    );

    out.position = vec4<f32>(clip_pos, 0.0, 1.0);
    out.uv = pos;
    out.screen_uv = pos / uniforms.viewport_size;
    out.instance_index = instance_index;

    return out;
}

// ============================================================================
// SDF and Blur Functions
// ============================================================================

fn sd_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let center = origin + half_size;
    let rel = p - center;
    let q = abs(rel) - half_size;

    var r: f32;
    if (rel.x < 0.0 && rel.y < 0.0) { r = radius.x; }
    else if (rel.x >= 0.0 && rel.y < 0.0) { r = radius.y; }
    else if (rel.x >= 0.0 && rel.y >= 0.0) { r = radius.z; }
    else { r = radius.w; }

    // Clamp radius to half the minimum dimension (CSS spec)
    r = min(r, min(half_size.x, half_size.y));

    let outer_dist = length(max(q + r, vec2<f32>(0.0)));
    let inner_dist = min(max(q.x + r, q.y + r), 0.0);
    return outer_dist + inner_dist - r;
}

fn shadow_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>, blur: f32) -> f32 {
    let d = sd_rounded_rect(p, origin, size, radius);
    let sigma = blur * 0.5;
    return 1.0 - smoothstep(-sigma * 2.0, sigma * 2.0, d);
}

fn calculate_clip_alpha(p: vec2<f32>, clip_bounds: vec4<f32>) -> f32 {
    let clip_min = clip_bounds.xy;
    let clip_max = clip_bounds.xy + clip_bounds.zw;
    let edge_dist = min(
        min(p.x - clip_min.x, clip_max.x - p.x),
        min(p.y - clip_min.y, clip_max.y - p.y)
    );
    return smoothstep(-0.5, 0.5, edge_dist);
}

fn adjust_saturation(color: vec3<f32>, saturation: f32) -> vec3<f32> {
    let luminance = dot(color, vec3<f32>(0.299, 0.587, 0.114));
    return mix(vec3<f32>(luminance), color, saturation);
}

// Gaussian weight function
fn gaussian_weight(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma));
}

// High quality blur using golden-angle spiral sampling
// CSS spec: blur(Npx) means standard deviation = N pixels
//
// Uses `textureSampleLevel(.., .., uv, 0.0)` for the same uniform-control-flow
// reason documented on the equivalent function in GLASS_SHADER above:
// the fragment shader `discard`s shadow-only fragments, so plain
// `textureSample` (which needs implicit derivatives) is illegal here.
// The backdrop has `mip_level_count: 1`, so explicit LOD 0 is identical.
fn blur_backdrop(uv: vec2<f32>, radius: f32, clip_bounds: vec4<f32>) -> vec4<f32> {
    if radius < 0.5 {
        return textureSampleLevel(backdrop_texture, backdrop_sampler, uv, 0.0);
    }

    let texel_size = 1.0 / uniforms.viewport_size;
    let sigma = radius; // CSS spec: blur radius IS the standard deviation

    // Clip bounds in UV space
    let clip_min = clip_bounds.xy / uniforms.viewport_size;
    let clip_max = (clip_bounds.xy + clip_bounds.zw) / uniforms.viewport_size;
    let has_clip = clip_bounds.x > -5000.0;

    // Start with center sample (highest weight)
    let center_uv = select(uv, clamp(uv, clip_min, clip_max), has_clip);
    var color = textureSampleLevel(backdrop_texture, backdrop_sampler, center_uv, 0.0);
    var total_weight = 1.0;

    // Golden angle spiral for smooth sample distribution
    let golden_angle = 2.39996323; // 137.5 degrees in radians

    // Sample out to 2.5 sigma for proper Gaussian coverage (captures ~99% of kernel)
    let sample_extent = radius * 2.5;

    // 6 rings with 12 samples each = 72 samples, linear spacing
    let num_rings = 6;
    let samples_per_ring = 12;

    for (var ring = 1; ring <= num_rings; ring++) {
        let ring_t = f32(ring) / f32(num_rings);
        let ring_radius = sample_extent * ring_t; // Linear spacing for uniform coverage
        let ring_offset = ring_radius * texel_size;

        for (var i = 0; i < samples_per_ring; i++) {
            let angle = f32(i) * (6.283185 / f32(samples_per_ring)) + f32(ring) * golden_angle;
            let offset = vec2<f32>(cos(angle), sin(angle)) * ring_offset;

            var sample_pos = uv + offset;
            sample_pos = select(sample_pos, clamp(sample_pos, clip_min, clip_max), has_clip);

            let weight = gaussian_weight(ring_radius, sigma);
            color += textureSampleLevel(backdrop_texture, backdrop_sampler, sample_pos, 0.0) * weight;
            total_weight += weight;
        }
    }

    return color / total_weight;
}

// Noise function for frosted texture
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let a = fract(sin(dot(i, vec2<f32>(127.1, 311.7))) * 43758.5453);
    let b = fract(sin(dot(i + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7))) * 43758.5453);
    let c = fract(sin(dot(i + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);
    let d = fract(sin(dot(i + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7))) * 43758.5453);

    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// ============================================================================
// Fragment Shader
// ============================================================================

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let prim = primitives[in.instance_index];
    let p = in.uv;

    // Calculate clip alpha
    let clip_alpha = calculate_clip_alpha(p, prim.clip_bounds);
    if clip_alpha < 0.001 {
        discard;
    }

    let origin = prim.bounds.xy;
    let size = prim.bounds.zw;

    // Shadow parameters
    let shadow_blur = prim.params2.z;
    let shadow_opacity = prim.params2.w;
    let shadow_offset_x = bitcast<f32>(prim.type_info.y);
    let shadow_offset_y = bitcast<f32>(prim.type_info.z);

    // Calculate SDF
    let d = sd_rounded_rect(p, origin, size, prim.corner_radius);
    let aa = 1.5; // Constant AA to avoid Vulkan triangle seam artifacts
    let mask = (1.0 - smoothstep(-aa, aa, d)) * clip_alpha;

    // Drop shadow
    let has_shadow = shadow_opacity > 0.001 && shadow_blur > 0.001;
    var shadow_color_premult = vec4<f32>(0.0);

    if has_shadow {
        let shadow_origin = origin + vec2<f32>(shadow_offset_x, shadow_offset_y);
        let shadow_alpha = shadow_rounded_rect(p, shadow_origin, size, prim.corner_radius, shadow_blur);
        shadow_color_premult = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha * shadow_opacity * clip_alpha);

        if mask < 0.001 {
            if shadow_alpha > 0.001 && clip_alpha > 0.001 {
                return shadow_color_premult;
            }
            discard;
        }
    } else {
        if mask < 0.001 {
            discard;
        }
    }

    // Glass parameters
    let blur_radius = prim.params.x;
    let saturation = prim.params.y;
    let brightness = prim.params.z;
    let noise_amount = prim.params.w;

    // Sample and blur backdrop directly (NO refraction, NO distortion)
    var backdrop = blur_backdrop(in.screen_uv, blur_radius, prim.clip_bounds);

    // Apply saturation and brightness
    var result_rgb = adjust_saturation(backdrop.rgb, saturation);
    result_rgb = result_rgb * brightness;

    // Apply tint as subtle additive overlay
    let tint = prim.tint_color;
    if tint.a > 0.001 {
        result_rgb = result_rgb + tint.rgb * tint.a * 0.5;
    }

    // Optional noise for frosted texture
    if noise_amount > 0.0 {
        let n = noise(p * 0.3);
        result_rgb = result_rgb + vec3<f32>((n - 0.5) * noise_amount * 0.02);
    }

    // Border color support
    let bc = prim.border_color;
    if bc.a > 0.001 {
        let border_width = prim.params2.x;
        let inner_dist = -d; // Distance from edge into the shape
        let edge_band = smoothstep(0.0, border_width * 0.3, inner_dist) *
                        (1.0 - smoothstep(border_width, border_width * 1.5, inner_dist));
        result_rgb = mix(result_rgb, bc.rgb, edge_band * bc.a * mask);
    }

    result_rgb = clamp(result_rgb, vec3<f32>(0.0), vec3<f32>(1.0));

    // Blend shadow underneath
    if has_shadow && shadow_color_premult.a > 0.001 {
        let shadow_contrib = shadow_color_premult.a * (1.0 - mask);
        let final_alpha = mask + shadow_contrib;
        if final_alpha > 0.001 {
            let final_rgb = (result_rgb * mask + shadow_color_premult.rgb * shadow_contrib) / final_alpha;
            return vec4<f32>(final_rgb, final_alpha);
        }
    }

    return vec4<f32>(result_rgb, mask);
}
"#;

/// Shader for compositing layers with blend modes
pub const COMPOSITE_SHADER: &str = r#"
// ============================================================================
// Blinc Compositor Shader
// ============================================================================

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct CompositeUniforms {
    opacity: f32,
    blend_mode: u32,
    _padding: vec2<f32>,
}

// Blend modes
const BLEND_NORMAL: u32 = 0u;
const BLEND_MULTIPLY: u32 = 1u;
const BLEND_SCREEN: u32 = 2u;
const BLEND_OVERLAY: u32 = 3u;
const BLEND_DARKEN: u32 = 4u;
const BLEND_LIGHTEN: u32 = 5u;

@group(0) @binding(0) var<uniform> uniforms: CompositeUniforms;
@group(0) @binding(1) var source_texture: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Fullscreen triangle
    let uv = vec2<f32>(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u)
    );

    out.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);

    return out;
}

fn blend_overlay(base: vec3<f32>, blend: vec3<f32>) -> vec3<f32> {
    return select(
        2.0 * base * blend,
        1.0 - 2.0 * (1.0 - base) * (1.0 - blend),
        base > vec3<f32>(0.5)
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(source_texture, source_sampler, in.uv);

    // Apply opacity
    var result = color;
    result.a *= uniforms.opacity;

    // Note: actual blending with destination happens in the blend state
    // This shader just prepares the source color

    return result;
}
"#;

/// Shader for tessellated path rendering (triangles with per-vertex colors)
pub const PATH_SHADER: &str = r#"
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
}

// ============================================================================
// SDF Functions for Clipping
// ============================================================================

// Rounded rectangle SDF
fn sd_rounded_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
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
        let data = aux_data[vec_idx];
        var vi: vec2<f32>;
        if (i % 2u) == 0u {
            vi = data.xy;
        } else {
            vi = data.zw;
        }

        let j = (i + 1u) % vertex_count;
        let vec_idx_j = aux_offset + (j / 2u);
        let data_j = aux_data[vec_idx_j];
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
    );

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
"#;

/// Shader for image rendering
///
/// Renders images with:
/// - UV cropping for box-fit modes
/// - Tinting and opacity
/// - Optional rounded corners
pub const IMAGE_SHADER: &str = include_str!("shaders/image.wgsl");

/// Shader for layer composition
///
/// Composites offscreen layer textures onto parent targets with:
/// - Blend mode support (Normal, Multiply, Screen, Overlay, etc.)
/// - Opacity application
/// - Source and destination rectangle mapping
pub const LAYER_COMPOSITE_SHADER: &str = r#"
// ============================================================================
// Layer Composition Shader
// ============================================================================
//
// Composites a layer texture onto a destination with blend modes and opacity.

// Blend mode constants (matching blinc_core::BlendMode)
const BLEND_NORMAL: u32 = 0u;
const BLEND_MULTIPLY: u32 = 1u;
const BLEND_SCREEN: u32 = 2u;
const BLEND_OVERLAY: u32 = 3u;
const BLEND_DARKEN: u32 = 4u;
const BLEND_LIGHTEN: u32 = 5u;
const BLEND_COLOR_DODGE: u32 = 6u;
const BLEND_COLOR_BURN: u32 = 7u;
const BLEND_HARD_LIGHT: u32 = 8u;
const BLEND_SOFT_LIGHT: u32 = 9u;
const BLEND_DIFFERENCE: u32 = 10u;
const BLEND_EXCLUSION: u32 = 11u;

struct LayerUniforms {
    // Source rectangle in layer texture (normalized 0-1)
    source_rect: vec4<f32>,  // x, y, width, height
    // Destination rectangle in viewport (pixels)
    dest_rect: vec4<f32>,    // x, y, width, height
    // Viewport size for coordinate conversion
    viewport_size: vec2<f32>,
    // Layer opacity (0.0 - 1.0)
    opacity: f32,
    // Blend mode (see constants above)
    blend_mode: u32,
    // Clip bounds (x, y, width, height) in pixels
    clip_bounds: vec4<f32>,
    // Clip corner radii (top-left, top-right, bottom-right, bottom-left)
    clip_radius: vec4<f32>,
    // Clip type: 0=none, 1=rect with optional rounded corners
    clip_type: u32,
    // 3D perspective transform (0 = disabled)
    perspective_d: f32,
    sin_rx: f32,
    cos_rx: f32,
    sin_ry: f32,
    cos_ry: f32,
    // In-plane (Z-axis) rotation, applied to the flat composite path.
    // Identity = (0.0, 1.0). Used by motion-bound subtrees whose
    // cached texture must rotate per frame (e.g. cn::spinner's
    // rotate_timeline) without re-baking.
    sin_rz: f32,
    cos_rz: f32,
}

@group(0) @binding(0) var<uniform> uniforms: LayerUniforms;
@group(0) @binding(1) var layer_texture: texture_2d<f32>;
@group(0) @binding(2) var layer_sampler: sampler;
@group(0) @binding(3) var dest_texture: texture_2d<f32>;
@group(0) @binding(4) var dest_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) frag_pos: vec2<f32>,  // Fragment position in viewport pixels
}

// SDF for rounded rectangle clipping
fn sd_rounded_rect_clip(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    // rect: x, y, width, height
    // radii: top-left, top-right, bottom-right, bottom-left
    let center = rect.xy + rect.zw * 0.5;
    let half_size = rect.zw * 0.5;
    let q = abs(p - center) - half_size;

    // Select corner radius based on quadrant
    var r: f32;
    if (p.x < center.x) {
        if (p.y < center.y) {
            r = radii.x;  // top-left
        } else {
            r = radii.w;  // bottom-left
        }
    } else {
        if (p.y < center.y) {
            r = radii.y;  // top-right
        } else {
            r = radii.z;  // bottom-right
        }
    }

    // Clamp radius to half the minimum dimension (CSS spec)
    r = min(r, min(half_size.x, half_size.y));

    let adjusted_q = q + r;
    return length(max(adjusted_q, vec2<f32>(0.0))) + min(max(adjusted_q.x, adjusted_q.y), 0.0) - r;
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Generate quad vertices from vertex index (0-5 for two triangles)
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),  // Top-left
        vec2<f32>(1.0, 0.0),  // Top-right
        vec2<f32>(0.0, 1.0),  // Bottom-left
        vec2<f32>(1.0, 0.0),  // Top-right
        vec2<f32>(1.0, 1.0),  // Bottom-right
        vec2<f32>(0.0, 1.0),  // Bottom-left
    );

    let local_pos = positions[vertex_index];

    // Map to source rectangle UV
    let uv = uniforms.source_rect.xy + local_pos * uniforms.source_rect.zw;

    var out: VertexOutput;

    if (uniforms.perspective_d > 0.0) {
        // 3D perspective compositing: distort quad with perspective projection.
        // The layer was rendered flat; we now apply rotate-x/rotate-y to the composite.
        let center = uniforms.dest_rect.xy + uniforms.dest_rect.zw * 0.5;
        let half = uniforms.dest_rect.zw * 0.5;

        // Local position relative to center (in pixels)
        var p = vec3<f32>(
            (local_pos.x - 0.5) * 2.0 * half.x,
            (local_pos.y - 0.5) * 2.0 * half.y,
            0.0
        );

        // Rotate around Y axis
        let ry_x = p.x * uniforms.cos_ry - p.z * uniforms.sin_ry;
        let ry_z = p.x * uniforms.sin_ry + p.z * uniforms.cos_ry;
        p.x = ry_x;
        p.z = ry_z;

        // Rotate around X axis
        let rx_y = p.y * uniforms.cos_rx - p.z * uniforms.sin_rx;
        let rx_z = p.y * uniforms.sin_rx + p.z * uniforms.cos_rx;
        p.y = rx_y;
        p.z = rx_z;

        // Perspective projection
        let d = uniforms.perspective_d;
        let w = (d + p.z) / d;
        let screen = center + p.xy / w;

        // Convert to NDC
        let ndc = (screen / uniforms.viewport_size) * 2.0 - 1.0;

        // Output with perspective w for correct UV interpolation
        out.position = vec4<f32>(ndc.x * w, -ndc.y * w, 0.0, w);
        out.frag_pos = screen;
    } else {
        // Standard flat compositing (no perspective). Apply in-plane
        // Z rotation around the dest rect's center: shift local_pos
        // into a centered (-0.5..0.5) frame, rotate by (sin_rz,
        // cos_rz), then scale by dest_size and shift back to the
        // destination origin. Identity rotation (sin=0, cos=1) leaves
        // local_pos unchanged, so the existing non-rotating paths see
        // no behavioural change.
        let centered = local_pos - vec2<f32>(0.5, 0.5);
        let half_size = uniforms.dest_rect.zw * 0.5;
        let centered_px = centered * uniforms.dest_rect.zw;
        let rotated_px = vec2<f32>(
            centered_px.x * uniforms.cos_rz - centered_px.y * uniforms.sin_rz,
            centered_px.x * uniforms.sin_rz + centered_px.y * uniforms.cos_rz,
        );
        let dest_pos = uniforms.dest_rect.xy + half_size + rotated_px;
        let ndc = (dest_pos / uniforms.viewport_size) * 2.0 - 1.0;
        out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
        out.frag_pos = dest_pos;
    }

    out.uv = uv;
    return out;
}

// ============================================================================
// Blend Mode Functions
// ============================================================================

fn blend_normal(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return src;
}

fn blend_multiply(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return src * dst;
}

fn blend_screen(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return 1.0 - (1.0 - src) * (1.0 - dst);
}

fn blend_overlay_channel(s: f32, d: f32) -> f32 {
    if (d < 0.5) {
        return 2.0 * s * d;
    } else {
        return 1.0 - 2.0 * (1.0 - s) * (1.0 - d);
    }
}

fn blend_overlay(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        blend_overlay_channel(src.r, dst.r),
        blend_overlay_channel(src.g, dst.g),
        blend_overlay_channel(src.b, dst.b)
    );
}

fn blend_darken(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return min(src, dst);
}

fn blend_lighten(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return max(src, dst);
}

fn blend_color_dodge_channel(s: f32, d: f32) -> f32 {
    if (s >= 1.0) {
        return 1.0;
    }
    return min(1.0, d / (1.0 - s));
}

fn blend_color_dodge(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        blend_color_dodge_channel(src.r, dst.r),
        blend_color_dodge_channel(src.g, dst.g),
        blend_color_dodge_channel(src.b, dst.b)
    );
}

fn blend_color_burn_channel(s: f32, d: f32) -> f32 {
    if (s <= 0.0) {
        return 0.0;
    }
    return 1.0 - min(1.0, (1.0 - d) / s);
}

fn blend_color_burn(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        blend_color_burn_channel(src.r, dst.r),
        blend_color_burn_channel(src.g, dst.g),
        blend_color_burn_channel(src.b, dst.b)
    );
}

fn blend_hard_light(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    // Hard light is overlay with src/dst swapped
    return vec3<f32>(
        blend_overlay_channel(dst.r, src.r),
        blend_overlay_channel(dst.g, src.g),
        blend_overlay_channel(dst.b, src.b)
    );
}

fn blend_soft_light_channel(s: f32, d: f32) -> f32 {
    if (s <= 0.5) {
        return d - (1.0 - 2.0 * s) * d * (1.0 - d);
    } else {
        var g: f32;
        if (d <= 0.25) {
            g = ((16.0 * d - 12.0) * d + 4.0) * d;
        } else {
            g = sqrt(d);
        }
        return d + (2.0 * s - 1.0) * (g - d);
    }
}

fn blend_soft_light(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        blend_soft_light_channel(src.r, dst.r),
        blend_soft_light_channel(src.g, dst.g),
        blend_soft_light_channel(src.b, dst.b)
    );
}

fn blend_difference(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return abs(src - dst);
}

fn blend_exclusion(src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    return src + dst - 2.0 * src * dst;
}

// Apply blend mode to colors
fn apply_blend_mode(src: vec3<f32>, dst: vec3<f32>, mode: u32) -> vec3<f32> {
    switch (mode) {
        case BLEND_MULTIPLY: { return blend_multiply(src, dst); }
        case BLEND_SCREEN: { return blend_screen(src, dst); }
        case BLEND_OVERLAY: { return blend_overlay(src, dst); }
        case BLEND_DARKEN: { return blend_darken(src, dst); }
        case BLEND_LIGHTEN: { return blend_lighten(src, dst); }
        case BLEND_COLOR_DODGE: { return blend_color_dodge(src, dst); }
        case BLEND_COLOR_BURN: { return blend_color_burn(src, dst); }
        case BLEND_HARD_LIGHT: { return blend_hard_light(src, dst); }
        case BLEND_SOFT_LIGHT: { return blend_soft_light(src, dst); }
        case BLEND_DIFFERENCE: { return blend_difference(src, dst); }
        case BLEND_EXCLUSION: { return blend_exclusion(src, dst); }
        default: { return blend_normal(src, dst); }  // BLEND_NORMAL
    }
}

// ============================================================================
// Fragment Shader
// ============================================================================

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Apply clip if enabled
    if (uniforms.clip_type == 1u) {
        let dist = sd_rounded_rect_clip(in.frag_pos, uniforms.clip_bounds, uniforms.clip_radius);
        if (dist > 0.5) {
            discard;
        }
    }

    // Sample layer texture (LOD 0 — non-uniform flow from discard above)
    let src = textureSampleLevel(layer_texture, layer_sampler, in.uv, 0.0);

    // Apply opacity
    var src_alpha = src.a * uniforms.opacity;

    // Apply anti-aliased clip edge
    if (uniforms.clip_type == 1u) {
        let dist = sd_rounded_rect_clip(in.frag_pos, uniforms.clip_bounds, uniforms.clip_radius);
        let clip_alpha = 1.0 - smoothstep(-0.5, 0.5, dist);
        src_alpha *= clip_alpha;
    }

    // Early out for fully transparent pixels
    if (src_alpha < 0.001) {
        discard;
    }

    // For non-Normal blend modes, sample the destination texture (pre-copied snapshot)
    // and apply the CSS blend function. The result is output as premultiplied alpha
    // so hardware blending (src + dst * (1-srcA)) produces the correct composite:
    //   final = blended * srcA + dst * (1-srcA) = mix(dst, blended, srcA)
    if (uniforms.blend_mode != BLEND_NORMAL) {
        // Compute screen UV from fragment position
        let screen_uv = in.frag_pos / uniforms.viewport_size;
        let dst = textureSampleLevel(dest_texture, dest_sampler, screen_uv, 0.0);

        // Unpremultiply source (src.a > 0 since src_alpha > 0.001 and opacity <= 1.0)
        let src_c = src.rgb / src.a;
        // Unpremultiply destination
        let dst_c = select(dst.rgb / dst.a, vec3<f32>(0.0, 0.0, 0.0), dst.a < 0.001);

        // Apply CSS blend mode
        let blended = apply_blend_mode(src_c, dst_c, uniforms.blend_mode);

        // Output premultiplied for hardware alpha compositing
        return vec4<f32>(blended * src_alpha, src_alpha);
    }

    // Normal blend: premultiplied alpha for hardware blending
    let premultiplied = vec4<f32>(src.rgb * src_alpha, src_alpha);
    return premultiplied;
}
"#;

/// Kawase blur shader for layer effects
///
/// Implements multi-pass Kawase blur which approximates Gaussian blur
/// with better performance. Each pass samples 5 points in an X pattern.
pub const BLUR_SHADER: &str = r#"
// ============================================================================
// Kawase Blur Shader (Layer Effects)
// ============================================================================
//
// Multi-pass blur using Kawase algorithm for efficient Gaussian approximation.
// Run multiple passes with increasing iteration values for stronger blur.

struct BlurUniforms {
    // Inverse texture size (1/width, 1/height)
    texel_size: vec2<f32>,
    // Base blur radius
    radius: f32,
    // Current iteration (0, 1, 2, ...) - affects sample offset
    iteration: u32,
    // Whether to blur alpha (1) or preserve original alpha (0)
    blur_alpha: u32,
    // Padding for 16-byte alignment
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
}

@group(0) @binding(0) var<uniform> uniforms: BlurUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
@group(0) @binding(2) var input_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@fragment
fn fs_kawase_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    // Kawase blur: each pass samples at increasing offsets scaled by the radius
    // More passes spread the blur further; radius controls per-pass spread
    let base_offset = f32(uniforms.iteration) + 0.5;
    let spread = max(uniforms.radius * 0.4, 1.0);
    let offset = base_offset * spread;
    let pixel_offset = offset * uniforms.texel_size;

    // Sample in + pattern (up, down, left, right) instead of X pattern
    let uv_up = clamp(in.uv + vec2<f32>(0.0, -pixel_offset.y), vec2<f32>(0.0), vec2<f32>(1.0));
    let uv_down = clamp(in.uv + vec2<f32>(0.0, pixel_offset.y), vec2<f32>(0.0), vec2<f32>(1.0));
    let uv_left = clamp(in.uv + vec2<f32>(-pixel_offset.x, 0.0), vec2<f32>(0.0), vec2<f32>(1.0));
    let uv_right = clamp(in.uv + vec2<f32>(pixel_offset.x, 0.0), vec2<f32>(0.0), vec2<f32>(1.0));

    // Sample 5 points in + pattern (center, up, down, left, right)
    let s0 = textureSample(input_texture, input_sampler, in.uv);
    let s1 = textureSample(input_texture, input_sampler, uv_up);
    let s2 = textureSample(input_texture, input_sampler, uv_down);
    let s3 = textureSample(input_texture, input_sampler, uv_left);
    let s4 = textureSample(input_texture, input_sampler, uv_right);

    if (uniforms.blur_alpha == 0u) {
        // CSS filter blur mode: blur all RGBA for visible effect on solid-color elements.
        // Alpha-weighted RGB averaging prevents dark fringing at transparent edges.
        // The alpha-restore pass (mode 2) will fix corner softening after all blur passes.
        let total_alpha = s0.a + s1.a + s2.a + s3.a + s4.a;
        let avg_alpha = total_alpha / 5.0;

        if (avg_alpha < 0.001) {
            return vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }

        let weighted_rgb = s0.rgb * s0.a + s1.rgb * s1.a + s2.rgb * s2.a + s3.rgb * s3.a + s4.rgb * s4.a;
        let avg_rgb = weighted_rgb / total_alpha;

        return vec4<f32>(avg_rgb, avg_alpha);
    } else if (uniforms.blur_alpha == 1u) {
        // Shadow blur mode: only blur alpha for shadow shape
        // Output white RGB since drop shadow shader uses uniform color, not texture RGB
        let total_alpha = s0.a + s1.a + s2.a + s3.a + s4.a;
        let avg_alpha = total_alpha / 5.0;

        return vec4<f32>(1.0, 1.0, 1.0, avg_alpha);
    } else {
        // Mode 2: passthrough — sample center pixel only (used for alpha restore pass)
        return s0;
    }
}

// Single-pass box blur for low quality mode
@fragment
fn fs_box_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = i32(uniforms.radius);
    let center = textureSample(input_texture, input_sampler, in.uv);

    if (uniforms.blur_alpha == 0u) {
        // Element blur mode: preserve alpha, blur RGB with alpha weighting
        var weighted_rgb = vec3<f32>(0.0);
        var total_alpha = 0.0;

        for (var x = -radius; x <= radius; x++) {
            for (var y = -radius; y <= radius; y++) {
                let offset = vec2<f32>(f32(x), f32(y)) * uniforms.texel_size;
                let sample_uv = clamp(in.uv + offset, vec2<f32>(0.0), vec2<f32>(1.0));
                let s = textureSample(input_texture, input_sampler, sample_uv);
                weighted_rgb += s.rgb * s.a;
                total_alpha += s.a;
            }
        }

        if (total_alpha < 0.001) {
            return vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }

        let avg_rgb = weighted_rgb / total_alpha;
        return vec4<f32>(avg_rgb, center.a);
    } else {
        // Shadow blur mode: only blur alpha for shadow shape
        // Output white RGB since drop shadow shader uses uniform color, not texture RGB
        var total_alpha = 0.0;
        var samples = 0.0;

        for (var x = -radius; x <= radius; x++) {
            for (var y = -radius; y <= radius; y++) {
                let offset = vec2<f32>(f32(x), f32(y)) * uniforms.texel_size;
                let sample_uv = clamp(in.uv + offset, vec2<f32>(0.0), vec2<f32>(1.0));
                let s = textureSample(input_texture, input_sampler, sample_uv);
                total_alpha += s.a;
                samples += 1.0;
            }
        }

        let avg_alpha = total_alpha / samples;
        return vec4<f32>(1.0, 1.0, 1.0, avg_alpha);
    }
}
"#;

/// Color matrix shader for layer effects
///
/// Applies a 4x5 color transformation matrix to achieve effects like:
/// grayscale, sepia, brightness, contrast, saturation adjustments.
pub const COLOR_MATRIX_SHADER: &str = r#"
// ============================================================================
// Color Matrix Shader (Layer Effects)
// ============================================================================
//
// Applies a 4x5 color transformation matrix:
// [R']   [m0  m1  m2  m3  m4 ]   [R]
// [G'] = [m5  m6  m7  m8  m9 ] * [G]
// [B']   [m10 m11 m12 m13 m14]   [B]
// [A']   [m15 m16 m17 m18 m19]   [A]
//                                [1]

struct ColorMatrixUniforms {
    // 4x5 matrix stored as 5 vec4s (rows)
    row0: vec4<f32>,  // [m0,  m1,  m2,  m3 ]
    row1: vec4<f32>,  // [m5,  m6,  m7,  m8 ]
    row2: vec4<f32>,  // [m10, m11, m12, m13]
    row3: vec4<f32>,  // [m15, m16, m17, m18]
    // Offset column (m4, m9, m14, m19)
    offset: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: ColorMatrixUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
@group(0) @binding(2) var input_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@fragment
fn fs_color_matrix(in: VertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(input_texture, input_sampler, in.uv);

    // Apply 4x4 matrix multiplication + offset
    var result: vec4<f32>;
    result.r = dot(uniforms.row0, src) + uniforms.offset.r;
    result.g = dot(uniforms.row1, src) + uniforms.offset.g;
    result.b = dot(uniforms.row2, src) + uniforms.offset.b;
    result.a = dot(uniforms.row3, src) + uniforms.offset.a;

    // Clamp to valid range
    return clamp(result, vec4<f32>(0.0), vec4<f32>(1.0));
}
"#;

/// Mask image shader for CSS mask-image support
///
/// Multiplies the layer's alpha by the mask image value.
/// Supports alpha mode (use mask alpha) and luminance mode (use mask luminance as alpha).
pub const MASK_IMAGE_SHADER: &str = r#"
// ============================================================================
// Mask Image Shader (Layer Effects)
// ============================================================================
//
// Applies a mask image to a layer: output.a = input.a * mask_value
// mask_mode: 0 = alpha (use mask.a), 1 = luminance (use dot(mask.rgb, luma))

struct MaskUniforms {
    // 0 = alpha, 1 = luminance
    mask_mode: u32,
    _pad: vec3<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: MaskUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
@group(0) @binding(2) var input_sampler: sampler;
@group(0) @binding(3) var mask_texture: texture_2d<f32>;
@group(0) @binding(4) var mask_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@fragment
fn fs_mask(in: VertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(input_texture, input_sampler, in.uv);
    let mask = textureSample(mask_texture, mask_sampler, in.uv);

    // Compute mask value based on mode
    var mask_alpha: f32;
    if (uniforms.mask_mode == 1u) {
        // Luminance mode: use weighted RGB as alpha
        mask_alpha = dot(mask.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)) * mask.a;
    } else {
        // Alpha mode: use mask alpha channel directly
        mask_alpha = mask.a;
    }

    // Multiply source by mask value (premultiplied alpha)
    return vec4<f32>(src.rgb * mask_alpha, src.a * mask_alpha);
}
"#;

/// Shadow colorize shader for layer effects
///
/// Takes a pre-blurred texture and colorizes its alpha channel to create shadow.
/// This is used after Kawase blur for smooth shadows at any radius.
pub const DROP_SHADOW_SHADER: &str = r#"
// ============================================================================
// Shadow Colorize Shader (Layer Effects)
// ============================================================================
//
// Takes a pre-blurred texture and:
// 1. Samples the blurred alpha at offset position for shadow shape
// 2. Colorizes with shadow color
// 3. Composites shadow behind original content
//
// This shader expects the input to already be blurred using Kawase blur passes.

struct DropShadowUniforms {
    // Shadow offset in pixels
    offset: vec2<f32>,
    // Blur radius (stored but not used - blur is pre-applied)
    blur_radius: f32,
    // Spread (expand/contract)
    spread: f32,
    // Shadow color (RGBA)
    color: vec4<f32>,
    // Texture size for offset calculation
    texel_size: vec2<f32>,
    // Padding
    _pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: DropShadowUniforms;
@group(0) @binding(1) var input_texture: texture_2d<f32>;
@group(0) @binding(2) var input_sampler: sampler;
// Original (unblurred) texture for compositing
@group(0) @binding(3) var original_texture: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

// Calculate minimum distance to an opaque pixel by sampling in a pattern
// This preserves the shape (including rounded corners) unlike blur-based approaches
fn sample_min_distance(uv: vec2<f32>, radius: f32, texel_size: vec2<f32>) -> f32 {
    // Check center first - if opaque, distance is 0
    let center = textureSampleLevel(original_texture, input_sampler, uv, 0.0);
    if (center.a > 0.5) {
        return 0.0;
    }

    // Sample in concentric rings to find nearest opaque pixel
    // Start with small radius and expand - this gives good quality with fewer samples
    var min_dist = radius + 1.0;

    // Sample at multiple distances and angles
    // Balanced for performance (8x8 = 64 samples max, with early exit)
    let num_angles = 8;
    let num_rings = 8;

    for (var ring = 1; ring <= num_rings; ring++) {
        let dist = (f32(ring) / f32(num_rings)) * radius;
        let pixel_dist = dist;

        for (var i = 0; i < num_angles; i++) {
            let angle = f32(i) * 6.28318530718 / f32(num_angles);
            let offset = vec2<f32>(cos(angle), sin(angle)) * pixel_dist * texel_size;
            let sample_uv = clamp(uv + offset, vec2<f32>(0.0), vec2<f32>(1.0));
            let s = textureSampleLevel(original_texture, input_sampler, sample_uv, 0.0);

            if (s.a > 0.5) {
                min_dist = min(min_dist, dist);
            }
        }

        // Early exit if we found an opaque pixel in this ring
        if (min_dist <= dist) {
            break;
        }
    }

    return min_dist;
}

@fragment
fn fs_drop_shadow(in: VertexOutput) -> @location(0) vec4<f32> {
    // Calculate shadow UV with offset
    let shadow_uv = clamp(in.uv - uniforms.offset * uniforms.texel_size, vec2<f32>(0.0), vec2<f32>(1.0));

    // Find minimum distance to the original shape
    let dist = sample_min_distance(shadow_uv, uniforms.blur_radius, uniforms.texel_size);

    // Convert distance to alpha using smooth falloff
    // At distance 0, alpha = 1. At distance = blur_radius, alpha ≈ 0
    var alpha = 1.0 - smoothstep(0.0, uniforms.blur_radius, dist);

    // Apply spread (expand/contract the shape)
    if (uniforms.spread != 0.0) {
        // Positive spread = larger shadow, negative = smaller
        let adjusted_dist = dist - uniforms.spread;
        alpha = 1.0 - smoothstep(0.0, uniforms.blur_radius, max(adjusted_dist, 0.0));
    }

    // Shadow color with computed alpha
    let shadow_a = uniforms.color.a * alpha;
    let shadow_rgb = uniforms.color.rgb;

    // Sample original (unblurred) content at current position
    let original = textureSampleLevel(original_texture, input_sampler, in.uv, 0.0);

    // Composite shadow behind original using porter-duff "over" for non-premultiplied colors
    let result_a = original.a + shadow_a * (1.0 - original.a);

    if (result_a < 0.001) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let result_rgb = (original.rgb * original.a + shadow_rgb * shadow_a * (1.0 - original.a)) / result_a;

    return vec4<f32>(result_rgb, result_a);
}
"#;

/// Glow effect shader for layer effects
///
/// Creates a radial glow around the shape by:
/// 1. Finding distance to nearest opaque pixel
/// 2. Applying smooth radial falloff based on blur + range
/// 3. Compositing glow behind original content
pub const GLOW_SHADER: &str = r#"
// ============================================================================
// Glow Effect Shader (Layer Effects)
// ============================================================================
//
// Creates an outer glow around shapes by:
// 1. Sampling to find distance to nearest opaque pixel
// 2. Applying Gaussian-like falloff from the shape edge
// 3. Compositing glow behind the original content

struct GlowUniforms {
    // Glow color (RGBA)
    color: vec4<f32>,
    // Blur softness (affects falloff smoothness)
    blur: f32,
    // Glow range (how far the glow extends)
    range: f32,
    // Glow opacity (0-1)
    opacity: f32,
    // Padding for alignment
    _pad0: f32,
    // Texture size for distance calculation
    texel_size: vec2<f32>,
    // Padding
    _pad1: vec2<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: GlowUniforms;
@group(0) @binding(1) var source_texture: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

// Find minimum distance to an opaque pixel within search_radius
fn find_edge_distance(uv: vec2<f32>, search_radius: f32, texel_size: vec2<f32>) -> f32 {
    // Check center first - if opaque, distance is 0
    let center = textureSampleLevel(source_texture, source_sampler, uv, 0.0);
    if (center.a > 0.5) {
        return 0.0;
    }

    // Sample in concentric rings to find nearest opaque pixel
    var min_dist = search_radius + 1.0;  // Start with "not found" value

    // Sample at multiple distances and angles
    // Balanced for performance (8x8 = 64 samples max, with early exit)
    let num_angles = 8;
    let num_rings = 8;

    for (var ring = 1; ring <= num_rings; ring++) {
        let dist = (f32(ring) / f32(num_rings)) * search_radius;
        let pixel_dist = dist;

        for (var i = 0; i < num_angles; i++) {
            let angle = f32(i) * 6.28318530718 / f32(num_angles);
            let offset = vec2<f32>(cos(angle), sin(angle)) * pixel_dist * texel_size;
            let sample_uv = clamp(uv + offset, vec2<f32>(0.0), vec2<f32>(1.0));
            let s = textureSampleLevel(source_texture, source_sampler, sample_uv, 0.0);

            if (s.a > 0.5) {
                min_dist = min(min_dist, dist);
            }
        }

        // Early exit if we found an opaque pixel in this ring
        if (min_dist <= dist) {
            break;
        }
    }

    return min_dist;
}

@fragment
fn fs_glow(in: VertexOutput) -> @location(0) vec4<f32> {
    // Total search distance = blur + range
    let search_radius = uniforms.blur + uniforms.range;

    // Find distance to nearest opaque pixel
    let dist = find_edge_distance(in.uv, search_radius, uniforms.texel_size);

    // Calculate glow alpha with Gaussian-like falloff
    // - At distance 0: we're inside the shape, no glow needed (original shows)
    // - At distance <= range: full glow intensity
    // - At distance > range: fade out over 'blur' distance
    var glow_alpha = 0.0;

    if (dist > 0.0 && dist <= search_radius) {
        // Distance from the extended glow edge
        // If dist <= range, we're in the "full glow" zone
        // If dist > range, we're in the "fade" zone
        if (dist <= uniforms.range) {
            // Inside the glow range - full intensity
            glow_alpha = 1.0;
        } else {
            // Fade zone: distance beyond range, fading over 'blur' distance
            let fade_dist = dist - uniforms.range;
            // Smooth Gaussian-like falloff
            let sigma = uniforms.blur * 0.5;
            glow_alpha = exp(-(fade_dist * fade_dist) / (2.0 * sigma * sigma));
        }
    }

    // Apply opacity
    glow_alpha *= uniforms.opacity * uniforms.color.a;

    // Sample original content
    let original = textureSampleLevel(source_texture, source_sampler, in.uv, 0.0);

    // Glow color (premultiplied)
    let glow_rgb = uniforms.color.rgb;

    // Composite glow behind original using porter-duff "over"
    // Result = Original + Glow * (1 - Original.a)
    let result_a = original.a + glow_alpha * (1.0 - original.a);

    if (result_a < 0.001) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let result_rgb = (original.rgb * original.a + glow_rgb * glow_alpha * (1.0 - original.a)) / result_a;

    return vec4<f32>(result_rgb, result_a);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_wgsl(source: &str) -> Result<(), String> {
        naga::front::wgsl::parse_str(source).map_err(|e| e.emit_to_string(source))?;
        Ok(())
    }

    fn run_tool(program: &str, args: &[&std::ffi::OsStr]) -> std::process::Output {
        std::process::Command::new(program)
            .args(args)
            .output()
            .unwrap_or_else(|err| {
                panic!(
                    "failed to run `{program}`. Install `naga` and `dxc` to run this check: {err}"
                )
            })
    }

    fn assert_tool_success(program: &str, args: &[&std::ffi::OsStr]) {
        let output = run_tool(program, args);
        if !output.status.success() {
            panic!(
                "`{program}` failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
    }

    fn assert_wgsl_compiles_to_dxil(name: &str, source: &str) {
        let root =
            std::env::temp_dir().join(format!("blinc-hlsl-dxc-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temporary shader check directory");

        let wgsl_path = root.join(format!("{name}.wgsl"));
        let hlsl_path = root.join(format!("{name}.hlsl"));
        let vs_path = root.join(format!("{name}.vs.dxil"));
        let ps_path = root.join(format!("{name}.ps.dxil"));

        std::fs::write(&wgsl_path, source).expect("write WGSL source for HLSL check");

        assert_tool_success(
            "naga",
            &[
                std::ffi::OsStr::new("--input-kind"),
                std::ffi::OsStr::new("wgsl"),
                std::ffi::OsStr::new("--shader-model"),
                std::ffi::OsStr::new("60"),
                wgsl_path.as_os_str(),
                hlsl_path.as_os_str(),
            ],
        );

        assert_tool_success(
            "dxc",
            &[
                std::ffi::OsStr::new("-T"),
                std::ffi::OsStr::new("vs_6_0"),
                std::ffi::OsStr::new("-E"),
                std::ffi::OsStr::new("vs_main"),
                std::ffi::OsStr::new("-Fo"),
                vs_path.as_os_str(),
                hlsl_path.as_os_str(),
            ],
        );

        assert_tool_success(
            "dxc",
            &[
                std::ffi::OsStr::new("-T"),
                std::ffi::OsStr::new("ps_6_0"),
                std::ffi::OsStr::new("-E"),
                std::ffi::OsStr::new("fs_main"),
                std::ffi::OsStr::new("-Fo"),
                ps_path.as_os_str(),
                hlsl_path.as_os_str(),
            ],
        );
    }

    /// Runtime shader compilation is the only thing that parses these WGSL
    /// strings — if a case label or a const declaration is malformed, the
    /// error only surfaces when the app actually tries to build a pipeline
    /// on device. These tests invoke naga directly so the workspace test
    /// suite catches syntax regressions (e.g. a stray `// comment {`
    /// swallowing a case-body brace) before they ship.
    #[test]
    fn sdf_shader_parses() {
        parse_wgsl(SDF_SHADER).expect("SDF_SHADER");
    }

    #[test]
    fn path_shader_parses() {
        parse_wgsl(PATH_SHADER).expect("PATH_SHADER");
    }

    #[test]
    fn text_shader_parses() {
        parse_wgsl(TEXT_SHADER).expect("TEXT_SHADER");
    }

    #[test]
    fn glass_shader_parses() {
        parse_wgsl(GLASS_SHADER).expect("GLASS_SHADER");
    }

    #[test]
    fn simple_glass_shader_parses() {
        parse_wgsl(SIMPLE_GLASS_SHADER).expect("SIMPLE_GLASS_SHADER");
    }

    #[test]
    fn composite_shader_parses() {
        parse_wgsl(COMPOSITE_SHADER).expect("COMPOSITE_SHADER");
    }

    #[test]
    fn layer_composite_shader_parses() {
        parse_wgsl(LAYER_COMPOSITE_SHADER).expect("LAYER_COMPOSITE_SHADER");
    }

    #[test]
    fn blur_shader_parses() {
        parse_wgsl(BLUR_SHADER).expect("BLUR_SHADER");
    }

    #[test]
    fn drop_shadow_shader_parses() {
        parse_wgsl(DROP_SHADOW_SHADER).expect("DROP_SHADOW_SHADER");
    }

    #[test]
    fn glow_shader_parses() {
        parse_wgsl(GLOW_SHADER).expect("GLOW_SHADER");
    }

    #[test]
    fn color_matrix_shader_parses() {
        parse_wgsl(COLOR_MATRIX_SHADER).expect("COLOR_MATRIX_SHADER");
    }

    #[test]
    fn mask_image_shader_parses() {
        parse_wgsl(MASK_IMAGE_SHADER).expect("MASK_IMAGE_SHADER");
    }

    #[test]
    fn sdf_core_shader_parses() {
        parse_wgsl(SDF_CORE_SHADER).expect("SDF_CORE_SHADER");
    }

    #[test]
    fn sdf_shadow_shader_parses() {
        parse_wgsl(SDF_SHADOW_SHADER).expect("SDF_SHADOW_SHADER");
    }

    #[test]
    fn sdf_3d_shader_parses() {
        parse_wgsl(SDF_3D_SHADER).expect("SDF_3D_SHADER");
    }

    #[test]
    fn sdf_notch_shader_parses() {
        parse_wgsl(SDF_NOTCH_SHADER).expect("SDF_NOTCH_SHADER");
    }

    #[test]
    fn sdf_core_vb_shader_parses() {
        parse_wgsl(SDF_CORE_VB_SHADER).expect("SDF_CORE_VB_SHADER");
    }

    #[test]
    fn sdf_shadow_vb_shader_parses() {
        parse_wgsl(SDF_SHADOW_VB_SHADER).expect("SDF_SHADOW_VB_SHADER");
    }

    #[test]
    fn sdf_3d_vb_shader_parses() {
        parse_wgsl(SDF_3D_VB_SHADER).expect("SDF_3D_VB_SHADER");
    }

    #[test]
    fn sdf_notch_vb_shader_parses() {
        parse_wgsl(SDF_NOTCH_VB_SHADER).expect("SDF_NOTCH_VB_SHADER");
    }

    #[test]
    fn sdf_core_dt_shader_parses() {
        parse_wgsl(SDF_CORE_DT_SHADER).expect("SDF_CORE_DT_SHADER");
    }

    #[test]
    fn sdf_shadow_dt_shader_parses() {
        parse_wgsl(SDF_SHADOW_DT_SHADER).expect("SDF_SHADOW_DT_SHADER");
    }

    /// Host-side guard for Windows issue #53.
    ///
    /// CI runs this on a Windows runner with the SDK `dxc.exe`; local runs can
    /// use any `naga` + `dxc` install on PATH. It catches generated-HLSL
    /// syntax/control-flow regressions in the shadow pipelines before they
    /// reach D3D pipeline creation.
    #[test]
    #[ignore = "requires `naga` and `dxc` CLIs"]
    fn sdf_shadow_hlsl_compiles_with_dxc() {
        assert_wgsl_compiles_to_dxil("sdf_shadow", SDF_SHADOW_SHADER);
        assert_wgsl_compiles_to_dxil("sdf_shadow_vb", SDF_SHADOW_VB_SHADER);
        assert_wgsl_compiles_to_dxil("sdf_shadow_dt", SDF_SHADOW_DT_SHADER);
    }

    #[test]
    fn sdf_3d_dt_shader_parses() {
        parse_wgsl(SDF_3D_DT_SHADER).expect("SDF_3D_DT_SHADER");
    }

    #[test]
    fn sdf_notch_dt_shader_parses() {
        parse_wgsl(SDF_NOTCH_DT_SHADER).expect("SDF_NOTCH_DT_SHADER");
    }

    #[test]
    fn text_dt_shader_parses() {
        parse_wgsl(TEXT_DT_SHADER).expect("TEXT_DT_SHADER");
    }

    #[test]
    fn glass_dt_shader_parses() {
        parse_wgsl(GLASS_DT_SHADER).expect("GLASS_DT_SHADER");
    }

    #[test]
    fn simple_glass_dt_shader_parses() {
        parse_wgsl(SIMPLE_GLASS_DT_SHADER).expect("SIMPLE_GLASS_DT_SHADER");
    }

    #[test]
    fn mesh_dt_shader_parses() {
        parse_wgsl(MESH_DT_SHADER).expect("MESH_DT_SHADER");
    }
}
