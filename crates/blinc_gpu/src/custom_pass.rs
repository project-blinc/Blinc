//! Custom Render Pass API
//!
//! Allows users to inject their own GPU render passes into the Blinc pipeline.
//! Includes helpers for custom bind groups, compute dispatch, and post-processing chains.
//!
//! # Render Pass Example
//!
//! ```ignore
//! use blinc_gpu::custom_pass::*;
//!
//! struct MyPostEffect {
//!     pipeline: Option<wgpu::RenderPipeline>,
//! }
//!
//! impl CustomRenderPass for MyPostEffect {
//!     fn label(&self) -> &str { "my_post_effect" }
//!     fn stage(&self) -> RenderStage { RenderStage::PostProcess }
//!
//!     fn initialize(&mut self, device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) {
//!         // Create your pipeline, bind groups, etc.
//!     }
//!
//!     fn render(&mut self, ctx: &RenderPassContext) {
//!         let mut encoder = ctx.device.create_command_encoder(&Default::default());
//!         // ... your render pass ...
//!         ctx.queue.submit(std::iter::once(encoder.finish()));
//!     }
//! }
//! ```
//!
//! # Custom Bind Group Example
//!
//! ```ignore
//! let mut builder = BindGroupBuilder::new("my_effect");
//! builder.add_uniform_buffer(my_uniforms_buffer.as_entire_binding());
//! builder.add_texture(&my_texture_view);
//! builder.add_sampler(&my_sampler);
//! builder.add_storage_buffer(my_data_buffer.as_entire_binding(), true); // read-only
//! let (layout, bind_group) = builder.build(device);
//! ```
//!
//! # Compute Dispatch Example
//!
//! ```ignore
//! let dispatch = ComputeDispatch {
//!     pipeline: &my_compute_pipeline,
//!     bind_group: &my_bind_group,
//!     workgroups: (64, 1, 1),
//!     label: "particle_sim",
//! };
//! dispatch.execute(device, queue);
//! ```

use wgpu::util::DeviceExt;

/// Stage in the rendering pipeline where a custom pass executes
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderStage {
    /// Before the main UI rendering.
    /// Good for: skyboxes, 3D scene backgrounds, clearing with custom colors.
    PreRender,
    /// Inside the 3D mesh HDR pipeline, between skybox and mesh passes.
    /// Good for: ground grids, scene helpers, wireframe overlays, custom
    /// 3D effects that need the camera view-projection. The target is
    /// the `Rgba16Float` HDR intermediate, not the final framebuffer —
    /// the pass output is tonemapped alongside the mesh.
    ///
    /// Camera data is available in `RenderPassContext::view_proj` and
    /// `camera_pos` when this stage runs.
    Scene3D,
    /// After all UI rendering.
    /// Good for: screen-space effects, overlays, debug visualizations, tone mapping.
    PostProcess,
}

/// Context provided to custom render passes each frame
pub struct RenderPassContext<'a> {
    /// GPU device for creating resources
    pub device: &'a wgpu::Device,
    /// GPU queue for submitting commands and uploading data
    pub queue: &'a wgpu::Queue,
    /// Current render target (the framebuffer or offscreen texture)
    pub target: &'a wgpu::TextureView,
    /// Viewport width in physical pixels
    pub viewport_width: u32,
    /// Viewport height in physical pixels
    pub viewport_height: u32,
    /// Surface texture format
    pub texture_format: wgpu::TextureFormat,
    /// Display scale factor (DPI)
    pub scale_factor: f64,
    /// View-projection matrix (column-major `[f32; 16]`). Only populated
    /// for `Scene3D` stage passes; `None` for PreRender/PostProcess.
    pub view_proj: Option<[f32; 16]>,
    /// Inverse view-projection matrix. Only populated for `Scene3D`.
    pub inv_view_proj: Option<[f32; 16]>,
    /// Camera world-space position. Only populated for `Scene3D`.
    pub camera_pos: Option<[f32; 3]>,
    /// Canvas viewport rect in physical pixels `[x, y, w, h]`. Scene3D
    /// passes should apply `set_viewport` + `set_scissor_rect` with
    /// these values so their output clips to the same region as the mesh.
    /// `None` = full target (no canvas wrapper).
    pub viewport: Option<[f32; 4]>,
}

