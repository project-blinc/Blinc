// ============================================================================
// Blinc SDF 3D Primitive Shader
//
// Handles primitives that use 3D SDF raymarching — when perspective.w
// (shape_type) > 0. This includes individual 3D shapes (shape_type 1-5:
// box, sphere, cylinder, torus, capsule) and 3D groups (shape_type 6).
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

    // Extract 3D rotation/perspective parameters
    let sin_rz = prim.rotation.x;
    let cos_rz = prim.rotation.y;
    let sin_ry = prim.rotation.z;
    let cos_ry = prim.rotation.w;
    let sin_rx = prim.perspective.x;
    let cos_rx = prim.perspective.y;
    let persp_d = prim.perspective.z;

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
    let bounds = vec4<f32>(ctr + min_p, max_p - min_p);

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
// SDF Functions (clip support)
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

// ============================================================================
// 3D Boolean Operations
// ============================================================================

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
    _ = d_fw_screen;

    let prim_type = prim.type_info.x;
    let fill_type = prim.type_info.y;
    let clip_type = prim.type_info.z;

    // Early clip test - discard if completely outside clip region (screen space)
    let clip_alpha = calculate_clip_alpha(p, prim.clip_bounds, prim.clip_radius, prim.clip_corner_shape, clip_type, prim.clip_fade);
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

    // Early type filter — discard non-3D primitives handled by other split pipelines
    if shape_type == 0u { discard; }

    let depth = prim.sdf_3d.x;

    // ── 3D SDF Raymarching Path (individual shapes) ──
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

    // Safety fallback: if neither 3D path triggered, discard
    discard;
    return vec4<f32>(0.0);
}
