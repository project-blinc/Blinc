//! Glyph sampling and compositing from producer records, without native payloads.
use super::image_clip::shaped;
use super::notch::smooth;
use crate::harness::TestHarness;
use blinc_gpu::display_list::atlas::{AtlasTexture, AtlasUpdate};
use blinc_gpu::display_list::plan::{Painter, plan};
use blinc_gpu::display_list::{DISPLAY_LIST_VERSION, RECORD_FLOATS};

macro_rules! fixture {
    ($name:literal, $scale:literal, $ext:literal) => {
        include_bytes!(concat!(
            "../../../../blinc_gpu/tests/fixtures/display_list/",
            $name,
            "_x",
            $scale,
            ".",
            $ext
        ))
    };
}
fn row(r: &[f32], i: usize) -> [f32; 4] {
    r[i * 4..i * 4 + 4].try_into().unwrap()
}
fn quant(v: f32) -> f32 {
    (v.clamp(0.0, 1.0) * 255.0).round() / 255.0
}
struct Atlas {
    w: usize,
    h: usize,
    channels: usize,
    bytes: Vec<u8>,
}
impl Atlas {
    fn read(file: &[u8], channels: usize) -> Self {
        let head: Vec<_> = file[..24]
            .chunks_exact(4)
            .map(|v| u32::from_le_bytes(v.try_into().unwrap()) as usize)
            .collect();
        let [w, h, x, y, cw, ch] = head.try_into().unwrap();
        assert_eq!(file.len(), 24 + cw * ch * channels);
        let mut bytes = vec![0; w * h * channels];
        for dy in 0..ch {
            let at = ((y + dy) * w + x) * channels;
            bytes[at..at + cw * channels]
                .copy_from_slice(&file[24 + dy * cw * channels..24 + (dy + 1) * cw * channels]);
        }
        Self {
            w,
            h,
            channels,
            bytes,
        }
    }
    fn upload(&self, harness: &TestHarness, texture: &mut AtlasTexture) {
        let (device, queue) = harness.gpu();
        texture
            .update(
                device,
                queue,
                &AtlasUpdate {
                    width: self.w as u32,
                    height: self.h as u32,
                    x: 0,
                    y: 0,
                    update_width: self.w as u32,
                    update_height: self.h as u32,
                    bytes: &self.bytes,
                },
            )
            .unwrap();
    }
    fn sample(&self, p: [f32; 2]) -> [f32; 4] {
        let [x, y] = p.map(|v| v - 0.5);
        let (ix, iy) = (x.floor() as i32, y.floor() as i32);
        let (fx, fy) = (x - x.floor(), y - y.floor());
        let at = |x: i32, y: i32, ch: usize| {
            let v = self.bytes[(y.clamp(0, self.h as i32 - 1) as usize * self.w
                + x.clamp(0, self.w as i32 - 1) as usize)
                * self.channels
                + ch] as f32
                / 255.0;
            if self.channels == 4 && ch < 3 {
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            } else {
                v
            }
        };
        std::array::from_fn(|ch| {
            if ch >= self.channels {
                return 0.0;
            }
            (at(ix, iy, ch) * (1.0 - fx) + at(ix + 1, iy, ch) * fx) * (1.0 - fy)
                + (at(ix, iy + 1, ch) * (1.0 - fx) + at(ix + 1, iy + 1, ch) * fx) * fy
        })
    }
}
fn glyph(pixel: [f32; 2], r: &[f32], list: &[f32], scale: f32, atlases: &[Atlas; 2]) -> [f32; 4] {
    let b = row(r, 0);
    let [a, bb, c, d] = row(r, 15);
    let det = a * d - bb * c;
    let dx = pixel[0] - b[0];
    let dy = pixel[1] - b[1];
    let local = [(d * dx - c * dy) / det, (a * dy - bb * dx) / det];
    if local[0] < 0.0 || local[1] < 0.0 || local[0] >= b[2] || local[1] >= b[3] {
        return [0.0; 4];
    }
    let types = row(r, 11);
    let mut clip = 1.0;
    if types[2] as u32 & 1 != 0 {
        let rect = row(r, 8);
        clip *= 1.0
            - smooth(
                -0.75,
                0.75,
                shaped(pixel, rect, row(r, 9), types[3]) * scale,
            );
        clip *= smooth(
            0.0,
            1.0,
            (pixel[0] - rect[0])
                .min(rect[0] + rect[2] - pixel[0])
                .min(pixel[1] - rect[1])
                .min(rect[1] + rect[3] - pixel[1])
                * scale,
        );
    }
    if types[2] as u32 & 2 != 0 {
        let unit = if bb.abs() < 1e-6 && c.abs() < 1e-6 {
            1.0
        } else {
            a.hypot(bb).max(c.hypot(d))
        };
        clip *= 1.0
            - smooth(
                -0.75,
                0.75,
                shaped(local, row(r, 6), row(r, 7), types[3]) * scale * unit,
            );
    }
    let fade_rect = row(r, 20);
    let fade = row(r, 21);
    for (amount, distance) in [
        (fade[0], pixel[1] - fade_rect[1]),
        (fade[1], fade_rect[0] + fade_rect[2] - pixel[0]),
        (fade[2], fade_rect[1] + fade_rect[3] - pixel[1]),
        (fade[3], pixel[0] - fade_rect[0]),
    ] {
        if amount > 0.0 {
            clip *= (distance / amount).clamp(0.0, 1.0);
        }
    }
    clip *= super::shape::coverage(pixel, r, list, scale);
    if clip < 0.001 {
        return [0.0; 4];
    }
    let uv = row(r, 10);
    let at = [
        uv[0] + local[0] / b[2] * (uv[2] - uv[0]),
        uv[1] + local[1] / b[3] * (uv[3] - uv[1]),
    ];
    let color = row(r, 2);
    match types[1] as u32 {
        0 => [
            color[0],
            color[1],
            color[2],
            color[3] * atlases[0].sample(at)[0].powf(0.7) * clip,
        ],
        1 => {
            let p = atlases[1].sample(at);
            [p[0], p[1], p[2], p[3] * color[3] * clip]
        }
        2 => [
            color[0],
            color[1],
            color[2],
            color[3] * atlases[1].sample(at)[3] * clip,
        ],
        _ => panic!("unknown glyph mode"),
    }
}
fn blend(dst: &mut [f32; 4], src: [f32; 4]) {
    for ch in 0..3 {
        dst[ch] = quant(src[ch] * src[3] + dst[ch] * (1.0 - src[3]));
    }
    dst[3] = quant(src[3] + dst[3] * (1.0 - src[3]));
}
fn blur(frame: &[[f32; 4]], w: usize, h: usize, sigma: f32) -> Vec<[f32; 4]> {
    let reach = (3.0 * sigma).ceil() as i32;
    let weights: Vec<_> = (-reach..=reach + 1)
        .map(|i| {
            (
                i,
                (-(i as f64).powi(2) / (2.0 * (sigma as f64).powi(2))).exp(),
            )
        })
        .collect();
    let total: f64 = weights.iter().map(|v| v.1).sum();
    let mut from = frame.to_vec();
    for vertical in [false, true] {
        let mut to = vec![[0.0; 4]; w * h];
        for y in 0..h {
            for x in 0..w {
                for &(offset, weight) in &weights {
                    let (sx, sy) = if vertical {
                        (x, (y as i32 + offset).clamp(0, h as i32 - 1) as usize)
                    } else {
                        ((x as i32 + offset).clamp(0, w as i32 - 1) as usize, y)
                    };
                    for ch in 0..4 {
                        to[y * w + x][ch] += from[sy * w + sx][ch] * (weight / total) as f32;
                    }
                }
                if !vertical {
                    to[y * w + x] = to[y * w + x].map(quant);
                }
            }
        }
        from = to;
    }
    from
}
fn reference(list: &[f32], scale: f32, atlases: &[Atlas; 2], clear: [f32; 4]) -> Vec<[f32; 4]> {
    let (w, h) = ((200.0 * scale) as usize, (120.0 * scale) as usize);
    let mut stack = Vec::new();
    let mut frame = vec![clear; w * h];
    for r in list.chunks_exact(RECORD_FLOATS) {
        match row(r, 11)[0] as u32 {
            40 => {
                stack.push(frame);
                frame = vec![[0.0; 4]; w * h];
            }
            41 => {
                let sigma = row(r, 2)[0];
                let inner = if sigma > 0.0 {
                    blur(&frame, w, h, sigma)
                } else {
                    frame
                };
                frame = stack.pop().unwrap();
                let matrix = [row(r, 3), row(r, 4), row(r, 5)];
                for (at, p) in inner.into_iter().enumerate() {
                    let rgb = if p[3] > 1e-4 {
                        [p[0] / p[3], p[1] / p[3], p[2] / p[3], 1.0]
                    } else {
                        [0.0, 0.0, 0.0, 1.0]
                    };
                    let c: [f32; 3] = matrix.map(|m| {
                        (0..4)
                            .map(|ch| m[ch] * rgb[ch])
                            .sum::<f32>()
                            .clamp(0.0, 1.0)
                    });
                    blend(&mut frame[at], [c[0], c[1], c[2], p[3] * row(r, 2)[3]]);
                }
            }
            7 => {
                for (at, dst) in frame.iter_mut().enumerate() {
                    let pixel = [
                        ((at % w) as f32 + 0.5) / scale,
                        ((at / w) as f32 + 0.5) / scale,
                    ];
                    blend(dst, glyph(pixel, r, list, scale, atlases));
                }
            }
            kind => panic!("unexpected record kind {kind}"),
        }
    }
    assert!(stack.is_empty());
    frame
}