/// Trait for user-defined render passes.
///
/// Implement this to inject custom GPU rendering into the Blinc pipeline.
/// The pass is initialized once, then `render()` is called each frame.
///
/// Passes are executed in registration order within their stage.
///
/// On native targets we require `Send` so that custom passes can be moved
/// across threads (e.g. background compositing on a worker thread). On
/// `wasm32` the entire wgpu API is single-threaded — `wgpu::RenderPipeline`
/// is `!Send` — so we drop the bound to keep the trait usable in the
/// browser. Web apps run on the main thread anyway.
#[cfg(not(target_arch = "wasm32"))]
pub trait CustomRenderPass: Send {
    /// Human-readable label for debugging and profiling
    fn label(&self) -> &str;

    /// Which stage of the pipeline this pass runs in
    fn stage(&self) -> RenderStage;

    /// Create GPU resources (pipelines, buffers, textures).
    ///
    /// Called once after registration, before the first `render()` call.
    fn initialize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    );

    /// Execute the render pass for this frame.
    ///
    /// Create a command encoder, begin your render pass(es), and submit.
    /// The `target` view is the current framebuffer — use `LoadOp::Load`
    /// to preserve existing content, or `LoadOp::Clear` to overwrite.
    fn render(&mut self, ctx: &RenderPassContext);

    /// Called when the viewport is resized.
    ///
    /// Override this to recreate size-dependent resources (e.g., render targets).
    fn resize(&mut self, _device: &wgpu::Device, _width: u32, _height: u32) {}

    /// Whether this pass is currently enabled.
    ///
    /// Disabled passes are skipped without removing them from the pipeline.
    fn enabled(&self) -> bool {
        true
    }
}

/// Wasm32 mirror of [`CustomRenderPass`] without the `Send` bound.
///
/// On the browser main thread there are no other threads to send to, and
/// `wgpu::RenderPipeline` is `!Send` on `wasm32-unknown-unknown`, so the
/// `Send` requirement would make every implementation impossible to compile.
/// Trait method signatures are identical to the native version.
#[cfg(target_arch = "wasm32")]
pub trait CustomRenderPass {
    /// Human-readable label for debugging and profiling
    fn label(&self) -> &str;

    /// Which stage of the pipeline this pass runs in
    fn stage(&self) -> RenderStage;

    /// Create GPU resources (pipelines, buffers, textures).
    fn initialize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    );

    /// Execute the render pass for this frame.
    fn render(&mut self, ctx: &RenderPassContext);

    /// Called when the viewport is resized.
    fn resize(&mut self, _device: &wgpu::Device, _width: u32, _height: u32) {}

    /// Whether this pass is currently enabled.
    fn enabled(&self) -> bool {
        true
    }
}

/// Manages registered custom render passes
pub(crate) struct CustomPassManager {
    passes: Vec<Box<dyn CustomRenderPass>>,
}

impl CustomPassManager {
    pub fn new() -> Self {
        Self { passes: Vec::new() }
    }

    /// Register a new custom render pass. It will be initialized on next frame.
    pub fn register(&mut self, pass: Box<dyn CustomRenderPass>) {
        self.passes.push(pass);
    }

    /// Initialize any uninitialized passes
    pub fn initialize_pending(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        for pass in &mut self.passes {
            pass.initialize(device, queue, format);
        }
    }

    /// Execute all enabled passes for the given stage
    pub fn execute_stage(&mut self, stage: RenderStage, ctx: &RenderPassContext) {
        for pass in &mut self.passes {
            if pass.stage() == stage && pass.enabled() {
                pass.render(ctx);
            }
        }
    }

