//! Executable spec for `sd_shaped_rect` in the SDF shaders.
//!
//! A port of the WGSL, kept in step with `shaders/sdf_core.wgsl` (and the
//! copies in `sdf_shadow.wgsl` / `sdf_notch.wgsl`). A signed distance
//! field has to carry unit gradient: the renderer divides by it for
//! antialiasing and offsets it by a constant for borders, so a field that
//! is merely the right SIGN gives thin borders and uneven AA even though
//! the silhouette looks right.
//!
//! These tests check the two properties the renderer depends on:
//!
//! - gradient magnitude near 1 on the boundary, so AA width and border
//!   width are uniform around the shape;
//! - a bounded Lipschitz slope everywhere, so no region advances faster
//!   than true distance.

type V2 = (f32, f32);

fn len2(v: V2) -> f32 {
    (v.0 * v.0 + v.1 * v.1).sqrt()
}

/// Sharp box, exact.
fn sd_box(q: V2) -> f32 {
    len2((q.0.max(0.0), q.1.max(0.0))) + q.0.max(q.1).min(0.0)
}

/// Distance to the superellipse `|t|_p = 1`, scaled by `r`.
///
/// `(|t|_p - 1) * r` is a level set, not a distance: its gradient is
/// root-2 for `p = 1` and drifts with `p`. Dividing by the norm's own
/// gradient length normalises it.
fn superellipse_dist(t: V2, p: f32, r: f32) -> f32 {
    // Factor the largest component out of the norm. Evaluating
    // `tx^p + ty^p` directly underflows f32 once p reaches 8 (a tiny t
    // raised to the eighth is below the smallest subnormal), which made f
    // zero, the gradient term infinite, and the distance collapse to -0.
    let m = t.0.max(t.1);
    if m <= 1e-20 {
        // At the corner's inner centre the arc is exactly r away, and the
        // box bounds the result anyway.
        return -r;
    }
    let a = t.0 / m;
    let b = t.1 / m;
    let s = a.powf(p) + b.powf(p);
    let sp = s.powf(1.0 / p);
    let f = m * sp;
    // grad |t|_p = ((t.x/f)^(p-1), (t.y/f)^(p-1)), written against the
    // factored form so nothing divides by an underflowed f. pow(0, 0) is
    // undefined in WGSL and p == 1 reaches it on the axes, so the bevel
    // takes its own exact branch instead.
    let gx = (a / sp).powf(p - 1.0);
    let gy = (b / sp).powf(p - 1.0);
    (f - 1.0) * r / len2((gx, gy)).max(1e-6)
}

pub fn sd_shaped_rect(pt: V2, origin: V2, size: V2, radius: [f32; 4], shape: [f32; 4]) -> f32 {
    let half = (size.0 * 0.5, size.1 * 0.5);
    let center = (origin.0 + half.0, origin.1 + half.1);
    let rel = (pt.0 - center.0, pt.1 - center.1);
    let q = (rel.0.abs() - half.0, rel.1.abs() - half.1);

    let (r, n) = match (rel.1 < 0.0, rel.0 > 0.0) {
        (true, true) => (radius[1], shape[1]),
        (true, false) => (radius[0], shape[0]),
        (false, true) => (radius[2], shape[2]),
        (false, false) => (radius[3], shape[3]),
    };
    let r = r.min(half.0.min(half.1));
    let box_d = sd_box(q);
    let qa = (q.0 + r, q.1 + r);
    let rr = r.max(0.001);

    // Notch: the box less the unbounded quadrant past the step. Its only
    // edges are the step's two faces, where a union of two bars
    // overestimated depth near the inner corners.
    if n <= -100.0 {
        let cut = sd_box((-qa.0, -qa.1));
        return box_d.max(-cut);
    }

    // Square: the box itself.
    if n >= 100.0 {
        return box_d;
    }

    // Circle: already exact, and cheaper than the general form.
    if (n - 1.0).abs() < 0.01 {
        return len2((qa.0.max(0.0), qa.1.max(0.0))) + qa.0.max(qa.1).min(0.0) - r;
    }

    // Scoop: a concave arc centred on the corner's tip, carved out of the
    // box across the whole quadrant.
    if n < 0.0 {
        let p = 2f32.powf(n.abs().min(5.0));
        let t = (q.0.abs() / rr, q.1.abs() / rr);
        return box_d.max(-superellipse_dist(t, p, r));
    }

    // Bevel: exact, and avoids p == 1 in the general path.
    if n.abs() < 0.01 {
        return box_d.max((qa.0 + qa.1 - r) / 2f32.sqrt());
    }

    // Convex superellipse, bounded by the box so the interior stays
    // continuous without a separate region gate.
    let p = 2f32.powf(n.abs().min(5.0));
    let t = (qa.0.max(0.0) / rr, qa.1.max(0.0) / rr);
    box_d.max(superellipse_dist(t, p, r))
}

