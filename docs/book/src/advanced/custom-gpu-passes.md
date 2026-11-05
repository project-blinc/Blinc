# Custom GPU Passes

Blinc's drawing API has two layers. The first is `DrawContext`, the
high-level imperative surface used by widgets, canvases, and the
component library: `fill_rect`, `draw_text`, `push_clip`, and so on.
Resolution-independent, analytical, SDF-backed. Most code never needs
anything else.

The second is the **custom GPU pass**, an escape hatch into raw `wgpu`.
You bring your own pipeline, your own buffers, and your own bind groups.
Blinc plumbs the device, queue, and target view through, runs your pass
at a frame-accurate position in its render loop, and clips your output
to a layout-aware viewport rect. Use this when you need:

- Retained per-instance buffers and instanced draw calls for thousands
  of objects in a scene.
- Compute shaders.
- Custom post-process effect chains.
- Direct interop with another `wgpu`-based crate.
- Any pipeline whose semantics don't fit the SDF primitive model.

There are two ways to schedule a custom pass. Pick by where it should
run.

## Two scheduling models

| You want to…                                              | Use                                  |
|-----------------------------------------------------------|--------------------------------------|
| Run **inline with a canvas closure**, clipped to the canvas's layout bounds | `DrawContext::run_gpu_pass` |
| Run at a fixed stage (pre-render / scene-3d / post-process) across the whole frame | `register_custom_pass` on the renderer |

The first is element-scoped and the more common case. The pass appears
in the tree alongside everything else, gets the canvas's bounds as its
scissor, and is dispatched inside the canvas's paint slot. The second
is global and best for skybox-style backgrounds or full-frame
post-processing.

This chapter focuses on the first. For the second, see the docs on
`blinc::gpu::CustomRenderPass` and `RenderStage`.

## The `CustomRenderPass` trait

Both scheduling models share the same trait. Implement it on a struct
that owns your GPU state:

```rust
use blinc::gpu::custom_pass::{CustomRenderPass, RenderPassContext, RenderStage};

struct Particles {
    pipeline: Option<wgpu::RenderPipeline>,
    vertex_buffer: Option<wgpu::Buffer>,
    instance_buffer: Option<wgpu::Buffer>,
}

impl CustomRenderPass for Particles {
    fn label(&self) -> &str { "particles" }
    fn stage(&self) -> RenderStage { RenderStage::PreRender }

    fn initialize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        // Build pipelines, buffers, bind groups once. Stored on
        // `self`, retained across frames.
    }

    fn render(&mut self, ctx: &RenderPassContext) {
        // Issue draws. `ctx.device`, `ctx.queue`, `ctx.target` are
        // the live wgpu handles for this frame.
    }
}
```

Key contract:

- `initialize` runs **once**, lazily, before the first `render` call.
  Build your pipeline and persistent buffers here.
- `render` runs **every frame** the pass is dispatched. Update
  per-frame uniforms with `queue.write_buffer`, encode a render pass
  with `LoadOp::Load` (so you compose over whatever Blinc already drew),
  draw, and submit.
- `ctx.viewport` is `Some([x, y, w, h])` in physical pixels when the
  pass is scoped to a canvas. Apply `set_viewport` + `set_scissor_rect`
  to clip your draws to that rect.

Inside `initialize` and `render`, you're writing plain `wgpu`. Blinc
neither restricts what you do nor inspects what you produce.

## Scoping a pass to a canvas

The natural place to embed a `CustomRenderPass` is inside a regular
`canvas()` widget. Wrap your pass with `GpuPass::new(...)` and call
`run_gpu_pass` from the canvas closure:

```rust
use blinc::gpu::GpuPass;

let pass = GpuPass::new(Particles { /* ... */ });
let pass_for_canvas = pass.clone();

div()
    .w(720.0)
    .h(540.0)
    .rounded(12.0)
    .child(
        canvas(move |ctx, bounds| {
            ctx.run_gpu_pass(
                &pass_for_canvas,
                Some(Rect::new(0.0, 0.0, bounds.width, bounds.height)),
            );
        })
        .size(720.0, 540.0),
    )
```

What's happening:

- `GpuPass::new` wraps your `CustomRenderPass` in an `Arc<Mutex<…>>` so
  the canvas closure (which is `Fn`, not `FnMut`) can hold a clone and
  pass `&pass` without needing your own `RefCell` or `Mutex`.
