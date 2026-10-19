//! Element-frame clips, evaluated from the producer's rows on the CPU.

use super::notch::{distance as notch_distance, fill, rounded, smooth};
use super::tests::{at, draw_records_at};
use crate::harness::TestHarness;
use blinc_gpu::display_list::{RECORD_FLOATS, Unsupported};

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!(
            "../../../../blinc_gpu/tests/fixtures/display_list/",
            $name,
            ".f32"
        ))
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect::<Vec<_>>()
    };
}

fn row(record: &[f32], index: usize) -> [f32; 4] {
    record[index * 4..index * 4 + 4].try_into().unwrap()
}

fn winding(p: [f32; 2], points: &[[f32; 2]]) -> i32 {
    let mut winding = 0;
    for edge in points.windows(2) {
        let [a, b] = edge else { unreachable!() };
        if a[0] >= 1e29 || b[0] >= 1e29 {
            continue;
        }
        let cross = (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1]);
        if a[1] <= p[1] && b[1] > p[1] && cross > 0.0 {
            winding += 1;
        }
        if a[1] > p[1] && b[1] <= p[1] && cross < 0.0 {
            winding -= 1;
        }
    }
    winding
}

/// Coverage in the clip's frame. Polygon contours retain their explicit closing edges.
pub(super) fn coverage(pixel: [f32; 2], record: &[f32], list: &[f32], scale: f32) -> f32 {
    let info = row(record, 23);
    if info[2] == 0.0 {
        return 1.0;
    }
    let [a, b, c, d] = row(record, 22);
    let p = [
        a * pixel[0] + c * pixel[1] + info[0],
        b * pixel[0] + d * pixel[1] + info[1],
    ];
    let aa = (0.25 * (a.hypot(b) + c.hypot(d)) / scale).max(1e-6);
    let shape = row(record, 24);
    if info[2] == 1.0 {
        let r = [shape[2].max(1e-4), shape[3].max(1e-4)];
        let u = [(p[0] - shape[0]) / r[0], (p[1] - shape[1]) / r[1]];
        let len = u[0].hypot(u[1]);
        let distance = (len - 1.0) * len / (u[0] / r[0]).hypot(u[1] / r[1]).max(1e-4);
        return 1.0 - smooth(-aa, aa, distance);
    }
    if info[2] == 2.0 {
        return 1.0 - smooth(-aa, aa, rounded(p, shape, [info[3]; 4]));
    }
    let first = shape[0] as usize * 4;
    let count = shape[1] as usize;
    let points: Vec<[f32; 2]> = (0..count)
        .map(|i| [list[first + 2 * i], list[first + 2 * i + 1]])
        .collect();
    let mut distance = f32::INFINITY;
    for edge in points.windows(2) {
        let [a, b] = edge else { unreachable!() };
        if a[0] >= 1e29 || b[0] >= 1e29 {
            continue;
        }
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let (px, py) = (p[0] - a[0], p[1] - a[1]);
        let t = ((px * ex + py * ey) / (ex * ex + ey * ey).max(1e-12)).clamp(0.0, 1.0);
        distance = distance.min((px - t * ex).hypot(py - t * ey));
    }
    let filled = |q| {
        let w = winding(q, &points);
        if shape[2] > 0.5 { w % 2 != 0 } else { w != 0 }
    };
    let s = aa * 0.7;
    let samples = [
        [p[0] + s, p[1] + s],
        [p[0] - s, p[1] + s],
        [p[0] + s, p[1] - s],
        [p[0] - s, p[1] - s],
    ];
    let inside = samples.into_iter().filter(|q| filled(*q)).count();
    if inside == 4 {
        return 1.0;
    }
    if inside == 0 {
        return 0.0;
    }
    1.0 - smooth(-aa, aa, if filled(p) { -distance } else { distance })
}

// The four-probe polygon override jumps at an edge. Compare probes away from
// numerical boundaries so interpolation rounding cannot change that override.
fn ambiguous_probe(pixel: [f32; 2], record: &[f32], list: &[f32], scale: f32) -> bool {
    if row(record, 23)[2] != 3.0 {
        return false;
    }
    let [a, b, c, d] = row(record, 22);
    let info = row(record, 23);
    let p = [
        a * pixel[0] + c * pixel[1] + info[0],
        b * pixel[0] + d * pixel[1] + info[1],
    ];
    let s = 0.7 * 0.25 * (a.hypot(b) + c.hypot(d)) / scale;
    let shape = row(record, 24);
    let first = shape[0] as usize * 4;
    for i in 1..shape[1] as usize {
        let a = &list[first + 2 * (i - 1)..first + 2 * i];
        let b = &list[first + 2 * i..first + 2 * i + 2];
        if a[0] >= 1e29 || b[0] >= 1e29 {
            continue;
        }
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        for (dx, dy) in [(s, s), (-s, s), (s, -s), (-s, -s)] {
            let (px, py) = (p[0] + dx - a[0], p[1] + dy - a[1]);
            let t = ((px * ex + py * ey) / (ex * ex + ey * ey).max(1e-12)).clamp(0.0, 1.0);
            if (px - t * ex).hypot(py - t * ey) < 0.001 {
                return true;
            }
        }
    }
    false
}

