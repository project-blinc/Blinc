//! ABI notch geometry and shading, evaluated independently on the CPU.

use super::tests::{at, draw_records_at};
use crate::harness::TestHarness;
use blinc_gpu::display_list::RECORD_FLOATS;

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!(
            "../../../../blinc_gpu/tests/fixtures/display_list/",
            $name,
            ".f32"
        ))
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect::<Vec<_>>()
    };
}

fn row(record: &[f32], index: usize) -> [f32; 4] {
    record[index * 4..index * 4 + 4].try_into().unwrap()
}

pub(super) fn rounded(p: [f32; 2], bounds: [f32; 4], radii: [f32; 4]) -> f32 {
    let x = p[0] - bounds[0] - bounds[2] * 0.5;
    let y = p[1] - bounds[1] - bounds[3] * 0.5;
    let corner = match (x > 0.0, y > 0.0) {
        (false, false) => 0,
        (true, false) => 1,
        (true, true) => 2,
        (false, true) => 3,
    };
    let radius = radii[corner].min(bounds[2].min(bounds[3]) * 0.5);
    let qx = x.abs() - bounds[2] * 0.5 + radius;
    let qy = y.abs() - bounds[3] * 0.5 + radius;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

fn triangle(p: [f32; 2], vertices: [[f32; 2]; 3]) -> f32 {
    let mut distance = f32::INFINITY;
    let mut signs = [0.0; 3];
    for i in 0..3 {
        let (a, b) = (vertices[i], vertices[(i + 1) % 3]);
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let (px, py) = (p[0] - a[0], p[1] - a[1]);
        let t = ((px * ex + py * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        distance = distance.min((px - ex * t).hypot(py - ey * t));
        signs[i] = ex * py - ey * px;
    }
    if signs.iter().all(|s| *s >= 0.0) || signs.iter().all(|s| *s <= 0.0) {
        -distance
    } else {
        distance
    }
}

fn union(a: f32, b: f32, radius: f32) -> f32 {
    a.min(b) - (radius - (a - b).abs()).max(0.0).powi(2) / (4.0 * radius)
}

fn edge(p: [f32; 2], d: f32, cx: f32, base: f32, modifier: [f32; 4], dir: f32, into: f32) -> f32 {
    let [kind, width, depth, radius] = modifier;
    if kind <= 0.5 || width <= 0.001 || depth <= 0.001 {
        return d;
    }
    let q = [p[0], (p[1] - base) * dir];
    let half = width * 0.5;
    if kind < 1.5 {
        let r = half.min(depth);
        let cy = depth - r;
        let disk = ((q[0] - cx).hypot(q[1] - cy) - r).max(cy - q[1]);
        let hollow = if cy > 0.001 {
            disk.min(rounded(q, [cx - half, 0.0, width, cy], [0.0; 4]))
        } else {
            disk
        };
        return -union(-d, hollow, radius.max(0.001));
    }
    if kind < 2.5 {
        let r = (half * half + depth * depth) / (2.0 * depth).max(0.001);
        let side = if q[1] < 0.0 {
            -1e5
        } else {
            (q[0] - cx).abs() - half
        };
        let cap = ((q[0] - cx).hypot(q[1] - (r - depth)) - r)
            .max(q[1] - into)
            .max(side);
        return union(d, cap, radius.max(0.001));
    }
    if kind < 3.5 {
        return -union(
            -d,
            triangle(q, [[cx - half, 0.0], [cx, depth], [cx + half, 0.0]]),
            1.5,
        );
    }
    let spread = half * (depth + into) / depth;
    let peak = triangle(q, [[cx - spread, into], [cx, -depth], [cx + spread, into]])
        .max((q[0] - cx).abs() - half);
    union(d, peak, 1.5)
}

pub(super) fn distance(p: [f32; 2], record: &[f32]) -> f32 {
    let bounds = row(record, 0);
    let corners = row(record, 25);
    let top = row(record, 26);
    let bottom = row(record, 27);
    let r = corners.map(f32::abs);
    let offsets = corners.map(|v| (-v).max(0.0));
    let protrudes = |m: [f32; 4]| {
        if (m[0] - 2.0).abs() < 0.5 || (m[0] - 4.0).abs() < 0.5 {
            m[2]
        } else {
            0.0
        }
    };
    let left = offsets[0].max(offsets[3]);
    let right = offsets[1].max(offsets[2]);
    let y0 = offsets[0].max(offsets[1]).max(protrudes(top));
    let y1 = offsets[3].max(offsets[2]).max(protrudes(bottom));
    let body = [
        left,
        y0,
        (bounds[2] - left - right).max(0.001),
        (bounds[3] - y0 - y1).max(0.001),
    ];
    let radii = std::array::from_fn(|i| if corners[i] < 0.0 { 0.0 } else { r[i] });
    let mut d = rounded(p, body, radii);
    let end = [body[0] + body[2], body[1] + body[3]];
    let into = body[2].min(body[3]) * 0.5;
    let buried = into.min(8.0);
    let room = (bounds[3] - y0 - y1).max(0.0);
    for i in 0..4 {
        if corners[i] >= 0.0 {
            continue;
        }
        let ry = r[i].min(room);
        let right = i == 1 || i == 2;
        let bottom = i >= 2;
        let width = if right { bounds[2] - end[0] } else { left };
        let x = if right { end[0] - buried } else { 0.0 };
        let y = if bottom { end[1] - ry } else { y0 };
        let cx = if right { bounds[2] } else { 0.0 };
        let cy = if bottom { end[1] - ry } else { y0 + ry };
        let ellipse = (((p[0] - cx) / width.max(0.001)).hypot((p[1] - cy) / ry.max(0.001)) - 1.0)
            * width.min(ry);
        let flare = rounded(p, [x, y, width + buried, ry], [0.0; 4]).max(-ellipse);
        d = d.min(flare);
    }
    d = edge(p, d, bounds[2] * 0.5, y0, top, 1.0, into);
    edge(p, d, bounds[2] * 0.5, end[1], bottom, -1.0, into)
}

pub(super) fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// Simpson integration of the normal density, independent of the shader's erf approximation.
fn normal_cdf(z: f32) -> f32 {
    let limit = z.abs().min(8.0);
    let h = limit / 64.0;
    let density = |x: f32| (-0.5 * x * x).exp() / (2.0 * std::f32::consts::PI).sqrt();
    let mut sum = density(0.0) + density(limit);
    for i in 1..64 {
        sum += (if i % 2 == 0 { 2.0 } else { 4.0 }) * density(i as f32 * h);
    }
    0.5 + z.signum() * h * sum / 3.0
}

pub(super) fn fill(p: [f32; 2], record: &[f32], list: &[f32]) -> [f32; 4] {
    let kind = row(record, 11)[1];
    if kind == 0.0 {
        return row(record, 2);
    }
    let g = row(record, 10);
    let t = if kind == 3.0 {
        ((p[0] - g[0]).atan2(-(p[1] - g[1])) - g[2]).rem_euclid(std::f32::consts::TAU)
            / std::f32::consts::TAU
    } else if kind == 2.0 {
        (p[0] - g[0]).hypot(p[1] - g[1]) / g[2]
    } else {
        ((p[0] - g[0]) * (g[2] - g[0]) + (p[1] - g[1]) * (g[3] - g[1]))
            / ((g[2] - g[0]).powi(2) + (g[3] - g[1]).powi(2))
    };
    let marker = row(record, 14)[3];
    let stops: Vec<([f32; 4], f32)> = if marker >= 2.0 {
        let start = (marker - 2.0) as usize * 4;
        (0..list[start] as usize)
            .map(|i| {
                (
                    list[start + 8 + i * 8..start + 12 + i * 8]
                        .try_into()
                        .unwrap(),
                    list[start + 12 + i * 8],
                )
            })
            .collect()
    } else {
        vec![(row(record, 2), 0.0), (row(record, 3), 1.0)]
    };
    if t <= stops[0].1 {
        return stops[0].0;
    }
    for pair in stops.windows(2) {
        let [(a, x), (b, y)] = pair else {
            unreachable!()
        };
        if t < *y {
            let t = ((t - x) / (y - x)).clamp(0.0, 1.0);
            let alpha = a[3] * (1.0 - t) + b[3] * t;
            return std::array::from_fn(|i| {
                if i == 3 {
                    alpha
                } else {
                    (a[i] * a[3] * (1.0 - t) + b[i] * b[3] * t) / alpha
                }
            });
        }
    }
    stops.last().unwrap().0
}

#[test]
#[ignore] // Requires GPU
fn abi_notches_match_the_producers_geometry_and_shading() {
    let harness = TestHarness::new().unwrap();
    for (name, list, count, layer) in [
        ("notch_corners", fixture!("notch_corners"), 1, false),
        ("notch_scoop", fixture!("notch_scoop"), 1, false),
        ("notch_bulge", fixture!("notch_bulge"), 1, false),
        ("notch_v", fixture!("notch_v"), 1, false),
        ("notch_short", fixture!("notch_short"), 1, false),
        ("notch_border", fixture!("notch_border"), 1, false),
        ("notch_gradient", fixture!("notch_gradient"), 1, false),
        ("notch_layer", fixture!("notch_layer"), 3, true),
        ("notch_shadow", fixture!("notch_shadow"), 1, false),
        ("notch_inset", fixture!("notch_inset"), 1, false),
        ("notch_affine", fixture!("notch_affine"), 1, false),
        ("notch_clip", fixture!("notch_clip"), 1, false),
    ] {
        let record = &list[if layer { RECORD_FLOATS } else { 0 }..][..RECORD_FLOATS];
        let bounds = row(record, 0);
        let [a, b, c, d] = row(record, 15);
        let det = a * d - b * c;
        for scale in [1.0, 2.0] {
            let label = format!("{name}_x{scale}");
            let (page, _) = draw_records_at(&harness, &label, &list, count, scale, None, &[]);
            let aa = 0.25 * (d.hypot(b) + c.hypot(a)) / (det.abs() * scale);
            let (mut worst, mut covered, mut empty) = (0i32, 0usize, 0usize);
            for y in 0..page.height() {
                for x in 0..page.width() {
                    let pixel = [(x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale];
                    let dx = pixel[0] - bounds[0];
                    let dy = pixel[1] - bounds[1];
                    let p = [(d * dx - c * dy) / det, (a * dy - b * dx) / det];
                    let dist = distance(p, record);
                    let flags = row(record, 11)[2] as u32;
                    let mut clip = 1.0;
                    let mut near_clip = false;
                    for (flag, q, rect, radii) in [
                        (1, pixel, row(record, 8), row(record, 9)),
                        (2, p, row(record, 6), row(record, 7)),
                    ] {
                        if flags & flag == 0 {
                            continue;
                        }
                        let cd = rounded(q, rect, radii);
                        near_clip |= cd.abs() < 1.5 / scale;
                        if cd > 0.0 {
                            clip = 0.0;
                        }
                    }
                    if near_clip {
                        continue;
                    }
                    let fade = row(record, 21);
                    let rect = row(record, 20);
                    for (distance, width) in [
                        pixel[1] - rect[1],
                        rect[0] + rect[2] - pixel[0],
                        rect[1] + rect[3] - pixel[1],
                        pixel[0] - rect[0],
                    ]
                    .into_iter()
                    .zip(fade)
                    {
                        if width > 0.0 {
                            clip *= (distance / width).clamp(0.0, 1.0);
                        }
                    }
                    let mut color = if row(record, 11)[0] == 3.0 {
                        let shadow = row(record, 6);
                        let inset = row(record, 11)[1] > 0.5;
                        let shifted = distance([p[0] - shadow[0], p[1] - shadow[1]], record);
                        let shade_d = if inset {
                            shifted + shadow[3]
                        } else {
                            shadow[3] - shifted
                        };
                        let shade = if shadow[2] < 0.001 {
                            (0.5 + shade_d).clamp(0.0, 1.0)
                        } else {
                            normal_cdf(2.0 * shade_d / shadow[2])
                        };
                        let mut color = row(record, 7).map(|v| v * shade);
                        let outside = smooth(-0.75, 0.75, dist);
                        color[3] *= if inset { 1.0 - outside } else { outside };
                        color
                    } else {
                        let mut color = fill(p, record, &list);
                        let border = row(record, 4)[0];
                        if border > 0.0 {
                            let edge = row(record, 16);
                            let amount = smooth(-aa, aa, dist + border) * edge[3];
                            color = std::array::from_fn(|i| {
                                color[i] * (1.0 - amount) + edge[i] * amount
                            });
                        }
                        color[3] *= 1.0 - smooth(-aa, aa, dist);
                        color
                    };
                    color[3] *= clip;
                    if layer {
                        for channel in &mut color[..3] {
                            *channel *= 0.8;
                        }
                        color[3] *= 0.5;
                    }
                    covered += (color[3] > 0.2) as usize;
                    empty += (color[3] < 0.001) as usize;
                    let got = at(&page, x, y);
                    for ch in 0..3 {
                        let want = (255.0 * (1.0 - color[3] + color[ch] * color[3])).round() as i32;
                        worst = worst.max((got[ch] as i32 - want).abs());
                    }
                }
            }
            println!("{label}: worst {worst}, {covered} covered and {empty} empty pixels");
            assert!(worst <= 3, "{label}: off by {worst}");
            assert!(
                covered > 50 && empty > 1000,
                "{label}: insufficient coverage"
            );
        }
    }
}
