//! The shader's curve maths against `blinc_animation/tests/vectors/curves.txt`: float64 values
//! computed from the maths alone, which the CPU code is checked against too. The shader runs
//! every case on the GPU and must land within a hundred-thousandth of the distance moved.

use crate::parity_harness as harness;

struct Case {
    kind: u32,
    params: [f32; 4],
    n: f32,
    jump: f32,
    from: f32,
    to: f32,
    duration: f32,
    v0: f32,
    elapsed: f32,
    value: f32,
    velocity: f32,
}

fn cases() -> Vec<Case> {
    include_str!("../../blinc_animation/tests/vectors/curves.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|line| {
            let f: Vec<f32> = line
                .split_whitespace()
                .map(|v| v.parse::<f64>().unwrap() as f32)
                .collect();
            Case {
                kind: f[0] as u32,
                params: [f[1], f[2], f[3], f[4]],
                n: f[5],
                jump: f[6],
                from: f[7],
                to: f[8],
                duration: f[9],
                v0: f[10],
                elapsed: f[11],
                value: f[12],
                velocity: f[13],
            }
        })
        .collect()
}

/// Three vec4s a case, as the shader's `Case` reads them.
fn pack(cases: &[Case]) -> Vec<[f32; 4]> {
    cases
        .iter()
        .flat_map(|c| {
            [
                [c.from, c.to, c.duration, c.v0],
                c.params,
                [c.kind as f32, c.n, c.jump, c.elapsed],
            ]
        })
        .collect()
}

fn run(device: &wgpu::Device, queue: &wgpu::Queue, cases: &[Case]) -> Vec<[f32; 4]> {
    use wgpu::{BindingType as B, ShaderStages as S};
    let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: S::COMPUTE,
        ty,
        count: None,
    };
    let storage = |read_only| B::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only },
        has_dynamic_offset: false,
        min_binding_size: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            entry(0, storage(true)),
            entry(1, storage(false)),
            entry(
                2,
                B::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
        ],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(crate::sdf_russl::curve_check::WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            }),
        ),
        module: &module,
        entry_point: Some("cs_main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let cases_buffer = harness::buffer(
        device,
        bytemuck::cast_slice(&pack(cases)),
        wgpu::BufferUsages::STORAGE,
    );
    let out_size = (cases.len() * 16) as u64;
    let out = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: out_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let count = harness::buffer(
        device,
        bytemuck::bytes_of(&[cases.len() as f32, 0.0, 0.0, 0.0]),
        wgpu::BufferUsages::UNIFORM,
    );
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: cases_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: out.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: count.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(cases.len().div_ceil(64) as u32, 1, 1);
    }
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: out_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_buffer_to_buffer(&out, 0, &readback, 0, out_size);
    queue.submit([encoder.finish()]);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    device
        .poll(wgpu::PollType::Wait)
        .expect("the copy finishes");
    let mapped = readback.slice(..).get_mapped_range();
    bytemuck::cast_slice(&mapped).to_vec()
}

#[test]
fn the_shader_curves_match_the_vectors() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let cases = cases();
    let gpu = run(&device, &queue, &cases);
    assert_eq!(gpu.len(), cases.len());
    let mut worst = [0f32; 5];
    for (case, got) in cases.iter().zip(&gpu) {
        let scale = (case.to - case.from).abs().max(1.0);
        let err = (got[0] - case.value).abs() / scale;
        worst[case.kind as usize] = worst[case.kind as usize].max(err);
        let allowed = 1e-4 * case.velocity.abs().max(scale * 10.0);
        assert!(
            (got[1] - case.velocity).abs() < allowed,
            "kind {} {:?} at {} ms: velocity {}, expected {}",
            case.kind,
            case.params,
            case.elapsed,
            got[1],
            case.velocity
        );
    }
    eprintln!("worst value error by kind (linear, bezier, steps, power, spring): {worst:?}");
    for (kind, err) in worst.iter().enumerate() {
        assert!(*err < 1e-5, "kind {kind}: worst value error {err:e}");
    }
}
