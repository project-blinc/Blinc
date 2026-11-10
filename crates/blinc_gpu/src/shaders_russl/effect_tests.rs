//! The layer effect shaders draw what the inline WGSL they replaced drew: the same inputs go
//! through them and the pixels are compared with goldens recorded from that WGSL.

use super::harness::{Input, Pass, SIZE, floats, gradient_bytes, need_device, render, texture};

/// A uniform buffer's words: floats and unsigned integers in field order.
#[derive(Copy, Clone)]
enum W {
    F(f32),
    U(u32),
}

fn words(values: &[W]) -> Vec<u8> {
    let raw: Vec<u32> = values
        .iter()
        .map(|w| match w {
            W::F(f) => f.to_bits(),
            W::U(u) => *u,
        })
        .collect();
    bytemuck::cast_slice(&raw).to_vec()
}

/// An RGBA8 texture of an opaque disc over a transparent background, with a colour gradient inside,
/// so effects that read alpha and distance have an edge to work on.
fn disc_bytes(seed: u32) -> Vec<u8> {
    let mut bytes = gradient_bytes(seed);
    let c = SIZE as f32 / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
            let at = ((y * SIZE + x) * 4) as usize;
            bytes[at + 3] = if d < 14.0 {
                255
            } else if d < 17.0 {
                128
            } else {
                0
            };
        }
    }
    bytes
}

fn upload(device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8]) -> wgpu::TextureView {
    texture(
        device,
        queue,
        SIZE,
        SIZE,
        wgpu::TextureFormat::Rgba8Unorm,
        bytes,
    )
}

fn linear(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

/// The directory of this machine's goldens, `.blinc/snapshots/blinc_gpu/<os>` at the workspace
/// root (`BLINC_SNAPSHOT_DIR` replaces `.blinc/snapshots`), as the recorder's snapshots keep
/// theirs: per machine and untracked, because adapters differ in how they round.
fn golden_dir() -> std::path::PathBuf {
    let root = std::env::var_os("BLINC_SNAPSHOT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .expect("the workspace root")
                .join(".blinc/snapshots")
        });
    root.join("blinc_gpu").join(std::env::consts::OS)
}

/// `russl` draws `pass` to what the golden `name` holds, the output of the shader it replaced:
/// every channel within two levels, on all but a few pixels. `BLINC_SNAPSHOT=bless` records the
/// golden; without goldens on this machine nothing is compared.
fn check(device: &wgpu::Device, queue: &wgpu::Queue, name: &str, russl: &str, pass: &Pass) {
    let dir = golden_dir();
    let path = dir.join(format!("{name}.png"));
    let bless = std::env::var("BLINC_SNAPSHOT").is_ok_and(|v| v == "bless");
    let recorded = std::fs::read_dir(&dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| e.path().extension().is_some_and(|x| x == "png"))
    });
    if !bless && !recorded {
        eprintln!("golden `{name}` skipped: no goldens for this platform");
        return;
    }
    let got = render(device, queue, russl, pass);
    if bless {
        std::fs::create_dir_all(&dir).unwrap();
        image::save_buffer(&path, &got, SIZE, SIZE, image::ColorType::Rgba8).unwrap();
        return;
    }
    let want = image::open(&path)
        .unwrap_or_else(|e| panic!("golden {name}: {e}"))
        .into_rgba8()
        .into_raw();
    assert_eq!(got.len(), want.len(), "the golden {name} has another size");
    let differing = got
        .as_chunks::<4>()
        .0
        .iter()
        .zip(want.as_chunks::<4>().0)
        .filter(|(g, w)| g.iter().zip(w.iter()).any(|(g, w)| g.abs_diff(*w) > 2))
        .count();
    assert!(
        differing <= 8,
        "{name}: {differing} of {} pixels differ from the golden",
        SIZE * SIZE
    );
}

