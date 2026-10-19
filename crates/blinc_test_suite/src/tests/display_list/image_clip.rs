//! Image clips evaluated from encoder rows, independently of their GPU payload.

use super::notch::{rounded, smooth};
use super::tests::{at, draw_records_at, test_image};
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
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect::<Vec<_>>()
    };
}

fn row(record: &[f32], index: usize) -> [f32; 4] {
    record[index * 4..index * 4 + 4].try_into().unwrap()
}

pub(super) fn shaped(p: [f32; 2], bounds: [f32; 4], radius: [f32; 4], n: f32) -> f32 {
    if n == 1.0 {
        return rounded(p, bounds, radius);
    }
    assert_eq!(n, 0.0, "this fixture uses bevel corners");
    let x = p[0] - bounds[0] - bounds[2] * 0.5;
    let y = p[1] - bounds[1] - bounds[3] * 0.5;
    let corner = match (x > 0.0, y > 0.0) {
        (false, false) => 0,
        (true, false) => 1,
        (true, true) => 2,
        (false, true) => 3,
    };
    let r = radius[corner].min(bounds[2].min(bounds[3]) * 0.5);
    let (qx, qy) = (x.abs() - bounds[2] * 0.5, y.abs() - bounds[3] * 0.5);
    let box_distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0);
    box_distance.max((qx + qy + r) * std::f32::consts::FRAC_1_SQRT_2)
}

pub(super) fn sample(pixel: [f32; 2], record: &[f32], list: &[f32], scale: f32) -> [f32; 4] {
    let bounds = row(record, 0);
    let [a, b, c, d] = row(record, 15);
    let det = a * d - b * c;
    let (dx, dy) = (pixel[0] - bounds[0], pixel[1] - bounds[1]);
    let p = [(d * dx - c * dy) / det, (a * dy - b * dx) / det];
    if p[0] < 0.0 || p[1] < 0.0 || p[0] >= bounds[2] || p[1] >= bounds[3] {
        return [0.0; 4];
    }
    let types = row(record, 11);
    let mut alpha = row(record, 3)[3];
    if types[2] as u32 & 1 != 0 {
        alpha *= 1.0
            - smooth(
                -0.75,
                0.75,
                shaped(pixel, row(record, 8), row(record, 9), types[3]) * scale,
            );
    }
    if types[2] as u32 & 2 != 0 {
        let aa = if [a, b, c, d] == [1.0, 0.0, 0.0, 1.0] {
            0.75 / scale
        } else {
            0.25 * (d.hypot(b) + c.hypot(a)) / (det.abs() * scale)
        };
        alpha *= 1.0 - smooth(-aa, aa, shaped(p, row(record, 6), row(record, 7), types[3]));
    }
    let fade_rect = row(record, 20);
    let fade = row(record, 21);
    for (amount, distance) in [
        (fade[0], pixel[1] - fade_rect[1]),
        (fade[1], fade_rect[0] + fade_rect[2] - pixel[0]),
        (fade[2], fade_rect[1] + fade_rect[3] - pixel[1]),
        (fade[3], pixel[0] - fade_rect[0]),
    ] {
        if amount > 0.0 {
            alpha *= (distance / amount).clamp(0.0, 1.0);
        }
    }
    alpha *= super::shape::coverage(pixel, record, list, scale);
    if alpha < 0.001 {
        alpha = 0.0;
    }
    // Linear sampling between the last red texel and the first green texel.
    let green = (p[0] / bounds[2] * 40.0 - 19.5).clamp(0.0, 1.0);
    [1.0 - green, green * 200.0 / 255.0, 0.0, alpha]
}

#[test]
#[ignore = "requires a GPU adapter"]
fn image_clips_follow_affines_fades_shapes_and_layer_origins() {
    let harness = TestHarness::new().expect("GPU harness");
    let image = test_image(&harness);
    for (name, list, count) in [
        ("image_local", fixture!("image_local"), 1),
        ("image_shape_affine", fixture!("image_shape_affine"), 1),
        ("image_cut_fade", fixture!("image_cut_fade"), 1),
        ("image_shape_ellipse", fixture!("image_shape_ellipse"), 1),
        ("image_shape_rect", fixture!("image_shape_rect"), 1),
        ("image_shape_polygon", fixture!("image_shape_polygon"), 1),
        (
            "image_shape_hole_even",
            fixture!("image_shape_hole_even"),
            1,
        ),
        (
            "image_shape_hole_nonzero",
            fixture!("image_shape_hole_nonzero"),
            1,
        ),
        ("image_corner", fixture!("image_corner"), 1),
        ("image_clip_layer", fixture!("image_clip_layer"), 3),
        ("image_clip_nested", fixture!("image_clip_nested"), 5),
        ("image_clip_pair", fixture!("image_clip_pair"), 2),
    ] {
        for scale in [1.0f32, 2.0] {
            let label = format!("{name}_x{scale}");
            let (page, missing) =
                draw_records_at(&harness, &label, &list, count, scale, Some(&image), &[]);
            assert!(missing.is_empty());
            let (mut worst, mut covered, mut empty, mut skipped) = (0, 0, 0, 0);
            let mut worst_at = (0, 0);
            for y in 0..page.height() {
                for x in 0..page.width() {
                    let pixel = [(x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale];
                    if list[..count * RECORD_FLOATS]
                        .chunks_exact(RECORD_FLOATS)
                        .any(|r| {
                            row(r, 11)[0] == 32.0
                                && super::shape::ambiguous_probe(pixel, r, &list, scale)
                        })
                    {
                        skipped += 1;
                        continue;
                    }
                    let mut stack = vec![[0.0f32; 4]];
                    for record in list[..count * RECORD_FLOATS].chunks_exact(RECORD_FLOATS) {
                        let color = match row(record, 11)[0] {
                            40.0 => {
                                stack.push([0.0; 4]);
                                continue;
                            }
                            41.0 => {
                                let mut color = stack.pop().unwrap();
                                // Fixtures use a brightness filter and group opacity.
                                for ch in &mut color[..3] {
                                    *ch *= 0.8 * 0.6;
                                }
                                color[3] *= 0.6;
                                color
                            }
                            32.0 => {
                                let mut color = sample(pixel, record, &list, scale);
                                for ch in 0..3 {
                                    color[ch] *= color[3];
                                }
                                color
                            }
                            other => panic!("unexpected kind {other}"),
                        };
                        let dst = stack.last_mut().unwrap();
                        for ch in 0..4 {
                            dst[ch] = color[ch] + dst[ch] * (1.0 - color[3]);
                        }
                    }
                    assert_eq!(stack.len(), 1);
                    let color = stack[0];
                    covered += (color[3] > 0.15) as usize;
                    empty += (color[3] == 0.0) as usize;
                    let got = at(&page, x, y);
                    for ch in 0..3 {
                        let want = ((color[ch] + 1.0 - color[3]) * 255.0).round() as i32;
                        let error = (got[ch] as i32 - want).abs();
                        if error > worst {
                            worst = error;
                            worst_at = (x, y);
                        }
                    }
                }
            }
            println!(
                "{label}: worst {worst} at {worst_at:?}, {covered} covered, {empty} empty, {skipped} boundary probes skipped"
            );
            assert!(skipped < 200, "{label}: too many ambiguous probes");
            assert!(
                covered > 100 && empty > 1000,
                "{label}: fixture must exercise both sides of a clip"
            );
            assert!(worst <= 3, "{label}: off by {worst}");
        }
    }
}