    /// Notify all passes of a resize
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        for pass in &mut self.passes {
            pass.resize(device, width, height);
        }
    }

    /// Check if any passes exist for a given stage
    pub fn has_passes(&self, stage: RenderStage) -> bool {
        self.passes
            .iter()
            .any(|p| p.stage() == stage && p.enabled())
    }

    /// Remove a pass by label. Returns true if found and removed.
    pub fn remove(&mut self, label: &str) -> bool {
        let len_before = self.passes.len();
        self.passes.retain(|p| p.label() != label);
        self.passes.len() < len_before
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GpuPass — bridge wrapper used by DrawContext::run_gpu_pass
// ─────────────────────────────────────────────────────────────────────────────

/// Canonical bridge between [`CustomRenderPass`] and
/// [`blinc_core::draw::DrawContext::run_gpu_pass`].
///
/// Wrap any `CustomRenderPass` with `GpuPass::new(...)` and pass `&pass`
/// into `ctx.run_gpu_pass()` from inside a canvas closure. The wrapper
/// is cheap to clone (it's an `Arc` internally) and carries its own
/// retained pipeline / buffer state across frames via the underlying
/// pass's `&mut self` methods.
///
/// The wrapper uses interior mutability (`Arc<Mutex<_>>`) because the
/// canvas render closure is `Fn`, not `FnMut`. Users capture the wrapper
/// by move and call `&pass` from within the closure; the GPU dispatch
/// site locks the mutex briefly to call `initialize` (once) and
/// `render` (every frame).
///
/// # Example
///
/// ```ignore
/// use blinc_gpu::{GpuPass, custom_pass::{CustomRenderPass, RenderStage, RenderPassContext}};
///
/// struct Particles { /* wgpu state on self */ }
/// impl CustomRenderPass for Particles {
///     fn label(&self) -> &str { "particles" }
///     fn stage(&self) -> RenderStage { RenderStage::PreRender }
///     fn initialize(&mut self, _d: &wgpu::Device, _q: &wgpu::Queue, _f: wgpu::TextureFormat) {}
///     fn render(&mut self, _ctx: &RenderPassContext) {}
/// }
///
/// let particles = GpuPass::new(Particles { /* ... */ });
/// // inside a canvas closure: `ctx.run_gpu_pass(&particles)`
/// ```
#[derive(Clone)]
pub struct GpuPass {
    pub(crate) inner: std::sync::Arc<std::sync::Mutex<GpuPassInner>>,
}

/// Locked state of a [`GpuPass`]. Crate-visible so the GPU paint
/// dispatch in `paint.rs` can drive `initialize` / `render` without
/// re-exporting the lock guard.
pub(crate) struct GpuPassInner {
    pub pass: Box<dyn CustomRenderPass>,
    pub initialized: bool,
}

/// A pass that draws nothing: the handle of a pending draw whose work the renderer does itself,
/// such as a flow shader.
struct NoPass;

impl CustomRenderPass for NoPass {
    fn label(&self) -> &str {
        "none"
    }
    fn stage(&self) -> RenderStage {
        RenderStage::PostProcess
    }
    fn initialize(&mut self, _: &wgpu::Device, _: &wgpu::Queue, _: wgpu::TextureFormat) {}
    fn render(&mut self, _: &RenderPassContext) {}
}

impl GpuPass {
    /// A pass that draws nothing.
    pub(crate) fn none() -> Self {
        Self::new(NoPass)
    }

    /// Identity of this shared callback, for retained paint fingerprints.
    #[doc(hidden)]
    pub fn identity(&self) -> usize {
        std::sync::Arc::as_ptr(&self.inner) as usize
    }
    /// Wrap a [`CustomRenderPass`] so it can be scheduled via
    /// `DrawContext::run_gpu_pass`. Initialization is deferred to the
    /// first frame that actually dispatches the pass.
    pub fn new<T: CustomRenderPass + 'static>(pass: T) -> Self {
        Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(GpuPassInner {
                pass: Box::new(pass),
                initialized: false,
            })),
        }
    }

    /// Initialize once and execute a captured canvas callback.
    #[doc(hidden)]
    pub fn initialize_and_render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        ctx: &RenderPassContext<'_>,
    ) {
        let mut guard = self.inner.lock().expect("GpuPass mutex poisoned");
        if !guard.initialized {
            guard.pass.initialize(device, queue, format);
            guard.initialized = true;
        }
        if guard.pass.enabled() {
            guard.pass.render(ctx);
        }
    }
}