// ---------------------------------------------------------------------

const ORIGIN: V2 = (0.0, 0.0);
const SIZE: V2 = (120.0, 80.0);
const R: f32 = 24.0;

fn shapes() -> Vec<(&'static str, f32)> {
    vec![
        ("round", 1.0),
        ("bevel", 0.0),
        ("squircle", 2.0),
        ("superellipse n=3", 3.0),
        ("scoop", -1.0),
        ("notch", -100.0),
        ("square", 100.0),
    ]
}

fn d_at(n: f32, pt: V2) -> f32 {
    sd_shaped_rect(pt, ORIGIN, SIZE, [R; 4], [n; 4])
}

/// Central-difference gradient.
fn grad(n: f32, pt: V2, h: f32) -> V2 {
    (
        (d_at(n, (pt.0 + h, pt.1)) - d_at(n, (pt.0 - h, pt.1))) / (2.0 * h),
        (d_at(n, (pt.0, pt.1 + h)) - d_at(n, (pt.0, pt.1 - h))) / (2.0 * h),
    )
}

/// The renderer draws a border by offsetting the field: `-(d + border)`.
/// That only yields the asked-for width if the field is a true distance,
/// so step inward from the boundary by a border width and require the
/// field to report it.
///
/// This is what `(|t|_p - 1) * r` failed: its gradient is root-2 for a
/// bevel, so the chamfer's border came out about 30% thin, and it drifted
/// around squircles, giving uneven width and AA.
///
/// Creases are skipped, not fudged: where the nearest feature changes the
/// gradient is genuinely undefined, and a sharp corner's diagonal is one.
/// At a point where the field is smooth, its gradient magnitude must be
/// exactly 1 — that is what makes it a distance rather than a level set.
///
/// A crease, where the nearest feature changes, has no gradient and is
/// skipped. Crucially it is detected by the forward and backward
/// differences pointing DIFFERENT WAYS, not by the magnitude being off:
/// gating on magnitude would skip precisely the defective points, which
/// is how an earlier version of this test passed against the unnormalised
/// `(|t|_p - 1) * r`.
#[test]
fn gradient_is_unit_where_the_field_is_smooth() {
    let h = 0.01;
    for (name, n) in shapes() {
        let mut worst = 0.0f32;
        let mut worst_at = (0.0, 0.0);
        let mut checked = 0usize;
        let mut y = -4.0;
        while y <= SIZE.1 + 4.0 {
            let mut x = -4.0;
            while x <= SIZE.0 + 4.0 {
                let pt = (x, y);
                if d_at(n, pt).abs() < 0.25 {
                    let d0 = d_at(n, pt);
                    let gf = (
                        (d_at(n, (pt.0 + h, pt.1)) - d0) / h,
                        (d_at(n, (pt.0, pt.1 + h)) - d0) / h,
                    );
                    let gb = (
                        (d0 - d_at(n, (pt.0 - h, pt.1))) / h,
                        (d0 - d_at(n, (pt.0, pt.1 - h))) / h,
                    );
                    let (lf, lb) = (len2(gf), len2(gb));
                    let smooth =
                        lf > 0.1 && lb > 0.1 && (gf.0 * gb.0 + gf.1 * gb.1) / (lf * lb) > 0.999;
                    if smooth {
                        checked += 1;
                        let err = (len2(grad(n, pt, h)) - 1.0).abs();
                        if err > worst {
                            worst = err;
                            worst_at = pt;
                        }
                    }
                }
                x += 0.7;
            }
            y += 0.7;
        }
        assert!(
            checked > 40,
            "{name}: only {checked} smooth boundary samples"
        );
        assert!(
            worst < 0.03,
            "{name}: gradient magnitude off by {worst:.3} at {worst_at:?}, so a border \
             there would be {:.0}% wrong",
            worst / (1.0 - worst).max(0.01) * 100.0
        );
    }
}

