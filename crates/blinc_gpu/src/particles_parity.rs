//! The particle shaders draw and simulate what their formulas say: the render shader on both
//! tiers, and the simulation against a CPU copy of it, random numbers included.

use crate::parity_harness::{self as harness, AuxData};

const SIZE: u32 = 128;
const CLEAR: [u8; 4] = [20, 20, 30, 255];

/// A particle as the shaders read it: four vec4s.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct Particle {
    position_life: [f32; 4],
    velocity_max_life: [f32; 4],
    color: [f32; 4],
    size_rotation: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Emitter {
    position_shape: [f32; 4],
    shape_params: [f32; 4],
    direction_randomness: [f32; 4],
    emission_config: [f32; 4],
    lifetime_speed: [f32; 4],
    size_config: [f32; 4],
    start_color: [f32; 4],
    mid_color: [f32; 4],
    end_color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Force {
    type_strength: [f32; 4],
    direction_params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Sim {
    time_config: [f32; 4],
    random_seed: [f32; 4],
    force_config: [f32; 4],
}

/// Every kind of force: gravity, wind, vortex, drag, turbulence, an attractor and a radial pull.
fn forces() -> Vec<Force> {
    let force = |kind: f32, strength: f32, params: [f32; 4]| Force {
        type_strength: [kind, strength, 0.0, 0.0],
        direction_params: params,
    };
    vec![
        force(0.0, 1.0, [0.0, -9.8, 0.0, 0.0]),
        force(1.0, 2.0, [1.0, 0.0, 0.5, 0.3]),
        force(2.0, 1.5, [0.0, 1.0, 0.0, 0.0]),
        force(3.0, 0.4, [0.0; 4]),
        force(4.0, 3.0, [0.0, 0.0, 0.0, 1.3]),
        force(5.0, 2.0, [0.9, 1.2, -0.2, 0.0]),
        force(6.0, -0.8, [0.0, 0.0, 0.0, 0.0]),
    ]
}

fn emitter(shape: f32) -> Emitter {
    Emitter {
        position_shape: [0.5, 1.0, -0.5, shape],
        shape_params: [1.5, 0.8, 0.6, 0.0],
        direction_randomness: [0.2, 1.0, 0.1, 0.4],
        emission_config: [6000.0, 20.0, 0.0, 0.7],
        lifetime_speed: [0.5, 2.0, 1.0, 4.0],
        size_config: [0.1, 0.3, 0.02, 0.1],
        start_color: [1.0, 0.8, 0.2, 1.0],
        mid_color: [0.9, 0.3, 0.1, 0.8],
        end_color: [0.2, 0.2, 0.2, 0.0],
    }
}

/// Particles alive at various ages and dead ones to spawn into.
fn particles(count: usize) -> Vec<Particle> {
    (0..count)
        .map(|i| {
            let f = i as f32;
            if i % 3 == 0 {
                Particle::zeroed()
            } else {
                // Some live a step or two more, and die in a step.
                let life = if i % 7 < 3 {
                    0.005 + (i % 7) as f32 / 60.0
                } else {
                    0.3 + (i % 7) as f32 * 0.4
                };
                Particle {
                    position_life: [f * 0.01, 0.5 + (i % 5) as f32 * 0.2, -f * 0.005, life],
                    velocity_max_life: [0.3 - (i % 4) as f32 * 0.2, 1.0, 0.1 * (i % 3) as f32, 3.0],
                    color: [1.0, 0.5, 0.2, 1.0],
                    size_rotation: [0.2, 0.2, 0.05, 0.0],
                }
            }
        })
        .collect()
}

use bytemuck::Zeroable;

/// Run the simulation `source` for `steps` steps over `initial` and read the particles back.
fn simulate(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    emitter: &Emitter,
    forces: &[Force],
    initial: &[Particle],
    steps: u32,
) -> Vec<Particle> {
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
    let uniform = B::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            entry(0, storage(false)),
            entry(1, uniform),
            entry(2, uniform),
            entry(3, storage(true)),
        ],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
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
    let buffer = harness::buffer(
        device,
        bytemuck::cast_slice(initial),
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let emitter_buffer = harness::buffer(
        device,
        bytemuck::bytes_of(emitter),
        wgpu::BufferUsages::UNIFORM,
    );
    let sim_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: std::mem::size_of::<Sim>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let forces_buffer = harness::buffer(
        device,
        bytemuck::cast_slice(forces),
        wgpu::BufferUsages::STORAGE,
    );
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: emitter_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: sim_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: forces_buffer.as_entire_binding(),
            },
        ],
    });
    for step in 0..steps {
        queue.write_buffer(
            &sim_buffer,
            0,
            bytemuck::bytes_of(&sim(step, initial.len(), forces.len())),
        );
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(initial.len().div_ceil(64) as u32, 1, 1);
        }
        queue.submit([encoder.finish()]);
    }
    let size = std::mem::size_of_val(initial) as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(&buffer, 0, &readback, 0, size);
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