impl blinc_core::draw::GpuPassHook for GpuPass {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Custom Bind Groups
// ─────────────────────────────────────────────────────────────────────────────

/// Entry type for bind group builder
enum BindGroupEntry<'a> {
    UniformBuffer(wgpu::BindingResource<'a>),
    StorageBuffer {
        resource: wgpu::BindingResource<'a>,
        read_only: bool,
    },
    Texture(&'a wgpu::TextureView),
    StorageTexture {
        view: &'a wgpu::TextureView,
        format: wgpu::TextureFormat,
        access: wgpu::StorageTextureAccess,
    },
    Sampler(&'a wgpu::Sampler),
    ComparisonSampler(&'a wgpu::Sampler),
}

/// Builder for creating custom wgpu bind groups with matching layouts.
///
/// Automatically generates both the `BindGroupLayout` and `BindGroup`
/// with correct binding indices and visibility flags.
pub struct BindGroupBuilder<'a> {
    label: &'a str,
    entries: Vec<BindGroupEntry<'a>>,
    visibility: wgpu::ShaderStages,
}

impl<'a> BindGroupBuilder<'a> {
    /// Create a new bind group builder.
    ///
    /// Default visibility is VERTEX | FRAGMENT. Override per-entry with
    /// `with_visibility()` before adding entries.
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            entries: Vec::new(),
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        }
    }

    /// Set visibility for subsequent entries.
    pub fn with_visibility(mut self, visibility: wgpu::ShaderStages) -> Self {
        self.visibility = visibility;
        self
    }

    /// Add a uniform buffer binding.
    pub fn add_uniform_buffer(&mut self, resource: wgpu::BindingResource<'a>) -> &mut Self {
        self.entries.push(BindGroupEntry::UniformBuffer(resource));
        self
    }

    /// Add a storage buffer binding.
    pub fn add_storage_buffer(
        &mut self,
        resource: wgpu::BindingResource<'a>,
        read_only: bool,
    ) -> &mut Self {
        self.entries.push(BindGroupEntry::StorageBuffer {
            resource,
            read_only,
        });
        self
    }

    /// Add a sampled texture binding (Float, filterable).
    pub fn add_texture(&mut self, view: &'a wgpu::TextureView) -> &mut Self {
        self.entries.push(BindGroupEntry::Texture(view));
        self
    }

    /// Add a storage texture binding (for compute write).
    pub fn add_storage_texture(
        &mut self,
        view: &'a wgpu::TextureView,
        format: wgpu::TextureFormat,
        access: wgpu::StorageTextureAccess,
    ) -> &mut Self {
        self.entries.push(BindGroupEntry::StorageTexture {
            view,
            format,
            access,
        });
        self
    }

    /// Add a filtering sampler binding.
    pub fn add_sampler(&mut self, sampler: &'a wgpu::Sampler) -> &mut Self {
        self.entries.push(BindGroupEntry::Sampler(sampler));
        self
    }

    /// Add a comparison sampler binding (for shadow maps).
    pub fn add_comparison_sampler(&mut self, sampler: &'a wgpu::Sampler) -> &mut Self {
        self.entries
            .push(BindGroupEntry::ComparisonSampler(sampler));
        self
    }

    /// Build both the layout and the bind group.
    pub fn build(self, device: &wgpu::Device) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
        let vis = self.visibility;

        let layout_entries: Vec<wgpu::BindGroupLayoutEntry> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let ty = match entry {
                    BindGroupEntry::UniformBuffer(_) => wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    BindGroupEntry::StorageBuffer { read_only, .. } => wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage {
                            read_only: *read_only,
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    BindGroupEntry::Texture(_) => wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    BindGroupEntry::StorageTexture { format, access, .. } => {
                        wgpu::BindingType::StorageTexture {
                            access: *access,
                            format: *format,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        }
                    }
                    BindGroupEntry::Sampler(_) => {
                        wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)
                    }
                    BindGroupEntry::ComparisonSampler(_) => {
                        wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)
                    }
                };

                wgpu::BindGroupLayoutEntry {
                    binding: i as u32,
                    visibility: vis,
                    ty,
                    count: None,
                }
            })
            .collect();

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(self.label),
            entries: &layout_entries,
        });

        let bind_entries: Vec<wgpu::BindGroupEntry> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: match entry {
                    BindGroupEntry::UniformBuffer(r)
                    | BindGroupEntry::StorageBuffer { resource: r, .. } => r.clone(),
                    BindGroupEntry::Texture(v) | BindGroupEntry::StorageTexture { view: v, .. } => {
                        wgpu::BindingResource::TextureView(v)
                    }
                    BindGroupEntry::Sampler(s) | BindGroupEntry::ComparisonSampler(s) => {
                        wgpu::BindingResource::Sampler(s)
                    }
                },
            })
            .collect();

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(self.label),
            layout: &layout,
            entries: &bind_entries,
        });

        (layout, bind_group)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Compute Dispatch
