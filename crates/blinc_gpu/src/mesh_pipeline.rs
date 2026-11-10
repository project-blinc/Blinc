//! 3D mesh rendering pipeline.
//!
//! Holds every resource the mesh pass needs — render pipeline, bind
//! group layouts, default texture fallbacks, IBL cubemap, shadow map,
//! HDR intermediate target, tonemap + bloom passes, and the per-mesh
//! GPU caches (vertex / index buffers, morph deltas, decoded
//! textures). Lazily created on first mesh draw via
//! `GpuRenderer::ensure_mesh_pipeline`.
//!
//! Separated from `renderer.rs` to keep that file centred on
//! frame-level orchestration. The `impl GpuRenderer` method blocks
//! that drive this pipeline live in the same module (see the
//! bottom of this file) — Rust's cross-file impl blocks keep
//! `GpuRenderer`'s private fields accessible once they're bumped
//! to `pub(crate)`.

#![allow(clippy::too_many_arguments)]

use wgpu::util::DeviceExt;

/// Maximum number of distinct meshes whose GPU buffers / textures we
/// keep warm between frames. When exceeded, the FIFO eviction policy
/// drops the oldest entry. Sized conservatively — 128 fits every
/// asset in the workspace examples, scales to large editor scenes,
/// and caps worst-case GPU memory at ~`128 × per-mesh-footprint`.
pub(crate) const MESH_CACHE_CAPACITY: usize = 128;

/// Upper bound on morph targets per mesh. CuteGirl G1 (current
/// stress-test asset) tops out at 152 on its face mesh; 256 covers
/// that plus headroom for rigs with denser blend-shape sets.
/// Storage cost: `256 × 4 bytes = 1 KB` per draw for the weights
/// buffer, negligible.
pub(crate) const MAX_MORPH_TARGETS: usize = 256;

/// FIFO-evicted cache size for per-mesh morph-delta storage
/// buffers. A face mesh with 152 targets × 5 K verts × 6 floats is
/// ~18 MB; at 32 entries cached, worst-case GPU footprint is
/// ~576 MB. Big but bounded — scenes with dozens of morph meshes
/// are rare.
pub(crate) const MORPH_CACHE_CAPACITY: usize = 32;

/// Per-mesh cached vertex + index GPU buffers.
pub(crate) struct MeshBufferCacheEntry {
    pub(crate) vertex: wgpu::Buffer,
    pub(crate) index: wgpu::Buffer,
    pub(crate) index_count: u32,
}

/// Upper bound on directional lights the mesh shader will sample.
/// Matches the `DirLight` array size in mesh.wgsl.
pub const MAX_DIR_LIGHTS: usize = 4;

/// A single directional light for the mesh shader. `direction`
/// points from the light toward the scene (i.e. the light's ray
/// travel direction; the shader negates it to get the surface→light
/// vector `L`).
#[derive(Clone, Copy, Debug)]
pub struct DirectionalLight {
    pub direction: [f32; 3],
    pub intensity: f32,
    pub color: [f32; 3],
}

impl DirectionalLight {
    pub const DEFAULT: Self = Self {
        direction: [0.0, -1.0, 0.3],
        intensity: 0.8,
        color: [1.0, 1.0, 1.0],
    };
}

/// Lazily-created 3D mesh rendering pipeline.
pub(crate) struct MeshPipeline {
    pub(crate) pipeline: wgpu::RenderPipeline,
    /// Weighted Blended OIT accumulation pipeline (McGuire & Bavoil
    /// 2013). Selected for `AlphaMode::Blend` materials. Same vertex
    /// stage as `pipeline`, fragment entry point is `fs_main_oit`,
    /// and the color targets are the `oit_accum_view` / `oit_reveal_view`
    /// intermediates — NOT the HDR target.
    ///
    /// Depth test is on (opaque geometry in front still occludes
    /// BLEND fragments) but depth write is off (BLEND fragments
    /// don't occlude each other — OIT handles ordering statistically,
    /// so we don't depend on submission order or per-triangle sort).
    ///
    /// The resulting accum/reveal pair is composited over the HDR
    /// target by `oit_composite_pipeline` once all BLEND draws for
    /// the frame have landed (on `is_last`).
    ///
    /// `None` without storage buffers (the WebGL2 path), where the
    /// per-target blend states it needs are unavailable: BLEND
    /// materials then draw through `pipeline` with premultiplied alpha
    /// and the OIT passes are skipped.
    pub(crate) oit_accum_pipeline: Option<wgpu::RenderPipeline>,
    /// Fullscreen composite pass that reads `oit_accum_view` and
    /// `oit_reveal_view` and blends the OIT result over the HDR
    /// intermediate. Runs once per frame on the last mesh, before
    /// bloom and tonemap.
    pub(crate) oit_composite_pipeline: wgpu::RenderPipeline,
    pub(crate) oit_composite_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) uniform_buffer: wgpu::Buffer,
    pub(crate) material_buffer: wgpu::Buffer,
    /// Default 1x1 white texture (used when material has no texture).
    pub(crate) default_texture: crate::image::GpuImage,
    /// Default flat normal map (128, 128, 255 = tangent-space up).
    pub(crate) default_normal_map: crate::image::GpuImage,
    /// Default black displacement map (no displacement).
    pub(crate) default_displacement: crate::image::GpuImage,
    /// Default 1x1 white metallic-roughness texture. Bound when the
    /// material has no MR texture; multiplying scalar metallic ×
    /// roughness by (1,1,1,1) produces the scalar-only path.
    pub(crate) default_metallic_roughness: crate::image::GpuImage,
    /// Default 1x1 white emissive texture. Bound when the material
    /// has no emissive texture; the shader gates on the
    /// `has_emissive_texture` flag so the default is only read for
    /// layout validation.
    pub(crate) default_emissive: crate::image::GpuImage,
    /// Default 1x1 white occlusion texture. Same rationale as the
    /// other defaults.
    pub(crate) default_occlusion: crate::image::GpuImage,
    pub(crate) sampler: wgpu::Sampler,
    /// Storage buffer for joint matrices (skeletal animation, max 256 joints).
    /// Used when `has_storage_buffers` is true.
    pub(crate) joint_buffer: wgpu::Buffer,
    /// Data-texture fallback for joint matrices (WebGL2 DT mode).
    /// Width=4 (one texel per mat4 row), height=num_joints, RGBA32Float.
    pub(crate) joint_data_texture: Option<wgpu::Texture>,
    pub(crate) joint_data_view: Option<wgpu::TextureView>,
    /// Per-draw morph-weights storage buffer. Holds up to
    /// [`MAX_MORPH_TARGETS`] floats — the mesh's current weights are
    /// copied in each draw.
    pub(crate) morph_weights_buffer: wgpu::Buffer,
    /// Dummy delta buffer (one zeroed vec4 pair) for meshes without
    /// morph targets. Bound so the bind-group layout is satisfied;
    /// `morph_target_count = 0` in the uniform guarantees the shader
    /// never reads it.
    pub(crate) morph_deltas_dummy: wgpu::Buffer,
    /// Per-mesh cache of uploaded morph-delta storage buffers, keyed
    /// by the `Arc<MeshData>` raw pointer. Deltas are static after
    /// glTF parse so the upload happens once per mesh, reused across
    /// frames. FIFO-evicted at [`MORPH_CACHE_CAPACITY`].
    pub(crate) morph_deltas_cache: std::collections::VecDeque<(usize, wgpu::Buffer)>,
    /// Depth buffer for the main mesh pass.
    pub(crate) main_depth: Option<wgpu::Texture>,
    pub(crate) main_depth_view: Option<wgpu::TextureView>,
    pub(crate) main_depth_size: (u32, u32),
    /// Shadow depth pass pipeline.
    pub(crate) shadow_pipeline: wgpu::RenderPipeline,
    pub(crate) shadow_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) shadow_uniform_buffer: wgpu::Buffer,
    /// 2048x2048 Depth32Float shadow map texture.
    pub(crate) shadow_map: wgpu::Texture,
    pub(crate) shadow_view: wgpu::TextureView,
    pub(crate) shadow_sampler: wgpu::Sampler,
    /// Procedural environment cubemap for IBL reflections.
    pub(crate) env_cubemap: wgpu::Texture,
    pub(crate) env_cubemap_view: wgpu::TextureView,
    pub(crate) env_sampler: wgpu::Sampler,
    /// Per-mesh vertex + index buffer cache. Keyed by the raw pointer
    /// of the mesh's `Arc<MeshData>`. FIFO-evicted at
    /// `MESH_CACHE_CAPACITY` to bound GPU memory for streaming scenes.
    pub(crate) cached_mesh_buffers: std::collections::HashMap<usize, MeshBufferCacheEntry>,
    pub(crate) cached_mesh_buffer_keys: std::collections::VecDeque<usize>,
    /// GPU texture cache, keyed by `TextureData::cache_key()`.
    /// Materials that reference the same underlying image share a
    /// single `GpuImage`.
    pub(crate) cached_gpu_images: std::collections::HashMap<usize, crate::image::GpuImage>,
    pub(crate) cached_gpu_image_keys: std::collections::VecDeque<usize>,
    /// Skybox pipeline — renders the environment cubemap as a
    /// background behind the mesh. Shares the cubemap texture/sampler
    /// but has its own bind group layout (camera vectors + cubemap).
    pub(crate) skybox_pipeline: wgpu::RenderPipeline,
    pub(crate) skybox_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) skybox_uniform_buffer: wgpu::Buffer,
    /// HDR intermediate texture (`Rgba16Float`). Meshes render here
    /// instead of the final framebuffer so specular + emissive above
    /// 1.0 accumulate without clipping; tonemap pass reads it.
    pub(crate) hdr_texture: Option<wgpu::Texture>,
    pub(crate) hdr_view: Option<wgpu::TextureView>,
    pub(crate) hdr_size: (u32, u32),
    /// OIT accumulation target (Rgba16Float). Sized to match the HDR
    /// intermediate. Holds Σ(premultiplied_color * w, alpha * w)
    /// across all overlapping BLEND fragments for the current frame.
    pub(crate) oit_accum_texture: Option<wgpu::Texture>,
    pub(crate) oit_accum_view: Option<wgpu::TextureView>,
    /// OIT reveal target (R8Unorm). Cleared to 1.0 at frame start;
    /// each BLEND fragment multiplies it by (1 - alpha) via a
    /// ZERO / OneMinusSrc blend factor. After all BLEND fragments
    /// land, reveal.r = Π(1 - α_i) — the "how much of the background
    /// is still visible" factor used by the composite pass.
    pub(crate) oit_reveal_texture: Option<wgpu::Texture>,
    pub(crate) oit_reveal_view: Option<wgpu::TextureView>,
    pub(crate) oit_size: (u32, u32),
    pub(crate) oit_composite_sampler: wgpu::Sampler,
    /// Fullscreen ACES tonemap pipeline + resources.
    pub(crate) tonemap_pipeline: wgpu::RenderPipeline,
    pub(crate) tonemap_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) tonemap_sampler: wgpu::Sampler,
    /// Bloom pipeline — shared for threshold-downsample and Kawase blur.
    pub(crate) bloom_pipeline: wgpu::RenderPipeline,
    pub(crate) bloom_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) bloom_uniform_buffer: wgpu::Buffer,
    /// Two half-res Rgba16Float ping-pong textures for bloom blur.
    pub(crate) bloom_a: Option<wgpu::Texture>,
    pub(crate) bloom_a_view: Option<wgpu::TextureView>,
    pub(crate) bloom_b: Option<wgpu::Texture>,
    pub(crate) bloom_b_view: Option<wgpu::TextureView>,
    pub(crate) bloom_size: (u32, u32),
}

