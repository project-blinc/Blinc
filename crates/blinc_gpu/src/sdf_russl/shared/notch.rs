//! The notch signed distance functions: rounded rects with concave corners and top and bottom
//! edge modifiers (scoop, bulge, cut, peak), in the shader's own units and in the ABI's.

russl::russl! {
    use super::common::{sd_ellipse, sd_rounded_rect};

    pub fn sd_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> f32 {
        let e0 = b - a; let e1 = c - b; let e2 = a - c;
        let v0 = p - a; let v1 = p - b; let v2 = p - c;
        let pq0 = v0 - e0 * clamp(dot(v0, e0) / max(dot(e0, e0), 1e-6), 0.0, 1.0);
        let pq1 = v1 - e1 * clamp(dot(v1, e1) / max(dot(e1, e1), 1e-6), 0.0, 1.0);
        let pq2 = v2 - e2 * clamp(dot(v2, e2) / max(dot(e2, e2), 1e-6), 0.0, 1.0);
        let s = sign(e0.x * e2.y - e0.y * e2.x);
        let d = min(
            min(
                Vec2::new(dot(pq0, pq0), s * (v0.x * e0.y - v0.y * e0.x)),
                Vec2::new(dot(pq1, pq1), s * (v1.x * e1.y - v1.y * e1.x))
            ),
            Vec2::new(dot(pq2, pq2), s * (v2.x * e2.y - v2.y * e2.x))
        );
        return -sqrt(d.x) * sign(d.y);
    }

    pub fn smin(a: f32, b: f32, k: f32) -> f32 {
        let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
        return mix(b, a, h) - k * h * (1.0 - h);
    }

    pub fn smax(a: f32, b: f32, k: f32) -> f32 {
        return -smin(-a, -b, k);
    }

    pub fn sd_notch(
        p: Vec2,
        outer_origin: Vec2,
        outer_size: Vec2,
        radii: Vec4,
        corner_types: Vec4,
        top_mod: Vec4,
        bottom_mod: Vec4
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
        let top_mod_h = if top_protrudes { top_mod.z } else { 0.0 };
        let bot_type = bottom_mod.x;
        let bot_protrudes = (bot_type > 1.5 && bot_type < 2.5) || (bot_type > 3.5 && bot_type < 4.5);
        let bot_mod_h = if bot_protrudes { bottom_mod.z } else { 0.0 };

        // Edge offsets — the inner rect is inset on each edge by the max of
        // (concave radius on adjacent corners, outward modifier height on that
        // edge). Matches `build_shape_path`'s left/right/top/bottom_offset math.
        let left_offset = if tl_concave || bl_concave { max(if tl_concave { tl_r } else { 0.0 }, if bl_concave { bl_r } else { 0.0 }) } else { 0.0 };
        let right_offset = if tr_concave || br_concave { max(if tr_concave { tr_r } else { 0.0 }, if br_concave { br_r } else { 0.0 }) } else { 0.0 };
        let top_offset = max(
            if tl_concave || tr_concave { max(if tl_concave { tl_r } else { 0.0 }, if tr_concave { tr_r } else { 0.0 }) } else { 0.0 },
            top_mod_h
        );
        let bottom_offset = max(
            if bl_concave || br_concave { max(if bl_concave { bl_r } else { 0.0 }, if br_concave { br_r } else { 0.0 }) } else { 0.0 },
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
        let inner_origin = outer_origin + Vec2::new(left_offset, top_offset);
        let inner_size = Vec2::new(
            max(outer_size.x - left_offset - right_offset, 0.001),
            max(outer_size.y - top_offset - bottom_offset, 0.001)
        );
        let inner_radii = Vec4::new(
            if tl_concave { 0.0 } else { tl_r },
            if tr_concave { 0.0 } else { tr_r },
            if br_concave { 0.0 } else { br_r },
            if bl_concave { 0.0 } else { bl_r }
        );

        let mut d = sd_rounded_rect(p, inner_origin, inner_size, inner_radii);

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
        let eff_tl_ry = if tl_concave { min(tl_r, tb_available) } else { tl_r };
        let eff_tr_ry = if tr_concave { min(tr_r, tb_available) } else { tr_r };
        let eff_br_ry = if br_concave { min(br_r, tb_available) } else { br_r };
        let eff_bl_ry = if bl_concave { min(bl_r, tb_available) } else { bl_r };

        if tl_concave {
            let box_origin = Vec2::new(outer_origin.x, inner_origin.y);
            let box_size = Vec2::new(left_offset + join_depth, eff_tl_ry);
            let box_sd = sd_rounded_rect(p, box_origin, box_size, Vec4::splat(0.0));
            // Elliptical arc: center on outer canvas edge, horizontal radius
            // stays at `left_offset`, vertical radius scales with available
            // height. At `eff_tl_ry == tl_r` this is the same circle as
            // before; at smaller `eff_tl_ry` it squishes vertically so the
            // arc still meets the top edge and inner body tangentially but
            // over a shorter vertical span.
            let c = Vec2::new(outer_origin.x, inner_origin.y + eff_tl_ry);
            let ell_sd = sd_ellipse(p, c, Vec2::new(left_offset, eff_tl_ry));
            let flare = max(box_sd, -ell_sd);
            d = min(d, flare);
        }
        if tr_concave {
            let right_width = outer_origin.x + outer_size.x - inner_right;
            let box_origin = Vec2::new(inner_right - join_depth, inner_origin.y);
            let box_size = Vec2::new(right_width + join_depth, eff_tr_ry);
            let box_sd = sd_rounded_rect(p, box_origin, box_size, Vec4::splat(0.0));
            let c = Vec2::new(outer_origin.x + outer_size.x, inner_origin.y + eff_tr_ry);
            let ell_sd = sd_ellipse(p, c, Vec2::new(right_width, eff_tr_ry));
            let flare = max(box_sd, -ell_sd);
            d = min(d, flare);
        }
        if br_concave {
            let right_width = outer_origin.x + outer_size.x - inner_right;
            let box_origin = Vec2::new(inner_right - join_depth, inner_bottom - eff_br_ry);
            let box_size = Vec2::new(right_width + join_depth, eff_br_ry);
            let box_sd = sd_rounded_rect(p, box_origin, box_size, Vec4::splat(0.0));
            let c = Vec2::new(outer_origin.x + outer_size.x, inner_bottom - eff_br_ry);
            let ell_sd = sd_ellipse(p, c, Vec2::new(right_width, eff_br_ry));
            let flare = max(box_sd, -ell_sd);
            d = min(d, flare);
        }
        if bl_concave {
            let box_origin = Vec2::new(outer_origin.x, inner_bottom - eff_bl_ry);
            let box_size = Vec2::new(left_offset + join_depth, eff_bl_ry);
            let box_sd = sd_rounded_rect(p, box_origin, box_size, Vec4::splat(0.0));
            let c = Vec2::new(outer_origin.x, inner_bottom - eff_bl_ry);
            let ell_sd = sd_ellipse(p, c, Vec2::new(left_offset, eff_bl_ry));
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
                let disk_sd = length(p - Vec2::new(cx, disk_cy)) - disk_r;
                let disk_lower = max(disk_sd, disk_cy - p.y);
                // When depth <= half_w the disk fills the bowl and the rect
                // has no height; unioning it anyway pulls the top edge down
                // across the scoop's full width through the smax below.
                let rect_h = disk_cy - base_y;
                let mut hollow_sd = disk_lower;
                if rect_h > 0.001 {
                    let rect_origin = Vec2::new(cx - half_w, base_y);
                    let rect_sd =
                        sd_rounded_rect(p, rect_origin, Vec2::new(top_w, rect_h), Vec4::splat(0.0));
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
                let disk_sd = length(p - Vec2::new(cx, y_c)) - r_bulge;
                // Above the base line the cap is the disk alone; below it, the
                // buried part is clipped to the cap's own width.
                let buried_clip = min(abs(p.x - cx) - half_w, p.y - base_y);
                let bulge_sd = max(max(disk_sd, p.y - base_y - deep_depth), buried_clip);
                d = smin(d, bulge_sd, max(top_cr, 0.001));
            } else if top_type < 3.5 { // cut — subtract a V-triangle
                d = smax(d, -sd_triangle(
                    p,
                    Vec2::new(cx - top_w * 0.5, base_y),
                    Vec2::new(cx, base_y + top_h),
                    Vec2::new(cx + top_w * 0.5, base_y)
                ), smin_k);
            } else { // peak — union a V-triangle protrusion
                // Extend along the sides, so the apex and the slopes are
                // unchanged and only the buried base moves.
                let spread = half_w * (top_h + deep_depth) / max(top_h, 0.001);
                let peak_sd = max(sd_triangle(
                    p,
                    Vec2::new(cx - spread, base_y + deep_depth),
                    Vec2::new(cx, base_y - top_h),
                    Vec2::new(cx + spread, base_y + deep_depth)
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
            if bot_type < 1.5 { // scoop — mirror of top: rect + half-disk above bottom baseline
                let disk_r = min(half_w, bot_h);
                let disk_cy = base_y - bot_h + disk_r;
                let disk_sd = length(p - Vec2::new(cx, disk_cy)) - disk_r;
                let disk_upper = max(disk_sd, p.y - disk_cy);
                let rect_h = base_y - disk_cy;
                let mut hollow_sd = disk_upper;
                if rect_h > 0.001 {
                    let rect_origin = Vec2::new(cx - half_w, disk_cy);
                    let rect_sd =
                        sd_rounded_rect(p, rect_origin, Vec2::new(bot_w, rect_h), Vec4::splat(0.0));
                    hollow_sd = min(rect_sd, disk_upper);
                }
                d = smax(d, -hollow_sd, max(bot_cr, 0.001));
            } else if bot_type < 2.5 { // bulge — circular arc cap with smooth ears
                let r_bulge = (half_w * half_w + bot_h * bot_h) / max(2.0 * bot_h, 0.001);
                let y_c = base_y + bot_h - r_bulge;
                let disk_sd = length(p - Vec2::new(cx, y_c)) - r_bulge;
                let buried_clip = min(abs(p.x - cx) - half_w, base_y - p.y);
                let bulge_sd = max(max(disk_sd, base_y - p.y - deep_depth), buried_clip);
                d = smin(d, bulge_sd, max(bot_cr, 0.001));
            } else if bot_type < 3.5 { // cut
                d = smax(d, -sd_triangle(
                    p,
                    Vec2::new(cx - bot_w * 0.5, base_y),
                    Vec2::new(cx, base_y - bot_h),
                    Vec2::new(cx + bot_w * 0.5, base_y)
                ), smin_k);
            } else { // peak
                let spread = half_w * (bot_h + deep_depth) / max(bot_h, 0.001);
                let peak_sd = max(sd_triangle(
                    p,
                    Vec2::new(cx - spread, base_y - deep_depth),
                    Vec2::new(cx, base_y + bot_h),
                    Vec2::new(cx + spread, base_y - deep_depth)
                ), abs(p.x - cx) - half_w);
                d = smin(d, peak_sd, smin_k);
            }
        }

        return d;
    }

    pub fn superellipse_dist(t: Vec2, p: f32, r: f32) -> f32 {
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
        let g = Vec2::new(pow(a / sp, p - 1.0), pow(b / sp, p - 1.0));
        return (f - 1.0) * r / max(length(g), 1e-6);
    }

    pub fn sd_shaped_rect(p: Vec2, origin: Vec2, size: Vec2, radius: Vec4, shape: Vec4) -> f32 {
        let half_size = size * 0.5;
        let center = origin + half_size;
        let rel = p - center;
        let q = abs(rel) - half_size;

        // Select corner radius and shape based on quadrant
        let mut r: f32;
        let mut n: f32;
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
        let box_d = length(max(q, Vec2::splat(0.0))) + min(max(q.x, q.y), 0.0);
        let qa = q + Vec2::splat(r);
        let rr = max(r, 0.001);

        // Notch: the box less the unbounded quadrant past the step, so its
        // only edges are the step's two faces. A union of two bars
        // misreported depth near the reentrant corner.
        if n <= -100.0 {
            let nq = -qa;
            let cut = length(max(nq, Vec2::splat(0.0))) + min(max(nq.x, nq.y), 0.0);
            return max(box_d, -cut);
        }

        // Square: the box itself.
        if n >= 100.0 {
            return box_d;
        }

        // Circle: already exact, and cheaper than the general form.
        if abs(n - 1.0) < 0.01 {
            return length(max(qa, Vec2::splat(0.0))) + min(max(qa.x, qa.y), 0.0) - r;
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
        //
        // Outside the corner square the curve does not apply, and the general form below reduces
        // to these values exactly, so the interior and straight edges (nearly every fragment of a
        // large rounded rect) skip its `pow` calls. Past both axes the arc is exactly r away.
        if qa.x <= 0.0 && qa.y <= 0.0 {
            return max(box_d, -r);
        }
        if qa.x <= 0.0 || qa.y <= 0.0 {
            return box_d;
        }
        let p_exp = pow(2.0, min(abs(n), 5.0));
        let t = max(qa, Vec2::splat(0.0)) / rr;
        return max(box_d, superellipse_dist(t, p_exp, r));
    }

    pub fn abi_notch_edge(p: Vec2, distance: f32, cx: f32, base: f32, m: Vec4, dir: f32, into: f32) -> f32 {
        if m.x <= 0.5 || m.y <= 0.001 || m.z <= 0.001 { return distance; }
        let half_w = m.y * 0.5;
        let q = Vec2::new(p.x, (p.y - base) * dir);
        if m.x < 1.5 {
            let radius = min(half_w, m.z);
            let cy = m.z - radius;
            let disk = max(length(q - Vec2::new(cx, cy)) - radius, cy - q.y);
            let mut hollow = disk;
            if cy > 0.001 {
                hollow = min(sd_rounded_rect(q, Vec2::new(cx - half_w, 0.0), Vec2::new(m.y, cy), Vec4::splat(0.0)), disk);
            }
            return smax(distance, -hollow, max(m.w, 0.001));
        }
        if m.x < 2.5 {
            let radius = (half_w * half_w + m.z * m.z) / max(2.0 * m.z, 0.001);
            let mut side = abs(q.x - cx) - half_w;
            if q.y < 0.0 { side = -1e5; }
            let cap = max(max(length(q - Vec2::new(cx, radius - m.z)) - radius, q.y - into), side);
            return smin(distance, cap, max(m.w, 0.001));
        }
        if m.x < 3.5 {
            return smax(distance, -sd_triangle(q, Vec2::new(cx - half_w, 0.0), Vec2::new(cx, m.z), Vec2::new(cx + half_w, 0.0)), 1.5);
        }
        let spread = half_w * (m.z + into) / m.z;
        let peak = max(sd_triangle(q, Vec2::new(cx - spread, into), Vec2::new(cx, -m.z), Vec2::new(cx + spread, into)), abs(q.x - cx) - half_w);
        return smin(distance, peak, 1.5);
    }

    pub fn abi_notch_flare(p: Vec2, origin: Vec2, size: Vec2, centre: Vec2, radii: Vec2) -> f32 {
        let ellipse = (length((p - centre) / max(radii, Vec2::splat(0.001))) - 1.0) * min(radii.x, radii.y);
        return max(sd_rounded_rect(p, origin, size, Vec4::splat(0.0)), -ellipse);
    }

    pub fn sd_abi_notch(p: Vec2, size: Vec2, r: Vec4, types: Vec4, top: Vec4, bottom: Vec4) -> f32 {
        let concave_x = types.x > 0.5;
        let concave_y = types.y > 0.5;
        let concave_z = types.z > 0.5;
        let concave_w = types.w > 0.5;
        let offsets = Vec4::new(
            if concave_x { r.x } else { 0.0 },
            if concave_y { r.y } else { 0.0 },
            if concave_z { r.z } else { 0.0 },
            if concave_w { r.w } else { 0.0 },
        );
        let left = max(offsets.x, offsets.w);
        let right = max(offsets.y, offsets.z);
        let top_h = if (top.x > 1.5 && top.x < 2.5) || (top.x > 3.5 && top.x < 4.5) { top.z } else { 0.0 };
        let bottom_h = if (bottom.x > 1.5 && bottom.x < 2.5) || (bottom.x > 3.5 && bottom.x < 4.5) { bottom.z } else { 0.0 };
        let top_offset = max(max(offsets.x, offsets.y), top_h);
        let bottom_offset = max(max(offsets.w, offsets.z), bottom_h);
        let origin = Vec2::new(left, top_offset);
        let body = max(size - Vec2::new(left + right, top_offset + bottom_offset), Vec2::splat(0.001));
        let radii = Vec4::new(
            if concave_x { 0.0 } else { r.x },
            if concave_y { 0.0 } else { r.y },
            if concave_z { 0.0 } else { r.z },
            if concave_w { 0.0 } else { r.w },
        );
        let mut d = sd_rounded_rect(p, origin, body, radii);
        let end = origin + body;
        let into = min(body.x, body.y) * 0.5;
        let join = min(8.0, into);
        let room = max(size.y - top_offset - bottom_offset, 0.0);
        if concave_x {
            let ry = min(r.x, room);
            d = min(d, abi_notch_flare(p, Vec2::new(0.0, origin.y), Vec2::new(left + join, ry), Vec2::new(0.0, origin.y + ry), Vec2::new(left, ry)));
        }
        if concave_y {
            let ry = min(r.y, room);
            let width = size.x - end.x;
            d = min(d, abi_notch_flare(p, Vec2::new(end.x - join, origin.y), Vec2::new(width + join, ry), Vec2::new(size.x, origin.y + ry), Vec2::new(width, ry)));
        }
        if concave_z {
            let ry = min(r.z, room);
            let width = size.x - end.x;
            d = min(d, abi_notch_flare(p, Vec2::new(end.x - join, end.y - ry), Vec2::new(width + join, ry), Vec2::new(size.x, end.y - ry), Vec2::new(width, ry)));
        }
        if concave_w {
            let ry = min(r.w, room);
            d = min(d, abi_notch_flare(p, Vec2::new(0.0, end.y - ry), Vec2::new(left + join, ry), Vec2::new(0.0, end.y - ry), Vec2::new(left, ry)));
        }
        d = abi_notch_edge(p, d, size.x * 0.5, origin.y, top, 1.0, into);
        return abi_notch_edge(p, d, size.x * 0.5, end.y, bottom, -1.0, into);
    }
}