// ─────────────────────────────────────────────────────────────────────────────

/// Dispatch a compute shader workload.
///
/// Encapsulates the pipeline, bind group, and workgroup dimensions
/// for a single compute dispatch call.
pub struct ComputeDispatch<'a> {
    /// The compute pipeline to execute
    pub pipeline: &'a wgpu::ComputePipeline,
    /// The bind group containing all buffers/textures
    pub bind_group: &'a wgpu::BindGroup,
    /// Number of workgroups in (x, y, z)
    pub workgroups: (u32, u32, u32),
    /// Debug label for GPU profiling
    pub label: &'a str,
}

impl<'a> ComputeDispatch<'a> {
    /// Execute the compute dispatch.
    pub fn execute(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some(self.label),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(self.label),
                timestamp_writes: None,
            });
            pass.set_pipeline(self.pipeline);
            pass.set_bind_group(0, self.bind_group, &[]);
            pass.dispatch_workgroups(self.workgroups.0, self.workgroups.1, self.workgroups.2);
        }

        queue.submit(std::iter::once(encoder.finish()));
    }
}

/// Helper to create a compute pipeline from WGSL source.
pub fn create_compute_pipeline(
    device: &wgpu::Device,
    label: &str,
    wgsl_source: &str,
    entry_point: &str,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some(entry_point),
        compilation_options: Default::default(),
        cache: None,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Post-Processing Effect Chain
// ─────────────────────────────────────────────────────────────────────────────

/// A single post-processing effect in a chain.
///
/// Each effect reads from an input texture and writes to the target.
/// Effects are composable — they chain by reading the previous effect's output.
pub trait PostProcessEffect: Send {
    /// Human-readable label for this effect
    fn label(&self) -> &str;

    /// Create GPU resources for this effect.
    fn initialize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    );

    /// Apply the effect: read from `input`, write to `output`.
    ///
    /// Both textures are the same size as the viewport.
    fn apply(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        width: u32,
        height: u32,
    );

    /// Called when viewport resizes. Recreate size-dependent resources.
    fn resize(&mut self, _device: &wgpu::Device, _width: u32, _height: u32) {}

    /// Whether this effect is currently active.
    fn enabled(&self) -> bool {
        true
    }
}

/// A chain of post-processing effects applied in sequence.
///
/// The chain manages intermediate textures and ping-pongs between them.
/// Register it as a `CustomRenderPass` with `PostProcess` stage.
///
/// # Example
///
/// ```ignore
/// let mut chain = PostProcessChain::new("my_effects");
/// chain.add_effect(Box::new(BloomEffect::new()));
/// chain.add_effect(Box::new(ToneMappingEffect::new()));
/// renderer.register_custom_pass(Box::new(chain));
/// ```
pub struct PostProcessChain {
    label: String,
    effects: Vec<Box<dyn PostProcessEffect>>,
    /// Ping-pong textures for effect chaining
    ping: Option<(wgpu::Texture, wgpu::TextureView)>,
    pong: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Copy pipeline for blitting between textures
    copy_pipeline: Option<wgpu::RenderPipeline>,
    copy_bind_group_layout: Option<wgpu::BindGroupLayout>,
    copy_sampler: Option<wgpu::Sampler>,
    texture_format: wgpu::TextureFormat,
    size: (u32, u32),
}

