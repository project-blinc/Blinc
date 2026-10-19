//! Canvas callbacks against independent record-space clipping and compositing.

use super::notch::smooth;
use crate::harness::TestHarness;
use blinc_gpu::GpuImage;
use blinc_gpu::display_list::canvas::{CanvasFrame, CanvasPipelineDescriptor};
use blinc_gpu::display_list::plan::{Painter, plan};
use blinc_gpu::display_list::{DISPLAY_LIST_VERSION, RECORD_FLOATS};
use std::sync::{Arc, Mutex};
use wgpu::util::DeviceExt;

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

fn sample(pixel: [f32; 2], record: &[f32], list: &[f32], ratio: f32) -> [f32; 4] {
    let bounds = row(record, 0);
    let [a, b, c, d] = row(record, 15);
    let det = a * d - b * c;
    let (dx, dy) = (pixel[0] - bounds[0], pixel[1] - bounds[1]);
    let local = [(d * dx - c * dy) / det, (a * dy - b * dx) / det];
    if local[0] < 0.0 || local[1] < 0.0 || local[0] >= bounds[2] || local[1] >= bounds[3] {
        return [0.0; 4];
    }
    let mut color = if row(record, 10)[0] == 8.0 {
        [0.8, 0.2, 0.4, 0.6]
    } else {
        [
            0.15 + 0.5 * (local[0] / bounds[2]).clamp(0.0, 1.0),
            0.25 + 0.4 * (local[1] / bounds[3]).clamp(0.0, 1.0),
            0.7,
            0.65,
        ]
    };
    let types = row(record, 11);
    let mut cover = 1.0;
    if types[2] as u32 & 1 != 0 {
        let distance = super::image_clip::shaped(pixel, row(record, 8), row(record, 9), types[3]);
        cover *= 1.0 - smooth(-0.75, 0.75, distance);
    }
    if types[2] as u32 & 2 != 0 {
        let aa = 0.25 * (d.hypot(b) + c.hypot(a)) / (det.abs() * ratio);
        cover *= 1.0
            - smooth(
                -aa,
                aa,
                super::image_clip::shaped(local, row(record, 6), row(record, 7), types[3]),
            );
    }
    let b = row(record, 20);
    let f = row(record, 21);
    for (amount, distance) in [
        (f[0], pixel[1] - b[1]),
        (f[1], b[0] + b[2] - pixel[0]),
        (f[2], b[1] + b[3] - pixel[1]),
        (f[3], pixel[0] - b[0]),
    ] {
        if amount > 0.0 {
            cover *= (distance / amount).clamp(0.0, 1.0);
        }
    }
    cover *= super::shape::coverage(pixel, record, list, ratio);
    color[3] *= if cover < 0.001 {
        0.0
    } else {
        cover * row(record, 3)[3]
    };
    color
}

fn blend(dst: &mut [f32; 4], straight: [f32; 4]) {
    for ch in 0..3 {
        dst[ch] = quant(straight[ch] * straight[3] + dst[ch] * (1.0 - straight[3]));
    }
    dst[3] = quant(straight[3] + dst[3] * (1.0 - straight[3]));
}

fn reference(pixel: [f32; 2], list: &[f32], count: usize, scale: f32) -> [f32; 4] {
    let mut stack = vec![[0.0; 4]];
    for record in list[..count * RECORD_FLOATS].chunks_exact(RECORD_FLOATS) {
        let color = match row(record, 11)[0] {
            40.0 => {
                stack.push([0.0; 4]);
                continue;
            }
            41.0 => {
                let src = stack.pop().unwrap();
                let matrix = [row(record, 3), row(record, 4), row(record, 5)];
                let unpremult = if src[3] > 1e-4 {
                    [src[0] / src[3], src[1] / src[3], src[2] / src[3], 1.0]
                } else {
                    [0.0, 0.0, 0.0, 1.0]
                };
                let c = matrix.map(|r| {
                    (0..4)
                        .map(|i| r[i] * unpremult[i])
                        .sum::<f32>()
                        .clamp(0.0, 1.0)
                });
                [c[0], c[1], c[2], src[3] * row(record, 2)[3]]
            }
            33.0 => sample(pixel, record, list, scale),
            0.0 | 32.0 => {
                let b = row(record, 0);
                if pixel[0] < b[0]
                    || pixel[1] < b[1]
                    || pixel[0] >= b[0] + b[2]
                    || pixel[1] >= b[1] + b[3]
                {
                    continue;
                }
                if row(record, 11)[0] == 32.0 {
                    [0.4, 0.6, 0.2, 0.8]
                } else {
                    row(record, 2)
                }
            }
            kind => panic!("unexpected canvas oracle kind {kind}"),
        };
        blend(stack.last_mut().unwrap(), color);
    }
    assert_eq!(stack.len(), 1);
    stack[0]
}

