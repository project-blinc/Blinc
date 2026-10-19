//! Intrinsic image fitting and sampling from encoder rows, independently of GPU payloads.
use super::notch::smooth;
use crate::harness::TestHarness;
use blinc_gpu::GpuImage;
use blinc_gpu::display_list::image::{ImageFit, ImageResource};
use blinc_gpu::display_list::plan::{Painter, plan};
use blinc_gpu::display_list::{DISPLAY_LIST_VERSION, RECORD_FLOATS};

macro_rules! fixture {
    ($name:literal,$scale:literal) => {
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
fn pixels() -> Vec<u8> {
    (0..40)
        .flat_map(|y| {
            (0..80).flat_map(move |x| {
                [
                    ((x * 3 + y * 2) % 256) as u8,
                    if x % 8 < 4 { 204 } else { 26 },
                    if y % 8 < 4 { 51 } else { 179 },
                    (32 + (x * 7 + y * 11) % 224) as u8,
                ]
            })
        })
        .collect()
}
fn texture(data: &[u8], uv: [f32; 2]) -> [f32; 4] {
    let x = uv[0] * 80.0 - 0.5;
    let y = uv[1] * 40.0 - 0.5;
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let at = |x: i32, y: i32, ch: usize| {
        data[(y.clamp(0, 39) as usize * 80 + x.clamp(0, 79) as usize) * 4 + ch] as f32 / 255.0
    };
    std::array::from_fn(|ch| {
        (at(ix, iy, ch) * (1.0 - fx) + at(ix + 1, iy, ch) * fx) * (1.0 - fy)
            + (at(ix, iy + 1, ch) * (1.0 - fx) + at(ix + 1, iy + 1, ch) * fx) * fy
    })
}
#[derive(Clone, Copy)]
struct Source<'a> {
    natural: [f32; 2],
    legacy: ImageFit,
    mask: bool,
    data: &'a [u8],
}
fn sample(pixel: [f32; 2], r: &[f32], list: &[f32], ratio: f32, source: Source<'_>) -> [f32; 4] {
    let Source {
        natural,
        legacy,
        mask,
        data,
    } = source;
    let b = row(r, 0);
    let [a, bb, c, d] = row(r, 15);
    let det = a * d - bb * c;
    let delta = [pixel[0] - b[0], pixel[1] - b[1]];
    let local = [
        (d * delta[0] - c * delta[1]) / det,
        (a * delta[1] - bb * delta[0]) / det,
    ];
    let mut alpha = super::image_clip::sample(pixel, r, list, ratio)[3];
    let mut uv = [local[0] / b[2], local[1] / b[3]];
    let mapped = row(r, 11)[1] == 3.0;
    if mapped || legacy != ImageFit::Fill {
        let fit = if mapped {
            row(r, 3)[2] as u8
        } else {
            legacy as u8
        };
        let contain = (b[2] / natural[0]).min(b[3] / natural[1]);
        let f = match fit {
            0 => (b[2] / natural[0]).max(b[3] / natural[1]),
            1 => contain,
            5 => contain.min(1.0),
            _ => 1.0,
        };
        let size = if fit == 2 {
            [b[2], b[3]]
        } else {
            [natural[0] * f, natural[1] * f]
        };
        let p = if mapped {
            row(r, 1)
        } else if fit == 3 {
            [0.0; 4]
        } else {
            [0.5, 0.5, 0.0, 0.0]
        };
        let origin = [
            (b[2] - size[0]) * p[0] + p[2],
            (b[3] - size[1]) * p[1] + p[3],
        ];
        let repeat = if mapped {
            row(r, 3)[0] as u8
        } else if fit == 3 {
            3
        } else {
            0
        };
        let aa = 0.25 * (d.hypot(bb) + c.hypot(a)) / (det.abs() * ratio);
        for axis in 0..2 {
            uv[axis] = (local[axis] - origin[axis]) / size[axis];
            if repeat & (1 << axis) != 0 {
                uv[axis] -= uv[axis].floor();
            } else {
                alpha *= 1.0
                    - smooth(
                        -aa,
                        aa,
                        (origin[axis] - local[axis]).max(local[axis] - origin[axis] - size[axis]),
                    );
            }
            let half = 0.5 / if axis == 0 { 80.0 } else { 40.0 };
            uv[axis] = uv[axis].clamp(half, 1.0 - half);
        }
    }
    let texel = texture(data, uv);
    if mask {
        let tint = row(r, 2);
        [
            tint[0],
            tint[1],
            tint[2],
            texel[3] * alpha * tint[3] / row(r, 3)[3],
        ]
    } else {
        [texel[0], texel[1], texel[2], texel[3] * alpha]
    }
}
fn reference(
    pixel: [f32; 2],
    list: &[f32],
    count: usize,
    ratio: f32,
    source: Source<'_>,
) -> [f32; 4] {
    let mut stack = vec![[0.0f32; 4]];
    for r in list[..count * RECORD_FLOATS].chunks_exact(RECORD_FLOATS) {
        let color = match row(r, 11)[0] {
            40.0 => {
                stack.push([0.0; 4]);
                continue;
            }
            41.0 => {
                let src = stack.pop().unwrap();
                let mut color = [0.0; 4];
                if src[3] > 0.0 {
                    for ch in 0..3 {
                        color[ch] = src[ch] / src[3] * 0.8;
                    }
                }
                color[3] = src[3] * 0.6;
                color
            }
            32.0 => sample(pixel, r, list, ratio, source),
            other => panic!("unexpected kind {other}"),
        };
        let dst = stack.last_mut().unwrap();
        for ch in 0..3 {
            dst[ch] = quant(color[ch] * color[3] + dst[ch] * (1.0 - color[3]));
        }
        dst[3] = quant(color[3] + dst[3] * (1.0 - color[3]));
    }
    assert_eq!(stack.len(), 1);
    stack[0]
}
#[test]
#[ignore] // Requires GPU
fn image_mappings_match_intrinsic_fitting_masks_and_background_brushes() {
    let harness = TestHarness::new().unwrap();
    let (device, queue) = harness.gpu();
    let data = pixels();
    let image = GpuImage::from_rgba(device, queue, &data, 80, 40, None);
    let mut painter = Painter::default();
    macro_rules! scenes {($($name:literal=>$count:literal),+ $(,)?)=>{vec![$(($name,fixture!($name,1),fixture!($name,2),$count)),+]};}
    for (name, x1, x2, count) in scenes! {
        "mapping_pair"=>2,"mapping_tile_clip_affine"=>1,"mapping_bg_repeat_x_layer"=>5,
        "mapping_cover"=>1,"mapping_contain"=>1,"mapping_fill"=>1,"mapping_none"=>1,"mapping_down"=>1,"mapping_tile"=>1,
        "mapping_contain_affine"=>1,"mapping_tile_clip"=>1,"mapping_none_layer"=>5,"mapping_mask"=>1,"mapping_legacy"=>1,
        "mapping_bg_cover"=>1,"mapping_bg_contain"=>1,"mapping_bg_repeat_x"=>1,"mapping_bg_repeat_y"=>1,"mapping_bg_repeat_both"=>1,
    } {
        let mut variants = vec![([80.0, 40.0], ImageFit::Fill, name.contains("mask"))];
        if name == "mapping_down" || name == "mapping_none" {
            variants.extend([
                ([20.0, 12.0], ImageFit::Fill, false),
                ([240.0, 180.0], ImageFit::Fill, false),
            ]);
        }
        if name == "mapping_legacy" {
            variants.extend(
                [
                    ImageFit::Cover,
                    ImageFit::Contain,
                    ImageFit::Tile,
                    ImageFit::None,
                    ImageFit::ScaleDown,
                ]
                .map(|fit| ([80.0, 40.0], fit, false)),
            );
        }
        if name == "mapping_tile_clip" {
            variants.push(([80.0, 40.0], ImageFit::Fill, true));
        }
        for (ratio, list) in [(1.0, x1), (2.0, x2)] {
            let planned = plan(&list, count, DISPLAY_LIST_VERSION, ratio).unwrap();
            assert!(
                planned.unsupported.is_empty(),
                "{name}: {:?}",
                planned.unsupported
            );
            for (variant, (natural, legacy, mask)) in variants.iter().copied().enumerate() {
                let label = format!("{name}_x{ratio}_{variant}");
                let path = harness.output_path(&label);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                let size = ((200.0 * ratio) as u32, (120.0 * ratio) as u32);
                harness
                    .render_with(size.0, size.1, &path, |renderer, target| {
                        assert!(
                            painter
                                .render_with_images(
                                    renderer,
                                    target,
                                    size,
                                    &planned,
                                    [0.0; 4],
                                    &|slot| if slot == 7 {
                                        Some(ImageResource {
                                            image: &image,
                                            natural_size: natural,
                                            fit: legacy,
                                            mask,
                                        })
                                    } else {
                                        None
                                    }
                                )
                                .is_empty()
                        );
                    })
                    .unwrap();
                let got = image::open(path).unwrap().to_rgba8();
                let (mut worst, mut covered) = (0, 0);
                let mut worst_at = (0, 0, 0);
                for y in 0..size.1 {
                    for x in 0..size.0 {
                        let pixel = [(x as f32 + 0.5) / ratio, (y as f32 + 0.5) / ratio];
                        let want = reference(
                            pixel,
                            &list,
                            count,
                            ratio,
                            Source {
                                natural,
                                legacy,
                                mask,
                                data: &data,
                            },
                        );
                        covered += (want[3] > 0.01) as usize;
                        for (ch, v) in want.iter().enumerate() {
                            let error =
                                (got.get_pixel(x, y)[ch] as i32 - (v * 255.0).round() as i32).abs();
                            if error > worst {
                                worst = error;
                                worst_at = (x, y, ch);
                            }
                        }
                    }
                }
                println!("{label}: worst {worst} at {worst_at:?}, {covered} covered");
                assert!(covered > 10, "{label}: fixture has no coverage");
                assert!(worst <= 1, "{label}: worst {worst} at {worst_at:?}");
            }
        }
    }
}