impl PostProcessChain {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            effects: Vec::new(),
            ping: None,
            pong: None,
            copy_pipeline: None,
            copy_bind_group_layout: None,
            copy_sampler: None,
            texture_format: wgpu::TextureFormat::Bgra8Unorm,
            size: (0, 0),
        }
    }

    /// Add an effect to the end of the chain.
    pub fn add_effect(&mut self, effect: Box<dyn PostProcessEffect>) {
        self.effects.push(effect);
    }

    /// Remove an effect by label.
    pub fn remove_effect(&mut self, label: &str) -> bool {
        let before = self.effects.len();
        self.effects.retain(|e| e.label() != label);
        self.effects.len() < before
    }

    fn create_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    fn ensure_textures(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.size == (width, height) && self.ping.is_some() {
            return;
        }
        self.size = (width, height);
        self.ping = Some(Self::create_texture(
            device,
            width,
            height,
            self.texture_format,
            "postprocess_ping",
        ));
        self.pong = Some(Self::create_texture(
            device,
            width,
            height,
            self.texture_format,
            "postprocess_pong",
        ));
    }

    fn ensure_copy_pipeline(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        if self.copy_pipeline.is_some() {
            return;
        }

        let shader_src = r#"
            @group(0) @binding(0) var src_texture: texture_2d<f32>;
            @group(0) @binding(1) var src_sampler: sampler;

            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) uv: vec2<f32>,
            }

            @vertex
            fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
                var positions = array<vec2<f32>, 6>(
                    vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
                    vec2(-1.0, 1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
                );
                var out: VertexOutput;
                out.position = vec4(positions[vi], 0.0, 1.0);
                out.uv = positions[vi] * vec2(0.5, -0.5) + 0.5;
                return out;
            }

            @fragment
            fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
                return textureSample(src_texture, src_sampler, input.uv);
            }
        "#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PostProcess Copy Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("PostProcess Copy Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("PostProcess Copy Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("PostProcess Copy Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("PostProcess Copy Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        self.copy_pipeline = Some(pipeline);
        self.copy_bind_group_layout = Some(bind_group_layout);
        self.copy_sampler = Some(sampler);
    }

    /// Blit `src` view onto `dst` view using the copy pipeline.
    fn blit(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
    ) {
        let layout = self.copy_bind_group_layout.as_ref().unwrap();
        let sampler = self.copy_sampler.as_ref().unwrap();
        let pipeline = self.copy_pipeline.as_ref().unwrap();

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PostProcess Blit"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("PostProcess Blit"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("PostProcess Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..6, 0..1);
        }

        queue.submit(std::iter::once(encoder.finish()));
    }
}

impl CustomRenderPass for PostProcessChain {
    fn label(&self) -> &str {
        &self.label
    }

    fn stage(&self) -> RenderStage {
        RenderStage::PostProcess
    }

    fn initialize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        self.texture_format = format;
        self.ensure_copy_pipeline(device, format);
        for effect in &mut self.effects {
            effect.initialize(device, queue, format);
        }
    }

    fn render(&mut self, ctx: &RenderPassContext) {
        let active: Vec<usize> = self
            .effects
            .iter()
            .enumerate()
            .filter(|(_, e)| e.enabled())
            .map(|(i, _)| i)
            .collect();

        if active.is_empty() {
            return;
        }

        self.ensure_textures(ctx.device, ctx.viewport_width, ctx.viewport_height);
        let (_, ping_view) = self.ping.as_ref().unwrap();
        let (_, pong_view) = self.pong.as_ref().unwrap();

        // Copy framebuffer → ping (so first effect reads from it)
        self.blit(ctx.device, ctx.queue, ctx.target, ping_view);

        // Chain: ping → pong → ping → pong ...
        // We need references that outlive the loop, but we can't borrow self.ping/pong
        // inside the loop because we also borrow self.effects mutably.
        // Work around by collecting view references ahead of time.
        let views = [ping_view as *const _, pong_view as *const _];

        for (step, &idx) in active.iter().enumerate() {
            let is_last = step == active.len() - 1;
            // Safety: views are valid for the lifetime of self, and we don't modify textures
            let input_view = unsafe { &*(views[step % 2]) };
            let output_view = if is_last {
                ctx.target
            } else {
                unsafe { &*(views[(step + 1) % 2]) }
            };

            self.effects[idx].apply(
                ctx.device,
                ctx.queue,
                input_view,
                output_view,
                ctx.viewport_width,
                ctx.viewport_height,
            );
        }
    }

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.ping = None;
        self.pong = None;
        self.size = (0, 0);
        for effect in &mut self.effects {
            effect.resize(device, width, height);
        }
    }

    fn enabled(&self) -> bool {
        self.effects.iter().any(|e| e.enabled())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Create a GPU buffer initialized with data.
pub fn create_buffer(
    device: &wgpu::Device,
    label: &str,
    data: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: data,
        usage,
    })
}

/// Create a fullscreen triangle-strip render pipeline from WGSL fragment source.
///
/// The shader must define `vs_main` and `fs_main` entry points.
/// Vertex shader should generate a fullscreen quad from `vertex_index`.
pub fn create_fullscreen_pipeline(
    device: &wgpu::Device,
    label: &str,
    wgsl_source: &str,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
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
    })
}