// ─────────────────────────────────────────────────────────────────────────────
// `impl GpuRenderer` blocks for every mesh-pipeline method
// ─────────────────────────────────────────────────────────────────────────────
//
// Kept as methods on `GpuRenderer` (not `MeshPipeline`) so call sites
// stay `self.render_mesh_data_batched(...)` — the whole rendering
// path is driven through the renderer object. Rust's cross-file impl
// blocks make this split transparent: same crate, same privacy
// semantics, just a different file.

use crate::renderer::{GpuRenderer, SHADOW_MAP_SIZE, mat4_inverse_flat};
use crate::shaders::MESH_DT_SHADER;

impl MeshPipeline {
    /// Drop the window-sized targets (depth, HDR, OIT, bloom); the next mesh draw makes them
    /// again. The pipelines, the shadow map and the environment map stay.
    pub(crate) fn release_targets(&mut self) {
        self.main_depth = None;
        self.main_depth_view = None;
        self.hdr_texture = None;
        self.hdr_view = None;
        self.oit_accum_texture = None;
        self.oit_accum_view = None;
        self.oit_reveal_texture = None;
        self.oit_reveal_view = None;
        self.bloom_a = None;
        self.bloom_a_view = None;
        self.bloom_b = None;
        self.bloom_b_view = None;
    }
}

