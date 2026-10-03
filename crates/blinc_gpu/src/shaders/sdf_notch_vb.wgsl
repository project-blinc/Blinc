// ============================================================================
// Vertex Buffer fallback variant — vs_main reads per-instance fields from
// an instance-stepped vertex buffer instead of indexing into the storage
// buffer. Used when the GPU adapter lacks VERTEX_STORAGE support.
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

    let la = vb_local_affine; // [a, b, c, d] of normalized 2x2 affine
    // Check if local_affine is non-identity (rotation, skew, or non-uniform scale)
    let has_local_affine = abs(la.x - 1.0) > 0.0001 || abs(la.y) > 0.0001
                        || abs(la.z) > 0.0001 || abs(la.w - 1.0) > 0.0001;

    var bounds: vec4<f32>;
    if has_local_affine {
        // General 2D affine (rotation, skew, non-uniform scale):
        // Transform the 4 corners of the local rect by the local_affine to find AABB
        let center = vb_bounds.xy + vb_bounds.zw * 0.5;
        let hw = vb_bounds.z * 0.5;
        let hh = vb_bounds.w * 0.5;
        // Transform corners: la * (+-hw, +-hh)
        // new_x = la.x * cx + la.z * cy, new_y = la.y * cx + la.w * cy
        let c0x = la.x * hw + la.z * hh;
        let c0y = la.y * hw + la.w * hh;
        let c1x = -la.x * hw + la.z * hh;
        let c1y = -la.y * hw + la.w * hh;
        let aabb_hw = max(abs(c0x), abs(c1x)) + blur_expand;
        let aabb_hh = max(abs(c0y), abs(c1y)) + blur_expand;
        bounds = vec4<f32>(center.x - aabb_hw, center.y - aabb_hh, aabb_hw * 2.0, aabb_hh * 2.0);
    } else {
        // Notches can reach outside `vb_bounds` via concave corners,
        // top/bottom bulge, or top/bottom peak modifiers. The SDF in fs_main
        // returns negative for those exterior pixels, but the fragment shader
        // only runs on pixels covered by this vertex quad -- so if we don't
        // expand the quad, the protrusion is invisible (every pixel outside
        // `vb_bounds` is never rasterized in the first place). Compute the
        // per-edge outward expansion from the notch parameters and fold it
        // into the quad size here. Scoop/cut modifiers go INWARD so they
        // don't need any expansion.
        var notch_left = 0.0;
        var notch_top = 0.0;
        var notch_right = 0.0;
        var notch_bottom = 0.0;

        let ct = vb_light; // (TL, TR, BR, BL) corner type flags
        let cr = vb_corner_radius;
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
        let top_type = vb_perspective.x;
        let top_height = vb_perspective.z;
        if (top_type > 1.5 && top_type < 2.5) || (top_type > 3.5 && top_type < 4.5) {
            notch_top = max(notch_top, top_height);
        }
        let bot_type = vb_sdf_3d.x;
        let bot_height = vb_sdf_3d.z;
        if (bot_type > 1.5 && bot_type < 2.5) || (bot_type > 3.5 && bot_type < 4.5) {
            notch_bottom = max(notch_bottom, bot_height);
        }

        bounds = vec4<f32>(
            vb_bounds.x - blur_expand - notch_left,
            vb_bounds.y - blur_expand - notch_top,
            vb_bounds.z + blur_expand * 2.0 + notch_left + notch_right,
            vb_bounds.w + blur_expand * 2.0 + notch_top + notch_bottom
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

// Corner type codes (stored as f32 in `prim.light` for PRIM_NOTCH).
//   0.0 = sharp or convex (distinguished by corner_radius magnitude)
//   1.0 = concave
//
// Modifier type codes (stored as f32 in `prim.perspective.x` / `prim.sdf_3d.x`).
//   0.0 = none   1.0 = scoop   2.0 = bulge   3.0 = cut   4.0 = peak
//
// All notch geometry is composed via SDF union (`min`) and subtraction
// (`max(d, -d_sub)`); no CPU tessellation is involved at any point.

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
