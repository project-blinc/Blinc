//! Layer-mask parameters and alpha from producer rows and trailing stop tables.
use crate::harness::TestHarness;
use blinc_gpu::GpuImage;
use blinc_gpu::display_list::plan::{Painter, plan};
use blinc_gpu::display_list::{DISPLAY_LIST_VERSION, RECORD_FLOATS};

macro_rules! fixture {
    ($name:literal, $scale:literal) => {
        include_bytes!(concat!(
            "../../../../blinc_gpu/tests/fixtures/display_list/",
            $name,
            "_x",
            $scale,
            ".f32"
        ))
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect::<Vec<_>>()
    };
}
fn row(r: &[f32], i: usize) -> [f32; 4] {
    r[i * 4..i * 4 + 4].try_into().unwrap()
}
fn quant(v: f32) -> f32 {
    (v.clamp(0.0, 1.0) * 255.0).round() / 255.0
}
fn alpha(pixel: [f32; 2], r: &[f32], list: &[f32]) -> f32 {
    let info = row(r, 16);
    if info[0] == 0.0 {
        return 1.0;
    }
    let frame = row(r, 18);
    let [a, b, c, d] = row(r, 19);
    let translate = row(r, 1);
    let p = [pixel[0] - translate[0], pixel[1] - translate[1]];
    let determinant = a * d - b * c;
    let uv = [
        ((d * p[0] - c * p[1]) / determinant - frame[0]) / frame[2],
        ((a * p[1] - b * p[0]) / determinant - frame[1]) / frame[3],
    ];
    let g = row(r, 17);
    let mut t = if info[0] == 1.0 {
        let dir = [g[2] - g[0], g[3] - g[1]];
        ((uv[0] - g[0]) * dir[0] + (uv[1] - g[1]) * dir[1])
            / (dir[0] * dir[0] + dir[1] * dir[1]).max(1e-6)
    } else if info[0] == 2.0 {
        (uv[0] - g[0]).hypot(uv[1] - g[1]) / g[2].max(1e-4)
    } else {
        let delta = [(uv[0] - g[0]) * frame[2], (uv[1] - g[1]) * frame[3]];
        ((delta[0].atan2(-delta[1]) - g[2]) / std::f32::consts::TAU).rem_euclid(1.0)
    };
    let at = (row(r, 14)[3] - 2.0) as usize * 4;
    let count = list[at] as usize;
    let stops: Vec<_> = list[at + 8..at + 8 + count * 8]
        .chunks_exact(8)
        .map(|v| (v[4], v[3]))
        .collect();
    if list[at + 1] != 0.0 {
        let start = stops[0].0;
        let period = stops[count - 1].0 - start;
        if period <= 1e-4 {
            return stops[0].1;
        }
        let phase = (t - start) / period;
        let fold = if list[at + 1] == 1.0 {
            phase.rem_euclid(1.0)
        } else {
            1.0 - (2.0 * (phase * 0.5).rem_euclid(1.0) - 1.0).abs()
        };
        t = start + fold * period;
    }
    let mut previous = stops[0];
    for next in &stops[1..] {
        if t < next.0 {
            return previous.1
                + (next.1 - previous.1)
                    * ((t - previous.0) / (next.0 - previous.0).max(1e-20)).clamp(0.0, 1.0);
        }
        previous = *next;
    }
    previous.1
}
fn reference(pixel: [f32; 2], list: &[f32], count: usize, scale: f32) -> [f32; 4] {
    let mut stack = vec![[0.0f32; 4]];
    for r in list[..count * RECORD_FLOATS].chunks_exact(RECORD_FLOATS) {
        let src = match row(r, 11)[0] as u32 {
            40 => {
                stack.push([0.0; 4]);
                continue;
            }
            41 => {
                let p = stack.pop().unwrap();
                let color = if p[3] > 1e-4 {
                    [p[0] / p[3], p[1] / p[3], p[2] / p[3], 1.0]
                } else {
                    [0.0, 0.0, 0.0, 1.0]
                };
                let matrix = [row(r, 3), row(r, 4), row(r, 5)];
                let c: [f32; 3] =
                    matrix.map(|m| (0..4).map(|i| m[i] * color[i]).sum::<f32>().clamp(0.0, 1.0));
                [
                    c[0],
                    c[1],
                    c[2],
                    p[3] * row(r, 2)[3] * alpha(pixel, r, list),
                ]
            }
            32 => {
                let coverage = super::image_clip::sample(pixel, r, list, scale)[3];
                [
                    204.0 / 255.0,
                    51.0 / 255.0,
                    153.0 / 255.0,
                    coverage * 191.0 / 255.0,
                ]
            }
            other => panic!("unexpected kind {other}"),
        };
        let dst = stack.last_mut().unwrap();
        for ch in 0..3 {
            dst[ch] = quant(src[ch] * src[3] + dst[ch] * (1.0 - src[3]));
        }
        dst[3] = quant(src[3] + dst[3] * (1.0 - src[3]));
    }
    assert_eq!(stack.len(), 1);
    stack[0]
}