#[test]
fn color_matrix_draws_what_the_inline_shader_drew() {
    let (device, queue) = need_device!();
    let input = upload(&device, &queue, &gradient_bytes(1));
    let sampler = linear(&device);
    let uniforms = floats(&[
        0.9, 0.1, 0.0, 0.0, 0.1, 0.8, 0.1, 0.0, 0.0, 0.2, 0.7, 0.0, 0.0, 0.0, 0.0, 1.0, 0.05, 0.0,
        -0.05, 0.0,
    ]);
    let pass = Pass {
        vertex: "vs_main",
        fragment: "fs_color_matrix",
        inputs: &[
            (0, Input::Uniform(&uniforms)),
            (1, Input::Texture(&input)),
            (2, Input::Sampler(&sampler)),
        ],
        vertices: 6,
    };
    check(
        &device,
        &queue,
        "color_matrix",
        super::color_matrix::WGSL,
        &pass,
    );
}

#[test]
fn mask_image_draws_what_the_inline_shader_drew_in_both_modes() {
    let (device, queue) = need_device!();
    let input = upload(&device, &queue, &gradient_bytes(2));
    let mask = upload(&device, &queue, &disc_bytes(6));
    let sampler = linear(&device);
    for mode in [0u32, 1] {
        let uniforms = words(&[
            W::U(mode),
            W::U(0),
            W::U(0),
            W::U(0),
            W::U(0),
            W::U(0),
            W::U(0),
            W::U(0),
        ]);
        let pass = Pass {
            vertex: "vs_main",
            fragment: "fs_mask",
            inputs: &[
                (0, Input::Uniform(&uniforms)),
                (1, Input::Texture(&input)),
                (2, Input::Sampler(&sampler)),
                (3, Input::Texture(&mask)),
                (4, Input::Sampler(&sampler)),
            ],
            vertices: 6,
        };
        check(
            &device,
            &queue,
            &format!("mask_image_{mode}"),
            super::mask_image::WGSL,
            &pass,
        );
    }
}

#[test]
fn composite_draws_what_the_inline_shader_drew() {
    let (device, queue) = need_device!();
    let source = upload(&device, &queue, &disc_bytes(3));
    let sampler = linear(&device);
    let uniforms = words(&[W::F(0.6), W::U(0), W::F(0.0), W::F(0.0)]);
    let pass = Pass {
        vertex: "vs_main",
        fragment: "fs_main",
        inputs: &[
            (0, Input::Uniform(&uniforms)),
            (1, Input::Texture(&source)),
            (2, Input::Sampler(&sampler)),
        ],
        vertices: 3,
    };
    check(&device, &queue, "composite", super::composite::WGSL, &pass);
}

#[test]
fn blur_draws_what_the_inline_shader_drew_in_every_pass_and_mode() {
    let (device, queue) = need_device!();
    let input = upload(&device, &queue, &disc_bytes(4));
    let sampler = linear(&device);
    let texel = 1.0 / SIZE as f32;
    let cases = [
        ("fs_kawase_blur", 6.0, 1, 0),
        ("fs_kawase_blur", 6.0, 2, 1),
        ("fs_kawase_blur", 6.0, 0, 2),
        ("fs_box_blur", 2.0, 0, 0),
        ("fs_box_blur", 2.0, 0, 1),
    ];
    for (fragment, radius, iteration, blur_alpha) in cases {
        let uniforms = words(&[
            W::F(texel),
            W::F(texel),
            W::F(radius),
            W::U(iteration),
            W::U(blur_alpha),
            W::F(0.0),
            W::F(0.0),
            W::F(0.0),
        ]);
        let pass = Pass {
            vertex: "vs_main",
            fragment,
            inputs: &[
                (0, Input::Uniform(&uniforms)),
                (1, Input::Texture(&input)),
                (2, Input::Sampler(&sampler)),
            ],
            vertices: 6,
        };
        check(
            &device,
            &queue,
            &format!("blur_{fragment}_{iteration}_{blur_alpha}"),
            super::blur::WGSL,
            &pass,
        );
    }
}

#[test]
fn glow_draws_what_the_inline_shader_drew() {
    let (device, queue) = need_device!();
    let source = upload(&device, &queue, &disc_bytes(5));
    let sampler = linear(&device);
    let texel = 1.0 / SIZE as f32;
    // Colour, blur, range, opacity, padding, texel size, padding.
    let uniforms = floats(&[
        1.0, 0.7, 0.2, 0.9, 6.0, 4.0, 0.8, 0.0, texel, texel, 0.0, 0.0,
    ]);
    let pass = Pass {
        vertex: "vs_main",
        fragment: "fs_glow",
        inputs: &[
            (0, Input::Uniform(&uniforms)),
            (1, Input::Texture(&source)),
            (2, Input::Sampler(&sampler)),
        ],
        vertices: 6,
    };
    check(&device, &queue, "glow", super::glow::WGSL, &pass);
}