- `ctx.run_gpu_pass` records the pass into the paint context's
  pending-pass list. The list is drained at composite time, after the
  SDF cache is blitted onto the swapchain.
- The `Some(Rect::new(...))` second argument plumbs through to
  `RenderPassContext::viewport`. Your pass uses it for `set_viewport`
  + `set_scissor_rect`. If you pass `None`, the GPU backend falls back
  to the current clip-stack AABB.

Mixing imperative draws with a custom pass in one closure works the
way you'd expect. Calls inside the closure run in source order, and
the custom pass composes over them inside its scissor:

```rust
canvas(move |ctx, bounds| {
    ctx.fill_rect(bounds.rect(), 8.0.into(), Color::BLACK.into());
    ctx.run_gpu_pass(&pass_for_canvas, Some(bounds.rect()));
    ctx.draw_text("particles", origin, &style);
})
```

## Retained buffers + instanced rendering

This is the headline use case. Below is the minimum a `CustomRenderPass`
needs to instance N objects against retained GPU buffers.

State stored on `self` (survives every frame):

```rust
struct InstancedGrid {
    pipeline: Option<wgpu::RenderPipeline>,
    vertex_buffer: Option<wgpu::Buffer>,   // base mesh (a unit quad)
    instance_buffer: Option<wgpu::Buffer>, // per-instance data
    uniform_buffer: Option<wgpu::Buffer>,  // per-frame uniforms
    bind_group: Option<wgpu::BindGroup>,
}
```

`initialize` builds them once:

```rust
fn initialize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) {
    let quad: [Vertex; 6] = /* six verts of a unit quad */;
    let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("grid_vb"),
        size: std::mem::size_of_val(&quad) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex_buffer, 0, bytemuck::bytes_of(&quad));

    let instances: Vec<Instance> = /* 4096 instances */;
    let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("grid_ib"),
        size: (std::mem::size_of::<Instance>() * instances.len()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&instance_buffer, 0, bytemuck::cast_slice(&instances));

    // Pipeline, uniforms, bind group… all stored on self.
    self.pipeline = Some(/* ... */);
    self.vertex_buffer = Some(vertex_buffer);
    self.instance_buffer = Some(instance_buffer);
}
```

`render` issues one instanced draw per frame:

```rust
fn render(&mut self, ctx: &RenderPassContext) {
    let (Some(pipeline), Some(vb), Some(ib), Some(ub), Some(bg)) =
        (&self.pipeline, &self.vertex_buffer, &self.instance_buffer,
         &self.uniform_buffer, &self.bind_group) else { return; };

    // Refresh per-frame uniforms (single 16-byte write).
    ctx.queue.write_buffer(ub, 0, bytemuck::bytes_of(&uniforms));

    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("grid_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    // Compose over whatever Blinc already drew below us.
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline);

        // Clip to the canvas's bounds. `viewport` is the
        // physical-pixel rect Blinc plumbed in from your
        // `run_gpu_pass` call — already clamped to the render
        // target, and zero-area rects are skipped at the dispatch
        // boundary. Pass it through verbatim; don't `.max(1.0)`
        // any axis or you walk into the wgpu scissor overflow
        // panic when the canvas sits exactly on the bottom edge
        // after a resize.
        if let Some([vx, vy, vw, vh]) = ctx.viewport {
            pass.set_viewport(vx, vy, vw, vh, 0.0, 1.0);
            pass.set_scissor_rect(vx as u32, vy as u32, vw as u32, vh as u32);
        }

        pass.set_bind_group(0, bg, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));   // base mesh
        pass.set_vertex_buffer(1, ib.slice(..));   // per-instance
        pass.draw(0..6, 0..INSTANCE_COUNT);        // one draw, N instances
    }
    ctx.queue.submit(std::iter::once(encoder.finish()));
}
```

Two `wgpu::VertexBufferLayout` entries (one with `step_mode: Vertex`,
one with `step_mode: Instance`) plus the rasterizer looping over the
instance buffer for you. The full working version lives in
`examples/blinc_app_examples/examples/gpu_pass_demo.rs`:

```sh
cargo run -p blinc_app_examples --example gpu_pass_demo
```

4 096 instanced quads, one draw call per frame, all buffers retained.

