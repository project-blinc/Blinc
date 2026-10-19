//! Backdrop pixels from raw encoder rows, with full-frame CPU sampling.

use super::notch::{distance as notch_distance, rounded, smooth};
use crate::harness::TestHarness;
use blinc_gpu::GpuImage;
use blinc_gpu::display_list::plan::{Painter, plan, render};
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
fn unit(v: [f32; 2]) -> [f32; 2] {
    let n = v[0].hypot(v[1]).max(1e-4);
    [v[0] / n, v[1] / n]
}
fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}
fn line(x: f32, w: f32) -> f32 {
    smooth(0.0, w * 0.3, x) * (1.0 - smooth(w, w * 1.5, x))
}
fn lens(x: f32, w: f32) -> f32 {
    (1.0 - (x / w).clamp(0.0, 1.0)).powi(2)
}
fn environment(p: [f32; 2]) -> f32 {
    let t = p[0] * 0.55 + p[1] * 0.83;
    0.25 + (0.5 + 0.5 * (7.0 * t + 0.8).sin()).powi(2) * 0.55
        + (0.5 + 0.5 * (18.0 * t - 1.2).sin()).powi(6) * 0.2
}
fn grain(p: [f32; 2]) -> f32 {
    let hash = |x: f32, y: f32| {
        let n = (x * 127.1 + y * 311.7).sin() * 43758.547;
        n - n.floor()
    };
    let (x, y) = (p[0].floor(), p[1].floor());
    let u = p.map(|n| {
        let f = n - n.floor();
        f * f * (3.0 - 2.0 * f)
    });
    let a = hash(x, y) * (1.0 - u[0]) + hash(x + 1.0, y) * u[0];
    let b = hash(x, y + 1.0) * (1.0 - u[0]) + hash(x + 1.0, y + 1.0) * u[0];
    a * (1.0 - u[1]) + b * u[1]
}

#[derive(Clone)]
struct Frame {
    w: usize,
    h: usize,
    pixels: Vec<[f32; 4]>,
}
impl Frame {
    fn new(w: usize, h: usize, clear: [f32; 4]) -> Self {
        Self {
            w,
            h,
            pixels: vec![clear; w * h],
        }
    }
    fn sample(&self, p: [f32; 2]) -> [f32; 4] {
        let (x, y) = (p[0] - 0.5, p[1] - 0.5);
        let (ix, iy) = (x.floor() as i32, y.floor() as i32);
        let (fx, fy) = (x - x.floor(), y - y.floor());
        let at = |x: i32, y: i32| {
            self.pixels[y.clamp(0, self.h as i32 - 1) as usize * self.w
                + x.clamp(0, self.w as i32 - 1) as usize]
        };
        let (a, b, c, d) = (
            at(ix, iy),
            at(ix + 1, iy),
            at(ix, iy + 1),
            at(ix + 1, iy + 1),
        );
        std::array::from_fn(|ch| {
            (a[ch] * (1.0 - fx) + b[ch] * fx) * (1.0 - fy) + (c[ch] * (1.0 - fx) + d[ch] * fx) * fy
        })
    }
    fn blend(&mut self, at: usize, straight: [f32; 4]) {
        let a = straight[3];
        for ch in 0..3 {
            self.pixels[at][ch] = quant(straight[ch] * a + self.pixels[at][ch] * (1.0 - a));
        }
        self.pixels[at][3] = quant(a + self.pixels[at][3] * (1.0 - a));
    }
}

fn pattern() -> Frame {
    let mut image = Frame::new(200, 120, [0.0; 4]);
    for y in 0..120 {
        for x in 0..200 {
            image.pixels[y * 200 + x] = [
                ((x * 7 + y * 3) % 251) as f32 / 255.0,
                if (x / 12 + y / 9) % 2 == 0 { 0.8 } else { 0.1 },
                ((x * 3 + y * 5) % 256) as f32 / 255.0,
                if x < 110 && y < 85 { 0.6 } else { 1.0 },
            ]
            .map(quant);
        }
    }
    image
}

