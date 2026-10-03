// ============================================================================
// Blinc SDF Core Shader — Rect, Circle, Ellipse
// ============================================================================
// Handles prim_type 0 (Rect), 1 (Circle), 2 (Ellipse) with full
// anti-aliasing, borders, shadows-behind-shape, gradients, mask
// gradients, CSS filters, clip paths, and CSS perspective.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) instance_index: u32,
    // Per-vertex barycentric weights; PRIM_MESH vertex stage emits
    // (1,0,0), (0,1,0), (0,0,1) for triangle corners 0/1/2 so the
    // hardware-interpolated value at any fragment tells us how far
    // that fragment is from each of the triangle's three edges. The
    // fragment shader then feathers alpha at silhouette edges only.
    @location(2) mesh_bary: vec3<f32>,
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
) -> VertexOutput {
    var out: VertexOutput;

    let prim = primitives[instance_index];

    // Expand bounds for shadow blur
    let blur_expand = prim.shadow.z * 3.0 + abs(prim.shadow.x) + abs(prim.shadow.y);

    // Check for rotation, skew, and 3D transforms.
    let sin_rz = prim.rotation.x;
    let cos_rz = prim.rotation.y;
    let sin_ry = prim.rotation.z;
    let cos_ry = prim.rotation.w;
    let sin_rx = prim.perspective.x;
    let cos_rx = prim.perspective.y;
    let persp_d = prim.perspective.z;
    let la = prim.local_affine; // [a, b, c, d] of normalized 2x2 affine
    let has_3d = abs(sin_ry) > 0.0001 || abs(sin_rx) > 0.0001 || persp_d > 0.001;
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
        bounds = vec4<f32>(
            prim.bounds.x - blur_expand,
            prim.bounds.y - blur_expand,
            prim.bounds.z + blur_expand * 2.0,
            prim.bounds.w + blur_expand * 2.0
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
    var pos = vec2<f32>(
        bounds.x + uv.x * bounds.z,
        bounds.y + uv.y * bounds.w
    );

    // PRIM_MESH replaces the quad geometry with the triangle's
    // three corners pulled from aux_data. Vertex indices 0..2
    // emit the real triangle; 3..5 collapse onto vertex 0 so
    // the second triangle of the underlying 6-vertex draw
    // degenerates to zero area and the rasteriser skips it.
    // Hardware rasterises the real triangle edge-to-edge, which
    // is the point of the SDF-stream rework: the fragment shader
    // no longer does a per-pixel triangle walk.
    //
    // `mesh_bary` emits (1,0,0), (0,1,0), (0,0,1) at the three
    // corners; hardware interpolates linearly across the
    // triangle so the fragment's `mesh_bary.x` == 0 exactly on
    // edge v1→v2, `mesh_bary.y` == 0 on edge v2→v0, and
    // `mesh_bary.z` == 0 on edge v0→v1. Paired with the per-edge
    // silhouette flags in `prim.corner_radius`, the fragment
    // shader can feather alpha at outer edges only and leave
    // interior tessellation seams fully opaque.
    var mesh_bary = vec3<f32>(0.0);
    if prim.type_info.x == 9u {
        let aux_off = u32(prim.border.z);
        let pack0 = aux_data[aux_off];
        let pack1 = aux_data[aux_off + 1u];
        switch vertex_index {
            case 0u: { pos = pack0.xy; mesh_bary = vec3<f32>(1.0, 0.0, 0.0); }
            case 1u: { pos = pack0.zw; mesh_bary = vec3<f32>(0.0, 1.0, 0.0); }
            case 2u: { pos = pack1.xy; mesh_bary = vec3<f32>(0.0, 0.0, 1.0); }
            default: { pos = pack0.xy; mesh_bary = vec3<f32>(1.0, 0.0, 0.0); }
        }
    }
    out.mesh_bary = mesh_bary;

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
                // Scissor-only for early discard. The polygon shape test
                // is deferred to fs_main after `sp` is computed, so it
                // can run against element-local coords (`sp - prim.bounds.xy`)
                // and rotate with motion bindings / CSS transforms.
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

    // Mesh-triangle barycentric derivatives — only used by `prim_type
    // == 9u` below, but `fwidth` must be evaluated in uniform control
    // flow. Hoist it here, alongside `d_fw_screen`, before any
    // `discard` or early `return` that depends on the non-uniform
    // `prim_type` / `clip_alpha`. Dawn (Chrome's WebGPU validator)
    // rejects the previous in-place `fwidth(mb)` call after the
    // `prim_type` switch returns from `case 7u`. See
    // https://www.w3.org/TR/WGSL/#uniformity.
    let mb_uniform = in.mesh_bary;
    let mb_fw_uniform = fwidth(mb_uniform);

    let prim_type = prim.type_info.x;
    let fill_type = prim.type_info.y;
    let clip_type = prim.type_info.z;

    // Early type filter — discard primitives handled by other split pipelines.
    // Allow prim_type 7 (text) — transformed text glyphs are rendered through
    // the SDF pipeline with css_affine, not the separate text pipeline.
    // Allow prim_type 9 (mesh triangle) — tessellated path fills routed
    // through the SDF stream so their submission order interleaves with
    // text in the same draw dispatch.
    if prim_type > 2u && prim_type != 7u && prim_type != 9u { discard; }

    // Early clip test - discard if completely outside clip region (screen space).
    // For CLIP_POLYGON the scissor-AABB portion runs here; the polygon's
    // winding-number shape test is deferred to after `sp` is computed
    // below so the polygon vertices (stored in element-local coords) can
    // be tested in the same frame the SDF evaluates — which means clips
    // rotate with motion-binding / CSS rotation automatically.
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

    // CLIP_POLYGON shape test in element-local coords. Polygon vertices
    // are stored in aux_data as supplied by the user (0..size range); the
    // walker no longer pre-transforms them to screen space. Using
    // `sp - prim.bounds.xy` puts the sample point in the same frame, so
    // any rotation (CSS, motion-binding timeline, motion-binding rotation
    // updated via apply_binding_deltas) naturally rotates the polygon.
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
            // Hardware already clipped this fragment to the
            // triangle; `d` just needs to read as "inside" for the
            // downstream smoothstep. Silhouette AA is folded in
            // below, after `fill_alpha` is computed — see the
            // comment on `fill_alpha` and the `mesh_bary`
            // smoothstep block.
            d = -1.0;
        }
        case 7u /* PRIM_TEXT */: {
            // `gradient_params` holds atlas PIXEL coords for text
            // glyphs (px_min, py_min, px_max, py_max), not pre-
            // divided UVs. Compute UVs against the CURRENT atlas
            // dimensions via `textureDimensions()` so atlas growth
            // (which doubles dims to fit new glyphs) automatically
            // produces correct UVs without needing a CPU-side
            // primitive rewrite. Pre-fix the CPU baked UVs at
            // prepare_text time; any later atlas growth made those
            // UVs reference the wrong sub-rect of the resized
            // texture → garbled glyphs after the first zoom.
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
            // degenerate (zero-width or zero-height) rectangle that
            // the layout pipeline can produce mid-frame (e.g. a
            // scroll container whose inner clip hasn't resolved a
            // dimension yet). `calculate_clip_alpha` above already
            // returned 1.0 in that case, so this extra edge fade
            // being 1.0 too matches expectation.
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
    var fill_alpha = 1.0 - smoothstep(-aa_width, aa_width, d);

    // Mesh primitive silhouette AA. `mesh_bary` is (1,0,0) at v0,
    // (0,1,0) at v1, (0,0,1) at v2 — so the component opposite
    // each edge drops to 0 on that edge. `prim.corner_radius`
    // carries the per-edge silhouette flag (1.0 = silhouette edge,
    // 0.0 = interior tessellation seam) in the order
    //   [0] = edge v0→v1  (bary.z drops to 0 here)
    //   [1] = edge v1→v2  (bary.x drops)
    //   [2] = edge v2→v0  (bary.y drops)
    //
    // Convert each barycentric to pixel-distance-to-edge via
    // `bary / fwidth(bary)` — this is the signed distance from the
    // fragment centre to the edge measured in screen-space pixels
    // (positive inside, 0 on the edge). We then use
    // `saturate(d_px + 0.5)` — a linear ramp centred on the edge
    // such that a pixel whose centre sits exactly on the edge gets
    // 50 % coverage, a pixel half a pixel inside is fully opaque,
    // and anything further inside stays at 1.0. This is closer to
    // true geometric coverage than `smoothstep(0, fw, bary)`, which
    // fades INWARD from the edge (shrinking triangles by half a
    // pixel) and gives dithered-looking edges because the alpha at
    // boundary pixels depends on exactly how close each pixel's
    // centre is to the edge.
    //
    // Interior (non-silhouette) edges have flag = 0, so `mix` keeps
    // the coverage at 1.0 — tessellation seams stay fully opaque.
    // Combining via `min()` gives distance-to-nearest-silhouette-edge
    // semantics, avoiding the double-darkening that `a * b * c` causes
    // at vertices where two silhouette edges meet.
    //
    // `fwidth` runs in uniform control flow here (computed
    // unconditionally before any prim_type branch) to satisfy WGSL's
    // derivative-uniformity rule.
    // `uniforms._padding.x` is the MSAA-active flag: 1.0 when the
    // render pass is writing into a multisampled target, 0.0 on
    // single-sampled paths. When hardware MSAA is doing the silhouette
    // coverage, stacking the shader's per-edge barycentric fade on
    // top under-fills partially-covered pixels — each rendered sample
    // carries a faded-down value before the hardware resolver
    // averages them, so a 2/4-covered pixel that should resolve to
    // 0.5 alpha comes out closer to 0.05. Skip the fade in that case;
    // keep it on single-sampled paths where it's the only AA we have.
    //
    // `fwidth` still runs in uniform control flow so the derivative
    // queries stay valid regardless of which branch the fragment
    // actually takes.
    let mb = mb_uniform;
    let mb_fw = mb_fw_uniform;
    let mesh_msaa_active = uniforms._padding.x > 0.5;
    if prim.type_info.x == 9u {
        if mesh_msaa_active {
            fill_alpha = 1.0;
        } else {
            let flags = prim.corner_radius.xyz;
            let d_x = mb.x / max(mb_fw.x, 1e-5);
            let d_y = mb.y / max(mb_fw.y, 1e-5);
            let d_z = mb.z / max(mb_fw.z, 1e-5);
            let a_x = saturate(d_x + 0.5);
            let a_y = saturate(d_y + 0.5);
            let a_z = saturate(d_z + 0.5);
            let m_x = mix(1.0, a_x, flags.y);
            let m_y = mix(1.0, a_y, flags.z);
            let m_z = mix(1.0, a_z, flags.x);
            fill_alpha = min(m_x, min(m_y, m_z));
        }
    }

    if fill_alpha < 0.001 {
        return result;
    }

    // Determine fill color
    var fill_color: vec4<f32>;
    // PRIM_MESH with `type_info.w == 1u` carries per-vertex colours
    // packed into `aux_data[offset+2..+5]` right after the triangle
    // positions. Barycentric interpolation reproduces authored
    // multi-stop gradients across the triangle without needing a
    // gradient atlas — fidelity tracks tessellation density. Honour
    // this before the fill_type switch so the 2-stop `mix` path
    // stays the precise per-fragment default.
    if prim.type_info.x == 9u && prim.type_info.w == 1u {
        let aux_off = u32(prim.border.z);
        let c0 = aux_data[aux_off + 2u];
        let c1 = aux_data[aux_off + 3u];
        let c2 = aux_data[aux_off + 4u];
        let mb = in.mesh_bary;
        fill_color = c0 * mb.x + c1 * mb.y + c2 * mb.z;
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

    // Check if any border is present (using max of all sides).
    // PRIM_MESH reuses `border[1]` for triangle count and `border[2]`
    // for the aux_data offset — non-zero values that would otherwise
    // trigger the border stroke below and paint `border_color`
    // (default opaque black) over the whole fill. Mesh primitives
    // have no border to render, so short-circuit past this block for
    // them.
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

            // Composite the border ON TOP of the fill using src-over alpha
            // compositing, NOT a four-component mix. The previous mix() pulled
            // the border colour's alpha into fill_color.a — so when the border
            // is semi-transparent (e.g. ColorToken::Border at 10 % alpha) over
            // an opaque body the border ring became ~90 % transparent and the
            // page bg leaked through as a dark "extra outline stroke" tracing
            // the rounded corner.
            //
            // Standard "over" in straight-alpha form:
            //   out_a   = src_a + dst_a * (1 - src_a)
            //   out_rgb = (src_rgb * src_a + dst_rgb * dst_a * (1 - src_a)) / out_a
            // Both branches matter:
            //   * opaque fill + semi-transparent border  → out_a stays 1.0,
            //     out_rgb = border * border_a + fill * (1 - border_a).
            //     Body stays opaque, corner ring tinted by border colour.
            //   * transparent fill (e.g. outline button) + semi-transparent
            //     border → out_a = border_a, out_rgb = border_rgb. The border
            //     renders at its own colour and alpha, exactly as before the
            //     bug fix. The premultiplied form without the /out_a division
            //     would have squared the alpha here (10 % × 10 % = 1 % effective)
            //     and made the outline button's border invisible.
            let border_t = border_blend * step(0.001, fill_alpha);
            let border_a = prim.border_color.a * border_t;
            let out_a = border_a + fill_color.a * (1.0 - border_a);
            let premult_rgb = prim.border_color.rgb * border_a
                + fill_color.rgb * fill_color.a * (1.0 - border_a);
            let new_rgb = select(
                premult_rgb / max(out_a, 0.0001),
                vec3<f32>(0.0),
                out_a < 0.0001
            );
            fill_color = vec4<f32>(new_rgb, out_a);
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