const GRADIENT: &str = r#"
@fragment fn fs_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = clamp(canvas_local(p.xy) / canvas.record[0].zw, vec2<f32>(0.0), vec2<f32>(1.0));
    return canvas_clip(p.xy, vec4<f32>(0.15 + 0.5 * uv.x, 0.25 + 0.4 * uv.y, 0.7, 0.65));
}
"#;

#[derive(Clone, Debug)]
struct Seen {
    slot: u32,
    size: [f32; 2],
    transform: [f32; 6],
    origin: [f32; 2],
    target: (u32, u32),
    ratio: f32,
    scale: f32,
}

fn register(harness: &TestHarness, painter: &mut Painter) -> Arc<Mutex<Vec<Seen>>> {
    let (device, queue) = harness.gpu();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let shader = format!(
        "{GRADIENT}\n@vertex fn custom_vertex(@location(0) uv: vec2<f32>) -> @builtin(position) vec4<f32> {{ return canvas_position(uv * canvas.record[0].zw); }}"
    );
    let layouts = [wgpu::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
    }];
    let mut descriptor = CanvasPipelineDescriptor::new(&shader);
    descriptor.vertex_entry = "custom_vertex";
    descriptor.buffers = &layouts;
    let pipeline =
        painter.create_canvas_pipeline(device, wgpu::TextureFormat::Rgba8Unorm, &descriptor, &[]);
    let vertices: [[f32; 2]; 6] = [
        [0.0, 0.0],
        [1.0, 0.0],
        [0.0, 1.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    ];
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Retained canvas vertices"),
        contents: &vertices
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let events = seen.clone();
    painter.set_canvas(7, move |frame: &mut CanvasFrame<'_>| {
        events.lock().unwrap().push(Seen {
            slot: frame.slot,
            size: frame.size,
            transform: frame.transform,
            origin: frame.target_origin,
            target: frame.target_size,
            ratio: frame.pixel_ratio,
            scale: frame.scale,
        });
        let mut pass = frame.begin_pass(&pipeline);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..3, 0..1);
        pass.draw(3..6, 0..1);
    });
    let texture = GpuImage::from_rgba(device, queue, &[204, 51, 102, 153], 1, 1, None);
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Cached canvas image"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(texture.view()),
        }],
    });
    let shader = r#"
@group(1) @binding(0) var cached: texture_2d<f32>;
@fragment fn fs_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return canvas_clip(p.xy, textureLoad(cached, vec2<i32>(0), 0));
}"#;
    let pipeline = painter.create_canvas_pipeline(
        device,
        wgpu::TextureFormat::Rgba8Unorm,
        &CanvasPipelineDescriptor::new(shader),
        &[&layout],
    );
    let events = seen.clone();
    painter.set_canvas(8, move |frame: &mut CanvasFrame<'_>| {
        events.lock().unwrap().push(Seen {
            slot: frame.slot,
            size: frame.size,
            transform: frame.transform,
            origin: frame.target_origin,
            target: frame.target_size,
            ratio: frame.pixel_ratio,
            scale: frame.scale,
        });
        let mut pass = frame.begin_pass(&pipeline);
        pass.set_bind_group(1, &group, &[]);
        pass.draw(0..6, 0..1);
    });
    seen
}

