// Same deep wgpu `Send` chain as blinc_gpu; see its lib.rs.
#![recursion_limit = "256"]
//! `DrawContext::run_gpu_pass` end-to-end demo.
//!
//! Shows the same pattern a user would reach for if they wanted to
//! retain their own GPU buffers for instanced rendering of a large
//! object scene: a `CustomRenderPass` implementation owns its
//! `wgpu::RenderPipeline`, its base-quad `wgpu::Buffer`, and its
//! per-instance `wgpu::Buffer`. All three are constructed once in
//! `initialize` and reused on every frame. The pass is wrapped with
//! `blinc_gpu::GpuPass::new(...)` so it can be passed through
//! `DrawContext::run_gpu_pass` from inside a regular `canvas()`
//! closure.
//!
//! Run with: cargo run -p blinc_app_examples --example gpu_pass_demo

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_core::{Color, Rect};
use blinc_gpu::GpuPass;
use blinc_gpu::custom_pass::{CustomRenderPass, RenderPassContext, RenderStage};
use bytemuck::{Pod, Zeroable};

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    blinc_app::windowed::WindowedApp::run(
        WindowConfig {
            title: "GPU Pass Demo · instanced grid".to_string(),
            width: 960,
            height: 720,
            resizable: true,
            ..Default::default()
        },
        build_ui,
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}

const GRID_DIM: u32 = 64; // 64 × 64 = 4096 instances
const INSTANCE_COUNT: u32 = GRID_DIM * GRID_DIM;

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    // Wrap the custom pass once. `GpuPass` clones cheaply (an
    // `Arc<Mutex<...>>` inside), so the closure captures a clone by
    // move. Mutation across frames goes through the wrapper's
    // internal lock when the dispatch site calls
    // `initialize_and_render`.
    let pass = GpuPass::new(InstancedGrid::new());
    let pass_for_canvas = pass.clone();

    // Continuous, infinite-loop timeline. We don't read its value;
    // the canvas reads time off `Instant::now()` inside the pass.
    // The timeline exists purely to keep Blinc's redraw chain warm.
    // Without it, the frame loop settles after the first paint and
    // our wgpu wobble freezes. Spinner widgets use the same trick.
    let tick = ctx.use_animated_timeline();
    let _ = tick.lock().unwrap().configure(|t| {
        let entry = t.add(0, 1000, 0.0, 1.0);
        t.set_loop(-1);
        t.start();
        entry
    });

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.05, 0.05, 0.08, 1.0))
        .flex_col()
        .items_center()
        .gap_px(12.0)
        .p(20.0)
        .child(
            text("4096 instanced quads, one draw call, buffers retained across frames")
                .size(18.0)
                .color(Color::WHITE),
        )
        .child(
            text("The canvas closure is plain Fn(...); no Mutex on the user side.")
                .size(13.0)
                .color(Color::rgba(0.7, 0.7, 0.8, 1.0)),
        )
        .child(
            div()
                .w(720.0)
                .h(540.0)
                .rounded(12.0)
                .bg(Color::rgba(0.02, 0.02, 0.04, 1.0))
                .child(
                    canvas(move |ctx, bounds| {
                        // Hand the pass through. The rect clips the
                        // wgpu draws to this canvas's region; pixels
                        // outside the canvas keep whatever the SDF
                        // static cache drew.
                        ctx.run_gpu_pass(
                            &pass_for_canvas,
                            Some(Rect::new(0.0, 0.0, bounds.width, bounds.height)),
                        );
                    })
                    .size(720.0, 540.0),
                ),
        )
}

// ─────────────────────────────────────────────────────────────────────────────
// The custom pass
// ─────────────────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Instance {
    /// Cell index (x, y) in the grid. Vertex shader resolves to NDC.
    cell: [f32; 2],
    /// Per-instance colour.
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Uniforms {
    grid_dim: f32,
    time: f32,
    _pad: [f32; 2],
}

struct InstancedGrid {
    pipeline: Option<wgpu::RenderPipeline>,
    vertex_buffer: Option<wgpu::Buffer>,
    instance_buffer: Option<wgpu::Buffer>,
    uniform_buffer: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,
    /// Wall-clock start, captured the first time `render` runs. Used to
    /// drive the per-cell wobble without any external state.
    // `web_time::Instant` is the cross-target shim: re-exports
    // `std::time::Instant` on desktop and routes to `performance.now()`
    // on `wasm32-unknown-unknown`. `std::time::Instant::now()` panics
    // ("time not implemented on this platform") on wasm, so the web
    // build crashed at the first frame before the swap.
    start: Option<web_time::Instant>,
}

impl InstancedGrid {
    fn new() -> Self {
        Self {
            pipeline: None,
            vertex_buffer: None,
            instance_buffer: None,
            uniform_buffer: None,
            bind_group: None,
            start: None,
        }
    }
}

impl CustomRenderPass for InstancedGrid {
    fn label(&self) -> &str {
        "instanced_grid"
    }