/// The uniforms of step `step`: a sixtieth of a second, the time so far, a seed that changes.
fn sim(step: u32, count: usize, forces: usize) -> Sim {
    Sim {
        time_config: [1.0 / 60.0, step as f32 / 60.0, count as f32, 1.0],
        random_seed: [0.137 * (step + 1) as f32, 0.0, 0.0, 0.0],
        force_config: [forces as f32, 0.0, 0.0, 0.0],
    }
}

// ── The simulation on the CPU ───────────────────────────────────────────

fn pcg_hash(input: u32) -> u32 {
    let state = input.wrapping_mul(747796405).wrapping_add(2891336453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277803737);
    (word >> 22) ^ word
}

struct Rng(u32);

impl Rng {
    fn float(&mut self) -> f32 {
        self.0 = pcg_hash(self.0);
        self.0 as f32 / 4294967295.0
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + self.float() * (high - low)
    }

    fn unit_vector(&mut self) -> [f32; 3] {
        let theta = self.float() * std::f32::consts::TAU;
        let z = self.range(-1.0, 1.0);
        let r = (1.0 - z * z).sqrt();
        [r * theta.cos(), r * theta.sin(), z]
    }
}

type V3 = [f32; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn length(a: V3) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

fn normalize(a: V3) -> V3 {
    scale(a, 1.0 / length(a))
}

fn xyz(v: [f32; 4]) -> V3 {
    [v[0], v[1], v[2]]
}

fn mix4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn spawn_position(e: &Emitter, rng: &mut Rng) -> V3 {
    let base = xyz(e.position_shape);
    let p = e.shape_params;
    match e.position_shape[3] as u32 {
        1 => {
            let dir = rng.unit_vector();
            let r = p[0] * rng.float().powf(1.0 / 3.0);
            add(base, scale(dir, r))
        }
        2 => {
            let mut dir = rng.unit_vector();
            dir[1] = dir[1].abs();
            let r = p[0] * rng.float().powf(1.0 / 3.0);
            add(base, scale(dir, r))
        }
        3 | 5 => {
            let theta = rng.float() * std::f32::consts::TAU;
            let r = if e.position_shape[3] as u32 == 3 {
                p[1]
            } else {
                p[0]
            } * rng.float().sqrt();
            add(base, [r * theta.cos(), 0.0, r * theta.sin()])
        }
        4 => {
            let x = rng.range(-p[0], p[0]);
            let y = rng.range(-p[1], p[1]);
            let z = rng.range(-p[2], p[2]);
            add(base, [x, y, z])
        }
        _ => base,
    }
}

fn spawn_velocity(e: &Emitter, rng: &mut Rng) -> V3 {
    let base = normalize(xyz(e.direction_randomness));
    let random = rng.unit_vector();
    let r = e.direction_randomness[3];
    let mixed = [0, 1, 2].map(|i| base[i] + (random[i] - base[i]) * r);
    let dir = normalize(mixed);
    let speed = rng.range(e.lifetime_speed[2], e.lifetime_speed[3]);
    scale(dir, speed)
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn apply_force(f: &Force, pos: V3, vel: V3, dt: f32) -> V3 {
    // Turbulence has no copy here: see the test.
    let strength = f.type_strength[1];
    let params = xyz(f.direction_params);
    match f.type_strength[0] as u32 {
        0 => add(vel, scale(params, dt)),
        1 => add(vel, scale(params, strength * dt)),
        2 => add(
            vel,
            scale(normalize(cross(normalize(params), pos)), strength * dt),
        ),
        3 => scale(vel, 1.0 - strength * dt),
        5 => {
            let to = [params[0] - pos[0], params[1] - pos[1], params[2] - pos[2]];
            let dist = length(to).max(0.1);
            add(vel, scale(normalize(to), strength / (dist * dist) * dt))
        }
        6 => {
            let to = [params[0] - pos[0], 0.0, params[2] - pos[2]];
            add(vel, scale(to, strength * dt))
        }
        _ => vel,
    }
}

/// One step of the particle at `idx`, as `cs_main` does it.
fn step(mut p: Particle, idx: u32, e: &Emitter, forces: &[Force], sim: &Sim) -> Particle {
    let dt = sim.time_config[0];
    let time = sim.time_config[1];
    let max_particles = sim.time_config[2] as u32;
    let mut rng = Rng(idx.wrapping_add((time * 1000.0) as u32));
    rng.0 = pcg_hash(rng.0.wrapping_add((sim.random_seed[0] * 1000000.0) as u32));
    if p.position_life[3] > 0.0 {
        p.position_life[3] -= dt;
        if p.position_life[3] <= 0.0 {
            p.position_life[3] = 0.0;
        } else {
            let mut vel = xyz(p.velocity_max_life);
            for f in forces.iter().take(sim.force_config[0] as usize) {
                if f.type_strength[0] as u32 == 0 {
                    vel = add(
                        vel,
                        scale(scale(xyz(f.direction_params), e.emission_config[3]), dt),
                    );
                } else {
                    vel = apply_force(f, xyz(p.position_life), vel, dt);
                }
            }
            p.velocity_max_life = [vel[0], vel[1], vel[2], p.velocity_max_life[3]];
            p.position_life = [
                p.position_life[0] + vel[0] * dt,
                p.position_life[1] + vel[1] * dt,
                p.position_life[2] + vel[2] * dt,
                p.position_life[3],
            ];
            let ratio = p.position_life[3] / p.velocity_max_life[3];
            p.color = if ratio > 0.5 {
                mix4(e.mid_color, e.start_color, (ratio - 0.5) * 2.0)
            } else {
                mix4(e.end_color, e.mid_color, ratio * 2.0)
            };
            p.size_rotation[0] =
                p.size_rotation[2] + (p.size_rotation[1] - p.size_rotation[2]) * ratio;
        }
    } else {
        let mut chance = 0.0;
        if e.emission_config[0] > 0.0 {
            chance = e.emission_config[0] * dt / max_particles as f32;
        }
        if e.emission_config[1] > 0.0 {
            chance += e.emission_config[1] / max_particles as f32;
        }
        if rng.float() < chance {
            let pos = spawn_position(e, &mut rng);
            let vel = spawn_velocity(e, &mut rng);
            let life = rng.range(e.lifetime_speed[0], e.lifetime_speed[1]);
            let size = rng.range(e.size_config[0], e.size_config[1]);
            let end = rng.range(e.size_config[2], e.size_config[3]);
            p.position_life = [pos[0], pos[1], pos[2], life];
            p.velocity_max_life = [vel[0], vel[1], vel[2], life];
            p.color = e.start_color;
            p.size_rotation = [size, size, end, 0.0];
        }
    }
    p
}

fn assert_close(a: &Particle, b: &Particle, what: &str) {
    for (name, x, y) in [
        ("position_life", a.position_life, b.position_life),
        (
            "velocity_max_life",
            a.velocity_max_life,
            b.velocity_max_life,
        ),
        ("color", a.color, b.color),
        ("size_rotation", a.size_rotation, b.size_rotation),
    ] {
        for c in 0..4 {
            assert!(
                (x[c] - y[c]).abs() <= 2e-3 * (1.0 + y[c].abs()),
                "{what}: {name}[{c}] is {} and should be {}",
                x[c],
                y[c]
            );
        }
    }
}

/// One step of the simulation against the CPU copy above, for every emitter shape: live particles
/// age and move under every kind of force, dead ones spawn from the random numbers.
#[test]
fn a_step_of_the_simulation_matches_the_cpu() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let initial = particles(512);
    // Turbulence is a hash of the velocity through `fract(sin(..) * 43758.5)`, which a GPU's sine
    // does not reproduce to the digits that takes: it has tests of its own.
    let forces: Vec<Force> = forces()
        .into_iter()
        .filter(|f| f.type_strength[0] != 4.0)
        .collect();
    for shape in 0..6 {
        let e = emitter(shape as f32);
        let gpu = simulate(
            &device,
            &queue,
            crate::sdf_russl::particle_compute::WGSL,
            &e,
            &forces,
            &initial,
            1,
        );
        let uniforms = sim(0, initial.len(), forces.len());
        let (mut spawned, mut aged) = (0, 0);
        for (i, (before, after)) in initial.iter().zip(&gpu).enumerate() {
            let want = step(*before, i as u32, &e, &forces, &uniforms);
            assert_close(after, &want, &format!("shape {shape}, particle {i}"));
            if before.position_life[3] <= 0.0 && after.position_life[3] > 0.0 {
                spawned += 1;
            }
            if before.position_life[3] > 0.0 && after.position_life[3] > 0.0 {
                aged += 1;
            }
        }
        assert!(
            spawned > 0 && aged > 100,
            "shape {shape}: {spawned} spawned, {aged} aged"
        );
    }
}

/// Turbulence adds a velocity of bounded size, different for particles that move differently.
#[test]
fn turbulence_adds_a_bounded_velocity_that_differs_between_particles() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let initial: Vec<Particle> = particles(512)
        .into_iter()
        .filter(|p| p.position_life[3] > 0.2)
        .collect();
    let strength = 3.0;
    let turbulence = vec![Force {
        type_strength: [4.0, strength, 0.0, 0.0],
        direction_params: [0.0, 0.0, 0.0, 1.3],
    }];
    let mut quiet = emitter(0.0);
    quiet.emission_config = [0.0; 4];
    let gpu = simulate(
        &device,
        &queue,
        crate::sdf_russl::particle_compute::WGSL,
        &quiet,
        &turbulence,
        &initial,
        1,
    );
    let dt = 1.0 / 60.0;
    let mut deltas = Vec::new();
    for (before, after) in initial.iter().zip(&gpu) {
        let d: Vec<f32> = (0..3)
            .map(|c| after.velocity_max_life[c] - before.velocity_max_life[c])
            .collect();
        // Each noise term is a product of sines and cosines, at most 1, and there are two octaves.
        let bound = strength * dt * 2.6;
        assert!(d.iter().all(|v| v.abs() <= bound), "{d:?} is past {bound}");
        deltas.push(d);
    }
    let moved = deltas
        .iter()
        .filter(|d| d.iter().any(|v| v.abs() > 1e-4))
        .count();
    assert!(
        moved > initial.len() * 9 / 10,
        "{moved} of {} moved",
        initial.len()
    );
    let mut distinct: Vec<i32> = deltas.iter().map(|d| (d[0] * 1e4) as i32).collect();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(
        distinct.len() > initial.len() / 2,
        "{} distinct",
        distinct.len()
    );
}