impl GpuRenderer {
    fn ensure_mesh_pipeline(&mut self) {
        if self.mesh_pipeline.is_some() {
            return;
        }

        // ── Main mesh shader ─────────────────────────────────────────────
        let shader_src = if self.has_storage_buffers {
            include_str!("shaders/mesh.wgsl")
        } else {
            MESH_DT_SHADER
        };
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Mesh Shader"),
                source: wgpu::ShaderSource::Wgsl(shader_src.into()),
            });

        // Bindings 0..=13 are shared; 14 and 15 are morph-target storage buffers that only the
        // storage-buffer shader reads, so the WebGL2 layout stops at 13.
        let layout_entries = [
            // 0: Uniforms (view_proj, model, light_view_proj, camera, light, flags)
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // 1: Material uniforms
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // 2: Base color texture
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            // 3: Texture sampler (shared for base color, normal, displacement)
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // 4: Normal map texture
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            // 5: Shadow map (depth texture)
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Depth,
                },
                count: None,
            },
            // 6: Shadow comparison sampler
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
            // 7: Displacement / height map texture
            wgpu::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            // 8: Joint matrices — storage buffer (normal) or texture (WebGL2 DT)
            wgpu::BindGroupLayoutEntry {
                binding: 8,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: if self.has_storage_buffers {
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    }
                } else {
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    }
                },
                count: None,
            },
            // 9: Metallic / roughness texture (glTF convention:
            // metallic in .b, roughness in .g, multiplied per-texel
            // by the scalar factors in MaterialUniforms).
            wgpu::BindGroupLayoutEntry {
                binding: 9,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            // 10: Emissive texture — multiplied per-texel by the
            // scalar `emissive` RGB in MaterialUniforms. Used for
            // self-lit surfaces (HUD glyphs, LED panels, etc.).
            wgpu::BindGroupLayoutEntry {
                binding: 10,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            // 11: Ambient occlusion texture — .r channel attenuates
            // the ambient/indirect diffuse term. `occlusion_strength`
            // in MaterialUniforms controls how much AO applies.
            wgpu::BindGroupLayoutEntry {
                binding: 11,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            // 12: Environment cubemap for IBL reflections
            wgpu::BindGroupLayoutEntry {
                binding: 12,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::Cube,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            // 13: Environment cubemap sampler
            wgpu::BindGroupLayoutEntry {
                binding: 13,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // 14: Morph-target deltas (positions + normals,
            // interleaved two-vec4-per-(target, vertex)).
            // Vertex stage only — the vertex shader's
            // per-target loop is the sole consumer.
            wgpu::BindGroupLayoutEntry {
                binding: 14,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // 15: Morph-target weights — one f32 per target,
            // updated per draw from the pose's weight
            // sink.
            wgpu::BindGroupLayoutEntry {
                binding: 15,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        let bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Mesh Bind Group Layout"),
                    entries: if self.has_storage_buffers {
                        &layout_entries
                    } else {
                        &layout_entries[..14]
                    },
                });

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Mesh Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let vertex_stride = std::mem::size_of::<blinc_core::draw::Vertex>() as u64;
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: vertex_stride,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position: vec3<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                // normal: vec3<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
                // uv: vec2<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 24,
                    shader_location: 2,
                },
                // color: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 3,
                },
                // tangent: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 4,
                },
                // joints: vec4<u32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32x4,
                    offset: 64,
                    shader_location: 5,
                },
                // weights: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 80,
                    shader_location: 6,
                },
            ],
        };

        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Mesh Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: std::slice::from_ref(&vertex_layout),
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    // Target the HDR intermediate (Rgba16Float), not the
                    // sRGB framebuffer — this preserves values above 1.0
                    // for specular and emissive. The tonemap pass later
                    // maps the HDR range down to the framebuffer's format.
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        // Premultiplied-alpha blending: the shader
                        // premultiplies `reflected * alpha` but leaves
                        // `emissive` un-attenuated so self-emitted
                        // light is preserved through transparent
                        // surfaces. `BlendState::ALPHA_BLENDING` would
                        // scale the whole final color by alpha,
                        // killing emissive glows on any BLEND material
                        // whose base-color mask has low alpha (e.g.
                        // buster_drone's body decal region around the
                        // nose).
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    // Culling disabled — glTF allows per-material
                    // `doubleSided`, but the mesh pipeline currently
                    // can't switch cull state per-draw. Some assets
                    // author inward-facing winding on interior
                    // components (shells that are meant to be
                    // doubleSided) and cull_mode: Back makes those
                    // geometries silently invisible. Rendering
                    // backfaces too is marginally more expensive but
                    // correct for every asset; making it material-
                    // driven would need a second pipeline variant.
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // Weighted Blended OIT accumulation pipeline. Targets the
        // `oit_accum` (Rgba16Float) and `oit_reveal` (R8Unorm)
        // intermediates via MRT; writes premultiplied-color × weight
        // into accum (additive) and alpha into reveal (combined with
        // a `ZERO, OneMinusSrc` blend to produce ∏(1 − α_i)).
        //
        // Depth test on, depth write off: opaque geometry in front
        // still occludes, but BLEND fragments don't occlude each
        // other — OIT handles ordering without a per-triangle sort.
        let oit_accum_pipeline = self.has_storage_buffers.then(|| {
            self.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Mesh Pipeline (OIT Accum)"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: std::slice::from_ref(&vertex_layout),
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main_oit"),
                        targets: &[
                            // @location(0) accum — additive,
                            // Rgba16Float sum of c*α*w and α*w.
                            Some(wgpu::ColorTargetState {
                                format: wgpu::TextureFormat::Rgba16Float,
                                blend: Some(wgpu::BlendState {
                                    color: wgpu::BlendComponent {
                                        src_factor: wgpu::BlendFactor::One,
                                        dst_factor: wgpu::BlendFactor::One,
                                        operation: wgpu::BlendOperation::Add,
                                    },
                                    alpha: wgpu::BlendComponent {
                                        src_factor: wgpu::BlendFactor::One,
                                        dst_factor: wgpu::BlendFactor::One,
                                        operation: wgpu::BlendOperation::Add,
                                    },
                                }),
                                write_mask: wgpu::ColorWrites::ALL,
                            }),
                            // @location(1) reveal — multiplicative
                            // transmission. Initial clear value is
                            // 1.0; each fragment's α shrinks it via
                            // `dst = 0 * src + (1 - src) * dst`.
                            Some(wgpu::ColorTargetState {
                                format: wgpu::TextureFormat::R8Unorm,
                                blend: Some(wgpu::BlendState {
                                    color: wgpu::BlendComponent {
                                        src_factor: wgpu::BlendFactor::Zero,
                                        dst_factor: wgpu::BlendFactor::OneMinusSrc,
                                        operation: wgpu::BlendOperation::Add,
                                    },
                                    alpha: wgpu::BlendComponent {
                                        src_factor: wgpu::BlendFactor::Zero,
                                        dst_factor: wgpu::BlendFactor::OneMinusSrc,
                                        operation: wgpu::BlendOperation::Add,
                                    },
                                }),
                                write_mask: wgpu::ColorWrites::RED,
                            }),
                        ],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: wgpu::TextureFormat::Depth32Float,
                        depth_write_enabled: false,
                        depth_compare: wgpu::CompareFunction::Less,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                })
        });

        // ── OIT composite pipeline ──────────────────────────────────────
        //
        // Fullscreen pass that reads `oit_accum` + `oit_reveal` and
        // blends the WBOIT result over the HDR intermediate. Runs
        // once per frame (on is_last, before bloom/tonemap).
        let oit_composite_shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("OIT Composite Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/oit_composite.wgsl").into()),
            });
        let oit_composite_bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("OIT Composite BGL"),
                    entries: &[
                        // 0: accum texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            },
                            count: None,
                        },
                        // 1: reveal texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            },
                            count: None,
                        },
                        // 2: sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                            count: None,
                        },
                    ],
                });
        let oit_composite_layout =
            self.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("OIT Composite Pipeline Layout"),
                    bind_group_layouts: &[&oit_composite_bind_group_layout],
                    push_constant_ranges: &[],
                });
        let oit_composite_pipeline =
            self.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("OIT Composite Pipeline"),
                    layout: Some(&oit_composite_layout),
                    vertex: wgpu::VertexState {
                        module: &oit_composite_shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &oit_composite_shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Rgba16Float,
                            // Over-operator with premultiplied-coverage
                            // source: final = src.rgb * src.a + dst * (1 - src.a)
                            blend: Some(wgpu::BlendState {
                                color: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::SrcAlpha,
                                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                    operation: wgpu::BlendOperation::Add,
                                },
                                alpha: wgpu::BlendComponent {
                                    src_factor: wgpu::BlendFactor::One,
                                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                    operation: wgpu::BlendOperation::Add,
                                },
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        cull_mode: None,
                        ..Default::default()
                    },
                    // No depth attachment — this is a fullscreen pass
                    // writing straight into HDR.
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

        let oit_composite_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("OIT Composite Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            // Texel-perfect fetch — the OIT intermediates are 1:1
            // with the HDR target, no filtering needed.
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // 3x mat4 (192) + camera+pad (16) + lights[4] (128) + tail (64) = 400 bytes.
        let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mesh Uniforms"),
            size: 400,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Material uniform layout:
        //   base_color: vec4           16
        //   metallic_roughness: vec2    8
        //   _pad_mr: f32 * 2            8
        //   emissive: vec3 + pad       16  (WGSL vec3 is 16-byte aligned)
        //   has_mr_texture: f32         4
        //   has_emissive_texture: f32   4
        //   has_occlusion_texture: f32  4
        //   occlusion_strength: f32     4
        //   alpha_mode: f32             4
        //   alpha_cutoff: f32           4
        //   _pad_am: f32 * 2            8
        //   uv_transform_matrix: vec4  16  (KHR_texture_transform)
        //   uv_transform_offset: vec2   8
        //   _pad_uv: f32 * 2            8
        //                              ──
        //                             112 bytes total.
        //
        // Must match `MaterialGpu` in the write path below AND
        // `MaterialUniforms` in mesh.wgsl / mesh_dt.wgsl exactly —
        // wgpu validates the buffer size against the write, so a
        // missed bump here panics at the first `write_buffer` call
        // (seen with the 96 → 112 jump when KHR_texture_transform
        // shipped).
        let material_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mesh Material"),
            size: 112,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Default 1x1 white texture for untextured meshes
        let default_texture = crate::image::GpuImage::from_rgba(
            &self.device,
            &self.queue,
            &[255, 255, 255, 255],
            1,
            1,
            Some("mesh_default_tex"),
        );

        // Default flat normal map (tangent-space up = 128,128,255)
        let default_normal_map = crate::image::GpuImage::from_rgba(
            &self.device,
            &self.queue,
            &[128, 128, 255, 255],
            1,
            1,
            Some("mesh_default_normal"),
        );

        // Default black displacement (no displacement)
        let default_displacement = crate::image::GpuImage::from_rgba(
            &self.device,
            &self.queue,
            &[0, 0, 0, 255],
            1,
            1,
            Some("mesh_default_displacement"),
        );

        // Default white metallic/roughness. Bound whenever the material
        // has no MR texture — the shader gates the sample on the
        // `has_metallic_roughness_texture` flag and uses only the scalar
        // factors in that branch, so the texture itself is only read for
        // bind-group layout validation, never sampled.
        let default_metallic_roughness = crate::image::GpuImage::from_rgba(
            &self.device,
            &self.queue,
            &[255, 255, 255, 255],
            1,
            1,
            Some("mesh_default_metallic_roughness"),
        );

        // Default white emissive texture — same gating as above.
        let default_emissive = crate::image::GpuImage::from_rgba(
            &self.device,
            &self.queue,
            &[255, 255, 255, 255],
            1,
            1,
            Some("mesh_default_emissive"),
        );

        // Default white occlusion texture — 1.0 means "no occlusion",
        // so even if a caller accidentally leaves the flag on the AO
        // term reduces to the multiplicative identity.
        let default_occlusion = crate::image::GpuImage::from_rgba(
            &self.device,
            &self.queue,
            &[255, 255, 255, 255],
            1,
            1,
            Some("mesh_default_occlusion"),
        );

        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Mesh Sampler"),
            // Trilinear filtering for PBR textures at distance. glTF
            // exporters frequently author one set of textures intended
            // to be used at multiple distances; without mipmap_filter
            // set, min_filter alone gives aliased highlights on
            // metallic-roughness maps viewed at steep angles.
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            // REPEAT wrap matches the glTF spec default and what
            // buster_drone's sampler explicitly requests (wrapS/T =
            // GL_REPEAT = 10497). ClampToEdge (the wgpu default) breaks
            // tiled terrain textures — their UVs often run past [0,1]
            // to tile — and also subtly breaks body meshes whose UV
            // shells straddle the 0/1 seam. The mesh loader no longer
            // clamps UVs to [0,1]; the sampler handles repeat natively.
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            ..Default::default()
        });

        // Joint matrices — identity matrix as default (no skinning)
        // Max 256 joints * 64 bytes per mat4x4 = 16384 bytes
        let identity: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let joint_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Mesh Joint Matrices"),
                contents: bytemuck::cast_slice(&identity),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });

        // DT mode: create a RGBA32Float texture for joint matrices.
        // Width=4 (one texel per mat4 row), height=256 (max joints).
        // Initialised with the identity matrix at row 0.
        let (joint_data_texture, joint_data_view) = if !self.has_storage_buffers {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Mesh Joint Data Texture"),
                size: wgpu::Extent3d {
                    width: 4,
                    height: 256,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            // Upload identity matrix at row 0: 4 texels (one per mat4 row)
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&identity),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * 16), // 4 texels * 16 bytes per RGBA32F texel
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: 4,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            (Some(tex), Some(view))
        } else {
            (None, None)
        };

        // ── Shadow pipeline ──────────────────────────────────────────────
        let shadow_shader_src = include_str!("shaders/shadow.wgsl");
        let shadow_shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Shadow Shader"),
                source: wgpu::ShaderSource::Wgsl(shadow_shader_src.into()),
            });

        let shadow_bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Shadow Bind Group Layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let shadow_pipeline_layout =
            self.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Shadow Pipeline Layout"),
                    bind_group_layouts: &[&shadow_bind_group_layout],
                    push_constant_ranges: &[],
                });

        // Shadow pass only needs position (first attribute), but uses same vertex buffer
        let shadow_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: vertex_stride,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            }],
        };

        let shadow_pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Shadow Pipeline"),
                layout: Some(&shadow_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shadow_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[shadow_vertex_layout],
                    compilation_options: Default::default(),
                },
                fragment: None, // depth-only pass
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: Some(wgpu::Face::Front), // front-face culling reduces shadow acne
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // light_view_proj + model = 128 bytes
        let shadow_uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shadow Uniforms"),
            size: 128,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Shadow map depth texture
        let shadow_map = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shadow Map"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_map.create_view(&wgpu::TextureViewDescriptor::default());

        // Comparison sampler for PCF shadow sampling
        let shadow_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow Comparison Sampler"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── IBL environment cubemap ─────────────────────────────────────
        //
        // The cubemap starts as a neutral gray fallback (0.15, 0.15, 0.15
        // in f16). SceneKit3D provides the real studio environment via
        // `set_environment_cubemap` → `upload_environment_cubemap`, which
        // overwrites the fallback on the first frame that has 3D content.
        let env_size: u32 = 128;
        let env_mip_count = (env_size as f32).log2() as u32 + 1;
        let env_cubemap = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("IBL Environment Cubemap"),
            size: wgpu::Extent3d {
                width: env_size,
                height: env_size,
                depth_or_array_layers: 6,
            },
            mip_level_count: env_mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Fill every face × mip with near-white linear gray.
        //
        // Tuning history:
        //   - `0.15` — original. Looked fine because the render
        //     path had an sRGB-decoding bug that made every lit
        //     diffuse pixel ~2.2× too bright.
        //   - `0.33` — post-gamma-fix compensation. Restored
        //     brightness on skin/clothing but dark materials
        //     (black cloth, leather) still read near-pitch-black
        //     because ambient × dark baseColor collapses to
        //     noise.
        //   - `0.75` — matches the "bright studio" look modern
        //     glTF viewers default to (Sketchfab, three.js
        //     `RoomEnvironment`, Babylon studio preset). Lifts
        //     shadowed areas on dark materials via IBL without
        //     over-exposing skin, because the multiply by
        //     `baseColor` naturally attenuates ambient for
        //     darker surfaces.
        //
        // f32_to_f16(0.75) = 0x3A00.
        let gray_r = 0x3A00u16; // 0.75
        let gray_a = 0x3C00u16; // 1.0
        for face in 0..6u32 {
            for mip in 0..env_mip_count {
                let mip_size = (env_size >> mip).max(1);
                let texels = (mip_size * mip_size) as usize;
                let mut data = Vec::with_capacity(texels * 8);
                for _ in 0..texels {
                    data.extend_from_slice(&gray_r.to_le_bytes());
                    data.extend_from_slice(&gray_r.to_le_bytes());
                    data.extend_from_slice(&gray_r.to_le_bytes());
                    data.extend_from_slice(&gray_a.to_le_bytes());
                }
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &env_cubemap,
                        mip_level: mip,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: face,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(mip_size * 8),
                        rows_per_image: Some(mip_size),
                    },
                    wgpu::Extent3d {
                        width: mip_size,
                        height: mip_size,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        let env_cubemap_view = env_cubemap.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        let env_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("IBL Environment Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Skybox pipeline ──────────────────────────────────────────────
        //
        // The skybox is a fixed screen-space gradient (not a sky dome),
        // so it has no bindings — no camera, no cubemap. Empty layout.
        let skybox_bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Skybox Bind Group Layout"),
                    entries: &[],
                });

        let skybox_pipeline = crate::custom_pass::create_fullscreen_pipeline(
            &self.device,
            "Skybox Pipeline",
            include_str!("shaders/skybox.wgsl"),
            wgpu::TextureFormat::Rgba16Float,
            &skybox_bind_group_layout,
        );

        let skybox_uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Skybox Uniforms"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Tonemap pipeline ────────────────────────────────────────────
        let tonemap_bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Tonemap Bind Group Layout"),
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
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            },
                            count: None,
                        },
                    ],
                });

        let tonemap_pipeline = crate::custom_pass::create_fullscreen_pipeline(
            &self.device,
            "Tonemap Pipeline",
            include_str!("shaders/tonemap.wgsl"),
            self.texture_format,
            &tonemap_bind_group_layout,
        );

        let tonemap_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Tonemap Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Bloom pipeline ──────────────────────────────────────────────
        let bloom_bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Bloom Bind Group Layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });

        let bloom_pipeline = crate::custom_pass::create_fullscreen_pipeline(
            &self.device,
            "Bloom Pipeline",
            include_str!("shaders/bloom.wgsl"),
            wgpu::TextureFormat::Rgba16Float,
            &bloom_bind_group_layout,
        );

        let bloom_uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Bloom Uniforms"),
            size: 16, // vec2 texel_size + f32 threshold + f32 mode
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let morph_weights_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mesh Morph Weights"),
            size: (MAX_MORPH_TARGETS * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // 32 bytes = one (pos_delta, nrm_delta) vec4 pair. Enough to
        // satisfy the storage-binding layout when the mesh has no
        // morph targets — the shader never reads it because
        // `morph_target_count = 0` short-circuits the loop.
        let morph_deltas_dummy = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mesh Morph Deltas (dummy)"),
            size: 32,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        self.mesh_pipeline = Some(MeshPipeline {
            pipeline,
            oit_accum_pipeline,
            oit_composite_pipeline,
            oit_composite_bind_group_layout,
            bind_group_layout,
            uniform_buffer,
            material_buffer,
            default_texture,
            default_normal_map,
            default_displacement,
            default_metallic_roughness,
            default_emissive,
            default_occlusion,
            sampler,
            joint_buffer,
            joint_data_texture,
            joint_data_view,
            morph_weights_buffer,
            morph_deltas_dummy,
            morph_deltas_cache: std::collections::VecDeque::with_capacity(MORPH_CACHE_CAPACITY),
            main_depth: None,
            main_depth_view: None,
            main_depth_size: (0, 0),
            shadow_pipeline,
            shadow_bind_group_layout,
            shadow_uniform_buffer,
            shadow_map,
            shadow_view,
            shadow_sampler,
            env_cubemap,
            env_cubemap_view,
            env_sampler,
            cached_mesh_buffers: std::collections::HashMap::new(),
            cached_mesh_buffer_keys: std::collections::VecDeque::new(),
            cached_gpu_images: std::collections::HashMap::new(),
            cached_gpu_image_keys: std::collections::VecDeque::new(),
            skybox_pipeline,
            skybox_bind_group_layout,
            skybox_uniform_buffer,
            hdr_texture: None,
            hdr_view: None,
            hdr_size: (0, 0),
            oit_accum_texture: None,
            oit_accum_view: None,
            oit_reveal_texture: None,
            oit_reveal_view: None,
            oit_size: (0, 0),
            oit_composite_sampler,
            tonemap_pipeline,
            tonemap_bind_group_layout,
            tonemap_sampler,
            bloom_pipeline,
            bloom_bind_group_layout,
            bloom_uniform_buffer,
            bloom_a: None,
            bloom_a_view: None,
            bloom_b: None,
            bloom_b_view: None,
            bloom_size: (0, 0),
        });
    }

    /// Upload externally-generated cubemap face/mip data into the mesh
    /// pipeline's environment cubemap texture. Requires `ensure_mesh_pipeline`
    /// to have been called first (the texture must already exist).
    pub fn upload_environment_cubemap(&mut self, data: &blinc_core::layer::CubemapData) {
        // Ensure the mesh pipeline (and its env cubemap texture) exists
        // before writing to it. Without this, the first frame of any
        // scene silently drops the upload — the pipeline is normally
        // lazy-created inside `render_mesh_data_batched`, which runs
        // AFTER `dispatch_pending_meshes` calls this function. Result:
        // the default procedural gray cubemap wins forever even if the
        // user called `SceneKit3D::set_hdri(...)` later.
        self.ensure_mesh_pipeline();
        let mp = match self.mesh_pipeline.as_mut() {
            Some(mp) => mp,
            None => return,
        };

        let expected = (6 * data.mip_count) as usize;
        if data.faces.len() != expected {
            tracing::warn!(
                "upload_environment_cubemap: expected {} face entries, got {}",
                expected,
                data.faces.len()
            );
            return;
        }

        // Recreate the cubemap texture if the incoming size differs
        // from the current one (e.g. procedural 128 → HDRI 256).
        let current_size = mp.env_cubemap.size();
        if current_size.width != data.size || current_size.height != data.size {
            let env_mip_count = (data.size as f32).log2() as u32 + 1;
            let new_tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("IBL Environment Cubemap"),
                size: wgpu::Extent3d {
                    width: data.size,
                    height: data.size,
                    depth_or_array_layers: 6,
                },
                mip_level_count: env_mip_count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            mp.env_cubemap_view = new_tex.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            });
            mp.env_cubemap = new_tex;
        }

        for face in 0..6u32 {
            for mip in 0..data.mip_count {
                let mip_size = (data.size >> mip).max(1);
                let idx = (face * data.mip_count + mip) as usize;
                let face_data = &data.faces[idx];
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &mp.env_cubemap,
                        mip_level: mip,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: face,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    face_data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(mip_size * 8),
                        rows_per_image: Some(mip_size),
                    },
                    wgpu::Extent3d {
                        width: mip_size,
                        height: mip_size,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
    }

    /// Render a mesh's shadow depth into the global shadow map.
    ///
    /// Must be called for every shadow-casting mesh in the frame BEFORE
    /// any main-pass draw runs, so that when [`Self::render_mesh_data_batched`]
    /// samples the shadow map it sees the fully populated scene depth.
    ///
    /// `shadow_batch_index` controls the shadow map's depth load op:
    /// - `0` → `Clear(1.0)` (reset the whole map for the new frame)
    /// - `>0` → `Load` (accumulate this mesh's depth alongside earlier
    ///   shadow casters)
    ///
    /// Caller responsibility: only invoke this for meshes whose material
    /// has `casts_shadows == true`, and count them consecutively starting
    /// from zero. The receiver-side `receives_shadows` flag is handled
    /// entirely in `render_mesh_data_batched` via the `light_view_proj`
    /// parameter.
    #[allow(clippy::too_many_arguments)]
    pub fn render_mesh_shadow_pass(
        &mut self,
        mesh: &std::sync::Arc<blinc_core::draw::MeshData>,
        transform: &[f32; 16],
        light_view_proj: &[f32; 16],
        shadow_batch_index: usize,
    ) {
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            return;
        }
        self.ensure_mesh_pipeline();

        // Reuse the same vertex/index cache the main pass uses. Key
        // on the INNER `Arc<Vec<Vertex>>` pointer — stable across
        // shallow `MeshData` clones (the common pattern where demos
        // do `Arc::new(per_draw)` per frame to stamp fresh skinning /
        // morph weights). Keying on the outer `Arc<MeshData>`
        // pointer instead uploads a fresh vertex/index buffer every
        // frame; those buffers can't be freed until the GPU finishes
        // the prior frame's commands, so memory grows by O(frames ×
        // meshes × vertex_bytes) until the process dies (observed:
        // 50 GB on the strangler rig after a few minutes).
        let mesh_ptr = std::sync::Arc::as_ptr(&mesh.vertices) as usize;
        let needs_buffers = !self
            .mesh_pipeline
            .as_ref()
            .unwrap()
            .cached_mesh_buffers
            .contains_key(&mesh_ptr);
        if needs_buffers {
            let vertex_data: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    mesh.vertices.as_ptr() as *const u8,
                    mesh.vertices.len() * std::mem::size_of::<blinc_core::draw::Vertex>(),
                )
            };
            let vertex_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mesh Vertices (cached)"),
                    contents: vertex_data,
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let index_data: &[u8] = bytemuck::cast_slice(&mesh.indices);
            let index_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mesh Indices (cached)"),
                    contents: index_data,
                    usage: wgpu::BufferUsages::INDEX,
                });
            let entry = MeshBufferCacheEntry {
                vertex: vertex_buffer,
                index: index_buffer,
                index_count: mesh.indices.len() as u32,
            };
            let mp_mut = self.mesh_pipeline.as_mut().unwrap();
            mp_mut.cached_mesh_buffers.insert(mesh_ptr, entry);
            mp_mut.cached_mesh_buffer_keys.push_back(mesh_ptr);
            while mp_mut.cached_mesh_buffer_keys.len() > MESH_CACHE_CAPACITY {
                if let Some(old_key) = mp_mut.cached_mesh_buffer_keys.pop_front() {
                    mp_mut.cached_mesh_buffers.remove(&old_key);
                }
            }
        }
        let (vertex_buffer, index_buffer, index_count) = {
            let mp = self.mesh_pipeline.as_ref().unwrap();
            let entry = mp
                .cached_mesh_buffers
                .get(&mesh_ptr)
                .expect("mesh buffer cache entry must exist after insert");
            (entry.vertex.clone(), entry.index.clone(), entry.index_count)
        };

        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct ShadowUniforms {
            light_view_proj: [f32; 16],
            model: [f32; 16],
        }

        let shadow_uniforms = ShadowUniforms {
            light_view_proj: *light_view_proj,
            model: *transform,
        };

        let mp = self.mesh_pipeline.as_ref().unwrap();
        self.queue.write_buffer(
            &mp.shadow_uniform_buffer,
            0,
            bytemuck::bytes_of(&shadow_uniforms),
        );

        let shadow_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow Bind Group"),
            layout: &mp.shadow_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: mp.shadow_uniform_buffer.as_entire_binding(),
            }],
        });

        // First shadow caster in the frame clears the map; subsequent
        // casters load so their depth accumulates with earlier draws.
        let load_op = if shadow_batch_index == 0 {
            wgpu::LoadOp::Clear(1.0)
        } else {
            wgpu::LoadOp::Load
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Shadow Pass"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Shadow Depth Pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &mp.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            pass.set_pipeline(&mp.shadow_pipeline);
            pass.set_bind_group(0, &shadow_bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..index_count, 0, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render mesh data with shadow mapping, normal mapping, and parallax displacement.
    ///
    /// When `light_view_proj` is provided AND the mesh material has
    /// `receives_shadows == true`, the main fragment shader samples the
    /// shadow map produced by prior [`Self::render_mesh_shadow_pass`]
    /// calls in the same frame. Passing `Some(...)` here without having
    /// populated the shadow map leaves the depth compare sampling a
    /// cleared/stale texture.
    #[allow(clippy::too_many_arguments)]
    pub fn render_mesh_data(
        &mut self,
        target: &wgpu::TextureView,
        mesh: &std::sync::Arc<blinc_core::draw::MeshData>,
        transform: &[f32; 16],
        view_proj: &[f32; 16],
        camera_pos: [f32; 3],
        lights: &[DirectionalLight],
        light_view_proj: Option<&[f32; 16]>,
        viewport: Option<[f32; 4]>,
    ) {
        // Defer to the batch-aware variant; single-mesh calls behave
        // exactly as before (clear + skybox on first, tonemap on last).
        self.render_mesh_data_batched(
            target,
            mesh,
            transform,
            view_proj,
            camera_pos,
            lights,
            light_view_proj,
            viewport,
            0,
            1,
        );
    }

    /// Batch-aware version of [`Self::render_mesh_data`].
    ///
    /// `batch_index` is the 0-based position of this mesh within the
    /// current frame's mesh batch; `batch_count` is the batch size.
    ///
    /// - When `batch_index == 0`, the HDR intermediate is cleared and
    ///   the skybox pass runs. Subsequent calls preserve the HDR so
    ///   every mesh in the batch accumulates into it.
    /// - When `batch_index + 1 == batch_count` (last mesh), the
    ///   bloom + tonemap passes run, writing the composited HDR to
    ///   the frame target.
    ///
    /// Without this batching, the skybox pass clears HDR on every
    /// single-mesh call — causing multi-mesh scenes (e.g. a 39-mesh
    /// glTF asset) to show only the *last* mesh drawn because each
    /// tonemap pass reads HDR with only that mesh's contribution.
    #[allow(clippy::too_many_arguments)]
    pub fn render_mesh_data_batched(
        &mut self,
        target: &wgpu::TextureView,
        mesh: &std::sync::Arc<blinc_core::draw::MeshData>,
        transform: &[f32; 16],
        view_proj: &[f32; 16],
        camera_pos: [f32; 3],
        lights: &[DirectionalLight],
        light_view_proj: Option<&[f32; 16]>,
        viewport: Option<[f32; 4]>,
        batch_index: usize,
        batch_count: usize,
    ) {
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            tracing::warn!(
                batch_index,
                batch_count,
                verts = mesh.vertices.len(),
                indices = mesh.indices.len(),
                "mesh skipped — empty vertex or index buffer"
            );
            return;
        }
        tracing::trace!(
            batch_index,
            batch_count,
            verts = mesh.vertices.len(),
            indices = mesh.indices.len(),
            alpha_mode = ?mesh.material.alpha_mode,
            "drawing mesh"
        );
        let is_first = batch_index == 0;
        let is_last = batch_index + 1 >= batch_count.max(1);

        self.ensure_mesh_pipeline();

        // ── Vertex + index buffer cache ──────────────────────────────────
        //
        // Keyed by the `Arc<MeshData>` raw pointer. Without this, every
        // `draw_mesh_data` call uploads fresh vertex / index buffers —
        // a 39-mesh asset like buster_drone would push ~1.8 GB through
        // the wgpu queue per frame, pinning the render thread at ~1fps.
        //
        // FIFO-evicted at `MESH_CACHE_CAPACITY`. Scenes that recycle
        // the same Arcs each frame (the normal case) hit the cache on
        // every call after the first; streaming-mesh apps evict their
        // oldest entry and pay a re-upload for it on next touch.
        //
        // Keyed on the inner `Arc<Vec<Vertex>>` pointer — see the
        // shadow-pass copy of this cache for the full rationale
        // (tl;dr: the outer `Arc<MeshData>` pointer changes on every
        // per-frame `Arc::new(per_draw)` clone, causing a fresh
        // buffer upload per frame and an unbounded GPU memory leak).
        let mesh_ptr = std::sync::Arc::as_ptr(&mesh.vertices) as usize;
        let needs_buffers = !self
            .mesh_pipeline
            .as_ref()
            .unwrap()
            .cached_mesh_buffers
            .contains_key(&mesh_ptr);
        if needs_buffers {
            // Safety: `Vertex` is `#[repr(C)]` with only f32/u32 fields
            // so a raw reinterpret to `&[u8]` is well-defined.
            let vertex_data: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    mesh.vertices.as_ptr() as *const u8,
                    mesh.vertices.len() * std::mem::size_of::<blinc_core::draw::Vertex>(),
                )
            };
            let vertex_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mesh Vertices (cached)"),
                    contents: vertex_data,
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let index_data: &[u8] = bytemuck::cast_slice(&mesh.indices);
            let index_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Mesh Indices (cached)"),
                    contents: index_data,
                    usage: wgpu::BufferUsages::INDEX,
                });
            let entry = MeshBufferCacheEntry {
                vertex: vertex_buffer,
                index: index_buffer,
                index_count: mesh.indices.len() as u32,
            };
            let mp_mut = self.mesh_pipeline.as_mut().unwrap();
            mp_mut.cached_mesh_buffers.insert(mesh_ptr, entry);
            mp_mut.cached_mesh_buffer_keys.push_back(mesh_ptr);
            // Cap the cache — drop the oldest entry (FIFO) when full.
            // An LRU policy would need a touch-on-hit step; FIFO is
            // good enough for the common case where every mesh in a
            // scene is drawn every frame (all entries are equally
            // hot).
            while mp_mut.cached_mesh_buffer_keys.len() > MESH_CACHE_CAPACITY {
                if let Some(old_key) = mp_mut.cached_mesh_buffer_keys.pop_front() {
                    mp_mut.cached_mesh_buffers.remove(&old_key);
                }
            }
        }

        // Pull cloned buffer handles out of the cache so the rest of
        // the function owns its references independently of any
        // further mutable borrows of `self.mesh_pipeline`. `wgpu::Buffer`
        // is reference-counted internally, so `.clone()` is cheap.
        let (vertex_buffer, index_buffer, index_count) = {
            let mp = self.mesh_pipeline.as_ref().unwrap();
            let entry = mp
                .cached_mesh_buffers
                .get(&mesh_ptr)
                .expect("mesh buffer cache entry must exist after insert");
            (entry.vertex.clone(), entry.index.clone(), entry.index_count)
        };

        // Shadow map population happens BEFORE this call via
        // `render_mesh_shadow_pass`. Here we only sample it; the map is
        // populated-or-not based on whether the caller passed a light
        // matrix, and each mesh opts into receiving shadows via its
        // own `receives_shadows` flag (caster status is irrelevant for
        // receiving).
        let shadow_map_populated = light_view_proj.is_some();

        // ── Upload main uniforms ─────────────────────────────────────────
        let identity_mat: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];

        /// One directional light in WGSL std140 layout: vec3 + f32
        /// pair, then vec3 + f32. Matches `DirLight` in mesh.wgsl.
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct ShaderDirLight {
            direction: [f32; 3],
            intensity: f32,
            color: [f32; 3],
            _pad: f32,
        }

        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct MeshUniforms {
            view_proj: [f32; 16],
            model: [f32; 16],
            light_view_proj: [f32; 16],
            camera_pos: [f32; 3],
            _pad: f32,
            lights: [ShaderDirLight; MAX_DIR_LIGHTS],
            light_count: u32,
            _pad_lc: [f32; 3],
            viewport_size: [f32; 2],
            has_texture: f32,
            has_normal_map: f32,
            shadow_enabled: f32,
            displacement_scale: f32,
            normal_scale: f32,
            has_skinning: f32,
            /// Number of morph targets for this mesh. Zero means the
            /// vertex-stage morph loop runs zero iterations and the
            /// dummy-bound delta / weights buffers are never read.
            morph_target_count: u32,
            /// Base-mesh vertex count, needed to index into the
            /// flattened `morph_deltas` array as
            /// `(target * morph_vertex_count + vertex_idx) * 2 + [0|1]`.
            morph_vertex_count: u32,
            _pad_morph0: u32,
            _pad_morph1: u32,
        }

        let has_texture = if mesh.material.base_color_texture.is_some() {
            1.0_f32
        } else {
            0.0
        };
        let has_normal_map = if mesh.material.normal_map.is_some() {
            1.0_f32
        } else {
            0.0
        };
        let displacement_scale = if mesh.material.displacement_map.is_some() {
            mesh.material.displacement_scale
        } else {
            0.0
        };

        // Pack up to MAX_DIR_LIGHTS into the shader array. Callers
        // passing more lights get them truncated (rare; tiny constant).
        let mut shader_lights = [ShaderDirLight {
            direction: [0.0, -1.0, 0.0],
            intensity: 0.0,
            color: [0.0, 0.0, 0.0],
            _pad: 0.0,
        }; MAX_DIR_LIGHTS];
        let light_count = lights.len().min(MAX_DIR_LIGHTS);
        for (slot, light) in shader_lights.iter_mut().zip(lights).take(light_count) {
            *slot = ShaderDirLight {
                direction: light.direction,
                intensity: light.intensity,
                color: light.color,
                _pad: 0.0,
            };
        }

        let uniforms = MeshUniforms {
            view_proj: *view_proj,
            model: *transform,
            light_view_proj: light_view_proj.copied().unwrap_or(identity_mat),
            camera_pos,
            _pad: 0.0,
            lights: shader_lights,
            light_count: light_count as u32,
            _pad_lc: [0.0; 3],
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            has_texture,
            has_normal_map,
            shadow_enabled: if shadow_map_populated && mesh.material.receives_shadows {
                1.0
            } else {
                0.0
            },
            displacement_scale,
            normal_scale: mesh.material.normal_scale,
            has_skinning: if mesh.skin.is_some() { 1.0 } else { 0.0 },
            morph_target_count: mesh.morph_targets.len() as u32,
            morph_vertex_count: mesh.vertices.len() as u32,
            _pad_morph0: 0,
            _pad_morph1: 0,
        };

        let mp = self.mesh_pipeline.as_ref().unwrap();
        self.queue
            .write_buffer(&mp.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // ── Upload material ──────────────────────────────────────────────
        //
        // Layout must match `MaterialUniforms` in mesh.wgsl. WGSL's
        // std140-like alignment puts the vec3 on a 16-byte boundary, so
        // `emissive` is followed by 4 bytes of implicit padding before
        // `unlit` — we represent that explicitly with `emissive_pad` to
        // keep the bytemuck Pod layout matching the shader's struct.
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct MaterialGpu {
            base_color: [f32; 4],
            metallic_roughness: [f32; 2],
            _pad_mr: [f32; 2],
            emissive: [f32; 3],
            unlit: f32,
            has_metallic_roughness_texture: f32,
            has_emissive_texture: f32,
            has_occlusion_texture: f32,
            occlusion_strength: f32,
            // Alpha mode is encoded as a float so WGSL's uniform layout
            // rules don't need a bool type (which isn't POD anyway).
            // Matches shader `MaterialUniforms.alpha_mode`.
            alpha_mode: f32,
            /// Mask-mode cutoff (glTF default 0.5; overrideable via
            /// `Material::alpha_cutoff`). Fragments whose base-color
            /// alpha is below this are `discard`ed. Ignored when
            /// `alpha_mode != Mask`.
            alpha_cutoff: f32,
            _pad_am: [f32; 2],
            /// Packed 2×2 matrix from `KHR_texture_transform`
            /// (offset + rotation + scale flattened at upload time).
            /// `[M.xx, M.xy, M.yx, M.yy]` — applied to interpolated
            /// UV in `shade()`. Identity when the material has no
            /// texture transform.
            uv_transform_matrix: [f32; 4],
            /// Offset pair for the same transform: `uv_out = M*uv + offset`.
            uv_transform_offset: [f32; 2],
            _pad_uv: [f32; 2],
        }

        let alpha_mode = match mesh.material.alpha_mode {
            blinc_core::draw::AlphaMode::Opaque => 0.0,
            blinc_core::draw::AlphaMode::Mask => 1.0,
            blinc_core::draw::AlphaMode::Blend => 2.0,
        };

        // Flatten the optional `KHR_texture_transform` into a packed
        // 2×2 matrix + offset. Spec form is
        // `uv_out = translate * rotate * scale * uv_in`, which
        // collapses to `M*uv + offset` where
        // `M = rotation × diag(scale)`. `None` → identity, which
        // costs the shader one free vec2-mul with zero branch.
        let (uv_transform_matrix, uv_transform_offset) =
            if let Some(t) = mesh.material.texture_transform.as_ref() {
                let (s, c) = t.rotation.sin_cos();
                (
                    [
                        c * t.scale[0],
                        -s * t.scale[1],
                        s * t.scale[0],
                        c * t.scale[1],
                    ],
                    t.offset,
                )
            } else {
                ([1.0, 0.0, 0.0, 1.0], [0.0, 0.0])
            };

        let mat = MaterialGpu {
            base_color: mesh.material.base_color,
            metallic_roughness: [mesh.material.metallic, mesh.material.roughness],
            _pad_mr: [0.0; 2],
            emissive: mesh.material.emissive,
            unlit: if mesh.material.unlit { 1.0 } else { 0.0 },
            has_metallic_roughness_texture: if mesh.material.metallic_roughness_texture.is_some() {
                1.0
            } else {
                0.0
            },
            has_emissive_texture: if mesh.material.emissive_texture.is_some() {
                1.0
            } else {
                0.0
            },
            has_occlusion_texture: if mesh.material.occlusion_texture.is_some() {
                1.0
            } else {
                0.0
            },
            occlusion_strength: mesh.material.occlusion_strength,
            alpha_mode,
            alpha_cutoff: mesh.material.alpha_cutoff,
            _pad_am: [0.0; 2],
            uv_transform_matrix,
            uv_transform_offset,
            _pad_uv: [0.0; 2],
        };
        self.queue
            .write_buffer(&mp.material_buffer, 0, bytemuck::bytes_of(&mat));

        // ── Upload textures — cache by source `Arc<[u8]>` pointer ─────────
        //
        // Multiple materials typically reference the same underlying
        // image (glTF explicitly reuses images across primitives —
        // buster_drone has 39 meshes but only 10 distinct textures).
        // Keying by `TextureData::cache_key()` means each unique image
        // becomes exactly one `GpuImage`, regardless of how many
        // materials point at it. FIFO-evicted at `MESH_CACHE_CAPACITY`.
        // Tuple layout: (slot_texture, label, is_color_slot).
        // `is_color_slot == true` flags sRGB-encoded slots (base
        // color, emissive) — picks Bc1RgbaUnormSrgb / Bc3RgbaUnormSrgb
        // for compressed uploads so the sampler decodes to linear in
        // the shader. Linear slots (normal map, MR, occlusion,
        // displacement) use the non-sRGB wgpu variants.
        let texture_slots: [(&Option<blinc_core::TextureData>, &str, bool); 6] = [
            (
                &mesh.material.base_color_texture,
                "mesh_base_color_tex",
                true,
            ),
            (&mesh.material.normal_map, "mesh_normal_map", false),
            (
                &mesh.material.displacement_map,
                "mesh_displacement_map",
                false,
            ),
            (
                &mesh.material.metallic_roughness_texture,
                "mesh_metallic_roughness_tex",
                false,
            ),
            (&mesh.material.emissive_texture, "mesh_emissive_tex", true),
            (
                &mesh.material.occlusion_texture,
                "mesh_occlusion_tex",
                false,
            ),
        ];
        for (td_opt, label, is_color) in &texture_slots {
            let Some(td) = td_opt.as_ref() else { continue };
            let key = td.cache_key();
            let already_cached = self
                .mesh_pipeline
                .as_ref()
                .unwrap()
                .cached_gpu_images
                .contains_key(&key);
            if already_cached {
                continue;
            }
            // Upload pixels via `with_bytes`, then drop the CPU copy.
            //
            // Route by `td.format`:
            //   - `Rgba8`     → `GpuImage::from_rgba` (legacy path)
            //   - `Bc*`       → `GpuImage::from_compressed` with the
            //                   slot's sRGB vs linear color space.
            //
            // A BC-encoded texture on an adapter without
            // `TEXTURE_COMPRESSION_BC` cannot be uploaded as BC, so it is
            // expanded to RGBA8 on the CPU and takes the uncompressed path.
            //
            // Cache eviction caveat: if an entry is FIFO-evicted
            // from `cached_gpu_images` and `with_bytes` returns None
            // (CPU copy dropped after first upload), the mesh
            // silently binds the 1×1 placeholder. See the pre-BC
            // memory-fix comment (commit 97f2223a) for why the CPU
            // drop stays despite this.
            use blinc_core::TexturePixelFormat;
            let compressed = td.format.is_compressed();
            let expand = compressed && !self.has_texture_compression_bc;
            let expanded = if expand {
                td.with_bytes(|bytes| {
                    crate::bc_decode::decode_to_rgba8(bytes, td.format, td.width, td.height)
                })
                .flatten()
            } else {
                None
            };
            if expand && expanded.is_none() {
                tracing::warn!(
                    label,
                    format = ?td.format,
                    "BC texture could not be expanded for a device without TEXTURE_COMPRESSION_BC — binding default"
                );
                continue;
            }
            let color_space = crate::image::GpuImage::compressed_color_space(*is_color);
            let Some(img) = td.with_bytes(|bytes| {
                tracing::info!(
                    label,
                    format = ?td.format,
                    bytes = bytes.len(),
                    width = td.width,
                    height = td.height,
                    is_color,
                    "uploading mesh texture"
                );
                if compressed && !expand {
                    crate::image::GpuImage::from_compressed(
                        &self.device,
                        &self.queue,
                        bytes,
                        td.format,
                        color_space,
                        td.width,
                        td.height,
                        Some(*label),
                    )
                } else {
                    debug_assert!(expand || td.format == TexturePixelFormat::Rgba8);
                    let bytes = expanded.as_deref().unwrap_or(bytes);
                    // Color slots (diffuse, emissive) upload as
                    // sRGB so the sampler decodes to linear on
                    // read — matches what `Bc1RgbaUnormSrgb` /
                    // `Bc3RgbaUnormSrgb` do on the compressed
                    // path. glTF encodes diffuse/emissive PNGs
                    // in sRGB by convention; uploading as plain
                    // `Rgba8Unorm` leaves shader math working on
                    // sRGB-encoded values, making lit surfaces
                    // read ~2× too bright.
                    if *is_color {
                        crate::image::GpuImage::from_rgba_srgb(
                            &self.device,
                            &self.queue,
                            bytes,
                            td.width,
                            td.height,
                            Some(*label),
                        )
                    } else {
                        crate::image::GpuImage::from_rgba(
                            &self.device,
                            &self.queue,
                            bytes,
                            td.width,
                            td.height,
                            Some(*label),
                        )
                    }
                }
            }) else {
                continue;
            };
            td.drop_cpu_bytes();
            let mp_mut = self.mesh_pipeline.as_mut().unwrap();
            mp_mut.cached_gpu_images.insert(key, img);
            mp_mut.cached_gpu_image_keys.push_back(key);
            while mp_mut.cached_gpu_image_keys.len() > MESH_CACHE_CAPACITY {
                if let Some(old_key) = mp_mut.cached_gpu_image_keys.pop_front() {
                    mp_mut.cached_gpu_images.remove(&old_key);
                    // FIFO eviction: the just-evicted texture will now
                    // render as the 1×1 white default. If the scene is
                    // still referencing it (most common on scenes
                    // with > MESH_CACHE_CAPACITY unique textures), the
                    // visual result is a silent "material went white"
                    // regression — one of the hardest bugs for app
                    // authors to diagnose. Log once per eviction so
                    // it at least shows up in the console; full LRU
                    // with per-frame reference tracking is the proper
                    // fix and lives in blinc_canvas_kit's BACKLOG.
                    tracing::warn!(
                        target: "blinc_gpu::mesh_texture_cache",
                        evicted_key = old_key,
                        capacity = MESH_CACHE_CAPACITY,
                        "evicting GPU texture — scenes with > {} unique textures will \
                         render the evicted material as white; consider splitting scenes \
                         or raising MESH_CACHE_CAPACITY",
                        MESH_CACHE_CAPACITY,
                    );
                }
            }
        }

        // ── Upload morph-target deltas (cached per mesh) ──────────────────
        //
        // Deltas are static after glTF parse, so we upload once per mesh
        // and reuse across every subsequent draw. Keyed by the
        // `Arc<Vec<MorphTarget>>` inner pointer — not `&Arc<MeshData>`,
        // which would be a stack address that varies per call, and not
        // `Arc::as_ptr(mesh)`, which a per-frame `Arc::new(mesh.clone())`
        // would rotate every frame. The morph_targets Arc is stable
        // across shallow mesh clones (the common per-draw pattern for
        // stamping fresh weights), so keying on it means animated
        // scenes hit the cache every frame.
        let morph_cache_key: Option<usize> = if !mesh.morph_targets.is_empty() {
            Some(std::sync::Arc::as_ptr(&mesh.morph_targets) as usize)
        } else {
            None
        };
        if let Some(key) = morph_cache_key {
            let mp_mut = self.mesh_pipeline.as_mut().unwrap();
            let already_cached = mp_mut.morph_deltas_cache.iter().any(|(k, _)| *k == key);
            if !already_cached {
                // Flatten all targets into a single `Vec<f32>`. Layout
                // per (target, vertex) is two vec4s: position delta
                // then normal delta. Targets without authored normals
                // emit zeroed normal deltas. Four floats per vec4 to
                // respect std430 alignment (the shader reads
                // `array<vec4<f32>>`).
                let vcount = mesh.vertices.len();
                let tcount = mesh.morph_targets.len();
                let mut flat: Vec<f32> = Vec::with_capacity(2 * tcount * vcount * 4);
                for target in mesh.morph_targets.iter() {
                    for v in 0..vcount {
                        // Position delta (vec3 + 0 padding).
                        let p = target
                            .delta_positions
                            .get(v)
                            .copied()
                            .unwrap_or([0.0, 0.0, 0.0]);
                        flat.extend_from_slice(&[p[0], p[1], p[2], 0.0]);
                        // Normal delta (vec3 + 0) — zero-filled for
                        // positions-only targets.
                        let n = target
                            .delta_normals
                            .as_ref()
                            .and_then(|ns| ns.get(v).copied())
                            .unwrap_or([0.0, 0.0, 0.0]);
                        flat.extend_from_slice(&[n[0], n[1], n[2], 0.0]);
                    }
                }
                let buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Mesh Morph Deltas"),
                        contents: bytemuck::cast_slice(&flat),
                        usage: wgpu::BufferUsages::STORAGE,
                    });
                mp_mut.morph_deltas_cache.push_back((key, buffer));
                while mp_mut.morph_deltas_cache.len() > MORPH_CACHE_CAPACITY {
                    mp_mut.morph_deltas_cache.pop_front();
                }
            }
        }

        // Upload this draw's weights (bounded by MAX_MORPH_TARGETS).
        if !mesh.morph_weights.is_empty() {
            let n = mesh.morph_weights.len().min(MAX_MORPH_TARGETS);
            self.queue.write_buffer(
                &self.mesh_pipeline.as_ref().unwrap().morph_weights_buffer,
                0,
                bytemuck::cast_slice(&mesh.morph_weights[..n]),
            );
        }

        let mp = self.mesh_pipeline.as_ref().unwrap();
        // Look up the `GpuImage` for each material slot, falling back to
        // the default placeholder texture when the slot is empty.
        let lookup = |td_opt: &Option<blinc_core::TextureData>| -> Option<&crate::image::GpuImage> {
            td_opt
                .as_ref()
                .and_then(|td| mp.cached_gpu_images.get(&td.cache_key()))
        };
        let base_view = lookup(&mesh.material.base_color_texture)
            .map_or_else(|| mp.default_texture.view(), |t| t.view());
        let normal_view = lookup(&mesh.material.normal_map)
            .map_or_else(|| mp.default_normal_map.view(), |t| t.view());
        let displacement_view = lookup(&mesh.material.displacement_map)
            .map_or_else(|| mp.default_displacement.view(), |t| t.view());
        let metallic_roughness_view = lookup(&mesh.material.metallic_roughness_texture)
            .map_or_else(|| mp.default_metallic_roughness.view(), |t| t.view());
        let emissive_view = lookup(&mesh.material.emissive_texture)
            .map_or_else(|| mp.default_emissive.view(), |t| t.view());
        let occlusion_view = lookup(&mesh.material.occlusion_texture)
            .map_or_else(|| mp.default_occlusion.view(), |t| t.view());

        // ── Upload joint matrices (skeletal animation) ───────────────────
        //
        // DT mode: upload to joint_data_texture (RGBA32Float, width=4,
        // height=num_joints). Storage-buffer mode: create a temporary
        // buffer or reuse the default one.
        let skin_joint_buf = if self.has_storage_buffers {
            mesh.skin.as_ref().map(|skin| {
                let count = skin.joint_matrices.len().min(256);
                let data: &[u8] = bytemuck::cast_slice(&skin.joint_matrices[..count]);
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Mesh Joint Matrices"),
                        contents: data,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    })
            })
        } else {
            // DT mode: upload joint matrices to the data texture
            if let Some(skin) = mesh.skin.as_ref() {
                let count = skin.joint_matrices.len().min(256);
                let data: &[u8] = bytemuck::cast_slice(&skin.joint_matrices[..count]);
                if let Some(ref tex) = mp.joint_data_texture {
                    self.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: tex,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * 16), // 4 texels * 16 bytes
                            rows_per_image: None,
                        },
                        wgpu::Extent3d {
                            width: 4,
                            height: count as u32,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
            None
        };
        let joint_buffer_ref = skin_joint_buf.as_ref().unwrap_or(&mp.joint_buffer);

        // Binding 8: joint matrices — storage buffer or data texture
        let joint_binding_8 = if self.has_storage_buffers {
            wgpu::BindGroupEntry {
                binding: 8,
                resource: joint_buffer_ref.as_entire_binding(),
            }
        } else {
            // DT mode: bind the joint data texture view
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(
                    mp.joint_data_view
                        .as_ref()
                        .expect("joint_data_view in DT mode"),
                ),
            }
        };

        // ── Create bind group ────────────────────────────────────────────
        let bind_group_entries = [
            wgpu::BindGroupEntry {
                binding: 0,
                resource: mp.uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: mp.material_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(base_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&mp.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(normal_view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&mp.shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(&mp.shadow_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(displacement_view),
            },
            joint_binding_8,
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::TextureView(metallic_roughness_view),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::TextureView(emissive_view),
            },
            wgpu::BindGroupEntry {
                binding: 11,
                resource: wgpu::BindingResource::TextureView(occlusion_view),
            },
            wgpu::BindGroupEntry {
                binding: 12,
                resource: wgpu::BindingResource::TextureView(&mp.env_cubemap_view),
            },
            wgpu::BindGroupEntry {
                binding: 13,
                resource: wgpu::BindingResource::Sampler(&mp.env_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 14,
                resource: {
                    // Resolve the delta buffer: cached per-mesh if
                    // any morph targets, else the zero-sized dummy.
                    let buf = morph_cache_key
                        .and_then(|k| {
                            mp.morph_deltas_cache
                                .iter()
                                .find(|(key, _)| *key == k)
                                .map(|(_, b)| b)
                        })
                        .unwrap_or(&mp.morph_deltas_dummy);
                    buf.as_entire_binding()
                },
            },
            wgpu::BindGroupEntry {
                binding: 15,
                resource: mp.morph_weights_buffer.as_entire_binding(),
            },
        ];
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Mesh Bind Group"),
            layout: &mp.bind_group_layout,
            entries: if self.has_storage_buffers {
                &bind_group_entries
            } else {
                &bind_group_entries[..14]
            },
        });

        // ── Main depth buffer ────────────────────────────────────────────
        //
        // Lazily (re)allocate a depth texture matching the current
        // viewport. Cached on the MeshPipeline so successive frames
        // reuse it; resized when the viewport changes. Without this,
        // the main mesh pass has no depth test and back faces draw
        // over front faces in mesh submission order.
        //
        // NOTE: we re-borrow `self.mesh_pipeline` here via a mutable
        // path because the earlier `let mp = self.mesh_pipeline.as_ref()`
        // borrow ended at the bind_group creation above.
        // ── Lazily (re)allocate depth + HDR textures ─────────────────────
        let (viewport_w, viewport_h) = self.viewport_size;
        self.mesh_targets_last_frame = self.current_frame();
        {
            let mp_mut = self.mesh_pipeline.as_mut().unwrap();
            let size = (viewport_w.max(1), viewport_h.max(1));
            if mp_mut.main_depth_size != size || mp_mut.main_depth.is_none() {
                let depth = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Mesh Main Depth"),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Depth32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                mp_mut.main_depth_view = Some(depth.create_view(&Default::default()));
                mp_mut.main_depth = Some(depth);
                mp_mut.main_depth_size = size;
            }
            if mp_mut.hdr_size != size || mp_mut.hdr_texture.is_none() {
                let hdr = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Mesh HDR Intermediate"),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                mp_mut.hdr_view = Some(hdr.create_view(&Default::default()));
                mp_mut.hdr_texture = Some(hdr);
                mp_mut.hdr_size = size;
            }
            // OIT intermediates — same size as HDR, cleared each frame.
            if mp_mut.oit_size != size || mp_mut.oit_accum_texture.is_none() {
                let accum = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("OIT Accum"),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let reveal = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("OIT Reveal"),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                mp_mut.oit_accum_view = Some(accum.create_view(&Default::default()));
                mp_mut.oit_reveal_view = Some(reveal.create_view(&Default::default()));
                mp_mut.oit_accum_texture = Some(accum);
                mp_mut.oit_reveal_texture = Some(reveal);
                mp_mut.oit_size = size;
            }
            // Bloom ping-pong textures at half resolution
            let bloom_w = (size.0 / 2).max(1);
            let bloom_h = (size.1 / 2).max(1);
            if mp_mut.bloom_size != (bloom_w, bloom_h) || mp_mut.bloom_a.is_none() {
                let bloom_desc = wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d {
                        width: bloom_w,
                        height: bloom_h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                };
                let a = self.device.create_texture(&bloom_desc);
                let b = self.device.create_texture(&bloom_desc);
                mp_mut.bloom_a_view = Some(a.create_view(&Default::default()));
                mp_mut.bloom_b_view = Some(b.create_view(&Default::default()));
                mp_mut.bloom_a = Some(a);
                mp_mut.bloom_b = Some(b);
                mp_mut.bloom_size = (bloom_w, bloom_h);
            }
        }

        // ── Skybox bind group (empty — shader is screen-space only) ─────
        let mp = self.mesh_pipeline.as_ref().unwrap();
        let skybox_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Skybox Bind Group"),
            layout: &mp.skybox_bind_group_layout,
            entries: &[],
        });

        // ── HDR pass: skybox + mesh → Rgba16Float intermediate ──────────
        let depth_view = mp
            .main_depth_view
            .as_ref()
            .expect("main_depth_view populated above");
        let hdr_view = mp.hdr_view.as_ref().expect("hdr_view populated above");
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Mesh Render"),
            });

        // Sub-pass A: skybox → HDR (no depth attachment — the skybox
        // pipeline was created without depth_stencil so it can't share
        // a render pass with the mesh pipeline that has depth enabled).
        //
        // Only the first mesh in a batch runs the skybox pass; it also
        // owns the `LoadOp::Clear` that resets the HDR intermediate
        // for the whole frame. Later meshes in the same batch skip
        // both so their contributions accumulate in HDR instead of
        // being wiped away.
        if is_first {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Skybox Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: hdr_view,
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
            pass.set_pipeline(&mp.skybox_pipeline);
            pass.set_bind_group(0, &skybox_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // OIT intermediate clear — accum to (0,0,0,0), reveal to 1.0.
        // Runs once per frame on is_first so later BLEND fragments
        // append to an empty accumulator. `reveal = 1.0` means "no
        // BLEND fragment has touched this pixel yet"; each fragment
        // multiplies it by (1 - α_i) via the pipeline's blend factor.
        if is_first && mp.oit_accum_pipeline.is_some() {
            let accum_view = mp
                .oit_accum_view
                .as_ref()
                .expect("oit_accum_view populated above");
            let reveal_view = mp
                .oit_reveal_view
                .as_ref()
                .expect("oit_reveal_view populated above");
            let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("OIT Clear Pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: accum_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: reveal_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 1.0,
                                g: 0.0,
                                b: 0.0,
                                a: 0.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: None,
                ..Default::default()
            });
        }

        // Sub-pass: Scene3D custom passes → HDR (between skybox and mesh).
        // Submit the skybox encoder first, then run Scene3D passes
        // (which create their own encoders), then re-borrow mp for mesh.
        self.queue.submit(std::iter::once(encoder.finish()));

        // Scene3D custom passes also need to run once per frame, not
        // per mesh — otherwise a 3D grid/lines pass would re-render
        // its geometry every mesh, stacking overdraw. Gate on
        // `is_first` for the same reason as the skybox.
        if is_first {
            let hdr_view_ptr = self
                .mesh_pipeline
                .as_ref()
                .unwrap()
                .hdr_view
                .as_ref()
                .unwrap() as *const wgpu::TextureView;
            let inv_vp = mat4_inverse_flat(view_proj);
            // SAFETY: hdr_view lives on MeshPipeline which is owned by
            // self. execute_scene3d_passes doesn't modify MeshPipeline's
            // hdr_view — it only accesses custom_passes.
            let hdr_ref = unsafe { &*hdr_view_ptr };
            self.execute_scene3d_passes(hdr_ref, 1.0, view_proj, &inv_vp, camera_pos, viewport);
        }

        // Re-create encoder + re-borrow mp for remaining passes
        let mp = self.mesh_pipeline.as_ref().unwrap();
        let depth_view = mp.main_depth_view.as_ref().unwrap();
        let hdr_view = mp.hdr_view.as_ref().unwrap();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Mesh Render (continued)"),
            });

        // Sub-pass B: mesh → HDR. Depth is `Clear(1.0)` only on the
        // first mesh of the batch — subsequent meshes `Load` so the
        // accumulated depth rejects fragments occluded by earlier
        // draws within the same frame.
        let depth_load_op = if is_first {
            wgpu::LoadOp::Clear(1.0)
        } else {
            wgpu::LoadOp::Load
        };
        let is_blend = matches!(mesh.material.alpha_mode, blinc_core::draw::AlphaMode::Blend)
            && mp.oit_accum_pipeline.is_some();
        if is_blend {
            // BLEND → OIT accum + reveal MRT. Both targets share the
            // main depth buffer (test only, no write) so opaque
            // geometry in front of a BLEND fragment still occludes it.
            let accum_view = mp.oit_accum_view.as_ref().unwrap();
            let reveal_view = mp.oit_reveal_view.as_ref().unwrap();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Mesh OIT Accum Pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: accum_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: reveal_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: depth_load_op,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(mp.oit_accum_pipeline.as_ref().unwrap());
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..index_count, 0, 0..1);
        } else {
            // OPAQUE / MASK → HDR directly (classic forward path).
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Mesh HDR Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: hdr_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: depth_load_op,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&mp.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..index_count, 0, 0..1);
        }

        // ── OIT composite → HDR ─────────────────────────────────────
        //
        // Once per frame (on is_last), before bloom/tonemap: blend the
        // OIT accum+reveal intermediates over the HDR target so all
        // downstream passes see a correctly-composited scene.
        //
        // The composite shader discards pixels where no BLEND
        // fragment wrote (reveal ≈ 1.0, coverage < 1e-4), so the
        // pass is a no-op on pixels that only had opaque geometry.
        if is_last && mp.oit_accum_pipeline.is_some() {
            let accum_view = mp.oit_accum_view.as_ref().unwrap();
            let reveal_view = mp.oit_reveal_view.as_ref().unwrap();
            let composite_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("OIT Composite Bind Group"),
                layout: &mp.oit_composite_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(accum_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(reveal_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&mp.oit_composite_sampler),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("OIT Composite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: hdr_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&mp.oit_composite_pipeline);
            pass.set_bind_group(0, &composite_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        // ── Bloom + tonemap passes ─────────────────────────────────────
        //
        // Only the LAST mesh in a batch composites HDR into the frame
        // target. Earlier meshes accumulate into HDR with the mesh
        // pass above and return so their contributions stay in HDR
        // until the final pass tonemaps the full scene.
        //
        // Pass B1: threshold + downsample HDR → bloom_a (half-res)
        // Pass B2: Kawase blur bloom_a → bloom_b
        // Pass B3: Kawase blur bloom_b → bloom_a (wider)
        if is_last {
            let bloom_a_view = mp.bloom_a_view.as_ref().unwrap();
            let bloom_b_view = mp.bloom_b_view.as_ref().unwrap();
            let (bloom_w, bloom_h) = mp.bloom_size;
            let bloom_texel = [1.0 / bloom_w as f32, 1.0 / bloom_h as f32];

            #[repr(C)]
            #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
            struct BloomUniforms {
                texel_size: [f32; 2],
                threshold: f32,
                mode: f32,
            }

            // B1: threshold+downsample HDR → bloom_a.
            //
            // Threshold at 0.6 (was 0.8) — catches more of the
            // sub-clamp specular highlights on bright materials
            // (tank-top folds, wet skin, metal edges) so the
            // composited bloom matches the "studio viewer" look
            // glTF-first tools (Sketchfab, three.js) default to.
            // 0.8 was effectively hiding the bloom for most
            // assets because post-tonemap values rarely exceed
            // 0.8 on diffuse-dominant surfaces.
            {
                let uniforms = BloomUniforms {
                    texel_size: bloom_texel,
                    threshold: 0.6,
                    mode: 0.0,
                };
                self.queue
                    .write_buffer(&mp.bloom_uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Bloom Threshold"),
                    layout: &mp.bloom_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: mp.bloom_uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(hdr_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&mp.tonemap_sampler),
                        },
                    ],
                });
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Bloom Threshold"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: bloom_a_view,
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
                pass.set_pipeline(&mp.bloom_pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.draw(0..3, 0..1);
            }

            // B2: blur bloom_a → bloom_b
            {
                let uniforms = BloomUniforms {
                    texel_size: bloom_texel,
                    threshold: 0.0,
                    mode: 1.0,
                };
                self.queue
                    .write_buffer(&mp.bloom_uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Bloom Blur 1"),
                    layout: &mp.bloom_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: mp.bloom_uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(bloom_a_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&mp.tonemap_sampler),
                        },
                    ],
                });
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Bloom Blur 1"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: bloom_b_view,
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
                pass.set_pipeline(&mp.bloom_pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.draw(0..3, 0..1);
            }

            // B3: blur bloom_b → bloom_a (second iteration for wider glow)
            {
                let uniforms = BloomUniforms {
                    texel_size: bloom_texel,
                    threshold: 0.0,
                    mode: 1.0,
                };
                self.queue
                    .write_buffer(&mp.bloom_uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Bloom Blur 2"),
                    layout: &mp.bloom_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: mp.bloom_uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(bloom_b_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&mp.tonemap_sampler),
                        },
                    ],
                });
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Bloom Blur 2"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: bloom_a_view,
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
                pass.set_pipeline(&mp.bloom_pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.draw(0..3, 0..1);
            }

            // Pass 2: tonemap HDR → framebuffer (ACES filmic + sRGB gamma)
            //
            // The tonemap pass reads the Rgba16Float intermediate and the
            // bloom result, composites them, and writes tonemapped sRGB to
            // the caller's frame target. Viewport/scissor are set here (not
            // on the mesh pass) so the tonemap fullscreen triangle only
            // writes within the canvas bounds while the mesh pass renders at
            // full HDR resolution for correct edge sampling.
            {
                let tonemap_bind_group =
                    self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Tonemap Bind Group"),
                        layout: &mp.tonemap_bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(hdr_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&mp.tonemap_sampler),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(bloom_a_view),
                            },
                        ],
                    });

                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Tonemap Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    ..Default::default()
                });

                pass.set_pipeline(&mp.tonemap_pipeline);

                if let Some([vx, vy, vw, vh]) = viewport {
                    // `dispatch_pending_meshes` already clamps the
                    // viewport to the target extent and skips zero-
                    // area dispatches. Don't re-inflate with
                    // `.max(1.0)` here — at the bottom/right edge
                    // that bump produces a scissor that overflows
                    // the target by one row/column and wgpu's
                    // strict scissor validator panics.
                    pass.set_viewport(vx, vy, vw, vh, 0.0, 1.0);
                    pass.set_scissor_rect(vx as u32, vy as u32, vw as u32, vh as u32);
                }

                pass.set_bind_group(0, &tonemap_bind_group, &[]);
                pass.draw(0..3, 0..1); // fullscreen triangle
            }
        } // end `if is_last` — bloom + tonemap run only on final mesh

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