#[test]
fn stepping_inward_by_a_border_width_reports_that_width() {
    const BORDER: f32 = 3.0;
    let h = 0.01;
    for (name, n) in shapes() {
        let mut worst = 0.0f32;
        let mut worst_at = (0.0, 0.0);
        let mut checked = 0usize;
        let mut y = -4.0;
        while y <= SIZE.1 + 4.0 {
            let mut x = -4.0;
            while x <= SIZE.0 + 4.0 {
                let pt = (x, y);
                let d0 = d_at(n, pt);
                if d0.abs() < 0.25 {
                    let g = grad(n, pt, h);
                    let gl = len2(g);
                    // |grad| near 1 means a single nearest feature; well
                    // below means a crease, where no normal exists.
                    if (gl - 1.0).abs() < 0.02 {
                        // Inward is against the gradient.
                        let inward = (pt.0 - g.0 / gl * BORDER, pt.1 - g.1 / gl * BORDER);
                        // Only where the nearest feature is still the same
                        // one. Step off the end of a face and a different
                        // edge becomes nearest, which is the medial axis
                        // doing its job, not a defect.
                        let gi = grad(n, inward, h);
                        let gil = len2(gi);
                        let aligned = gil > 0.5 && (g.0 * gi.0 + g.1 * gi.1) / (gl * gil) > 0.9999;
                        if aligned {
                            let want = d0 - BORDER;
                            let err = (d_at(n, inward) - want).abs();
                            checked += 1;
                            if err > worst {
                                worst = err;
                                worst_at = pt;
                            }
                        }
                    }
                }
                x += 0.7;
            }
            y += 0.7;
        }
        assert!(
            checked > 50,
            "{name}: only {checked} boundary samples had a normal"
        );
        assert!(
            worst < BORDER * 0.05,
            "{name}: stepping {BORDER} inward from {worst_at:?} was off by {worst:.3}, \
             so a {BORDER}px border would be {:.0}% wrong",
            worst / BORDER * 100.0
        );
    }
}

#[test]
fn slope_is_bounded_everywhere() {
    let h = 0.25;
    for (name, n) in shapes() {
        let mut worst = 0.0f32;
        let mut worst_at = (0.0, 0.0);
        let mut y = -20.0;
        while y <= SIZE.1 + 20.0 {
            let mut x = -20.0;
            while x <= SIZE.0 + 20.0 {
                let g = len2(grad(n, (x, y), h));
                if g.is_finite() && g > worst {
                    worst = g;
                    worst_at = (x, y);
                }
                x += 1.0;
            }
            y += 1.0;
        }
        assert!(
            worst <= 1.15,
            "{name}: slope {worst:.3} at {worst_at:?} exceeds 1.15, so the field \
             advances faster than true distance"
        );
    }
}

#[test]
fn every_field_is_finite() {
    for (name, n) in shapes() {
        let mut y = -20.0;
        while y <= SIZE.1 + 20.0 {
            let mut x = -20.0;
            while x <= SIZE.0 + 20.0 {
                let d = d_at(n, (x, y));
                assert!(d.is_finite(), "{name}: non-finite at ({x}, {y})");
                x += 0.5;
            }
            y += 0.5;
        }
    }
}