#[test]
#[ignore] // Requires GPU
fn shape_clips_stay_in_their_elements_frame_through_affines_and_layers() {
    let harness = TestHarness::new().unwrap();
    for (name, list, count, layer, flatten) in [
        ("shape_ellipse", fixture!("shape_ellipse"), 1, false, false),
        ("shape_rect", fixture!("shape_rect"), 1, false, false),
        ("shape_polygon", fixture!("shape_polygon"), 1, false, false),
        (
            "shape_hole_even",
            fixture!("shape_hole_even"),
            1,
            false,
            false,
        ),
        (
            "shape_hole_nonzero",
            fixture!("shape_hole_nonzero"),
            1,
            false,
            false,
        ),
        ("shape_affine", fixture!("shape_affine"), 1, false, false),
        (
            "shape_inherited",
            fixture!("shape_inherited"),
            2,
            false,
            false,
        ),
        ("shape_shadow", fixture!("shape_shadow"), 1, false, false),
        ("shape_notch", fixture!("shape_notch"), 1, false, false),
        ("shape_layer", fixture!("shape_layer"), 3, true, false),
        ("shape_flatten", fixture!("shape_flatten"), 5, false, true),
    ] {
        for scale in [1.0f32, 2.0] {
            let label = format!("{name}_x{scale}");
            let (page, _) = draw_records_at(
                &harness,
                &label,
                &list,
                count,
                scale,
                None,
                if flatten {
                    &[Unsupported::LayerEffect]
                } else {
                    &[]
                },
            );
            let (mut worst, mut covered, mut empty) = (0i32, 0usize, 0usize);
            let mut worst_at = (0, 0);
            let mut skipped = 0;
            for y in 0..page.height() {
                for x in 0..page.width() {
                    let pixel = [(x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale];
                    if list[..count * RECORD_FLOATS]
                        .chunks_exact(RECORD_FLOATS)
                        .any(|r| ambiguous_probe(pixel, r, &list, scale))
                    {
                        skipped += 1;
                        continue;
                    }
                    let mut want = [1.0f32; 3];
                    let mut any_alpha: f32 = 0.0;
                    for record in list[..count * RECORD_FLOATS].chunks_exact(RECORD_FLOATS) {
                        let kind = row(record, 11)[0];
                        if kind != 0.0 && kind != 3.0 {
                            continue;
                        }
                        let bounds = row(record, 0);
                        let [a, b, c, d] = row(record, 15);
                        let det = a * d - b * c;
                        let (dx, dy) = (pixel[0] - bounds[0], pixel[1] - bounds[1]);
                        let p = [(d * dx - c * dy) / det, (a * dy - b * dx) / det];
                        let unit = scale * det.abs().sqrt();
                        let notched = row(record, 25).iter().any(|v| *v != 0.0)
                            || row(record, 26)[0] > 0.0
                            || row(record, 27)[0] > 0.0;
                        let body = [0.0, 0.0, bounds[2], bounds[3]];
                        let dist = if notched {
                            notch_distance(p, record)
                        } else {
                            rounded(p, body, row(record, 1))
                        };
                        let mut color = if kind == 3.0 {
                            let shadow = row(record, 6);
                            assert_eq!(shadow[2], 0.0);
                            let grown = [
                                shadow[0] - shadow[3],
                                shadow[1] - shadow[3],
                                bounds[2] + 2.0 * shadow[3],
                                bounds[3] + 2.0 * shadow[3],
                            ];
                            let shade = (0.5 - rounded(p, grown, [0.0; 4]) * unit).clamp(0.0, 1.0);
                            let mut color = row(record, 7).map(|v| v * shade);
                            color[3] *= smooth(-0.75, 0.75, dist * unit);
                            color
                        } else {
                            let aa = if notched {
                                0.25 * (d.hypot(b) + c.hypot(a)) / (det.abs() * scale)
                            } else {
                                0.5 / unit
                            };
                            let mut color = fill(p, record, &list);
                            color[3] *= 1.0 - smooth(-aa, aa, dist);
                            color
                        };
                        color[3] *= coverage(pixel, record, &list, scale);
                        if layer {
                            for ch in &mut color[..3] {
                                *ch *= 0.8;
                            }
                            color[3] *= 0.5;
                        }
                        any_alpha = any_alpha.max(color[3]);
                        for ch in 0..3 {
                            want[ch] = color[ch] * color[3] + want[ch] * (1.0 - color[3]);
                        }
                    }
                    let got = at(&page, x, y);
                    for ch in 0..3 {
                        let error = (got[ch] as i32 - (want[ch] * 255.0).round() as i32).abs();
                        if error > worst {
                            worst = error;
                            worst_at = (x, y);
                        }
                    }
                    covered += (any_alpha > 0.2) as usize;
                    empty += (any_alpha < 0.001) as usize;
                }
            }
            println!(
                "{label}: worst {worst} at {worst_at:?}, {covered} covered and {empty} empty pixels, {skipped} boundary probes skipped"
            );
            assert!(
                skipped < 200,
                "{label}: too many ambiguous probes: {skipped}"
            );
            assert!(worst <= 3, "{label}: off by {worst}");
            assert!(
                covered > 50 && empty > 1000,
                "{label}: insufficient coverage"
            );
        }
    }
}