    fn stage(&self) -> RenderStage {
        // Canvas-scoped passes don't use the stage selector; the canvas
        // dispatch site invokes us directly. The value here is only
        // consulted for renderer-level registration via
        // `register_custom_pass`, which this demo doesn't use.
        RenderStage::PreRender
    }

    fn initialize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        // Unit quad in -0.5..0.5: two tris, six vertices, no index
        // buffer.
        let quad = [
            Vertex { pos: [-0.5, -0.5] },
            Vertex { pos: [0.5, -0.5] },
            Vertex { pos: [-0.5, 0.5] },
            Vertex { pos: [-0.5, 0.5] },
            Vertex { pos: [0.5, -0.5] },
            Vertex { pos: [0.5, 0.5] },
        ];
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instanced_grid_vb"),
            size: std::mem::size_of_val(&quad) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vertex_buffer, 0, bytemuck::bytes_of(&quad));

        // One instance per grid cell. Colour is a static rainbow over
        // the grid; the time-driven motion happens in the vertex
        // shader via the uniform.
        let mut instances = Vec::with_capacity(INSTANCE_COUNT as usize);
        for y in 0..GRID_DIM {
            for x in 0..GRID_DIM {
                let u = x as f32 / (GRID_DIM - 1) as f32;
                let v = y as f32 / (GRID_DIM - 1) as f32;
                instances.push(Instance {
                    cell: [x as f32, y as f32],
                    color: [u, v, 1.0 - u, 1.0],
                });
            }
        }
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instanced_grid_ib"),
            size: (std::mem::size_of::<Instance>() * instances.len()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&instance_buffer, 0, bytemuck::cast_slice(&instances));

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instanced_grid_ub"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("instanced_grid_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("instanced_grid_bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("instanced_grid_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("instanced_grid_pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("instanced_grid_pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    // Vertex buffer 0: quad positions.
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        }],
                    },
                    // Vertex buffer 1: per-instance data.
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 0,
                                shader_location: 1,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 8,
                                shader_location: 2,
                            },
                        ],
                    },
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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

        self.pipeline = Some(pipeline);
        self.vertex_buffer = Some(vertex_buffer);
        self.instance_buffer = Some(instance_buffer);
        self.uniform_buffer = Some(uniform_buffer);
        self.bind_group = Some(bind_group);
    }

    fn render(&mut self, ctx: &RenderPassContext) {
        let (Some(pipeline), Some(vb), Some(ib), Some(ub), Some(bg)) = (
            &self.pipeline,
            &self.vertex_buffer,
            &self.instance_buffer,
            &self.uniform_buffer,
            &self.bind_group,
        ) else {
            return;
        };

        // Per-frame uniform update: single 16-byte write.
        let start = *self.start.get_or_insert_with(web_time::Instant::now);
        let time = start.elapsed().as_secs_f32();
        let uniforms = Uniforms {
            grid_dim: GRID_DIM as f32,
            time,
            _pad: [0.0, 0.0],
        };
        ctx.queue.write_buffer(ub, 0, bytemuck::bytes_of(&uniforms));

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("instanced_grid_encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("instanced_grid_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: ctx.target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        // Compose over whatever the SDF cache drew (the
                        // card's background).
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            // Blinc's dispatch site already clamps `ctx.viewport` to
            // the render target and skips the dispatch entirely when
            // the rect collapses to zero area. Pass the rect through
            // as-is; no `.max(1.0)` games. Re-inflating a clamped
            // zero-size rect to size 1 walks right back into the
            // wgpu scissor-overflow panic when the canvas sits on
            // the bottom / right edge after a resize.
            if let Some([vx, vy, vw, vh]) = ctx.viewport {
                pass.set_viewport(vx, vy, vw, vh, 0.0, 1.0);
                pass.set_scissor_rect(vx as u32, vy as u32, vw as u32, vh as u32);
            }
            pass.set_bind_group(0, bg, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_vertex_buffer(1, ib.slice(..));
            pass.draw(0..6, 0..INSTANCE_COUNT);
        }
        ctx.queue.submit(std::iter::once(encoder.finish()));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Shader
// ─────────────────────────────────────────────────────────────────────────────

const SHADER: &str = r#"
struct Uniforms {
    grid_dim: f32,
    time: f32,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> U: Uniforms;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) v_pos: vec2<f32>,
    @location(1) inst_cell: vec2<f32>,
    @location(2) inst_color: vec4<f32>,
) -> VsOut {
    let cell_size = 2.0 / U.grid_dim;
    let cell_center = vec2<f32>(
        -1.0 + (inst_cell.x + 0.5) * cell_size,
        -1.0 + (inst_cell.y + 0.5) * cell_size,
    );

    // Per-cell wobble driven by time + cell index.
    let phase = U.time + (inst_cell.x + inst_cell.y) * 0.18;
    let scale = 0.6 + 0.35 * sin(phase);
    let pos = cell_center + v_pos * cell_size * scale;

    var out: VsOut;
    out.clip = vec4<f32>(pos, 0.0, 1.0);
    out.color = vec4<f32>(inst_color.rgb * (0.5 + 0.5 * scale), inst_color.a);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;