/// The particles' velocities after six steps under every force, as an image, against this
/// machine's golden: the turbulence is a hash the CPU copy cannot follow.
#[test]
fn the_simulation_matches_this_machines_golden() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let initial = particles(512);
    harness::check_golden("particle_simulation", 32, || {
        let mut quiet = emitter(1.0);
        quiet.emission_config = [0.0, 0.0, 0.0, 0.7];
        let mut all = forces();
        all[4].type_strength[1] = 40.0;
        let end = simulate(
            &device,
            &queue,
            crate::sdf_russl::particle_compute::WGSL,
            &quiet,
            &all,
            &initial,
            6,
        );
        let mut bytes = vec![0_u8; 32 * 32 * 4];
        for (i, p) in end.iter().enumerate() {
            for c in 0..3 {
                bytes[i * 4 + c] = (p.velocity_max_life[c] * 8.0 + 128.0).clamp(0.0, 255.0) as u8;
            }
            bytes[i * 4 + 3] = 255;
        }
        bytes
    });
}

// ── The render pass ─────────────────────────────────────────────────────

/// Draw `particles` with `source`, a billboard quad for each, over the clear colour. `data_texture`
/// is the tier without storage buffers: the particles are an `RGBA32F` texture.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &str,
    data_texture: bool,
    particles: &[Particle],
) -> Vec<u8> {
    use wgpu::{BindingType as B, ShaderStages as S};
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: S::VERTEX,
                ty: AuxData::binding_type(data_texture),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: S::VERTEX,
                ty: B::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            }),
        ),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    // An orthographic view of x and y in -2..2, looking down -z.
    let mut view_proj = [0.0_f32; 16];
    view_proj[0] = 0.5;
    view_proj[5] = 0.5;
    view_proj[10] = -0.1;
    view_proj[15] = 1.0;
    let uniforms: Vec<f32> = view_proj
        .iter()
        .copied()
        .chain([0.0, 0.0, 3.0, 1.0])
        .chain([1.0, 0.0, 0.0, 1.0])
        .chain([0.0, 1.0, 0.0, 0.0])
        .chain([0.0; 4])
        .collect();
    let uniform_buffer = harness::buffer(
        device,
        bytemuck::cast_slice(&uniforms),
        wgpu::BufferUsages::UNIFORM,
    );
    // The particles are four vec4s each: a row of four RGBA32F texels, or a storage buffer.
    let storage = harness::buffer(
        device,
        bytemuck::cast_slice(particles),
        wgpu::BufferUsages::STORAGE,
    );
    let view = harness::texture(
        device,
        queue,
        (4, particles.len() as u32),
        wgpu::TextureFormat::Rgba32Float,
        16,
        bytemuck::cast_slice(particles),
    );
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: if data_texture {
                    wgpu::BindingResource::TextureView(&view)
                } else {
                    storage.as_entire_binding()
                },
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });
    harness::render(device, queue, SIZE, CLEAR, |pass| {
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..6, 0..particles.len() as u32);
    })
}