#[test]
#[ignore] // Requires GPU
fn large_layer_masks_sample_all_stops_and_reuse_their_table() {
    let harness = TestHarness::new().unwrap();
    let (device, queue) = harness.gpu();
    let image = GpuImage::from_rgba(device, queue, &[204, 51, 153, 191], 1, 1, None);
    let mut painter = Painter::default();
    macro_rules! cases { ($($name:literal),*) => { [$(($name, fixture!($name, "1"), fixture!($name, "2"))),*] }; }
    // Cross both table/uniform and capacity transitions repeatedly on one Painter.
    for (name, x1, x2) in cases!(
        "mask_many_16",
        "mask_many_17",
        "mask_many_33",
        "mask_many_rows",
        "mask_many_17",
        "mask_many_16",
        "mask_many_repeat",
        "mask_many_reflect",
        "mask_many_hard",
        "mask_many_radial",
        "mask_many_conic",
        "mask_many_affine_nested"
    ) {
        for (scale, list) in [(1.0, x1), (2.0, x2)] {
            let count = if name.contains("nested") { 5 } else { 3 };
            let planned = plan(&list, count, DISPLAY_LIST_VERSION, scale).unwrap();
            assert!(
                planned.unsupported.is_empty(),
                "{name}: {:?}",
                planned.unsupported
            );
            let label = format!("{name}_x{scale}");
            let path = harness.output_path(&label);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let size = ((200.0 * scale) as u32, (120.0 * scale) as u32);
            harness
                .render_with(size.0, size.1, &path, |renderer, target| {
                    assert!(
                        painter
                            .render(renderer, target, size, &planned, [0.0; 4], &|slot| (slot
                                == 7)
                                .then_some(&image))
                            .is_empty()
                    );
                })
                .unwrap();
            let got = image::open(path).unwrap().to_rgba8();
            let (mut worst, mut covered, mut worst_at) = (0, 0, (0, 0, 0));
            for y in 0..size.1 {
                for x in 0..size.0 {
                    let pixel = [(x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale];
                    let expected = reference(pixel, &list, count, scale);
                    covered += (expected[3] > 0.0) as usize;
                    for (ch, expected) in expected.into_iter().enumerate() {
                        let delta = (got.get_pixel(x, y)[ch] as i32
                            - (expected * 255.0).round() as i32)
                            .abs();
                        if delta > worst {
                            worst = delta;
                            worst_at = (x, y, ch);
                        }
                    }
                }
            }
            println!("{label}: worst {worst}/255 at {worst_at:?}, {covered} covered");
            assert!(covered > 500);
            assert!(worst <= 1, "{label}: {worst} at {worst_at:?}");
        }
    }
}
