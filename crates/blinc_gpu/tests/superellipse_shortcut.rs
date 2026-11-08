//! The shortcut in `sd_shaped_rect` for fragments outside a corner square returns what the
//! general convex superellipse form returns there.
//!
//! A CPU port of both, over a grid of points, radii and exponents. These tests do not need a GPU.

fn pow(a: f32, b: f32) -> f32 {
    a.powf(b)
}

fn superellipse_dist(t: (f32, f32), p: f32, r: f32) -> f32 {
    let m = t.0.max(t.1);
    if m <= 1e-20 {
        return -r;
    }
    let (a, b) = (t.0 / m, t.1 / m);
    let s = pow(a, p) + pow(b, p);
    let sp = pow(s, 1.0 / p);
    let f = m * sp;
    let g = (pow(a / sp, p - 1.0), pow(b / sp, p - 1.0));
    (f - 1.0) * r / (g.0.hypot(g.1)).max(1e-6)
}

/// The convex branch of `sd_shaped_rect`, with and without the shortcut.
fn convex(q: (f32, f32), r: f32, n: f32, shortcut: bool) -> f32 {
    let box_d = q.0.max(0.0).hypot(q.1.max(0.0)) + q.0.max(q.1).min(0.0);
    let qa = (q.0 + r, q.1 + r);
    let rr = r.max(0.001);
    if shortcut && qa.0 <= 0.0 && qa.1 <= 0.0 {
        return box_d.max(-r);
    }
    if shortcut && (qa.0 <= 0.0 || qa.1 <= 0.0) {
        return box_d;
    }
    let p_exp = pow(2.0, n.abs().min(5.0));
    let t = (qa.0.max(0.0) / rr, qa.1.max(0.0) / rr);
    box_d.max(superellipse_dist(t, p_exp, r))
}

#[test]
fn the_shortcut_matches_the_general_form_outside_the_corner_square() {
    let mut checked = 0;
    for n in [1.084_f32, 1.5, 2.0, 3.0] {
        for r in [4.0_f32, 12.0, 36.0, 72.0] {
            for xi in -80..=80 {
                for yi in -80..=80 {
                    // q is the offset from the box corner: negative is inside.
                    let q = (xi as f32 * 3.0, yi as f32 * 3.0);
                    let qa = (q.0 + r, q.1 + r);
                    if qa.0 > 0.0 && qa.1 > 0.0 {
                        continue;
                    }
                    let with = convex(q, r, n, true);
                    let without = convex(q, r, n, false);
                    assert!(
                        (with - without).abs() < 1e-3,
                        "n {n} r {r} q {q:?}: shortcut {with}, general {without}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 100_000);
}