#[test]
fn drop_shadow_draws_what_the_inline_shader_drew_with_and_without_spread() {
    let (device, queue) = need_device!();
    let original = upload(&device, &queue, &disc_bytes(7));
    let sampler = linear(&device);
    let texel = 1.0 / SIZE as f32;
    for spread in [0.0, 2.0, -1.5] {
        // Offset, blur radius, spread, colour, texel size, padding.
        let uniforms = floats(&[
            3.0, 4.0, 8.0, spread, 0.1, 0.1, 0.3, 0.8, texel, texel, 0.0, 0.0,
        ]);
        let pass = Pass {
            vertex: "vs_main",
            fragment: "fs_drop_shadow",
            // The blurred input texture (binding 1) is not read by either shader.
            inputs: &[
                (0, Input::Uniform(&uniforms)),
                (2, Input::Sampler(&sampler)),
                (3, Input::Texture(&original)),
            ],
            vertices: 6,
        };
        check(
            &device,
            &queue,
            &format!("drop_shadow_{}", (spread * 10.0) as i32),
            super::drop_shadow::WGSL,
            &pass,
        );
    }
}

#[test]
fn layer_composite_draws_what_the_inline_shader_drew_for_every_blend_mode() {
    let (device, queue) = need_device!();
    let layer = upload(&device, &queue, &disc_bytes(8));
    let dest = upload(&device, &queue, &gradient_bytes(9));
    let sampler = linear(&device);
    let size = SIZE as f32;
    // A case is (blend mode, clip type, perspective depth, rotation in radians).
    let mut cases = vec![(0u32, 0u32, 0.0f32, 0.0f32)];
    cases.extend((1..=11).map(|mode| (mode, 1, 0.0, 0.3)));
    cases.push((0, 1, 0.0, 0.0));
    cases.push((0, 0, 400.0, 0.0));
    cases.push((2, 1, 400.0, 0.0));
    for (blend_mode, clip_type, perspective_d, angle) in cases {
        let (sin_rz, cos_rz) = (angle as f32).sin_cos();
        let (sin_rx, cos_rx) = (0.25f32).sin_cos();
        let (sin_ry, cos_ry) = (0.35f32).sin_cos();
        let uniforms = words(&[
            // source_rect, dest_rect
            W::F(0.0),
            W::F(0.0),
            W::F(1.0),
            W::F(1.0),
            W::F(8.0),
            W::F(6.0),
            W::F(size - 16.0),
            W::F(size - 14.0),
            // viewport_size, opacity, blend_mode
            W::F(size),
            W::F(size),
            W::F(0.85),
            W::U(blend_mode),
            // clip_bounds, clip_radius
            W::F(12.0),
            W::F(10.0),
            W::F(40.0),
            W::F(40.0),
            W::F(6.0),
            W::F(10.0),
            W::F(14.0),
            W::F(4.0),
            // clip_type, perspective_d, sin_rx, cos_rx, sin_ry, cos_ry, sin_rz, cos_rz
            W::U(clip_type),
            W::F(perspective_d),
            W::F(sin_rx),
            W::F(cos_rx),
            W::F(sin_ry),
            W::F(cos_ry),
            W::F(sin_rz),
            W::F(cos_rz),
        ]);
        let pass = Pass {
            vertex: "vs_main",
            fragment: "fs_main",
            inputs: &[
                (0, Input::Uniform(&uniforms)),
                (1, Input::Texture(&layer)),
                (2, Input::Sampler(&sampler)),
                (3, Input::Texture(&dest)),
                (4, Input::Sampler(&sampler)),
            ],
            vertices: 6,
        };
        check(
            &device,
            &queue,
            &format!(
                "layer_composite_{blend_mode}_{clip_type}_{}_{}",
                perspective_d as i32,
                (angle * 10.0) as i32
            ),
            super::layer_composite::WGSL,
            &pass,
        );
    }
}