## How the pass composes with the rest of the frame

Blinc's frame loop is documented in the [GPU
Rendering](../architecture/gpu-rendering.md) chapter. The short
version: a canvas closure runs every frame inside the paint walker.
SDF primitives the closure emits land in either the static cache or the
dynamic batch. A custom pass scheduled via `run_gpu_pass` is **drained
separately** and dispatched after `composite_frame` has blitted the
static cache onto the swapchain, but before any overlay panels on top.

What this means concretely:

- Your pass draws **over** the canvas's other content (its background,
  whatever `fill_rect`s the closure emitted, ancestor styling).
- Overlay panels rendered by the layout system still appear **on top
  of** your pass. Modal dialogs and tooltips don't get hidden behind
  custom WebGPU content.
- The canvas wrapper opts the subtree out of the static-cache fast
  path automatically (canvases run every frame by contract). There's
  no extra invalidation work to do.
- Custom passes are not recorded by `RecordingContext`. If you replay a
  recorded canvas, custom passes silently no-op during replay (the
  trait method has a default empty impl, and only `GpuPaintContext`
  overrides it).

## Bounds vs. clipping

The `viewport` argument to `run_gpu_pass` is your contract with the
canvas:

- `Some(Rect::new(0.0, 0.0, bounds.width, bounds.height))`: clip to
  the canvas's full layout box. The most common form.
- `Some(Rect::new(margin, margin, w - 2.0*margin, h - 2.0*margin))`:
  clip to a sub-region. Useful if your wgpu output should leave a
  border for SDF chrome around it.
- `None`: fall back to whatever clip the GPU paint context has on its
  stack. The wrapping widget has almost certainly pushed one for its
  layout bounds, so this is usually equivalent to the first form.

What you do *inside* the pass is up to you. Coordinate systems, depth,
and blend state are all local; Blinc doesn't interpose. The only state
it cares about is the `set_scissor_rect` you apply to clip your output
to the canvas region. If you skip that, your pass will draw to the
whole frame target.

## Sharing state with the rest of your UI

Because the canvas closure is `Fn` (not `FnMut`), you can't mutate
captured-by-move variables directly. Two clean patterns:

**Time-driven motion inside the pass.** Capture a start `Instant` on
first `render` call; compute elapsed every frame. No external state
needed.

```rust
fn render(&mut self, ctx: &RenderPassContext) {
    let start = *self.start.get_or_insert_with(std::time::Instant::now);
    let time = start.elapsed().as_secs_f32();
    // …write `time` into your uniform buffer…
}
```

**External signals.** Wrap any state you need to mutate from outside
the closure (frame counter, mouse-derived target, animation
parameters) in an `Arc<Mutex<...>>` or one of Blinc's signals. Read it
from inside `render`:

```rust
struct Field {
    /* GPU resources… */
    target: Arc<Mutex<glam::Vec2>>,  // shared with UI
}

impl CustomRenderPass for Field {
    fn render(&mut self, ctx: &RenderPassContext) {
        let target = *self.target.lock().unwrap();
        // …feed target into the uniform…
    }
}

// Elsewhere, UI mutates the shared cell:
let target = Arc::new(Mutex::new(glam::Vec2::ZERO));
on_mouse_move(|p| *target.lock().unwrap() = p);
```

Don't try to peek into `GpuPass` itself; the wrapper deliberately
doesn't expose its inner pass. Hold the shared state through your own
`Arc` instead.

## When NOT to use a custom pass

A custom pass is the right tool when your work doesn't fit the SDF
primitive model: instanced meshes, compute, multi-pass post-effects.
For most UI work it's the wrong tool. Heavier to write, harder to
debug, and locked to one GPU backend.

Default to the high-level path:

- For shapes and gradients: `DrawContext::fill_rect`,
  `stroke_rect`, `fill_circle`, `fill_path`.
- For animations: spring physics, CSS keyframes, motion bindings. The
  compositor patches GPU primitives in place for these. See
  [Performance Tips](performance.md).
- For 3D scenes: `blinc_canvas_kit`'s `SceneKit3D` + `draw_mesh_data`.
  Handles cameras, lighting, IBL, and shadow maps without you writing
  any wgpu.

Reach for custom passes only when the simpler tools genuinely can't
express what you need.