/// A scoop removes material at the corner, so the tip must be OUTSIDE.
/// The old code negated the convex superellipse about the corner's inner
/// centre, which left the tip filled and cut a disc out of the interior.
#[test]
fn scoop_carves_the_corner_and_keeps_the_middle() {
    let tip = (0.5, 0.5);
    assert!(
        d_at(-1.0, tip) > 0.0,
        "scoop: corner tip is filled, so the arc is inside out"
    );
    let mid = (SIZE.0 * 0.5, SIZE.1 * 0.5);
    assert!(d_at(-1.0, mid) < 0.0, "scoop: centre is not inside");
    // A point just inside the arc, measured along the diagonal from the tip.
    let inside_arc = (R * 0.8, R * 0.8);
    assert!(
        d_at(-1.0, inside_arc) < 0.0,
        "scoop: carved too much, the interior past the arc should be filled"
    );
}

/// A notch cuts a rectangular step. The tip is outside; both step faces
/// are boundaries; the interior past the step is filled.
#[test]
fn notch_cuts_a_step() {
    assert!(d_at(-100.0, (0.5, 0.5)) > 0.0, "notch: tip is filled");
    assert!(
        d_at(-100.0, (R + 2.0, R + 2.0)) < 0.0,
        "notch: interior past the step should be filled"
    );
    assert!(
        d_at(-100.0, (SIZE.0 * 0.5, SIZE.1 * 0.5)) < 0.0,
        "notch: centre is not inside"
    );

    // The step's two faces meet in a reentrant corner. A point one unit
    // inside it along the diagonal is root-2 from the nearest boundary,
    // which is the corner itself. A union of two bars reports 1 instead,
    // reading as nearer to the edge than it is and thinning the border.
    let diag = (R + 1.0, R + 1.0);
    let d = d_at(-100.0, diag);
    assert!(
        (d + 2f32.sqrt()).abs() < 0.01,
        "notch: depth at the reentrant corner is {d:.4}, want -{:.4}",
        2f32.sqrt()
    );
}

/// The bevel's chamfer is a straight 45-degree face, so its distance is
/// exact and its border must not come out thin.
#[test]
fn bevel_chamfer_is_exact() {
    // On the chamfer: qa.x + qa.y == r, i.e. the line through (r,0)-(0,r)
    // measured from the corner.
    let on_face = (R * 0.5, R * 0.5);
    let d = d_at(0.0, on_face);
    assert!(
        d.abs() < 0.001,
        "bevel: chamfer midpoint should sit on the boundary, got {d:.4}"
    );
    let g = len2(grad(0.0, on_face, 0.05));
    assert!(
        (g - 1.0).abs() < 0.02,
        "bevel: chamfer gradient {g:.4}, so its border would be {:.0}% off",
        (1.0 / g - 1.0) * 100.0
    );
}

/// Square and round are the reference shapes; if these drift, the port is
/// wrong rather than the maths.
#[test]
fn square_and_round_match_their_closed_forms() {
    let half = (SIZE.0 * 0.5, SIZE.1 * 0.5);
    for pt in [(10.0, 10.0), (-5.0, 40.0), (60.0, 40.0), (130.0, 90.0)] {
        let rel = (pt.0 - half.0, pt.1 - half.1);
        let q = (rel.0.abs() - half.0, rel.1.abs() - half.1);
        assert!(
            (d_at(100.0, pt) - sd_box(q)).abs() < 1e-4,
            "square should equal the box SDF at {pt:?}"
        );
        let qa = (q.0 + R, q.1 + R);
        let want = len2((qa.0.max(0.0), qa.1.max(0.0))) + qa.0.max(qa.1).min(0.0) - R;
        assert!(
            (d_at(1.0, pt) - want).abs() < 1e-4,
            "round should equal the rounded-box SDF at {pt:?}"
        );
    }
}