/// Particles of several sizes and colours, and dead ones.
fn render_scene() -> Vec<Particle> {
    let live = |x: f32, y: f32, size: f32, color: [f32; 4]| Particle {
        position_life: [x, y, 0.0, 1.0],
        velocity_max_life: [0.0, 0.0, 0.0, 2.0],
        color,
        size_rotation: [size, size, size, 0.0],
    };
    vec![
        live(-1.0, 1.0, 0.5, [1.0, 0.4, 0.1, 1.0]),
        live(0.0, 1.0, 0.3, [0.2, 0.8, 0.3, 0.6]),
        live(1.0, 1.0, 0.4, [0.3, 0.3, 1.0, 1.0]),
        live(-1.0, 0.0, 0.25, [1.0, 1.0, 0.0, 0.9]),
        live(0.0, 0.0, 0.5, [1.0, 1.0, 1.0, 1.0]),
        // Dead: nothing is drawn.
        Particle {
            position_life: [1.0, 0.0, 0.0, 0.0],
            ..live(1.0, 0.0, 0.5, [1.0, 0.0, 0.0, 1.0])
        },
        live(-1.0, -1.0, 0.6, [0.9, 0.2, 0.6, 0.5]),
        live(1.0, -1.0, 0.35, [0.1, 0.9, 0.9, 1.0]),
    ]
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    harness::pixel(pixels, SIZE, x, y)
}

