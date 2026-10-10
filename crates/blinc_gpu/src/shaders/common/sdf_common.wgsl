// Shared SDF helpers, composed into every sdf_*.wgsl by build.rs.
//
// WGSL has no include, so these were copy-pasted into each shader: a fix
// then had to be applied by hand to nine or twelve files (2ad76a9,
// 178a63b).
//
// Two rules decide what may live here, and naga enforces both because the
// composed output is what gets validated:
//
//   1. No reference to a binding — `primitives`, `aux_data`, `aux_tex`,
//      `uniforms` and friends. The prelude is prepended BEFORE a shader
//      declares them. A helper that reads aux data stays with the shader
//      that owns it, which is why `sdf_3d_eval` and the `eval_group_*`
//      family are not here.
//   2. It may only call other helpers in this file, and must appear
//      after them: build.rs keeps this order when it subsets, so the
//      order below is topological.

fn op_intersect(d1: f32, d2: f32) -> f32 { return max(d1, d2); }

fn op_smooth_intersect(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) + k * h * (1.0 - h);
}

fn op_smooth_subtract(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (d2 + d1) / k, 0.0, 1.0);
    return mix(d1, -d2, h) + k * h * (1.0 - h);
}

fn op_smooth_union(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) - k * h * (1.0 - h);
}

fn op_subtract(d1: f32, d2: f32) -> f32 { return max(d1, -d2); }

fn op_union(d1: f32, d2: f32) -> f32 { return min(d1, d2); }

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

fn compute_uv_cylinder(hp: vec3<f32>, half_h: f32) -> vec2<f32> {
    let u = atan2(hp.z, hp.x) / (2.0 * 3.14159) + 0.5;
    let v = (hp.y / max(half_h, 0.001) + 1.0) * 0.5;
    return vec2<f32>(u, v);
}

fn compute_uv_sphere(hp: vec3<f32>) -> vec2<f32> {
    let n = normalize(hp + vec3<f32>(0.0001));
    let u = atan2(n.z, n.x) / (2.0 * 3.14159) + 0.5;
    let v = asin(clamp(n.y, -1.0, 1.0)) / 3.14159 + 0.5;
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

fn erf(x: f32) -> f32 {
    let s = sign(x);
    let a = abs(x);
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
    return s * y;
}

fn quarter_ellipse_sdf(point: vec2<f32>, radii: vec2<f32>) -> f32 {
    // Avoid division by zero
    let safe_radii = max(radii, vec2<f32>(0.001));
    // Map to unit circle space
    let circle_vec = point / safe_radii;
    let unit_circle_sdf = length(circle_vec) - 1.0;
    // Scale back using average radius for distance approximation
    return unit_circle_sdf * (safe_radii.x + safe_radii.y) * -0.5;
}

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

fn rotate_x_inv(p: vec3<f32>, s: f32, c: f32) -> vec3<f32> {
    return vec3<f32>(p.x, c * p.y + s * p.z, -s * p.y + c * p.z);
}

fn rotate_y_inv(p: vec3<f32>, s: f32, c: f32) -> vec3<f32> {
    return vec3<f32>(c * p.x - s * p.z, p.y, s * p.x + c * p.z);
}

fn rotate_z_inv(p: vec3<f32>, s: f32, c: f32) -> vec3<f32> {
    return vec3<f32>(c * p.x + s * p.y, -s * p.x + c * p.y, p.z);
}

fn sd_box_3d(p: vec3<f32>, half_ext: vec3<f32>, r: f32) -> f32 {
    let q = abs(p) - half_ext + vec3<f32>(r);
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0) - r;
}

fn sd_capsule_3d(p: vec3<f32>, h: f32, r: f32) -> f32 {
    let py = p.y - clamp(p.y, -h, h);
    return length(vec3<f32>(p.x, py, p.z)) - r;
}

fn sd_circle(p: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    return length(p - center) - radius;
}

fn sd_cylinder_3d(p: vec3<f32>, h: f32, r: f32) -> f32 {
    let d = vec2<f32>(length(p.xz) - r, abs(p.y) - h);
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0)));
}