#[test]
#[ignore] // Requires GPU
fn colour_glyphs_and_their_shadows_match_the_producers_atlases() {
    let harness = TestHarness::new().unwrap();
    let mut painter = Painter::default();
    macro_rules! cases {
        ($($name:literal),*) => { [$((
            $name,
            [(1.0, fixture!($name, "1", "f32").as_slice(), fixture!($name, "1", "atlas").as_slice(), fixture!($name, "1", "rgba").as_slice()),
             (2.0, fixture!($name, "2", "f32").as_slice(), fixture!($name, "2", "atlas").as_slice(), fixture!($name, "2", "rgba").as_slice())]
        )),*] };
    }
    for (name, cases) in cases!(
        "colour_plain",
        "colour_opacity",
        "colour_shadow",
        "colour_clip",
        "colour_transform",
        "colour_layer",
        "colour_blur"
    ) {
        for (scale, bytes, mask_bytes, color_bytes) in cases {
            let list: Vec<_> = bytes
                .chunks_exact(4)
                .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
                .collect();
            let atlases = [Atlas::read(mask_bytes, 1), Atlas::read(color_bytes, 4)];
            let (device, _) = harness.gpu();
            let mut mask = AtlasTexture::new(device, false);
            let mut color = AtlasTexture::new(device, true);
            atlases[0].upload(&harness, &mut mask);
            atlases[1].upload(&harness, &mut color);
            harness.set_glyph_atlas(mask.view(), color.view());
            let clear = if name == "colour_plain" {
                [0.2, 0.4, 0.6, 1.0]
            } else {
                [0.0; 4]
            };
            check(
                &harness,
                &mut painter,
                &format!("{name}_x{scale}"),
                &list,
                scale,
                &atlases,
                clear,
            );
        }
    }
}