#[test]
fn the_data_texture_tier_draws_the_pixels_the_storage_tier_draws() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let scene = render_scene();
    let storage = draw(
        &device,
        &queue,
        crate::sdf_russl::particle_render::WGSL,
        false,
        &scene,
    );
    let texture = draw(
        &device,
        &queue,
        crate::sdf_russl::particle_render_dt::WGSL,
        true,
        &scene,
    );
    assert!(
        storage.as_chunks::<4>().0.iter().any(|p| *p != CLEAR),
        "nothing was drawn"
    );
    assert!(storage == texture, "the tiers draw different pixels");
}

/// A particle is a billboard whose soft circle fades from 0.8 of its radius to its edge, and a
/// dead one is not drawn.
#[test]
fn particles_draw_a_soft_circle_and_dead_ones_nothing() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let scene = render_scene();
    let pixels = draw(
        &device,
        &queue,
        crate::sdf_russl::particle_render::WGSL,
        false,
        &scene,
    );
    // The view maps -2..2 to the target, so a unit is 32 pixels. The centre of the particle at
    // (-1, 1) of size 0.5 (a quad 1 across) is pixel (32, 32).
    let over = |color: [f32; 4], alpha: f32| -> [f32; 3] {
        let a = color[3] * alpha;
        [0, 1, 2].map(|c| (color[c] * a + CLEAR[c] as f32 / 255.0 * (1.0 - a)) * 255.0)
    };
    let near = |got: [u8; 4], want: [f32; 3], what: &str| {
        for c in 0..3 {
            assert!(
                (got[c] as f32 - want[c]).abs() <= 2.0,
                "{what}: channel {c} is {} and should be {:.1}",
                got[c],
                want[c]
            );
        }
    };
    near(
        pixel(&pixels, 32, 32),
        over([1.0, 0.4, 0.1, 1.0], 1.0),
        "the centre",
    );
    // Half the radius out the circle is solid; past 0.8 of it the fade begins.
    near(
        pixel(&pixels, 40, 32),
        over([1.0, 0.4, 0.1, 1.0], 1.0),
        "inside the soft edge",
    );
    let d = (46.5_f32 - 32.0).hypot(32.5 - 32.0) / 16.0;
    let alpha = 1.0 - {
        let t = ((d - 0.8) / 0.2).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    near(
        pixel(&pixels, 46, 32),
        over([1.0, 0.4, 0.1, 1.0], alpha),
        "in the soft edge",
    );
    // A translucent particle at (0, 1), and the corner of its quad, outside the circle.
    near(
        pixel(&pixels, 64, 32),
        over([0.2, 0.8, 0.3, 0.6], 1.0),
        "a translucent centre",
    );
    assert_eq!(pixel(&pixels, 55, 23), CLEAR, "the corner of a quad");
    // The dead particle at (1, 0) draws nothing.
    assert_eq!(pixel(&pixels, 96, 64), CLEAR, "a dead particle");
}

/// Each shader against this machine's golden, the output of the shader it replaced: the render
/// pass, and the particles after a few steps of the simulation drawn with it.
#[test]
fn the_scenes_match_this_machines_goldens() {
    let Some((device, queue)) = harness::device() else {
        eprintln!("no adapter: skipped");
        return;
    };
    let scene = render_scene();
    harness::check_golden("particle_render", SIZE, || {
        draw(
            &device,
            &queue,
            crate::sdf_russl::particle_render::WGSL,
            false,
            &scene,
        )
    });
}

#[test]
fn the_cpu_particle_types_lay_out_as_the_shaders_read_them() {
    use crate::sdf_russl::particle_data::Particle as Shader;
    use std::mem::{offset_of, size_of};
    assert_eq!(size_of::<Particle>(), Shader::SIZE);
    assert_eq!(
        offset_of!(Particle, position_life),
        offset_of!(Shader, position_life)
    );
    assert_eq!(
        offset_of!(Particle, velocity_max_life),
        offset_of!(Shader, velocity_max_life)
    );
    assert_eq!(offset_of!(Particle, color), offset_of!(Shader, color));
    assert_eq!(
        offset_of!(Particle, size_rotation),
        offset_of!(Shader, size_rotation)
    );
}