fn sd_ellipse(p: vec2<f32>, center: vec2<f32>, radii: vec2<f32>) -> f32 {
    let p_centered = p - center;
    let p_norm = p_centered / radii;
    let dist = length(p_norm);
    return (dist - 1.0) * min(radii.x, radii.y);
}

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

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn smax(a: f32, b: f32, k: f32) -> f32 {
    return -smin(-a, -b, k);
}

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

    // How far each added piece reaches INTO the body.
    //
    // The shape is a union of a body and added pieces, and each piece
    // shares an edge with the body exactly. Inside the union near that
    // shared edge, min/smin returns the distance to the piece's own
    // edge, which is not part of the outline — so a border derived from
    // the distance rings the join. Burying each piece a little way
    // inside the body puts every point near a join deep within at least
    // one piece, and the interior distance then measures to the outline.
    //
    // The buried part is hidden, so it changes no silhouette. Capped so
    // a small shape cannot have a piece reach through it.
    let join_depth = min(8.0, min(inner_size.x, inner_size.y) * 0.5);

    // A bulge or a peak is buried deeper, up to half the body's smaller
    // side, so an inset ring wider than `join_depth` still lies inside the
    // piece where it meets the body. Each is clipped to its own width below
    // its base line, so the buried part cannot square off the body's
    // rounded corners. A flare is not buried this far: its far end would
    // bend the distance gradient.
    let deep_depth = min(inner_size.x, inner_size.y) * 0.5;

    // `k` controls how wide the `smin` blend zone is for the cut and the
    // peak. Keep it below `2 * aa_width + 1` (~2 px) — too large and
    // smin's blend region inflates pixels that are actually just outside
    // both sub-shapes. A flare is joined with a plain `min` instead: its
    // top edge lies along the body's edge, and a blend there would bulge
    // the shared edge and leave a step where the flare rect ends.
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
        let box_size = vec2<f32>(left_offset + join_depth, eff_tl_ry);
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
        d = min(d, flare);
    }
    if tr_concave {
        let right_width = outer_origin.x + outer_size.x - inner_right;
        let box_origin = vec2<f32>(inner_right - join_depth, inner_origin.y);
        let box_size = vec2<f32>(right_width + join_depth, eff_tr_ry);
        let box_sd = sd_rounded_rect(p, box_origin, box_size, vec4<f32>(0.0));
        let c = vec2<f32>(outer_origin.x + outer_size.x, inner_origin.y + eff_tr_ry);
        let ell_sd = sd_ellipse(p, c, vec2<f32>(right_width, eff_tr_ry));
        let flare = max(box_sd, -ell_sd);
        d = min(d, flare);
    }
    if br_concave {
        let right_width = outer_origin.x + outer_size.x - inner_right;
        let box_origin = vec2<f32>(inner_right - join_depth, inner_bottom - eff_br_ry);
        let box_size = vec2<f32>(right_width + join_depth, eff_br_ry);
        let box_sd = sd_rounded_rect(p, box_origin, box_size, vec4<f32>(0.0));
        let c = vec2<f32>(outer_origin.x + outer_size.x, inner_bottom - eff_br_ry);
        let ell_sd = sd_ellipse(p, c, vec2<f32>(right_width, eff_br_ry));
        let flare = max(box_sd, -ell_sd);
        d = min(d, flare);
    }
    if bl_concave {
        let box_origin = vec2<f32>(outer_origin.x, inner_bottom - eff_bl_ry);
        let box_size = vec2<f32>(left_offset + join_depth, eff_bl_ry);
        let box_sd = sd_rounded_rect(p, box_origin, box_size, vec4<f32>(0.0));
        let c = vec2<f32>(outer_origin.x, inner_bottom - eff_bl_ry);
        let ell_sd = sd_ellipse(p, c, vec2<f32>(left_offset, eff_bl_ry));
        let flare = max(box_sd, -ell_sd);
        d = min(d, flare);
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
            // When depth <= half_w the disk fills the bowl and the rect
            // has no height; unioning it anyway pulls the top edge down
            // across the scoop's full width through the smax below.
            let rect_h = disk_cy - base_y;
            var hollow_sd = disk_lower;
            if rect_h > 0.001 {
                let rect_origin = vec2<f32>(cx - half_w, base_y);
                let rect_sd =
                    sd_rounded_rect(p, rect_origin, vec2<f32>(top_w, rect_h), vec4<f32>(0.0));
                hollow_sd = min(rect_sd, disk_lower);
            }
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
            // Above the base line the cap is the disk alone; below it, the
            // buried part is clipped to the cap's own width.
            let buried_clip = min(abs(p.x - cx) - half_w, p.y - base_y);
            let bulge_sd = max(max(disk_sd, p.y - base_y - deep_depth), buried_clip);
            d = smin(d, bulge_sd, max(top_cr, 0.001));
        } else if top_type < 3.5 { // cut — subtract a V-triangle
            d = smax(d, -sd_triangle(
                p,
                vec2<f32>(cx - top_w * 0.5, base_y),
                vec2<f32>(cx, base_y + top_h),
                vec2<f32>(cx + top_w * 0.5, base_y)
            ), smin_k);
        } else { // peak — union a V-triangle protrusion
            // Extend along the sides, so the apex and the slopes are
            // unchanged and only the buried base moves.
            let spread = half_w * (top_h + deep_depth) / max(top_h, 0.001);
            let peak_sd = max(sd_triangle(
                p,
                vec2<f32>(cx - spread, base_y + deep_depth),
                vec2<f32>(cx, base_y - top_h),
                vec2<f32>(cx + spread, base_y + deep_depth)
            ), abs(p.x - cx) - half_w);
            d = smin(d, peak_sd, smin_k);
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
            let rect_h = base_y - disk_cy;
            var hollow_sd = disk_upper;
            if rect_h > 0.001 {
                let rect_origin = vec2<f32>(cx - half_w, disk_cy);
                let rect_sd =
                    sd_rounded_rect(p, rect_origin, vec2<f32>(bot_w, rect_h), vec4<f32>(0.0));
                hollow_sd = min(rect_sd, disk_upper);
            }
            d = smax(d, -hollow_sd, max(bot_cr, 0.001));
        } else if bot_type < 2.5 { // bulge — circular arc cap with smooth ears
            let r_bulge = (half_w * half_w + bot_h * bot_h) / max(2.0 * bot_h, 0.001);
            let y_c = base_y + bot_h - r_bulge;
            let disk_sd = length(p - vec2<f32>(cx, y_c)) - r_bulge;
            let buried_clip = min(abs(p.x - cx) - half_w, base_y - p.y);
            let bulge_sd = max(max(disk_sd, base_y - p.y - deep_depth), buried_clip);
            d = smin(d, bulge_sd, max(bot_cr, 0.001));
        } else if bot_type < 3.5 { // cut
            d = smax(d, -sd_triangle(
                p,
                vec2<f32>(cx - bot_w * 0.5, base_y),
                vec2<f32>(cx, base_y - bot_h),
                vec2<f32>(cx + bot_w * 0.5, base_y)
            ), smin_k);
        } else { // peak
            let spread = half_w * (bot_h + deep_depth) / max(bot_h, 0.001);
            let peak_sd = max(sd_triangle(
                p,
                vec2<f32>(cx - spread, base_y - deep_depth),
                vec2<f32>(cx, base_y + bot_h),
                vec2<f32>(cx + spread, base_y - deep_depth)
            ), abs(p.x - cx) - half_w);
            d = smin(d, peak_sd, smin_k);
        }
    }

    return d;
}

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

fn sd_sphere_3d(p: vec3<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn sd_torus_3d(p: vec3<f32>, major_r: f32, minor_r: f32) -> f32 {
    let q = vec2<f32>(length(p.xz) - major_r, p.y);
    return length(q) - minor_r;
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

// Corner radii of a box shadow grown by `spread`, as CSS does: a corner grows
// by the spread only as far as it is already round, so a square corner stays
// square.
fn shadow_spread_radius(radius: vec4<f32>, spread: f32) -> vec4<f32> {
    if spread > 0.0 {
        let u = min(radius / spread, vec4<f32>(1.0)) - vec4<f32>(1.0);
        return radius + spread * (vec4<f32>(1.0) + u * u * u);
    }
    return max(radius + vec4<f32>(spread), vec4<f32>(0.0));
}

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