#[test]
#[ignore] // Requires GPU
fn canvas_callbacks_draw_clips_transforms_layers_and_cached_textures() {
    let harness = TestHarness::new().unwrap();
    let (device, queue) = harness.gpu();
    let image = GpuImage::from_rgba(device, queue, &[102, 153, 51, 204], 1, 1, None);
    let mut painter = Painter::default();
    let seen = register(&harness, &mut painter);
    macro_rules! scenes { ($($name:literal => $count:literal),+ $(,)?) => { vec![$(($name,fixture!($name,1),fixture!($name,2),$count)),+] }; }
    for (name, x1, x2, count) in scenes! {
        "canvas_pair"=>2, "canvas_plain"=>1,"canvas_local"=>1,"canvas_cut_fade"=>1,"canvas_shape_ellipse"=>1,
        "canvas_shape_affine"=>1,"canvas_shape_rect"=>1,"canvas_shape_polygon"=>1,
        "canvas_shape_hole_even"=>1,"canvas_shape_hole_nonzero"=>1,"canvas_corner"=>1,
        "canvas_clip_layer"=>3,"canvas_clip_nested"=>5,"canvas_order"=>5,
    } {
        for (scale, list) in [(1.0, x1), (2.0, x2)] {
            seen.lock().unwrap().clear();
            let label = format!("{name}_x{scale}");
            let planned = plan(&list, count, DISPLAY_LIST_VERSION, scale).unwrap();
            assert!(planned.unsupported.is_empty(), "{:?}", planned.unsupported);
            let path = harness.output_path(&label);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let size = ((200.0 * scale) as u32, (120.0 * scale) as u32);
            harness
                .render_with(size.0, size.1, &path, |renderer, target| {
                    let missing =
                        painter.render(renderer, target, size, &planned, [0.0; 4], &|slot| {
                            if slot == 9 { Some(&image) } else { None }
                        });
                    assert!(missing.is_empty());
                    assert!(painter.missing_canvases().is_empty());
                })
                .unwrap();
            let got = image::open(path).unwrap().to_rgba8();
            let (mut worst, mut skipped, mut covered) = (0, 0, 0);
            let mut worst_at = (0, 0, 0);
            for y in 0..size.1 {
                for x in 0..size.0 {
                    let pixel = [(x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale];
                    if list[..count * RECORD_FLOATS]
                        .chunks_exact(RECORD_FLOATS)
                        .any(|r| {
                            row(r, 11)[0] == 33.0
                                && super::shape::ambiguous_probe(pixel, r, &list, scale)
                        })
                    {
                        skipped += 1;
                        continue;
                    }
                    let want = reference(pixel, &list, count, scale);
                    covered += (want[3] > 0.05) as usize;
                    for (ch, expected) in want.iter().enumerate() {
                        let error = (got.get_pixel(x, y)[ch] as i32
                            - (expected * 255.0).round() as i32)
                            .abs();
                        if error > worst {
                            worst = error;
                            worst_at = (x, y, ch);
                        }
                    }
                }
            }
            println!(
                "{label}: worst {worst} at {worst_at:?}, {covered} covered, {skipped} boundary probes skipped"
            );
            assert!(covered > 100);
            assert!(skipped < 10);
            assert!(worst <= 1, "{label}: worst {worst} at {worst_at:?}");
            let events = seen.lock().unwrap();
            let records: Vec<_> = list[..count * RECORD_FLOATS]
                .chunks_exact(RECORD_FLOATS)
                .filter(|r| row(r, 11)[0] == 33.0)
                .collect();
            assert_eq!(events.len(), records.len());
            for (event, record) in events.iter().zip(records) {
                assert_eq!(event.slot, row(record, 10)[0] as u32);
                assert_eq!(event.ratio, scale);
                assert_eq!(event.scale, row(record, 10)[1]);
                assert_eq!(event.size, [row(record, 0)[2], row(record, 0)[3]]);
                for (got, want) in event.transform.iter().zip(
                    row(record, 15)
                        .into_iter()
                        .chain(row(record, 0)[..2].iter().copied()),
                ) {
                    assert!((got - want).abs() < 1e-4);
                }
                if name.contains("clip_layer") || name.contains("clip_nested") {
                    assert!(event.origin[0] > 0.0 && event.origin[1] > 0.0, "{event:?}");
                    assert!(
                        event.target.0 < size.0 || event.target.1 < size.1,
                        "canvas expanded its layer: {event:?}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn canvas_registration_reports_missing_slots_and_resets_each_frame() {
    let harness = TestHarness::new().unwrap();
    let mut painter = Painter::default();
    let seen = register(&harness, &mut painter);
    let list = fixture!("canvas_order", 1);
    let planned = plan(&list, 5, DISPLAY_LIST_VERSION, 1.0).unwrap();
    let path = harness.output_path("canvas_registration");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    assert!(painter.remove_canvas(7));
    assert!(!painter.remove_canvas(7));
    harness
        .render_with(200, 120, &path, |renderer, target| {
            assert_eq!(
                painter.render(renderer, target, (200, 120), &planned, [0.0; 4], &|_| None),
                [9]
            );
            assert_eq!(painter.missing_canvases(), [7]);
        })
        .unwrap();
    assert_eq!(seen.lock().unwrap().len(), 1);
    let replacement = Arc::new(Mutex::new(0));
    let called = replacement.clone();
    painter.set_canvas(8, move |_: &mut CanvasFrame<'_>| {
        *called.lock().unwrap() += 1;
    });
    painter.set_canvas(7, |_: &mut CanvasFrame<'_>| {});
    harness
        .render_with(200, 120, &path, |renderer, target| {
            painter.render(renderer, target, (200, 120), &planned, [0.0; 4], &|_| None);
            assert!(painter.missing_canvases().is_empty());
        })
        .unwrap();
    assert_eq!(*replacement.lock().unwrap(), 1);
    assert_eq!(seen.lock().unwrap().len(), 1);
}