fn check_page(name: &str, got: &image::RgbaImage, want: Vec<[f32; 4]>) {
    let size = got.dimensions();
    let (mut worst, mut worst_at, mut covered) = (0, (0, 0, 0), 0);
    for (i, (pixel, expected)) in got.pixels().zip(want).enumerate() {
        if expected[3] > 0.0 {
            covered += 1;
        }
        for ch in 0..4 {
            let delta = (pixel[ch] as i32 - (expected[ch] * 255.0).round() as i32).abs();
            if delta > worst {
                worst = delta;
                worst_at = (i % size.0 as usize, i / size.0 as usize, ch);
            }
        }
    }
    println!("{name}: worst {worst}/255 at {worst_at:?}, {covered} covered");
    assert!(covered > 100);
    // Fractional rotated sampling accumulates texture-filter and blend quantization.
    let tolerance = if name.starts_with("colour_transform") {
        4
    } else {
        1
    };
    assert!(worst <= tolerance, "{name}: {worst} at {worst_at:?}");
}
#[allow(clippy::too_many_arguments)]
fn check(
    harness: &TestHarness,
    painter: &mut Painter,
    name: &str,
    list: &[f32],
    scale: f32,
    atlases: &[Atlas; 2],
    clear: [f64; 4],
) {
    let planned = plan(
        list,
        list.len() / RECORD_FLOATS,
        DISPLAY_LIST_VERSION,
        scale,
    )
    .unwrap();
    assert!(
        planned.unsupported.is_empty(),
        "{name}: {:?}",
        planned.unsupported
    );
    let path = harness.output_path(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let size = ((200.0 * scale) as u32, (120.0 * scale) as u32);
    harness
        .render_with(size.0, size.1, &path, |renderer, target| {
            assert!(
                painter
                    .render(renderer, target, size, &planned, clear, &|_| None)
                    .is_empty()
            );
        })
        .unwrap();
    let want = reference(list, scale, atlases, clear.map(|v| v as f32));
    let got = image::open(path).unwrap().to_rgba8();
    check_page(name, &got, want);
}

#[test]
#[ignore] // Requires GPU
fn colour_atlas_partial_updates_and_growth_keep_pixel_rectangles() {
    let harness = TestHarness::new().unwrap();
    let mut painter = Painter::default();
    let list: Vec<_> = fixture!("colour_shadow", "1", "f32")
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    let mut atlases = [
        Atlas::read(fixture!("colour_shadow", "1", "atlas"), 1),
        Atlas::read(fixture!("colour_shadow", "1", "rgba"), 4),
    ];
    let (device, queue) = harness.gpu();
    let mut mask = AtlasTexture::new(device, false);
    let mut color = AtlasTexture::new(device, true);
    atlases[0].upload(&harness, &mut mask);
    atlases[1].upload(&harness, &mut color);
    harness.set_glyph_atlas(mask.view(), color.view());
    check(
        &harness,
        &mut painter,
        "colour_update_full",
        &list,
        1.0,
        &atlases,
        [0.0; 4],
    );
    let mut patch = Vec::new();
    for y in 0..16 {
        for x in 0..19 {
            let rgba = [40 + x as u8 * 9, 180, 30, 32 + y as u8 * 13];
            patch.extend_from_slice(&rgba);
            let at = ((y + 5) * atlases[1].w + x + 4) * 4;
            atlases[1].bytes[at..at + 4].copy_from_slice(&rgba);
        }
    }
    color
        .update(
            device,
            queue,
            &AtlasUpdate {
                width: atlases[1].w as u32,
                height: atlases[1].h as u32,
                x: 4,
                y: 5,
                update_width: 19,
                update_height: 16,
                bytes: &patch,
            },
        )
        .unwrap();
    check(
        &harness,
        &mut painter,
        "colour_update_partial",
        &list,
        1.0,
        &atlases,
        [0.0; 4],
    );
    let old_width = atlases[1].w;
    let old_height = atlases[1].h;
    let mut bytes = vec![0; old_width * old_height * 16];
    for y in 0..old_height {
        bytes[y * old_width * 8..y * old_width * 8 + old_width * 4]
            .copy_from_slice(&atlases[1].bytes[y * old_width * 4..(y + 1) * old_width * 4]);
    }
    atlases[1].w *= 2;
    atlases[1].h *= 2;
    atlases[1].bytes = bytes;
    atlases[1].upload(&harness, &mut color);
    harness.set_glyph_atlas(mask.view(), color.view());
    check(
        &harness,
        &mut painter,
        "colour_update_growth",
        &list,
        1.0,
        &atlases,
        [0.0; 4],
    );
}

#[test]
#[ignore] // Requires GPU
fn native_mask_and_colour_glyph_modes_keep_their_pixels() {
    use blinc_gpu::{GpuGlyph, GpuPrimitive, PrimitiveBatch};
    let harness = TestHarness::with_config(crate::harness::TestHarnessConfig {
        sample_count: 1,
        ..Default::default()
    })
    .unwrap();
    let mut list: Vec<_> = fixture!("colour_opacity", "1", "f32")
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    let atlases = [
        Atlas::read(fixture!("colour_opacity", "1", "atlas"), 1),
        Atlas::read(fixture!("colour_opacity", "1", "rgba"), 4),
    ];
    let (device, _) = harness.gpu();
    let mut mask = AtlasTexture::new(device, false);
    let mut color = AtlasTexture::new(device, true);
    atlases[0].upload(&harness, &mut mask);
    atlases[1].upload(&harness, &mut color);
    harness.set_glyph_atlas(mask.view(), color.view());
    let mut batch = PrimitiveBatch::new();
    for r in list.chunks_exact(RECORD_FLOATS) {
        let mode = row(r, 11)[1];
        let glyph = GpuGlyph {
            bounds: row(r, 0),
            uv_bounds: row(r, 10),
            color: row(r, 2),
            flags: [mode, 0.0, 0.0, 0.0],
            ..Default::default()
        };
        let primitive = GpuPrimitive::from_glyph(&glyph);
        assert_eq!(primitive.type_info[1], mode as u32);
        batch.primitives.push(primitive);
    }
    // Native colour mode uses atlas alpha alone; native mask mode uses text alpha.
    for r in list.chunks_exact_mut(RECORD_FLOATS) {
        if r[45] == 1.0 {
            r[11] = 1.0;
        }
    }
    let path = harness.output_path("colour_native_modes");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    harness.render_to_png(&batch, 200, 120, &path).unwrap();
    check_page(
        "colour_native_modes",
        &image::open(path).unwrap().to_rgba8(),
        reference(&list, 1.0, &atlases, [1.0; 4]),
    );
}