// Dense integer samples, including the final neighbour of the paired GPU kernel.
fn kernel(sigma: f32) -> Vec<(i32, f32)> {
    let sigma = (sigma as f64).max(1e-4);
    let reach = (3.0 * sigma).ceil() as i32;
    let weights: Vec<_> = (-reach..=reach + 1)
        .map(|i| (i, (-(i as f64).powi(2) / (2.0 * sigma * sigma)).exp()))
        .collect();
    let total: f64 = weights.iter().map(|v| v.1).sum();
    weights
        .into_iter()
        .map(|(i, w)| (i, (w / total) as f32))
        .collect()
}

fn box_distance(p: [f32; 2], record: &[f32]) -> f32 {
    if row(record, 25).iter().any(|v| *v != 0.0)
        || row(record, 26)[0] > 0.0
        || row(record, 27)[0] > 0.0
    {
        return notch_distance(p, record);
    }
    assert_eq!(row(record, 12), [1.0; 4]);
    let bounds = row(record, 0);
    rounded(p, [0.0, 0.0, bounds[2], bounds[3]], row(record, 1))
}

fn glass_pixel(
    pixel: [f32; 2],
    record: &[f32],
    list: &[f32],
    rows: &Frame,
    taps: &[(f32, f32)],
    scale: f32,
) -> [f32; 4] {
    let bounds = row(record, 0);
    let [a, b, c, d] = row(record, 15);
    let det = a * d - b * c;
    let (dx, dy) = (pixel[0] - bounds[0], pixel[1] - bounds[1]);
    let local = [(d * dx - c * dy) / det, (a * dy - b * dx) / det];
    let grow = [1.5 / a.hypot(b).max(0.01), 1.5 / c.hypot(d).max(0.01)];
    if local[0] < -grow[0]
        || local[1] < -grow[1]
        || local[0] > bounds[2] + grow[0]
        || local[1] > bounds[3] + grow[1]
    {
        return [0.0; 4];
    }
    let distance = box_distance(local, record);
    let aa = 0.25 * (d.hypot(b) + c.hypot(a)) / (det.abs() * scale);
    let mut cover = 1.0 - smooth(-aa, aa, distance);
    let types = row(record, 11);
    if types[2] as u32 & 1 != 0 {
        cover *= 1.0 - smooth(-0.75, 0.75, rounded(pixel, row(record, 8), row(record, 9)));
    }
    if types[2] as u32 & 2 != 0 {
        let fringe = if [a, b, c, d] == [1.0, 0.0, 0.0, 1.0] {
            0.75
        } else {
            aa
        };
        cover *= 1.0
            - smooth(
                -fringe,
                fringe,
                rounded(local, row(record, 6), row(record, 7)),
            );
    }
    let (f, rect) = (row(record, 21), row(record, 20));
    for (amount, distance) in [
        (f[0], pixel[1] - rect[1]),
        (f[1], rect[0] + rect[2] - pixel[0]),
        (f[2], rect[1] + rect[3] - pixel[1]),
        (f[3], pixel[0] - rect[0]),
    ] {
        if amount > 0.0 {
            cover *= (distance / amount).clamp(0.0, 1.0);
        }
    }
    cover *= super::shape::coverage(pixel, record, list, scale);
    if cover < 0.001 {
        return [0.0; 4];
    }
    let glass = row(record, 10);
    let liquid = glass[0] > 0.5;
    let inner = (-distance).max(0.0);
    let bevel = glass[1].abs().clamp(0.0, 1.0);
    let rim = (25.0f32.min(bounds[2].min(bounds[3]) * 0.2) * bevel).max(1e-4);
    let (mut normal, mut local_normal, mut offset, mut bend) = ([0.0; 2], [0.0; 2], [0.0; 2], 0.0);
    if liquid {
        let gradient = [
            box_distance([local[0] + 0.5, local[1]], record)
                - box_distance([local[0] - 0.5, local[1]], record),
            box_distance([local[0], local[1] + 0.5], record)
                - box_distance([local[0], local[1] - 0.5], record),
        ];
        local_normal = unit(gradient);
        normal = unit([
            a * gradient[0] + c * gradient[1],
            b * gradient[0] + d * gradient[1],
        ]);
        bend = lens(inner, rim);
        offset = normal.map(|n| n * bend * 60.0 * glass[1] * a.hypot(b) * scale);
    }
    let aberration = row(record, 14)[0];
    let dispersion = 0.12 * aberration + 0.38 * aberration.powi(3);
    let split = offset.map(|v| v * dispersion / bevel.max(1e-4));
    let mut texel = [0.0; 4];
    let (mut red, mut blue) = ([0.0; 2], [0.0; 2]);
    let disperse = dot(split, split) > 1e-6;
    for &(i, w) in taps {
        let p = [
            pixel[0] * scale + offset[0],
            pixel[1] * scale + offset[1] + i,
        ];
        let sample = rows.sample(p);
        for ch in 0..4 {
            texel[ch] += sample[ch] * w;
        }
        if disperse {
            let r = rows.sample([p[0] + split[0], p[1] + split[1]]);
            let b = rows.sample([p[0] - split[0], p[1] - split[1]]);
            red[0] += r[0] * w;
            red[1] += r[3] * w;
            blue[0] += b[2] * w;
            blue[1] += b[3] * w;
        }
    }
    if disperse {
        texel = [red[0], texel[1], blue[0], texel[3].max(red[1]).max(blue[1])];
    }
    let matrix = [row(record, 3), row(record, 4), row(record, 5)];
    let color = if texel[3] > 1e-4 {
        [
            texel[0] / texel[3],
            texel[1] / texel[3],
            texel[2] / texel[3],
            1.0,
        ]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    let mut rgb: [f32; 3] = std::array::from_fn(|ch| {
        (0..4)
            .map(|i| matrix[ch][i] * color[i])
            .sum::<f32>()
            .clamp(0.0, 1.0)
    });
    let mut tint = matrix.map(|r| r[3].clamp(0.0, 1.0));
    let mut tint_alpha = (1.0 - (matrix[0][0] + matrix[1][1] + matrix[2][2]) / 3.0).clamp(0.0, 1.0);
    if liquid {
        let width = glass[2].max(1e-4);
        let line = line(inner, width) * bevel;
        let light = [glass[3].cos(), glass[3].sin()].map(|v| if glass[1] < 0.0 { -v } else { v });
        let shift = aberration * width * 0.75;
        let lit_rgb = [
            self::line(inner + shift, width) * bevel,
            line,
            self::line(inner - shift, width) * bevel,
        ]
        .map(|v| v * 0.6 * (0.2 + 0.8 * dot(normal, light.map(|v| -v)).max(0.0)));
        let lit = lit_rgb.into_iter().fold(0.0, f32::max);
        let edge = row(record, 16);
        let shade = smooth(width * 2.5, width * 8.0, inner)
            * (1.0 - smooth(width * 8.0, width * 24.0, inner))
            * 0.04
            * bevel;
        let via = row(record, 13);
        let va = via[3].clamp(0.0, 1.0);
        if edge[3] > 0.001 {
            let k = line * edge[3];
            for ch in 0..3 {
                rgb[ch] = rgb[ch] * (1.0 - k) + edge[ch] * k;
                tint[ch] = tint[ch] * (1.0 - k) + edge[ch] * k;
            }
            tint_alpha = k + tint_alpha * (1.0 - k);
        } else {
            for ch in 0..3 {
                tint[ch] = tint[ch] * (1.0 - lit) + lit_rgb[ch];
            }
            tint_alpha = lit + tint_alpha * (1.0 - lit);
        }
        for ch in 0..3 {
            rgb[ch] = ((rgb[ch] - shade) * (1.0 - va) + via[ch] * va).clamp(0.0, 1.0);
            if edge[3] <= 0.001 {
                rgb[ch] = (rgb[ch] * (1.0 - lit) + lit_rgb[ch]).clamp(0.0, 1.0);
            }
            tint[ch] = tint[ch] * (1.0 - shade) * (1.0 - va) + via[ch] * va;
        }
        tint_alpha = va + (shade + tint_alpha * (1.0 - shade)) * (1.0 - va);
        if texel[3] < 0.999 && bend > 0.0 {
            let span = bounds[2].min(bounds[3]).max(1.0);
            let bend_local = local_normal.map(|v| v * bend * 60.0 / span * glass[1]);
            let point = [
                (local[0] - bounds[2] * 0.5) / span + bend_local[0],
                (local[1] - bounds[3] * 0.5) / span + bend_local[1],
            ];
            let separate = bend_local.map(|v| v * dispersion / bevel.max(1e-4));
            let middle = environment(point);
            let env = [
                environment([point[0] + separate[0], point[1] + separate[1]]),
                middle,
                environment([point[0] - separate[0], point[1] - separate[1]]),
            ]
            .map(|v| middle * 0.65 + v * 0.35);
            let strength = (bend * (1.0 - bend) * 4.0 * 0.18 + bend * 0.06) * bevel;
            let alpha = env.into_iter().fold(0.0, f32::max) * strength;
            for ch in 0..3 {
                tint[ch] = tint[ch] * (1.0 - alpha) + env[ch] * strength;
            }
            tint_alpha = alpha + tint_alpha * (1.0 - alpha);
        }
        let band = bend * (1.0 - bend) * 4.0 * 0.12 * bevel;
        tint = tint.map(|v| v * (1.0 - band));
        tint_alpha = band + tint_alpha * (1.0 - band);
        let spectrum = [
            lens(inner + aberration * 0.6, rim),
            bend,
            lens(inner - aberration * 0.6, rim),
        ];
        let reflected =
            0.85 * dot(normal, light.map(|v| -v)).max(0.0) + 0.35 * dot(normal, light).max(0.0);
        let spec_rgb =
            spectrum.map(|v| (bend * bend * reflected * 0.65 + v * v * reflected * 0.35) * bevel);
        let spec = spec_rgb.into_iter().fold(0.0, f32::max);
        for ch in 0..3 {
            tint[ch] = tint[ch] * (1.0 - spec) + spec_rgb[ch];
        }
        tint_alpha = (spec + tint_alpha * (1.0 - spec)).max(0.04);
    }
    let alpha = texel[3] + tint_alpha * (1.0 - texel[3]);
    if alpha > 1e-4 {
        for ch in 0..3 {
            rgb[ch] = (rgb[ch] * texel[3] + tint[ch] * (1.0 - texel[3])) / alpha;
        }
    }
    if row(record, 2)[2] > 0.0 {
        let n = (grain(pixel.map(|v| v * 0.3)) - 0.5)
            * row(record, 2)[2]
            * if liquid { 0.005 } else { 0.02 };
        rgb = rgb.map(|v| (v + n).clamp(0.0, 1.0));
    }
    [rgb[0], rgb[1], rgb[2], alpha * cover * row(record, 2)[3]]
}

fn reference(list: &[f32], count: usize, scale: f32, transparent: bool) -> Frame {
    let (w, h) = ((200.0 * scale) as usize, (120.0 * scale) as usize);
    let mut frame = Frame::new(w, h, if transparent { [0.0; 4] } else { [1.0; 4] });
    let mut stack = Vec::new();
    let image = pattern();
    for record in list[..count * RECORD_FLOATS].chunks_exact(RECORD_FLOATS) {
        match row(record, 11)[0] {
            40.0 => {
                stack.push(frame);
                frame = Frame::new(w, h, [0.0; 4]);
            }
            41.0 => {
                let inner = frame;
                frame = stack.pop().unwrap();
                let matrix = [row(record, 3), row(record, 4), row(record, 5)];
                assert_eq!(row(record, 2)[0], 0.0);
                for (at, p) in inner.pixels.iter().copied().enumerate() {
                    let rgb = if p[3] > 1e-4 {
                        [p[0] / p[3], p[1] / p[3], p[2] / p[3], 1.0]
                    } else {
                        [0.0, 0.0, 0.0, 1.0]
                    };
                    let c: [f32; 3] =
                        matrix.map(|r| (0..4).map(|i| r[i] * rgb[i]).sum::<f32>().clamp(0.0, 1.0));
                    frame.blend(at, [c[0], c[1], c[2], p[3] * row(record, 2)[3]]);
                }
            }
            32.0 => {
                for at in 0..w * h {
                    let pixel = [
                        (at % w) as f32 / scale + 0.5 / scale,
                        (at / w) as f32 / scale + 0.5 / scale,
                    ];
                    frame.blend(at, image.sample(pixel));
                }
            }
            0.0 => {
                for at in 0..w * h {
                    let pixel = [
                        ((at % w) as f32 + 0.5) / scale,
                        ((at / w) as f32 + 0.5) / scale,
                    ];
                    let b = row(record, 0);
                    let color = row(record, 2);
                    let dist = rounded(pixel, b, row(record, 1));
                    frame.blend(
                        at,
                        [
                            color[0],
                            color[1],
                            color[2],
                            color[3] * (1.0 - smooth(-0.5, 0.5, dist * scale)),
                        ],
                    );
                }
            }
            42.0 => {
                let source = frame.clone();
                let taps = kernel(row(record, 2)[0]);
                let mut rows = Frame::new(w, h, [0.0; 4]);
                for y in 0..h {
                    for x in 0..w {
                        let mut sum = [0.0; 4];
                        for &(dx, k) in &taps {
                            let sample = source.pixels
                                [y * w + (x as i32 + dx).clamp(0, w as i32 - 1) as usize];
                            for ch in 0..4 {
                                sum[ch] += sample[ch] * k;
                            }
                        }
                        rows.pixels[y * w + x] = sum.map(quant);
                    }
                }
                // The consumer offsets paired fetches by refraction before sampling.
                let vertical: Vec<_> = taps
                    .chunks_exact(2)
                    .map(|pair| {
                        let weight = pair[0].1 + pair[1].1;
                        (pair[0].0 as f32 + pair[1].1 / weight.max(1e-30), weight)
                    })
                    .collect();
                for at in 0..w * h {
                    let pixel = [
                        ((at % w) as f32 + 0.5) / scale,
                        ((at / w) as f32 + 0.5) / scale,
                    ];
                    frame.blend(
                        at,
                        glass_pixel(pixel, record, list, &rows, &vertical, scale),
                    );
                }
            }
            kind => panic!("unhandled oracle record {kind}"),
        }
    }
    assert!(stack.is_empty());
    frame
}

#[test]
#[ignore = "requires a GPU adapter"]
fn backdrops_match_full_frame_sampling_in_paint_order() {
    let harness = TestHarness::new().unwrap();
    let (device, queue) = harness.gpu();
    let image = pattern();
    let bytes: Vec<_> = image
        .pixels
        .iter()
        .flat_map(|p| p.map(|v| (v * 255.0).round() as u8))
        .collect();
    let image = GpuImage::from_rgba(device, queue, &bytes, 200, 120, Some("backdrop source"));
    macro_rules! cases { ($($name:literal => $count:literal),* $(,)?) => { [$(($name,$count,fixture!($name,"1"),fixture!($name,"2"))),*] }; }
    let mut painter = Painter::default();
    for (name, count, x1, x2) in cases! {
        "backdrop_blur"=>2,"backdrop_zero"=>2,"backdrop_wide"=>2,"backdrop_frost"=>2,
        "backdrop_liquid"=>2,"backdrop_border"=>2,"backdrop_transparent"=>2,"backdrop_affine"=>2,
        "backdrop_notch"=>2,"backdrop_clip"=>2,"backdrop_cut"=>2,"backdrop_shape"=>2,
        "backdrop_layers"=>7,"backdrop_order"=>4,"backdrop_matrix"=>2,"backdrop_flat"=>2,"backdrop_stretch"=>2,"backdrop_noline"=>2,"backdrop_offscreen"=>2,
    } {
        for (scale, list) in [(1.0, x1), (2.0, x2)] {
            let label = format!("{name}_x{scale}");
            let transparent = name == "backdrop_transparent";
            let clear = if transparent { [0.0; 4] } else { [1.0; 4] };
            let planned = plan(&list, count, DISPLAY_LIST_VERSION, scale).unwrap();
            assert!(planned.unsupported.is_empty(), "{:?}", planned.unsupported);
            let path = harness.output_path(&label);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            harness
                .render_with(
                    (200.0 * scale) as u32,
                    (120.0 * scale) as u32,
                    &path,
                    |renderer, target| {
                        let missing = painter.render(
                            renderer,
                            target,
                            ((200.0 * scale) as u32, (120.0 * scale) as u32),
                            &planned,
                            clear,
                            &|slot| if slot == 7 { Some(&image) } else { None },
                        );
                        assert!(missing.is_empty());
                    },
                )
                .unwrap();
            let got = image::open(path).unwrap().to_rgba8();
            let want = reference(&list, count, scale, transparent);
            let (mut worst, mut skipped) = (0, 0);
            let mut worst_at = (0, 0, 0);
            for y in 0..want.h {
                for x in 0..want.w {
                    let pixel = [(x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale];
                    if list[..count * RECORD_FLOATS]
                        .chunks_exact(RECORD_FLOATS)
                        .any(|r| {
                            row(r, 11)[0] == 42.0
                                && super::shape::ambiguous_probe(pixel, r, &list, scale)
                        })
                    {
                        skipped += 1;
                        continue;
                    }
                    for (ch, expected) in want.pixels[y * want.w + x].iter().enumerate() {
                        let error = (got.get_pixel(x as u32, y as u32)[ch] as i32
                            - (expected * 255.0).round() as i32)
                            .abs();
                        if error > worst {
                            worst = error;
                            worst_at = (x, y, ch);
                        }
                    }
                }
            }
            println!("{label}: worst {worst} at {worst_at:?}, {skipped} boundary probes skipped");
            assert!(skipped < 200, "{label}: too many polygon boundaries");
            if worst > 3 {
                let (x, y, _) = worst_at;
                println!(
                    "got {:?}, want {:?}",
                    got.get_pixel(x as u32, y as u32),
                    want.pixels[y * want.w + x]
                );
            }
            assert!(worst <= 3, "{label}: off by {worst}");
        }
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn backdrop_targets_only_require_render_attachment_usage() {
    let harness = TestHarness::new().unwrap();
    let (device, queue) = harness.gpu();
    let image = GpuImage::from_rgba(
        device,
        queue,
        &[180; 200 * 120 * 4],
        200,
        120,
        Some("backdrop input"),
    );
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("render-only backdrop target"),
        size: wgpu::Extent3d {
            width: 200,
            height: 120,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let list = fixture!("backdrop_liquid", "1");
    let planned = plan(&list, 2, DISPLAY_LIST_VERSION, 1.0).unwrap();
    let path = harness.output_path("backdrop_render_usage");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    harness
        .render_with(200, 120, &path, |renderer, _| {
            let missing = render(renderer, &view, (200, 120), &planned, [0.0; 4], &|_| {
                Some(&image)
            });
            assert!(missing.is_empty());
        })
        .unwrap();
    device.poll(wgpu::PollType::Wait).unwrap();
}
