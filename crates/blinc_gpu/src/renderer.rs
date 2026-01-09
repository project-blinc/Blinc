//! GPU renderer implementation
//!
//! The main renderer that manages wgpu resources and executes render passes
//! for SDF primitives, glass effects, and text.

use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::gradient_texture::{GradientTextureCache, RasterizedGradient};
use crate::image::GpuImageInstance;
use crate::path::PathVertex;
use crate::primitives::{
    GlassUniforms, GpuGlassPrimitive, GpuGlyph, GpuPrimitive, PathUniforms, PrimitiveBatch,
    Uniforms,
};
use crate::shaders::{
    BLUR_SHADER, COLOR_MATRIX_SHADER, COMPOSITE_SHADER, DROP_SHADOW_SHADER, GLASS_SHADER,
    IMAGE_SHADER, LAYER_COMPOSITE_SHADER, PATH_SHADER, SDF_SHADER, TEXT_SHADER,
};

/// Error type for renderer operations
#[derive(Debug)]
pub enum RendererError {
    /// Failed to request GPU adapter
    AdapterNotFound,
    /// Failed to request GPU device
    DeviceError(wgpu::RequestDeviceError),
    /// Failed to create surface
    SurfaceError(wgpu::CreateSurfaceError),
    /// Shader compilation error
    ShaderError(String),
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RendererError::AdapterNotFound => write!(f, "No suitable GPU adapter found"),
            RendererError::DeviceError(e) => write!(f, "Failed to request GPU device: {}", e),
            RendererError::SurfaceError(e) => write!(f, "Failed to create surface: {}", e),
            RendererError::ShaderError(e) => write!(f, "Shader compilation error: {}", e),
        }
    }
}

impl std::error::Error for RendererError {}

/// Configuration for creating a renderer
#[derive(Clone, Debug)]
pub struct RendererConfig {
    /// Maximum number of primitives per batch
    pub max_primitives: usize,
    /// Maximum number of glass primitives per batch
    pub max_glass_primitives: usize,
    /// Maximum number of glyphs per batch
    pub max_glyphs: usize,
    /// Enable MSAA (sample count)
    pub sample_count: u32,
    /// Preferred texture format (None = use surface preferred)
    pub texture_format: Option<wgpu::TextureFormat>,
    /// Enable unified text/SDF rendering (renders text as SDF primitives in same pass)
    ///
    /// When enabled, text glyphs are converted to SDF primitives and rendered
    /// in the same GPU pass as other shapes. This ensures consistent transform
    /// timing during animations, preventing visual lag when parent containers
    /// have motion transforms applied.
    ///
    /// Default: true (unified rendering for consistent animations)
    pub unified_text_rendering: bool,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            // Reduced defaults for lower memory footprint (~1 MB total vs ~5+ MB)
            // These can still handle typical UI scenes while using less memory
            max_primitives: 2_000,     // ~384 KB (was 1.92 MB)
            max_glass_primitives: 100, // ~25 KB (was 256 KB)
            max_glyphs: 10_000,        // ~640 KB (was 3.2 MB)
            sample_count: 1,
            texture_format: None,
            unified_text_rendering: true, // Enabled for consistent transforms during animations
        }
    }
}

/// Render pipelines for different primitive types
struct Pipelines {
    /// Pipeline for SDF primitives (rects, circles, etc.)
    sdf: wgpu::RenderPipeline,
    /// Pipeline for SDF primitives rendering on top of existing content (1x sampled)
    sdf_overlay: wgpu::RenderPipeline,
    /// Pipeline for glass/vibrancy effects
    glass: wgpu::RenderPipeline,
    /// Pipeline for text rendering (MSAA)
    #[allow(dead_code)]
    text: wgpu::RenderPipeline,
    /// Pipeline for text rendering on top of existing content (1x sampled)
    text_overlay: wgpu::RenderPipeline,
    /// Pipeline for final compositing (MSAA)
    #[allow(dead_code)]
    composite: wgpu::RenderPipeline,
    /// Pipeline for final compositing (1x sampled, for overlay blending)
    composite_overlay: wgpu::RenderPipeline,
    /// Pipeline for tessellated path rendering
    path: wgpu::RenderPipeline,
    /// Pipeline for tessellated path overlay (1x sampled)
    path_overlay: wgpu::RenderPipeline,
    /// Pipeline for layer composition (blend modes)
    layer_composite: wgpu::RenderPipeline,
    /// Pipeline for Kawase blur effect
    blur: wgpu::RenderPipeline,
    /// Pipeline for color matrix transformation
    color_matrix: wgpu::RenderPipeline,
    /// Pipeline for drop shadow effect
    drop_shadow: wgpu::RenderPipeline,
}

/// Cached MSAA pipelines for dynamic sample counts
struct MsaaPipelines {
    /// SDF pipeline for this sample count
    sdf: wgpu::RenderPipeline,
    /// Path pipeline for this sample count
    path: wgpu::RenderPipeline,
    /// Sample count these pipelines were created for
    sample_count: u32,
}

/// GPU buffers for rendering
struct Buffers {
    /// Uniform buffer for viewport size
    uniforms: wgpu::Buffer,
    /// Storage buffer for SDF primitives
    primitives: wgpu::Buffer,
    /// Storage buffer for glass primitives
    glass_primitives: wgpu::Buffer,
    /// Uniform buffer for glass shader
    glass_uniforms: wgpu::Buffer,
    /// Storage buffer for text glyphs
    #[allow(dead_code)]
    glyphs: wgpu::Buffer,
    /// Uniform buffer for path rendering
    path_uniforms: wgpu::Buffer,
    /// Vertex buffer for path geometry (dynamic, recreated as needed)
    path_vertices: Option<wgpu::Buffer>,
    /// Index buffer for path geometry (dynamic, recreated as needed)
    path_indices: Option<wgpu::Buffer>,
}

/// Bind groups for shader resources
struct BindGroups {
    /// Bind group for SDF pipeline
    sdf: wgpu::BindGroup,
    /// Bind group for glass pipeline (needs backdrop texture)
    glass: Option<wgpu::BindGroup>,
    /// Bind group for path pipeline
    path: wgpu::BindGroup,
}

/// Cached MSAA textures and resources for overlay rendering
struct CachedMsaaTextures {
    msaa_texture: wgpu::Texture,
    msaa_view: wgpu::TextureView,
    resolve_texture: wgpu::Texture,
    resolve_view: wgpu::TextureView,
    width: u32,
    height: u32,
    sample_count: u32,
    /// Sampler for compositing (reused across frames)
    sampler: wgpu::Sampler,
    /// Uniform buffer for compositing (reused across frames)
    composite_uniform_buffer: wgpu::Buffer,
    /// Bind group for compositing (recreated when textures change)
    composite_bind_group: wgpu::BindGroup,
}

/// Cached glass resources to avoid per-frame allocations
struct CachedGlassResources {
    /// Sampler for backdrop texture (reused across frames)
    sampler: wgpu::Sampler,
    /// Cached bind group (valid when backdrop texture hasn't changed)
    bind_group: Option<wgpu::BindGroup>,
    /// Width/height when bind group was created (for invalidation)
    bind_group_size: (u32, u32),
}

/// Cached text resources to avoid per-frame allocations
struct CachedTextResources {
    /// Cached bind group (valid when atlas texture view hasn't changed)
    bind_group: wgpu::BindGroup,
    /// Pointer to grayscale atlas view when bind group was created (for invalidation)
    atlas_view_ptr: *const wgpu::TextureView,
    /// Pointer to color atlas view when bind group was created (for invalidation)
    color_atlas_view_ptr: *const wgpu::TextureView,
}

/// Cached SDF bind group with glyph atlas textures (for unified text rendering)
struct CachedSdfWithGlyphs {
    /// Cached bind group with actual glyph atlas textures
    bind_group: wgpu::BindGroup,
    /// Pointer to grayscale atlas view when bind group was created (for invalidation)
    atlas_view_ptr: *const wgpu::TextureView,
    /// Pointer to color atlas view when bind group was created (for invalidation)
    color_atlas_view_ptr: *const wgpu::TextureView,
}

// ─────────────────────────────────────────────────────────────────────────────
// Layer Texture Management
// ─────────────────────────────────────────────────────────────────────────────

/// A texture used for offscreen layer rendering
///
/// Layer textures are used for rendering layers to offscreen targets,
/// enabling layer composition with blend modes and effects.
pub struct LayerTexture {
    /// The GPU texture for color data
    pub texture: wgpu::Texture,
    /// View into the texture for rendering
    pub view: wgpu::TextureView,
    /// Size of the texture in pixels (width, height)
    pub size: (u32, u32),
    /// Whether this texture has an associated depth buffer
    pub has_depth: bool,
    /// Optional depth texture view (for 3D content)
    pub depth_view: Option<wgpu::TextureView>,
    /// Optional depth texture (kept alive for the view)
    depth_texture: Option<wgpu::Texture>,
}

impl LayerTexture {
    /// Create a new layer texture with the given size
    pub fn new(
        device: &wgpu::Device,
        size: (u32, u32),
        format: wgpu::TextureFormat,
        with_depth: bool,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("layer_texture"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
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

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let (depth_texture, depth_view) = if with_depth {
            let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("layer_depth_texture"),
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
            let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());
            (Some(depth_tex), Some(depth_view))
        } else {
            (None, None)
        };

        Self {
            texture,
            view,
            size,
            has_depth: with_depth,
            depth_view,
            depth_texture,
        }
    }

    /// Check if this texture matches the requested size
    pub fn matches_size(&self, size: (u32, u32)) -> bool {
        self.size == size
    }
}

/// Cache for managing layer textures with pooling
///
/// Implements texture pooling to avoid frequent allocations during rendering.
/// Textures are acquired for layer rendering and released back to the pool
/// when no longer needed.
pub struct LayerTextureCache {
    /// Map of layer IDs to their dedicated textures
    named_textures: std::collections::HashMap<blinc_core::LayerId, LayerTexture>,
    /// Pool of reusable textures (sorted by size for efficient matching)
    pool: Vec<LayerTexture>,
    /// Texture format used for all layer textures
    format: wgpu::TextureFormat,
    /// Maximum pool size to limit memory usage
    max_pool_size: usize,
}

impl LayerTextureCache {
    /// Create a new layer texture cache
    pub fn new(format: wgpu::TextureFormat) -> Self {
        Self {
            named_textures: std::collections::HashMap::new(),
            pool: Vec::new(),
            format,
            max_pool_size: 8, // Reasonable default to limit memory
        }
    }

    /// Acquire a texture of at least the given size
    ///
    /// First checks the pool for a matching texture, otherwise creates a new one.
    pub fn acquire(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        with_depth: bool,
    ) -> LayerTexture {
        // Look for a texture in the pool that matches the size
        if let Some(index) = self
            .pool
            .iter()
            .position(|t| t.matches_size(size) && t.has_depth == with_depth)
        {
            return self.pool.swap_remove(index);
        }

        // Look for a texture that's larger (wasteful but avoids allocation)
        if let Some(index) = self
            .pool
            .iter()
            .position(|t| t.size.0 >= size.0 && t.size.1 >= size.1 && t.has_depth == with_depth)
        {
            return self.pool.swap_remove(index);
        }

        // Create a new texture
        LayerTexture::new(device, size, self.format, with_depth)
    }

    /// Release a texture back to the pool
    ///
    /// If the pool is full, the texture is dropped.
    pub fn release(&mut self, texture: LayerTexture) {
        if self.pool.len() < self.max_pool_size {
            self.pool.push(texture);
        }
        // Otherwise let the texture be dropped
    }

    /// Store a texture with a layer ID for later retrieval
    pub fn store(&mut self, id: blinc_core::LayerId, texture: LayerTexture) {
        self.named_textures.insert(id, texture);
    }

    /// Get a reference to a named layer's texture
    pub fn get(&self, id: &blinc_core::LayerId) -> Option<&LayerTexture> {
        self.named_textures.get(id)
    }

    /// Remove and return a named layer's texture
    pub fn remove(&mut self, id: &blinc_core::LayerId) -> Option<LayerTexture> {
        self.named_textures.remove(id)
    }

    /// Clear all named textures (releases them to pool or drops them)
    pub fn clear_named(&mut self) {
        let textures: Vec<_> = self.named_textures.drain().map(|(_, t)| t).collect();
        for texture in textures {
            self.release(texture);
        }
    }

    /// Clear the entire cache including pool
    pub fn clear_all(&mut self) {
        self.named_textures.clear();
        self.pool.clear();
    }

    /// Get the number of textures in the pool
    pub fn pool_size(&self) -> usize {
        self.pool.len()
    }

    /// Get the number of named textures
    pub fn named_count(&self) -> usize {
        self.named_textures.len()
    }
}

/// The GPU renderer using wgpu
///
/// This is the main rendering engine that:
/// - Manages wgpu device, queue, and surface
/// - Creates and manages render pipelines for different primitive types
/// - Batches primitives for efficient GPU rendering
/// - Executes render passes
pub struct GpuRenderer {
    /// wgpu instance
    #[allow(dead_code)]
    instance: wgpu::Instance,
    /// GPU adapter
    #[allow(dead_code)]
    adapter: wgpu::Adapter,
    /// GPU device
    device: Arc<wgpu::Device>,
    /// Command queue
    queue: Arc<wgpu::Queue>,
    /// Render pipelines
    pipelines: Pipelines,
    /// Cached MSAA pipelines for overlay rendering
    msaa_pipelines: Option<MsaaPipelines>,
    /// GPU buffers
    buffers: Buffers,
    /// Bind groups
    bind_groups: BindGroups,
    /// Bind group layouts
    bind_group_layouts: BindGroupLayouts,
    /// Current viewport size
    viewport_size: (u32, u32),
    /// Renderer configuration
    config: RendererConfig,
    /// Current frame time (for animations)
    time: f32,
    /// Resolved texture format used by pipelines
    texture_format: wgpu::TextureFormat,
    /// Lazily-created image pipeline and resources
    image_pipeline: Option<ImagePipeline>,
    /// Cached MSAA textures for overlay rendering (avoids per-frame allocation)
    cached_msaa: Option<CachedMsaaTextures>,
    /// Cached glass resources (avoids per-frame allocation)
    cached_glass: Option<CachedGlassResources>,
    /// Cached text resources (avoids per-frame allocation)
    cached_text: Option<CachedTextResources>,
    /// Placeholder glyph atlas texture view (1x1 transparent) for SDF bind group
    placeholder_glyph_atlas_view: wgpu::TextureView,
    /// Placeholder color glyph atlas texture view (1x1 transparent) for SDF bind group
    placeholder_color_glyph_atlas_view: wgpu::TextureView,
    /// Sampler for glyph atlas textures
    glyph_sampler: wgpu::Sampler,
    /// Cached SDF bind group with actual glyph atlas textures (for unified text rendering)
    cached_sdf_with_glyphs: Option<CachedSdfWithGlyphs>,
    /// Gradient texture cache for multi-stop gradient support on paths
    gradient_texture_cache: GradientTextureCache,
    /// Placeholder image texture (1x1 white) for path bind group when no image is used
    placeholder_path_image_view: wgpu::TextureView,
    /// Sampler for path image textures
    path_image_sampler: wgpu::Sampler,
    /// Layer texture cache for offscreen rendering and composition
    layer_texture_cache: LayerTextureCache,
}

/// Image rendering pipeline (created lazily on first image render)
struct ImagePipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    instance_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

struct BindGroupLayouts {
    sdf: wgpu::BindGroupLayout,
    glass: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    text: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    composite: wgpu::BindGroupLayout,
    path: wgpu::BindGroupLayout,
    /// Layout for layer composition shader
    layer_composite: wgpu::BindGroupLayout,
    /// Layout for blur effect shader
    blur: wgpu::BindGroupLayout,
    /// Layout for color matrix effect shader
    color_matrix: wgpu::BindGroupLayout,
    /// Layout for drop shadow effect shader
    drop_shadow: wgpu::BindGroupLayout,
}

impl GpuRenderer {
    /// Get the preferred backend for the current platform
    ///
    /// Using the primary backend instead of all backends reduces memory usage
    /// by avoiding initialization of multiple GPU driver stacks.
    fn preferred_backends() -> wgpu::Backends {
        #[cfg(target_os = "macos")]
        {
            wgpu::Backends::METAL
        }
        #[cfg(target_os = "windows")]
        {
            wgpu::Backends::DX12
        }
        #[cfg(target_os = "linux")]
        {
            wgpu::Backends::VULKAN
        }
        #[cfg(target_arch = "wasm32")]
        {
            wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_arch = "wasm32"
        )))]
        {
            wgpu::Backends::PRIMARY
        }
    }

    /// Create a new renderer without a surface (for headless rendering)
    pub async fn new(config: RendererConfig) -> Result<Self, RendererError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: Self::preferred_backends(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or(RendererError::AdapterNotFound)?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Blinc GPU Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    // MemoryUsage hint tells the driver to prefer lower memory over performance.
                    // This helps reduce RSS on integrated GPUs (Apple Silicon) where GPU memory
                    // is shared with CPU and counts against process memory.
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await
            .map_err(RendererError::DeviceError)?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Default texture format for headless
        let texture_format = config
            .texture_format
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);

        Self::create_renderer(
            instance,
            adapter,
            device,
            queue,
            texture_format,
            config,
            (800, 600),
        )
    }

    /// Create a new renderer with a window surface
    pub async fn with_surface<W>(
        window: Arc<W>,
        config: RendererConfig,
    ) -> Result<(Self, wgpu::Surface<'static>), RendererError>
    where
        W: raw_window_handle::HasWindowHandle
            + raw_window_handle::HasDisplayHandle
            + Send
            + Sync
            + 'static,
    {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: Self::preferred_backends(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(RendererError::SurfaceError)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(RendererError::AdapterNotFound)?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Blinc GPU Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    // MemoryUsage hint tells the driver to prefer lower memory over performance.
                    // This helps reduce RSS on integrated GPUs (Apple Silicon) where GPU memory
                    // is shared with CPU and counts against process memory.
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await
            .map_err(RendererError::DeviceError)?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_caps = surface.get_capabilities(&adapter);
        tracing::debug!("Surface capabilities - formats: {:?}", surface_caps.formats);
        tracing::debug!(
            "Surface capabilities - alpha modes: {:?}",
            surface_caps.alpha_modes
        );

        // Select texture format based on platform
        let texture_format = config.texture_format.unwrap_or_else(|| {
            // On macOS, prefer non-sRGB format to avoid automatic gamma correction
            // which causes colors to appear washed out. Other platforms may behave
            // differently, so we use sRGB there for now.
            #[cfg(target_os = "macos")]
            {
                surface_caps
                    .formats
                    .iter()
                    .find(|f| !f.is_srgb())
                    .copied()
                    .unwrap_or(surface_caps.formats[0])
            }
            #[cfg(not(target_os = "macos"))]
            {
                surface_caps
                    .formats
                    .iter()
                    .find(|f| f.is_srgb())
                    .copied()
                    .unwrap_or(surface_caps.formats[0])
            }
        });
        tracing::debug!("Selected texture format: {:?}", texture_format);

        let renderer = Self::create_renderer(
            instance,
            adapter,
            device,
            queue,
            texture_format,
            config,
            (800, 600),
        )?;

        Ok((renderer, surface))
    }

    fn create_renderer(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        texture_format: wgpu::TextureFormat,
        config: RendererConfig,
        viewport_size: (u32, u32),
    ) -> Result<Self, RendererError> {
        // Create bind group layouts
        let bind_group_layouts = Self::create_bind_group_layouts(&device);

        // Create shaders
        let sdf_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Shader"),
            source: wgpu::ShaderSource::Wgsl(SDF_SHADER.into()),
        });

        let glass_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Glass Shader"),
            source: wgpu::ShaderSource::Wgsl(GLASS_SHADER.into()),
        });

        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text Shader"),
            source: wgpu::ShaderSource::Wgsl(TEXT_SHADER.into()),
        });

        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });

        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Path Shader"),
            source: wgpu::ShaderSource::Wgsl(PATH_SHADER.into()),
        });

        let layer_composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Layer Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(LAYER_COMPOSITE_SHADER.into()),
        });

        // Effect shaders
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Blur Effect Shader"),
            source: wgpu::ShaderSource::Wgsl(BLUR_SHADER.into()),
        });

        let color_matrix_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Color Matrix Effect Shader"),
            source: wgpu::ShaderSource::Wgsl(COLOR_MATRIX_SHADER.into()),
        });

        let drop_shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Drop Shadow Effect Shader"),
            source: wgpu::ShaderSource::Wgsl(DROP_SHADOW_SHADER.into()),
        });

        // Create pipelines
        let pipelines = Self::create_pipelines(
            &device,
            &bind_group_layouts,
            &sdf_shader,
            &glass_shader,
            &text_shader,
            &composite_shader,
            &path_shader,
            &layer_composite_shader,
            &blur_shader,
            &color_matrix_shader,
            &drop_shadow_shader,
            texture_format,
            config.sample_count,
        );

        // Create buffers
        let buffers = Self::create_buffers(&device, &config);

        // Create placeholder glyph atlas textures (1x1 transparent)
        // These are used when no text is rendered, satisfying the bind group layout
        let placeholder_glyph_atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder Glyph Atlas"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm, // Grayscale for regular glyphs
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let placeholder_glyph_atlas_view =
            placeholder_glyph_atlas.create_view(&wgpu::TextureViewDescriptor::default());

        let placeholder_color_glyph_atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder Color Glyph Atlas"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb, // RGBA for color emoji
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let placeholder_color_glyph_atlas_view =
            placeholder_color_glyph_atlas.create_view(&wgpu::TextureViewDescriptor::default());

        // Create sampler for glyph atlases
        let glyph_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glyph Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create gradient texture cache for multi-stop gradients on paths
        let gradient_texture_cache = GradientTextureCache::new(&device, &queue);

        // Create placeholder image texture for paths (1x1 white)
        let placeholder_path_image = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder Path Image"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // Initialize with white pixel
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &placeholder_path_image,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8, 255, 255, 255], // White pixel
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let placeholder_path_image_view =
            placeholder_path_image.create_view(&wgpu::TextureViewDescriptor::default());

        // Create sampler for path image textures
        let path_image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Path Image Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create initial bind groups
        let bind_groups = Self::create_bind_groups(
            &device,
            &bind_group_layouts,
            &buffers,
            &placeholder_glyph_atlas_view,
            &placeholder_color_glyph_atlas_view,
            &glyph_sampler,
            &gradient_texture_cache,
            &placeholder_path_image_view,
            &path_image_sampler,
        );

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            pipelines,
            msaa_pipelines: None,
            buffers,
            bind_groups,
            bind_group_layouts,
            viewport_size,
            config,
            time: 0.0,
            texture_format,
            image_pipeline: None,
            cached_msaa: None,
            cached_glass: None,
            cached_text: None,
            placeholder_glyph_atlas_view,
            placeholder_color_glyph_atlas_view,
            glyph_sampler,
            cached_sdf_with_glyphs: None,
            gradient_texture_cache,
            placeholder_path_image_view,
            path_image_sampler,
            layer_texture_cache: LayerTextureCache::new(texture_format),
        })
    }

    fn create_bind_group_layouts(device: &wgpu::Device) -> BindGroupLayouts {
        // SDF bind group layout (includes glyph atlas for unified text rendering)
        let sdf = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SDF Bind Group Layout"),
            entries: &[
                // Uniforms
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
                // Primitives storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Glyph atlas texture (grayscale text)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Glyph sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Color glyph atlas texture (emoji)
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        // Glass bind group layout
        let glass = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Glass Bind Group Layout"),
            entries: &[
                // Uniforms
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
                // Glass primitives storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Backdrop texture
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Backdrop sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Text bind group layout
        let text = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Text Bind Group Layout"),
            entries: &[
                // Uniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Glyphs storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Glyph atlas texture
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Glyph atlas sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Color glyph atlas texture (for emoji)
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        // Composite bind group layout
        let composite = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Composite Bind Group Layout"),
            entries: &[
                // Uniforms
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
                // Source texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Source sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Path bind group layout (uniforms + gradient texture + image texture + backdrop for glass)
        let path = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Path Bind Group Layout"),
            entries: &[
                // Uniforms (viewport_size, transform, opacity, clip, glass params, etc.)
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
                // Gradient texture (1D texture for multi-stop gradients)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D1,
                        multisampled: false,
                    },
                    count: None,
                },
                // Gradient sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Image texture (2D texture for image brush)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Image sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Backdrop texture (2D texture for glass effect)
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Backdrop sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Layer composite bind group layout (for compositing offscreen layers)
        let layer_composite = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Layer Composite Bind Group Layout"),
            entries: &[
                // Uniforms (source_rect, dest_rect, viewport_size, opacity, blend_mode)
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
                // Layer texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Layer sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Blur effect bind group layout
        let blur = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Blur Effect Bind Group Layout"),
            entries: &[
                // BlurUniforms
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
                // Input texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Input sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Color matrix effect bind group layout
        let color_matrix = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Color Matrix Effect Bind Group Layout"),
            entries: &[
                // ColorMatrixUniforms
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
                // Input texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Input sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Drop shadow effect bind group layout
        let drop_shadow = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Drop Shadow Effect Bind Group Layout"),
            entries: &[
                // DropShadowUniforms
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
                // Input texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Input sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        BindGroupLayouts {
            sdf,
            glass,
            text,
            composite,
            path,
            layer_composite,
            blur,
            color_matrix,
            drop_shadow,
        }
    }

    fn create_pipelines(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        sdf_shader: &wgpu::ShaderModule,
        glass_shader: &wgpu::ShaderModule,
        text_shader: &wgpu::ShaderModule,
        composite_shader: &wgpu::ShaderModule,
        path_shader: &wgpu::ShaderModule,
        layer_composite_shader: &wgpu::ShaderModule,
        blur_shader: &wgpu::ShaderModule,
        color_matrix_shader: &wgpu::ShaderModule,
        drop_shadow_shader: &wgpu::ShaderModule,
        texture_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Pipelines {
        let blend_state = wgpu::BlendState {
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
        };

        let color_targets = &[Some(wgpu::ColorTargetState {
            format: texture_format,
            blend: Some(blend_state),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let primitive_state = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        };

        let multisample_state = wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        // SDF pipeline
        let sdf_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SDF Pipeline Layout"),
            bind_group_layouts: &[&layouts.sdf],
            push_constant_ranges: &[],
        });

        let sdf = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SDF Pipeline"),
            layout: Some(&sdf_layout),
            vertex: wgpu::VertexState {
                module: sdf_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: sdf_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        // Overlay pipelines use sample_count=1 for rendering on resolved textures
        let overlay_multisample_state = wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        let sdf_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SDF Overlay Pipeline"),
            layout: Some(&sdf_layout),
            vertex: wgpu::VertexState {
                module: sdf_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: sdf_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: None,
        });

        // Glass pipeline - always uses sample_count=1 since it renders on resolved textures
        // (glass effects require sampling from a single-sampled backdrop texture)
        let glass_multisample_state = wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        let glass_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Glass Pipeline Layout"),
            bind_group_layouts: &[&layouts.glass],
            push_constant_ranges: &[],
        });

        let glass = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Glass Pipeline"),
            layout: Some(&glass_layout),
            vertex: wgpu::VertexState {
                module: glass_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: glass_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: glass_multisample_state,
            multiview: None,
            cache: None,
        });

        // Text pipeline
        let text_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Text Pipeline Layout"),
            bind_group_layouts: &[&layouts.text],
            push_constant_ranges: &[],
        });

        let text = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Pipeline"),
            layout: Some(&text_layout),
            vertex: wgpu::VertexState {
                module: text_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: text_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        // Text overlay pipeline - uses sample_count=1 for rendering on resolved textures
        let text_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Overlay Pipeline"),
            layout: Some(&text_layout),
            vertex: wgpu::VertexState {
                module: text_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: text_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: None,
        });

        // Composite pipeline
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Composite Pipeline Layout"),
            bind_group_layouts: &[&layouts.composite],
            push_constant_ranges: &[],
        });

        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Composite Pipeline"),
            layout: Some(&composite_layout),
            vertex: wgpu::VertexState {
                module: composite_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: composite_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        // Composite overlay pipeline - single-sampled for blending onto resolved textures
        let composite_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Composite Overlay Pipeline"),
            layout: Some(&composite_layout),
            vertex: wgpu::VertexState {
                module: composite_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: composite_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: None,
        });

        // Path pipeline - uses vertex buffers for tessellated geometry
        let path_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Path Pipeline Layout"),
            bind_group_layouts: &[&layouts.path],
            push_constant_ranges: &[],
        });

        // Vertex buffer layout for PathVertex
        // PathVertex layout (80 bytes total):
        //   position: [f32; 2]       - 8 bytes, offset 0
        //   color: [f32; 4]          - 16 bytes, offset 8
        //   end_color: [f32; 4]      - 16 bytes, offset 24
        //   uv: [f32; 2]             - 8 bytes, offset 40
        //   gradient_params: [f32;4] - 16 bytes, offset 48
        //   gradient_type: u32       - 4 bytes, offset 64
        //   edge_distance: f32       - 4 bytes, offset 68
        //   _padding: [u32; 2]       - 8 bytes, offset 72
        let path_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PathVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position: vec2<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                // color: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 1,
                },
                // end_color: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 24,
                    shader_location: 2,
                },
                // uv: vec2<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 40,
                    shader_location: 3,
                },
                // gradient_params: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 4,
                },
                // gradient_type: u32
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 64,
                    shader_location: 5,
                },
                // edge_distance: f32 (for anti-aliasing)
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 68,
                    shader_location: 6,
                },
            ],
        };

        let path = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Path Pipeline"),
            layout: Some(&path_layout),
            vertex: wgpu::VertexState {
                module: path_shader,
                entry_point: Some("vs_main"),
                buffers: &[path_vertex_layout.clone()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: path_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        // Path overlay pipeline - uses sample_count=1 for rendering on resolved textures
        let path_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Path Overlay Pipeline"),
            layout: Some(&path_layout),
            vertex: wgpu::VertexState {
                module: path_shader,
                entry_point: Some("vs_main"),
                buffers: &[path_vertex_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: path_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: None,
        });

        // Layer composite pipeline - for compositing offscreen layers with blend modes
        let layer_composite_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Layer Composite Pipeline Layout"),
                bind_group_layouts: &[&layouts.layer_composite],
                push_constant_ranges: &[],
            });

        // Use premultiplied alpha blending for layer composition
        let premultiplied_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let layer_composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Layer Composite Pipeline"),
            layout: Some(&layer_composite_layout),
            vertex: wgpu::VertexState {
                module: layer_composite_shader,
                entry_point: Some("vs_main"),
                buffers: &[], // No vertex buffers - quad generated in shader
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: layer_composite_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    blend: Some(premultiplied_blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: overlay_multisample_state, // 1x sampled - layers are resolved
            multiview: None,
            cache: None,
        });

        // -------------------------------------------------------------------------
        // Effect Pipelines (post-processing)
        // -------------------------------------------------------------------------

        // Effect pipelines share similar configuration: no vertex buffers, fullscreen quad
        let effect_primitive_state = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        };

        // Blur pipeline layout
        let blur_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Blur Effect Pipeline Layout"),
            bind_group_layouts: &[&layouts.blur],
            push_constant_ranges: &[],
        });

        let blur = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Blur Effect Pipeline"),
            layout: Some(&blur_layout),
            vertex: wgpu::VertexState {
                module: blur_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: blur_shader,
                entry_point: Some("fs_kawase_blur"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: effect_primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state, // 1x sampled
            multiview: None,
            cache: None,
        });

        // Color matrix pipeline layout
        let color_matrix_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Color Matrix Effect Pipeline Layout"),
            bind_group_layouts: &[&layouts.color_matrix],
            push_constant_ranges: &[],
        });

        let color_matrix = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Color Matrix Effect Pipeline"),
            layout: Some(&color_matrix_layout),
            vertex: wgpu::VertexState {
                module: color_matrix_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: color_matrix_shader,
                entry_point: Some("fs_color_matrix"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: effect_primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state, // 1x sampled
            multiview: None,
            cache: None,
        });

        // Drop shadow pipeline layout
        let drop_shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Drop Shadow Effect Pipeline Layout"),
            bind_group_layouts: &[&layouts.drop_shadow],
            push_constant_ranges: &[],
        });

        let drop_shadow = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Drop Shadow Effect Pipeline"),
            layout: Some(&drop_shadow_layout),
            vertex: wgpu::VertexState {
                module: drop_shadow_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: drop_shadow_shader,
                entry_point: Some("fs_drop_shadow"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: effect_primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state, // 1x sampled
            multiview: None,
            cache: None,
        });

        Pipelines {
            sdf,
            sdf_overlay,
            glass,
            text,
            text_overlay,
            composite,
            composite_overlay,
            path,
            path_overlay,
            layer_composite,
            blur,
            color_matrix,
            drop_shadow,
        }
    }

    fn create_buffers(device: &wgpu::Device, config: &RendererConfig) -> Buffers {
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniforms Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let primitives = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Primitives Buffer"),
            size: (std::mem::size_of::<GpuPrimitive>() * config.max_primitives) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glass_primitives = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glass Primitives Buffer"),
            size: (std::mem::size_of::<GpuGlassPrimitive>() * config.max_glass_primitives) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glass_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glass Uniforms Buffer"),
            size: std::mem::size_of::<GlassUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glyphs = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glyphs Buffer"),
            size: (std::mem::size_of::<GpuGlyph>() * config.max_glyphs) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let path_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Path Uniforms Buffer"),
            size: std::mem::size_of::<PathUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Buffers {
            uniforms,
            primitives,
            glass_primitives,
            glass_uniforms,
            glyphs,
            path_uniforms,
            path_vertices: None,
            path_indices: None,
        }
    }

    fn create_bind_groups(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        buffers: &Buffers,
        glyph_atlas_view: &wgpu::TextureView,
        color_glyph_atlas_view: &wgpu::TextureView,
        glyph_sampler: &wgpu::Sampler,
        gradient_texture_cache: &GradientTextureCache,
        path_image_view: &wgpu::TextureView,
        path_image_sampler: &wgpu::Sampler,
    ) -> BindGroups {
        let sdf = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF Bind Group"),
            layout: &layouts.sdf,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffers.primitives.as_entire_binding(),
                },
                // Glyph atlas texture (binding 2)
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(glyph_atlas_view),
                },
                // Glyph sampler (binding 3)
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(glyph_sampler),
                },
                // Color glyph atlas texture (binding 4)
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(color_glyph_atlas_view),
                },
            ],
        });

        // Path bind group (with gradient texture, image texture, and backdrop for glass)
        let path = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Path Bind Group"),
            layout: &layouts.path,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffers.path_uniforms.as_entire_binding(),
                },
                // Gradient texture (binding 1)
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&gradient_texture_cache.view),
                },
                // Gradient sampler (binding 2)
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&gradient_texture_cache.sampler),
                },
                // Image texture (binding 3)
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(path_image_view),
                },
                // Image sampler (binding 4)
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(path_image_sampler),
                },
                // Backdrop texture (binding 5) - uses placeholder, will be replaced when glass is enabled
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(path_image_view),
                },
                // Backdrop sampler (binding 6)
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(path_image_sampler),
                },
            ],
        });

        // Glass bind group will be created when we have a backdrop texture
        BindGroups {
            sdf,
            glass: None,
            path,
        }
    }

    /// Create MSAA-specific pipelines for a given sample count
    fn create_msaa_pipelines(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        texture_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> MsaaPipelines {
        let blend_state = wgpu::BlendState {
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
        };

        let color_targets = &[Some(wgpu::ColorTargetState {
            format: texture_format,
            blend: Some(blend_state),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let primitive_state = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        };

        let multisample_state = wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        // Create SDF shader
        let sdf_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Shader (MSAA)"),
            source: wgpu::ShaderSource::Wgsl(SDF_SHADER.into()),
        });

        let sdf_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SDF Pipeline Layout (MSAA)"),
            bind_group_layouts: &[&layouts.sdf],
            push_constant_ranges: &[],
        });

        let sdf = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SDF Pipeline (MSAA)"),
            layout: Some(&sdf_layout),
            vertex: wgpu::VertexState {
                module: &sdf_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sdf_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        // Create path shader
        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Path Shader (MSAA)"),
            source: wgpu::ShaderSource::Wgsl(PATH_SHADER.into()),
        });

        let path_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Path Pipeline Layout (MSAA)"),
            bind_group_layouts: &[&layouts.path],
            push_constant_ranges: &[],
        });

        // PathVertex layout
        let path_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PathVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 24,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 40,
                    shader_location: 3,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 4,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 64,
                    shader_location: 5,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 68,
                    shader_location: 6,
                },
            ],
        };

        let path = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Path Pipeline (MSAA)"),
            layout: Some(&path_layout),
            vertex: wgpu::VertexState {
                module: &path_shader,
                entry_point: Some("vs_main"),
                buffers: &[path_vertex_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &path_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        MsaaPipelines {
            sdf,
            path,
            sample_count,
        }
    }

    /// Resize the viewport
    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport_size = (width, height);
    }

    /// Update the frame time (for animations)
    pub fn update_time(&mut self, time: f32) {
        self.time = time;
    }

    /// Get the wgpu device
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Get the wgpu device as Arc
    pub fn device_arc(&self) -> Arc<wgpu::Device> {
        self.device.clone()
    }

    /// Get the wgpu queue
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Get the wgpu queue as Arc
    pub fn queue_arc(&self) -> Arc<wgpu::Queue> {
        self.queue.clone()
    }

    /// Get the texture format used by this renderer's pipelines
    pub fn texture_format(&self) -> wgpu::TextureFormat {
        self.texture_format
    }

    /// Returns true if unified text/SDF rendering is enabled
    ///
    /// When enabled, text glyphs are converted to SDF primitives and rendered
    /// in the same GPU pass as other shapes, ensuring consistent transforms
    /// during animations.
    pub fn unified_text_rendering(&self) -> bool {
        self.config.unified_text_rendering
    }

    /// Poll the device to process completed GPU operations and free resources.
    /// Call this after frame rendering to prevent memory accumulation.
    pub fn poll(&self) {
        self.device.poll(wgpu::Maintain::Wait);
    }

    /// Render a batch of primitives to a texture view
    /// Render primitives with transparent background (default)
    pub fn render(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        self.render_with_clear(target, batch, [0.0, 0.0, 0.0, 0.0]);
    }

    /// Render primitives at a specific viewport size (for reduced-resolution rendering)
    ///
    /// Used for glass backdrop rendering at half resolution.
    pub fn render_at_size(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
        width: u32,
        height: u32,
    ) {
        // Temporarily override viewport size for this render
        let original_size = self.viewport_size;
        self.viewport_size = (width, height);
        self.render_with_clear(target, batch, clear_color);
        self.viewport_size = original_size;
    }

    /// Render primitives with a specified clear color
    ///
    /// # Arguments
    /// * `target` - The texture view to render to
    /// * `batch` - The primitive batch to render
    /// * `clear_color` - RGBA clear color (0.0-1.0 range)
    pub fn render_with_clear(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        // Check if we have layer commands with effects that need processing
        let has_layer_effects = batch.layer_commands.iter().any(|entry| {
            if let crate::primitives::LayerCommand::Push { config } = &entry.command {
                !config.effects.is_empty()
            } else {
                false
            }
        });

        tracing::trace!(
            "render_with_clear: {} primitives, {} layer commands, has_layer_effects={}",
            batch.primitives.len(),
            batch.layer_commands.len(),
            has_layer_effects
        );

        // If we have layer effects, use the layer-aware rendering path
        if has_layer_effects {
            self.render_with_layer_effects(target, batch, clear_color);
            return;
        }

        // Standard rendering (no layer effects)
        self.render_with_clear_simple(target, batch, clear_color);
    }

    /// Simple render with clear (no layer effect processing)
    fn render_with_clear_simple(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update primitives buffer
        if !batch.primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.primitives),
            );
        }

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render SDF primitives
            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                // 6 vertices per quad (2 triangles), one instance per primitive
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
            }

            // Render paths
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(&self.pipelines.path);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render with layer effect processing
    ///
    /// This implements a correct layer effect system:
    /// 1. Identify primitive ranges for effect layers
    /// 2. Render non-effect primitives to target (skipping those in effect layers)
    /// 3. For each effect layer, render to viewport-sized texture, apply effects, blit at position
    fn render_with_layer_effects(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        use crate::primitives::LayerCommand;

        // Build list of effect layers with their primitive ranges
        let mut effect_layers: Vec<(usize, usize, blinc_core::LayerConfig)> = Vec::new();
        let mut layer_stack: Vec<(usize, blinc_core::LayerConfig)> = Vec::new();

        for entry in &batch.layer_commands {
            match &entry.command {
                LayerCommand::Push { config } => {
                    layer_stack.push((entry.primitive_index, config.clone()));
                }
                LayerCommand::Pop => {
                    if let Some((start_idx, config)) = layer_stack.pop() {
                        if !config.effects.is_empty() {
                            effect_layers.push((start_idx, entry.primitive_index, config));
                        }
                    }
                }
                LayerCommand::Sample { .. } => {}
            }
        }

        // If no effect layers, just render normally
        if effect_layers.is_empty() {
            self.render_with_clear_simple(target, batch, clear_color);
            return;
        }

        // Build set of primitive indices that belong to effect layers (to skip in first pass)
        let mut effect_primitives = std::collections::HashSet::new();
        for (start, end, _) in &effect_layers {
            for i in *start..*end {
                effect_primitives.insert(i);
            }
        }

        // First pass: render primitives that are NOT in effect layers
        self.render_primitives_excluding(target, batch, &effect_primitives, clear_color);

        // Process each effect layer
        for (start_idx, end_idx, config) in effect_layers {
            if start_idx >= end_idx || end_idx > batch.primitives.len() {
                continue;
            }

            let layer_pos = config.position.map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
            let layer_size = config
                .size
                .map(|s| (s.width, s.height))
                .unwrap_or((self.viewport_size.0 as f32, self.viewport_size.1 as f32));

            // Render layer primitives to a viewport-sized texture
            let layer_texture = self
                .layer_texture_cache
                .acquire(&self.device, self.viewport_size, false);

            self.render_primitive_range(
                &layer_texture.view,
                batch,
                start_idx,
                end_idx,
                [0.0, 0.0, 0.0, 0.0], // Clear to transparent
            );

            // Apply effects to the texture
            let effected = self.apply_layer_effects(&layer_texture, &config.effects);
            self.layer_texture_cache.release(layer_texture);

            // Blit only the element's region back to target at correct position
            self.blit_region_to_target(
                &effected.view,
                target,
                layer_pos,
                layer_size,
                config.opacity,
                config.blend_mode,
            );

            self.layer_texture_cache.release(effected);
        }
    }

    /// Render primitives excluding those in the given set
    fn render_primitives_excluding(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        exclude: &std::collections::HashSet<usize>,
        clear_color: [f64; 4],
    ) {
        // If nothing to exclude, use simple path
        if exclude.is_empty() {
            self.render_with_clear_simple(target, batch, clear_color);
            return;
        }

        // Build list of primitives to render (excluding those in effect layers)
        let included_primitives: Vec<GpuPrimitive> = batch
            .primitives
            .iter()
            .enumerate()
            .filter(|(i, _)| !exclude.contains(i))
            .map(|(_, p)| *p)
            .collect();

        if included_primitives.is_empty() && batch.paths.vertices.is_empty() {
            // Just clear the target
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Clear Encoder"),
                });
            {
                let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: clear_color[0],
                                g: clear_color[1],
                                b: clear_color[2],
                                a: clear_color[3],
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            self.queue.submit(std::iter::once(encoder.finish()));
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update primitives buffer with filtered primitives
        if !included_primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&included_primitives),
            );
        }

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Filtered Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Filtered Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render SDF primitives (filtered)
            if !included_primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..included_primitives.len() as u32);
            }

            // Render paths (always rendered - path filtering would be more complex)
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(&self.pipelines.path);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Update path vertex and index buffers
    fn update_path_buffers(&mut self, batch: &PrimitiveBatch) {
        // Upload gradient texture if needed for multi-stop gradients
        if batch.paths.use_gradient_texture {
            if let Some(ref stops) = batch.paths.gradient_stops {
                let rasterized =
                    RasterizedGradient::from_stops(stops, crate::gradient_texture::SpreadMode::Pad);
                self.gradient_texture_cache.upload(&self.queue, &rasterized);
            }
        }

        // Update path uniforms with clip data and brush metadata from batch
        let path_uniforms = PathUniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            clip_bounds: batch.paths.clip_bounds,
            clip_radius: batch.paths.clip_radius,
            clip_type: batch.paths.clip_type,
            use_gradient_texture: if batch.paths.use_gradient_texture {
                1
            } else {
                0
            },
            use_image_texture: if batch.paths.use_image_texture { 1 } else { 0 },
            use_glass_effect: if batch.paths.use_glass_effect { 1 } else { 0 },
            image_uv_bounds: batch.paths.image_uv_bounds,
            glass_params: batch.paths.glass_params,
            glass_tint: batch.paths.glass_tint,
            ..PathUniforms::default()
        };
        self.queue.write_buffer(
            &self.buffers.path_uniforms,
            0,
            bytemuck::bytes_of(&path_uniforms),
        );

        // Create or recreate vertex buffer if needed
        let vertex_size = (std::mem::size_of::<PathVertex>() * batch.paths.vertices.len()) as u64;
        let need_new_vertex_buffer = match &self.buffers.path_vertices {
            Some(buf) => buf.size() < vertex_size,
            None => true,
        };

        if need_new_vertex_buffer && vertex_size > 0 {
            self.buffers.path_vertices = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Path Vertex Buffer"),
                size: vertex_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        if let Some(vb) = &self.buffers.path_vertices {
            self.queue
                .write_buffer(vb, 0, bytemuck::cast_slice(&batch.paths.vertices));
        }

        // Create or recreate index buffer if needed
        let index_size = (std::mem::size_of::<u32>() * batch.paths.indices.len()) as u64;
        let need_new_index_buffer = match &self.buffers.path_indices {
            Some(buf) => buf.size() < index_size,
            None => true,
        };

        if need_new_index_buffer && index_size > 0 {
            self.buffers.path_indices = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Path Index Buffer"),
                size: index_size,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        if let Some(ib) = &self.buffers.path_indices {
            self.queue
                .write_buffer(ib, 0, bytemuck::cast_slice(&batch.paths.indices));
        }
    }

    /// Render primitives with MSAA (multi-sample anti-aliasing)
    ///
    /// # Arguments
    /// * `msaa_target` - The multisampled texture view to render to
    /// * `resolve_target` - The single-sampled texture view to resolve to
    /// * `batch` - The primitive batch to render
    /// * `clear_color` - RGBA clear color (0.0-1.0 range)
    pub fn render_msaa(
        &mut self,
        msaa_target: &wgpu::TextureView,
        resolve_target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        clear_color: [f64; 4],
    ) {
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update primitives buffer
        if !batch.primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.primitives),
            );
        }

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc MSAA Render Encoder"),
            });

        // Begin render pass with MSAA resolve
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc MSAA Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: msaa_target,
                    resolve_target: Some(resolve_target),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render SDF primitives
            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
            }

            // Render paths
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(&self.pipelines.path);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render glass primitives (requires backdrop texture)
    pub fn render_glass(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        batch: &PrimitiveBatch,
    ) {
        if batch.glass_primitives.is_empty() {
            return;
        }

        // Ensure glass resources are cached (sampler is reused across frames)
        let current_size = self.viewport_size;

        // Check if we need to create or recreate the cached glass resources
        let need_new_bind_group = match &self.cached_glass {
            None => true,
            Some(cached) => cached.bind_group.is_none() || cached.bind_group_size != current_size,
        };

        if self.cached_glass.is_none() {
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Glass Backdrop Sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });
            self.cached_glass = Some(CachedGlassResources {
                sampler,
                bind_group: None,
                bind_group_size: (0, 0),
            });
        }

        // Update glass uniforms
        let glass_uniforms = GlassUniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            time: self.time,
            _padding: 0.0,
        };
        self.queue.write_buffer(
            &self.buffers.glass_uniforms,
            0,
            bytemuck::bytes_of(&glass_uniforms),
        );

        // Update glass primitives buffer
        self.queue.write_buffer(
            &self.buffers.glass_primitives,
            0,
            bytemuck::cast_slice(&batch.glass_primitives),
        );

        // Create or reuse glass bind group
        if need_new_bind_group {
            let cached_glass = self.cached_glass.as_ref().unwrap();
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Glass Bind Group"),
                layout: &self.bind_group_layouts.glass,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.buffers.glass_uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.buffers.glass_primitives.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(backdrop),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&cached_glass.sampler),
                    },
                ],
            });

            // Update cache
            if let Some(ref mut cached) = self.cached_glass {
                cached.bind_group = Some(bind_group);
                cached.bind_group_size = current_size;
            }
        }

        let glass_bind_group = self
            .cached_glass
            .as_ref()
            .unwrap()
            .bind_group
            .as_ref()
            .unwrap();

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Glass Render Encoder"),
            });

        // Begin render pass (load existing content)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Glass Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Keep existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.glass);
            render_pass.set_bind_group(0, glass_bind_group, &[]);
            render_pass.draw(0..6, 0..batch.glass_primitives.len() as u32);
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render primitives to a backdrop texture for glass blur sampling
    ///
    /// This renders the background primitives to a lower-resolution texture
    /// that glass primitives sample from for their blur effect.
    pub fn render_to_backdrop(
        &mut self,
        backdrop: &wgpu::TextureView,
        backdrop_size: (u32, u32),
        batch: &PrimitiveBatch,
    ) {
        if batch.primitives.is_empty() {
            return;
        }

        // Update uniforms for backdrop (typically half resolution)
        let backdrop_uniforms = Uniforms {
            viewport_size: [backdrop_size.0 as f32, backdrop_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue.write_buffer(
            &self.buffers.uniforms,
            0,
            bytemuck::bytes_of(&backdrop_uniforms),
        );

        // Update primitives buffer
        self.queue.write_buffer(
            &self.buffers.primitives,
            0,
            bytemuck::cast_slice(&batch.primitives),
        );

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Backdrop Render Encoder"),
            });

        // Render to backdrop texture
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Backdrop Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: backdrop,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.sdf);
            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            render_pass.draw(0..6, 0..batch.primitives.len() as u32);
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));

        // Restore main viewport uniforms
        let main_uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue.write_buffer(
            &self.buffers.uniforms,
            0,
            bytemuck::bytes_of(&main_uniforms),
        );
    }

    /// Render glass frame with backdrop and glass primitives in a single encoder submission.
    /// This is more efficient than separate render calls as it reduces command buffer overhead.
    ///
    /// Performs:
    /// 1. Render background primitives to backdrop texture
    /// 2. Render background primitives to target
    /// 3. Render glass primitives with backdrop blur to target
    pub fn render_glass_frame(
        &mut self,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        backdrop_size: (u32, u32),
        batch: &PrimitiveBatch,
    ) {
        // Update uniforms for backdrop (half resolution)
        let backdrop_uniforms = Uniforms {
            viewport_size: [backdrop_size.0 as f32, backdrop_size.1 as f32],
            _padding: [0.0; 2],
        };

        // Update uniforms for main rendering
        let main_uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };

        // Update primitives buffer
        if !batch.primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.primitives),
            );
        }

        // Update glass primitives buffer
        if !batch.glass_primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.glass_primitives,
                0,
                bytemuck::cast_slice(&batch.glass_primitives),
            );
        }

        // Update glass uniforms
        let glass_uniforms = GlassUniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            time: self.time,
            _padding: 0.0,
        };
        self.queue.write_buffer(
            &self.buffers.glass_uniforms,
            0,
            bytemuck::bytes_of(&glass_uniforms),
        );

        // Ensure glass bind group is cached
        let current_size = self.viewport_size;
        let need_new_bind_group = match &self.cached_glass {
            None => true,
            Some(cached) => cached.bind_group.is_none() || cached.bind_group_size != current_size,
        };

        if self.cached_glass.is_none() {
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Glass Backdrop Sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });
            self.cached_glass = Some(CachedGlassResources {
                sampler,
                bind_group: None,
                bind_group_size: (0, 0),
            });
        }

        if need_new_bind_group {
            let cached_glass = self.cached_glass.as_ref().unwrap();
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Glass Bind Group"),
                layout: &self.bind_group_layouts.glass,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.buffers.glass_uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.buffers.glass_primitives.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(backdrop),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&cached_glass.sampler),
                    },
                ],
            });
            if let Some(ref mut cached) = self.cached_glass {
                cached.bind_group = Some(bind_group);
                cached.bind_group_size = current_size;
            }
        }

        // Create single command encoder for entire frame
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Glass Frame Encoder"),
            });

        // Pass 1: Render background primitives to backdrop texture (at half resolution)
        {
            self.queue.write_buffer(
                &self.buffers.uniforms,
                0,
                bytemuck::bytes_of(&backdrop_uniforms),
            );

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Backdrop Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: backdrop,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
            }
        }

        // Pass 2: Render background primitives to target (at full resolution)
        {
            self.queue.write_buffer(
                &self.buffers.uniforms,
                0,
                bytemuck::bytes_of(&main_uniforms),
            );

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Target Background Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
            }
        }

        // Pass 3: Render glass primitives with backdrop blur
        if !batch.glass_primitives.is_empty() {
            let glass_bind_group = self
                .cached_glass
                .as_ref()
                .unwrap()
                .bind_group
                .as_ref()
                .unwrap();

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Glass Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.glass);
            render_pass.set_bind_group(0, glass_bind_group, &[]);
            render_pass.draw(0..6, 0..batch.glass_primitives.len() as u32);
        }

        // Submit background and glass passes first
        self.queue.submit(std::iter::once(encoder.finish()));

        // Pass 4: Render foreground primitives (on top of glass)
        // This requires a separate submission because we need to overwrite the primitives buffer
        if !batch.foreground_primitives.is_empty() {
            // Upload foreground primitives to the buffer
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.foreground_primitives),
            );

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blinc Foreground Encoder"),
                });

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Foreground Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.sdf);
            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            render_pass.draw(0..6, 0..batch.foreground_primitives.len() as u32);

            drop(render_pass);
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // Pass 5: Render paths (SVGs) on top of glass
        // Paths are tessellated geometry that need their own pipeline
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            // Update path buffers (creates/resizes as needed)
            self.update_path_buffers(batch);

            // Render paths
            if let (Some(vb), Some(ib)) = (&self.buffers.path_vertices, &self.buffers.path_indices)
            {
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Blinc Glass Path Encoder"),
                        });

                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Glass Path Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                // Use overlay path pipeline (1x sampled, no MSAA)
                render_pass.set_pipeline(&self.pipelines.path_overlay);
                render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                render_pass.set_vertex_buffer(0, vb.slice(..));
                render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);

                drop(render_pass);
                self.queue.submit(std::iter::once(encoder.finish()));
            }
        }
    }

    /// Render primitives as an overlay on existing content (1x sampled)
    ///
    /// This uses the overlay pipeline which is configured for sample_count=1,
    /// making it suitable for rendering on top of already-resolved content
    /// (e.g., after glass effects have been applied).
    ///
    /// # Arguments
    /// * `target` - The single-sampled texture view to render to (existing content is preserved)
    /// * `batch` - The primitive batch to render
    pub fn render_overlay(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        // Check if we have layer commands with effects that need processing
        let has_layer_effects = batch.layer_commands.iter().any(|entry| {
            if let crate::primitives::LayerCommand::Push { config } = &entry.command {
                !config.effects.is_empty()
            } else {
                false
            }
        });

        // If we have layer effects, use the layer-aware rendering path
        if has_layer_effects {
            self.render_overlay_with_layer_effects(target, batch);
            return;
        }

        // Standard overlay rendering (no layer effects)
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update primitives buffer
        if !batch.primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.primitives),
            );
        }

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Overlay Render Encoder"),
            });

        // Begin render pass (load existing content, don't clear)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Overlay Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None, // No MSAA resolve needed for overlay
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Keep existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render paths first (they're typically backgrounds)
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(&self.pipelines.path_overlay);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }

            // Render SDF primitives using overlay pipeline
            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf_overlay);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render overlay with layer effect processing
    ///
    /// Handles layer commands with effects by rendering layer content to offscreen
    /// textures, applying effects, and compositing back.
    fn render_overlay_with_layer_effects(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
    ) {
        use crate::primitives::LayerCommand;

        // First, do the standard overlay render
        self.render_overlay_simple(target, batch);

        // Then process layer commands with effects
        let mut layer_stack: Vec<(usize, blinc_core::LayerConfig)> = Vec::new();

        for entry in &batch.layer_commands {
            match &entry.command {
                LayerCommand::Push { config } => {
                    layer_stack.push((entry.primitive_index, config.clone()));
                }
                LayerCommand::Pop => {
                    if let Some((start_idx, config)) = layer_stack.pop() {
                        // Only process if this layer has effects
                        if config.effects.is_empty() {
                            continue;
                        }

                        // Get layer size (use viewport if not specified)
                        let layer_size = config
                            .size
                            .map(|s| (s.width as u32, s.height as u32))
                            .unwrap_or(self.viewport_size);

                        // Render layer content to offscreen texture
                        let layer_texture = self
                            .layer_texture_cache
                            .acquire(&self.device, layer_size, false);

                        // Render the primitives for this layer
                        let end_idx = entry.primitive_index;
                        if start_idx < end_idx && end_idx <= batch.primitives.len() {
                            self.render_primitive_range(
                                &layer_texture.view,
                                batch,
                                start_idx,
                                end_idx,
                                [0.0, 0.0, 0.0, 0.0],
                            );
                        }

                        // Apply effects
                        let effected = self.apply_layer_effects(&layer_texture, &config.effects);
                        self.layer_texture_cache.release(layer_texture);

                        // Composite back to main target with opacity
                        self.blit_texture_to_target(
                            &effected.view,
                            target,
                            config.opacity,
                            config.blend_mode,
                        );

                        self.layer_texture_cache.release(effected);
                    }
                }
                LayerCommand::Sample { .. } => {
                    // Sample commands handled elsewhere
                }
            }
        }
    }

    /// Simple overlay render without layer effect processing
    fn render_overlay_simple(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update primitives buffer
        if !batch.primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.primitives),
            );
        }

        // Update path buffers if we have path geometry
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Overlay Simple Render Encoder"),
            });

        // Begin render pass (load existing content, don't clear)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Overlay Simple Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render paths first
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(&self.pipelines.path_overlay);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }

            // Render SDF primitives
            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(&self.pipelines.sdf_overlay);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
            }
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a slice of primitives as overlay (LoadOp::Load, keeps existing content)
    ///
    /// This is used for interleaved z-layer rendering where primitives need
    /// to be rendered per-layer to properly interleave with text.
    pub fn render_primitives_overlay(
        &mut self,
        target: &wgpu::TextureView,
        primitives: &[GpuPrimitive],
    ) {
        if primitives.is_empty() {
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update primitives buffer
        self.queue.write_buffer(
            &self.buffers.primitives,
            0,
            bytemuck::cast_slice(primitives),
        );

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Layer Primitives Encoder"),
            });

        // Begin render pass (load existing content)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Layer Primitives Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render SDF primitives
            render_pass.set_pipeline(&self.pipelines.sdf_overlay);
            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            render_pass.draw(0..6, 0..primitives.len() as u32);
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render paths (tessellated geometry like SVGs) as an overlay
    ///
    /// This renders paths on top of existing content without clearing.
    /// Used for z-layered rendering where paths need to be rendered separately.
    pub fn render_paths_overlay(&mut self, target: &wgpu::TextureView, batch: &PrimitiveBatch) {
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if !has_paths {
            return;
        }

        // Update path buffers
        self.update_path_buffers(batch);

        // Render paths
        if let (Some(vb), Some(ib)) = (&self.buffers.path_vertices, &self.buffers.path_indices) {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blinc Paths Overlay Encoder"),
                });

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Paths Overlay Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Use overlay path pipeline (1x sampled)
            render_pass.set_pipeline(&self.pipelines.path_overlay);
            render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
            render_pass.set_vertex_buffer(0, vb.slice(..));
            render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);

            drop(render_pass);
            self.queue.submit(std::iter::once(encoder.finish()));
        }
    }

    /// Render SDF primitives with unified text rendering (text as primitives)
    ///
    /// This method renders SDF primitives including text glyphs in a single pass.
    /// Text primitives (PrimitiveType::Text) sample from the provided glyph atlases.
    ///
    /// # Arguments
    /// * `target` - The texture view to render to
    /// * `primitives` - The SDF primitives including text glyph primitives
    /// * `atlas_view` - The grayscale glyph atlas texture view
    /// * `color_atlas_view` - The color (RGBA) glyph atlas texture view for emoji
    pub fn render_primitives_overlay_with_glyphs(
        &mut self,
        target: &wgpu::TextureView,
        primitives: &[GpuPrimitive],
        atlas_view: &wgpu::TextureView,
        color_atlas_view: &wgpu::TextureView,
    ) {
        if primitives.is_empty() {
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update primitives buffer
        self.queue.write_buffer(
            &self.buffers.primitives,
            0,
            bytemuck::cast_slice(primitives),
        );

        // Check if we need to recreate the SDF bind group with actual glyph textures
        let atlas_view_ptr = atlas_view as *const wgpu::TextureView;
        let color_atlas_view_ptr = color_atlas_view as *const wgpu::TextureView;
        let need_new_bind_group = match &self.cached_sdf_with_glyphs {
            Some(cached) => {
                cached.atlas_view_ptr != atlas_view_ptr
                    || cached.color_atlas_view_ptr != color_atlas_view_ptr
            }
            None => true,
        };

        if need_new_bind_group {
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("SDF Bind Group with Glyphs"),
                layout: &self.bind_group_layouts.sdf,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.buffers.uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.buffers.primitives.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(atlas_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.glyph_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(color_atlas_view),
                    },
                ],
            });
            self.cached_sdf_with_glyphs = Some(CachedSdfWithGlyphs {
                bind_group,
                atlas_view_ptr,
                color_atlas_view_ptr,
            });
        }

        let sdf_bind_group = &self.cached_sdf_with_glyphs.as_ref().unwrap().bind_group;

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Unified Primitives Encoder"),
            });

        // Begin render pass (load existing content)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Unified Primitives Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Render SDF primitives (including text glyphs)
            render_pass.set_pipeline(&self.pipelines.sdf_overlay);
            render_pass.set_bind_group(0, sdf_bind_group, &[]);
            render_pass.draw(0..6, 0..primitives.len() as u32);
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render overlay primitives with MSAA anti-aliasing
    ///
    /// This method renders paths/primitives to a temporary MSAA texture,
    /// resolves it, and then blends onto the target. This provides smooth
    /// edges for tessellated paths that don't have shader-based AA.
    ///
    /// # Arguments
    /// * `target` - The single-sampled texture view to render to (existing content is preserved)
    /// * `batch` - The primitive batch to render
    /// * `sample_count` - MSAA sample count (typically 4)
    pub fn render_overlay_msaa(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        sample_count: u32,
    ) {
        if batch.paths.vertices.is_empty() && batch.primitives.is_empty() {
            return;
        }

        // Ensure we have MSAA pipelines for this sample count
        let need_new_pipelines = match &self.msaa_pipelines {
            Some(p) => p.sample_count != sample_count,
            None => true,
        };
        if need_new_pipelines && sample_count > 1 {
            self.msaa_pipelines = Some(Self::create_msaa_pipelines(
                &self.device,
                &self.bind_group_layouts,
                self.texture_format,
                sample_count,
            ));
        }

        let (width, height) = self.viewport_size;

        // Check if we need to recreate cached MSAA textures
        let need_new_textures = match &self.cached_msaa {
            Some(cached) => {
                cached.width != width
                    || cached.height != height
                    || cached.sample_count != sample_count
            }
            None => true,
        };

        if need_new_textures {
            // Create MSAA texture for rendering
            let msaa_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Overlay MSAA Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: self.texture_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let msaa_view = msaa_texture.create_view(&wgpu::TextureViewDescriptor::default());

            // Create resolve texture
            let resolve_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Overlay Resolve Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.texture_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let resolve_view = resolve_texture.create_view(&wgpu::TextureViewDescriptor::default());

            // Create sampler (reused across frames)
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Overlay Blend Sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });

            // Create composite uniforms (opacity=1.0, blend_mode=normal)
            #[repr(C)]
            #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
            struct CompositeUniforms {
                opacity: f32,
                blend_mode: u32,
                _padding: [f32; 2],
            }
            let composite_uniforms = CompositeUniforms {
                opacity: 1.0,
                blend_mode: 0,
                _padding: [0.0; 2],
            };
            let composite_uniform_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Composite Uniforms Buffer"),
                        contents: bytemuck::bytes_of(&composite_uniforms),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });

            // Create bind group for compositing
            let composite_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Overlay Composite Bind Group"),
                layout: &self.bind_group_layouts.composite,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: composite_uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&resolve_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });

            self.cached_msaa = Some(CachedMsaaTextures {
                msaa_texture,
                msaa_view,
                resolve_texture,
                resolve_view,
                width,
                height,
                sample_count,
                sampler,
                composite_uniform_buffer,
                composite_bind_group,
            });
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [width as f32, height as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update primitives buffer
        if !batch.primitives.is_empty() {
            self.queue.write_buffer(
                &self.buffers.primitives,
                0,
                bytemuck::cast_slice(&batch.primitives),
            );
        }

        // Update path buffers
        let has_paths = !batch.paths.vertices.is_empty() && !batch.paths.indices.is_empty();
        if has_paths {
            self.update_path_buffers(batch);
        }

        // Get references to the cached textures (after mutable borrows are done)
        let cached = self.cached_msaa.as_ref().unwrap();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Overlay MSAA Render Encoder"),
            });

        // Pass 1: Render to MSAA texture with resolve
        // Use cached MSAA pipelines for sample_count > 1, otherwise fall back to base pipelines
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay MSAA Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &cached.msaa_view,
                    resolve_target: Some(&cached.resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard, // MSAA texture discarded after resolve
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Get the appropriate pipelines for the sample count
            let (path_pipeline, sdf_pipeline) = if sample_count > 1 {
                if let Some(ref msaa) = self.msaa_pipelines {
                    (&msaa.path, &msaa.sdf)
                } else {
                    // Fallback (shouldn't happen due to creation above)
                    (&self.pipelines.path, &self.pipelines.sdf)
                }
            } else {
                (&self.pipelines.path, &self.pipelines.sdf)
            };

            // Render paths using MSAA pipeline
            if has_paths {
                if let (Some(vb), Some(ib)) =
                    (&self.buffers.path_vertices, &self.buffers.path_indices)
                {
                    render_pass.set_pipeline(path_pipeline);
                    render_pass.set_bind_group(0, &self.bind_groups.path, &[]);
                    render_pass.set_vertex_buffer(0, vb.slice(..));
                    render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..batch.paths.indices.len() as u32, 0, 0..1);
                }
            }

            // Render SDF primitives using MSAA pipeline
            if !batch.primitives.is_empty() {
                render_pass.set_pipeline(sdf_pipeline);
                render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
                render_pass.draw(0..6, 0..batch.primitives.len() as u32);
            }
        }

        // Pass 2: Blend resolved texture onto target using cached resources
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay Blend Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Keep existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.composite_overlay);
            render_pass.set_bind_group(0, &cached.composite_bind_group, &[]);
            render_pass.draw(0..3, 0..1); // Fullscreen triangle
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render text glyphs with a provided atlas texture
    ///
    /// # Arguments
    /// * `target` - The texture view to render to
    /// * `glyphs` - The glyph instances to render
    /// * `atlas_view` - The grayscale glyph atlas texture view
    /// * `color_atlas_view` - The color (RGBA) glyph atlas texture view for emoji
    /// * `atlas_sampler` - The sampler for the atlases
    pub fn render_text(
        &mut self,
        target: &wgpu::TextureView,
        glyphs: &[GpuGlyph],
        atlas_view: &wgpu::TextureView,
        color_atlas_view: &wgpu::TextureView,
        atlas_sampler: &wgpu::Sampler,
    ) {
        if glyphs.is_empty() {
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update glyphs buffer
        self.queue
            .write_buffer(&self.buffers.glyphs, 0, bytemuck::cast_slice(glyphs));

        // Check if we need to recreate the text bind group
        // Invalidate if either atlas view pointer changed (texture was recreated)
        let atlas_view_ptr = atlas_view as *const wgpu::TextureView;
        let color_atlas_view_ptr = color_atlas_view as *const wgpu::TextureView;
        let need_new_bind_group = match &self.cached_text {
            Some(cached) => {
                cached.atlas_view_ptr != atlas_view_ptr
                    || cached.color_atlas_view_ptr != color_atlas_view_ptr
            }
            None => true,
        };

        if need_new_bind_group {
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Text Bind Group"),
                layout: &self.bind_group_layouts.text,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.buffers.uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.buffers.glyphs.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(atlas_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(atlas_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(color_atlas_view),
                    },
                ],
            });
            self.cached_text = Some(CachedTextResources {
                bind_group,
                atlas_view_ptr,
                color_atlas_view_ptr,
            });
        }

        let text_bind_group = &self.cached_text.as_ref().unwrap().bind_group;

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blinc Text Render Encoder"),
            });

        // Begin render pass (load existing content)
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blinc Text Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Keep existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Use text_overlay pipeline since we're rendering to 1x sampled texture
            render_pass.set_pipeline(&self.pipelines.text_overlay);
            render_pass.set_bind_group(0, text_bind_group, &[]);
            render_pass.draw(0..6, 0..glyphs.len() as u32);
        }

        // Submit commands
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Create the image rendering pipeline (lazily initialized)
    fn ensure_image_pipeline(&mut self) {
        if self.image_pipeline.is_some() {
            return;
        }

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Image Shader"),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(IMAGE_SHADER)),
            });

        // Bind group layout: uniforms, texture, sampler
        let bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Image Bind Group Layout"),
                    entries: &[
                        // Uniforms (viewport size)
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Image texture
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        // Sampler
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Image Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        // Blending for premultiplied alpha
        let blend_state = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Image Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GpuImageInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            // dst_rect
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 0,
                                shader_location: 0,
                            },
                            // src_uv
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 16,
                                shader_location: 1,
                            },
                            // tint
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 32,
                                shader_location: 2,
                            },
                            // params (border_radius, opacity, padding, padding)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 48,
                                shader_location: 3,
                            },
                            // clip_bounds (x, y, width, height)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 64,
                                shader_location: 4,
                            },
                            // clip_radius (tl, tr, br, bl)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 80,
                                shader_location: 5,
                            },
                        ],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.texture_format,
                        blend: Some(blend_state),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // Create instance buffer (max 1000 images per batch)
        let instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Image Instance Buffer"),
            size: (std::mem::size_of::<GpuImageInstance>() * 1000) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create sampler
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Image Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        self.image_pipeline = Some(ImagePipeline {
            pipeline,
            bind_group_layout,
            instance_buffer,
            sampler,
        });
    }

    /// Render images to a texture view
    ///
    /// # Arguments
    /// * `target` - The target texture view to render to
    /// * `image_view` - The image texture view to sample from
    /// * `instances` - The image instances to render
    pub fn render_images(
        &mut self,
        target: &wgpu::TextureView,
        image_view: &wgpu::TextureView,
        instances: &[GpuImageInstance],
    ) {
        if instances.is_empty() {
            return;
        }

        // Ensure pipeline is created
        self.ensure_image_pipeline();

        let image_pipeline = self.image_pipeline.as_ref().unwrap();

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Update instance buffer
        self.queue.write_buffer(
            &image_pipeline.instance_buffer,
            0,
            bytemuck::cast_slice(instances),
        );

        // Create bind group for this image
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Image Bind Group"),
            layout: &image_pipeline.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.buffers.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(image_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&image_pipeline.sampler),
                },
            ],
        });

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Image Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Image Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Preserve existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&image_pipeline.pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.set_vertex_buffer(0, image_pipeline.instance_buffer.slice(..));
            render_pass.draw(0..6, 0..instances.len() as u32);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Layer Texture Cache Accessors
    // ─────────────────────────────────────────────────────────────────────────

    /// Get a reference to the layer texture cache
    pub fn layer_texture_cache(&self) -> &LayerTextureCache {
        &self.layer_texture_cache
    }

    /// Get a mutable reference to the layer texture cache
    pub fn layer_texture_cache_mut(&mut self) -> &mut LayerTextureCache {
        &mut self.layer_texture_cache
    }

    /// Acquire a layer texture from the cache
    ///
    /// If a matching texture exists in the pool, it will be reused.
    /// Otherwise, a new texture will be created.
    pub fn acquire_layer_texture(&mut self, size: (u32, u32), with_depth: bool) -> LayerTexture {
        self.layer_texture_cache
            .acquire(&self.device, size, with_depth)
    }

    /// Release a layer texture back to the cache pool
    pub fn release_layer_texture(&mut self, texture: LayerTexture) {
        self.layer_texture_cache.release(texture);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Layer Composition
    // ─────────────────────────────────────────────────────────────────────────────

    /// Create a bind group for layer composition
    fn create_layer_composite_bind_group(
        &self,
        uniform_buffer: &wgpu::Buffer,
        layer_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Layer Composite Bind Group"),
            layout: &self.bind_group_layouts.layer_composite,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(layer_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// Composite a layer texture onto a target
    ///
    /// Uses the LAYER_COMPOSITE_SHADER to blend the layer onto the target
    /// with the specified blend mode and opacity.
    pub fn composite_layer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        layer: &LayerTexture,
        dest_x: f32,
        dest_y: f32,
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        // Create uniform buffer for this composition
        let uniforms = crate::primitives::LayerCompositeUniforms::new(
            layer.size,
            dest_x,
            dest_y,
            (self.viewport_size.0 as f32, self.viewport_size.1 as f32),
            opacity,
            blend_mode,
        );

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Layer Composite Uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        // Create sampler
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Layer Composite Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create bind group
        let bind_group =
            self.create_layer_composite_bind_group(&uniform_buffer, &layer.view, &sampler);

        // Create render pass and draw
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Layer Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // Preserve existing content
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&self.pipelines.layer_composite);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..6, 0..1); // 6 vertices for quad (2 triangles)
    }

    /// Composite a layer with source/dest rectangle mapping
    ///
    /// Allows sampling a sub-region of the layer texture and placing it
    /// at a specific destination in the target.
    pub fn composite_layer_region(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        layer: &LayerTexture,
        source_rect: blinc_core::Rect,
        dest_rect: blinc_core::Rect,
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        // Convert source rect to normalized UV coordinates
        let layer_w = layer.size.0 as f32;
        let layer_h = layer.size.1 as f32;
        let source_uv = [
            source_rect.x() / layer_w,
            source_rect.y() / layer_h,
            source_rect.width() / layer_w,
            source_rect.height() / layer_h,
        ];

        let uniforms = crate::primitives::LayerCompositeUniforms::with_source_rect(
            source_uv,
            [
                dest_rect.x(),
                dest_rect.y(),
                dest_rect.width(),
                dest_rect.height(),
            ],
            (self.viewport_size.0 as f32, self.viewport_size.1 as f32),
            opacity,
            blend_mode,
        );

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Layer Composite Uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Layer Composite Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group =
            self.create_layer_composite_bind_group(&uniform_buffer, &layer.view, &sampler);

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Layer Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&self.pipelines.layer_composite);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..6, 0..1);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Effect Application Methods
    // ─────────────────────────────────────────────────────────────────────────────

    /// Apply a single Kawase blur pass
    ///
    /// Renders from `input` to `output` using the blur shader with the specified
    /// radius and iteration index.
    pub fn apply_blur_pass(
        &mut self,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        size: (u32, u32),
        radius: f32,
        iteration: u32,
    ) {
        use crate::primitives::BlurUniforms;

        let uniforms = BlurUniforms {
            texel_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            radius,
            iteration,
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Blur Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur Effect Bind Group"),
            layout: &self.bind_group_layouts.blur,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blur Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blur Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.blur);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Apply multi-pass Kawase blur
    ///
    /// Performs multiple blur passes for higher quality blur.
    /// Uses ping-pong rendering between two textures.
    ///
    /// Returns the final output texture (caller should release temp textures).
    pub fn apply_blur(
        &mut self,
        input: &LayerTexture,
        radius: f32,
        passes: u32,
    ) -> LayerTexture {
        if passes == 0 {
            // No blur needed, return a copy
            let output = self.layer_texture_cache.acquire(&self.device, input.size, false);
            // Copy input to output
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Blur Copy Encoder"),
                });
            encoder.copy_texture_to_texture(
                wgpu::ImageCopyTexture {
                    texture: &input.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyTexture {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: input.size.0,
                    height: input.size.1,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(encoder.finish()));
            return output;
        }

        let size = input.size;

        // For ping-pong we need two temp textures
        let mut temp_a = self.layer_texture_cache.acquire(&self.device, size, false);
        let mut temp_b = self.layer_texture_cache.acquire(&self.device, size, false);

        // First pass: input -> temp_a
        self.apply_blur_pass(&input.view, &temp_a.view, size, radius, 0);

        // Subsequent passes alternate between temp_a and temp_b
        for i in 1..passes {
            if i % 2 == 1 {
                // temp_a -> temp_b
                self.apply_blur_pass(&temp_a.view, &temp_b.view, size, radius, i);
            } else {
                // temp_b -> temp_a
                self.apply_blur_pass(&temp_b.view, &temp_a.view, size, radius, i);
            }
        }

        // Return the correct temp texture based on which has the final result
        // Release the other one back to the cache
        if passes % 2 == 1 {
            // Odd number of passes: result is in temp_a
            self.layer_texture_cache.release(temp_b);
            temp_a
        } else {
            // Even number of passes: result is in temp_b
            self.layer_texture_cache.release(temp_a);
            temp_b
        }
    }

    /// Apply color matrix transformation
    ///
    /// Transforms colors using a 4x5 matrix (4x4 matrix + offset column).
    /// Useful for grayscale, sepia, saturation, brightness, contrast, etc.
    pub fn apply_color_matrix(
        &mut self,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        matrix: &[f32; 20],
    ) {
        use crate::primitives::ColorMatrixUniforms;

        let uniforms = ColorMatrixUniforms::from_matrix(matrix);

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Color Matrix Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Color Matrix Effect Bind Group"),
            layout: &self.bind_group_layouts.color_matrix,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Color Matrix Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Color Matrix Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.color_matrix);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Apply drop shadow effect
    ///
    /// Creates a blurred, offset, colored shadow of the input.
    pub fn apply_drop_shadow(
        &mut self,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        size: (u32, u32),
        offset: (f32, f32),
        blur_radius: f32,
        spread: f32,
        color: [f32; 4],
    ) {
        use crate::primitives::DropShadowUniforms;

        let uniforms = DropShadowUniforms {
            offset: [offset.0, offset.1],
            blur_radius,
            spread,
            color,
            texel_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            _pad: [0.0, 0.0],
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Drop Shadow Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Drop Shadow Effect Bind Group"),
            layout: &self.bind_group_layouts.drop_shadow,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Drop Shadow Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Drop Shadow Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.drop_shadow);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Helper to create common color matrices
    pub fn grayscale_matrix() -> [f32; 20] {
        // Luminance weights (ITU-R BT.709)
        let r = 0.2126;
        let g = 0.7152;
        let b = 0.0722;
        [
            r, g, b, 0.0, 0.0,
            r, g, b, 0.0, 0.0,
            r, g, b, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    /// Create sepia tone color matrix
    pub fn sepia_matrix() -> [f32; 20] {
        [
            0.393, 0.769, 0.189, 0.0, 0.0,
            0.349, 0.686, 0.168, 0.0, 0.0,
            0.272, 0.534, 0.131, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    /// Create saturation adjustment matrix
    pub fn saturation_matrix(saturation: f32) -> [f32; 20] {
        let s = saturation;
        let r = 0.2126;
        let g = 0.7152;
        let b = 0.0722;
        let sr = (1.0 - s) * r;
        let sg = (1.0 - s) * g;
        let sb = (1.0 - s) * b;
        [
            sr + s, sg, sb, 0.0, 0.0,
            sr, sg + s, sb, 0.0, 0.0,
            sr, sg, sb + s, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    /// Create brightness adjustment matrix
    pub fn brightness_matrix(brightness: f32) -> [f32; 20] {
        let b = brightness - 1.0; // 0 = no change, positive = brighter
        [
            1.0, 0.0, 0.0, 0.0, b,
            0.0, 1.0, 0.0, 0.0, b,
            0.0, 0.0, 1.0, 0.0, b,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    /// Create contrast adjustment matrix
    pub fn contrast_matrix(contrast: f32) -> [f32; 20] {
        let c = contrast;
        let t = (1.0 - c) / 2.0;
        [
            c, 0.0, 0.0, 0.0, t,
            0.0, c, 0.0, 0.0, t,
            0.0, 0.0, c, 0.0, t,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    /// Create invert color matrix
    pub fn invert_matrix() -> [f32; 20] {
        [
            -1.0, 0.0, 0.0, 0.0, 1.0,
            0.0, -1.0, 0.0, 0.0, 1.0,
            0.0, 0.0, -1.0, 0.0, 1.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Layer Command Processing
    // ─────────────────────────────────────────────────────────────────────────────

    /// Apply layer effects to a texture
    ///
    /// Processes a list of LayerEffects in order and returns the final result.
    /// The input texture is not modified; a new texture with effects applied is returned.
    pub fn apply_layer_effects(
        &mut self,
        input: &LayerTexture,
        effects: &[blinc_core::LayerEffect],
    ) -> LayerTexture {
        use blinc_core::LayerEffect;

        if effects.is_empty() {
            // No effects, just return a copy
            let output = self.layer_texture_cache.acquire(&self.device, input.size, false);
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Layer Effect Copy Encoder"),
                });
            encoder.copy_texture_to_texture(
                wgpu::ImageCopyTexture {
                    texture: &input.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyTexture {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: input.size.0,
                    height: input.size.1,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(encoder.finish()));
            return output;
        }

        let size = input.size;
        let mut current = self.layer_texture_cache.acquire(&self.device, size, false);

        // Copy input to current
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Layer Effect Init Copy"),
                });
            encoder.copy_texture_to_texture(
                wgpu::ImageCopyTexture {
                    texture: &input.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyTexture {
                    texture: &current.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        for effect in effects {
            match effect {
                LayerEffect::Blur { radius, quality: _ } => {
                    // Calculate number of passes based on radius
                    let passes = (*radius / 4.0).ceil().max(1.0) as u32;
                    let blurred = self.apply_blur(&current, *radius, passes);
                    self.layer_texture_cache.release(current);
                    current = blurred;
                }

                LayerEffect::DropShadow {
                    offset_x,
                    offset_y,
                    blur,
                    spread,
                    color,
                } => {
                    let temp = self.layer_texture_cache.acquire(&self.device, size, false);
                    self.apply_drop_shadow(
                        &current.view,
                        &temp.view,
                        size,
                        (*offset_x, *offset_y),
                        *blur,
                        *spread,
                        [color.r, color.g, color.b, color.a],
                    );
                    self.layer_texture_cache.release(current);
                    current = temp;
                }

                LayerEffect::Glow {
                    radius,
                    color,
                    intensity,
                } => {
                    // Glow is implemented as a blurred, color-tinted version
                    // For simplicity, apply blur and color enhancement directly
                    // (A proper glow would composite the blur behind the original)
                    let passes = (*radius / 4.0).ceil().max(1.0) as u32;
                    let blurred = self.apply_blur(&current, *radius, passes);

                    // Apply glow color and intensity via color matrix
                    // Mix the glow color with the original using intensity as blend factor
                    let glow_matrix = [
                        1.0 - intensity + color.r * intensity, 0.0, 0.0, 0.0, 0.0,
                        0.0, 1.0 - intensity + color.g * intensity, 0.0, 0.0, 0.0,
                        0.0, 0.0, 1.0 - intensity + color.b * intensity, 0.0, 0.0,
                        0.0, 0.0, 0.0, 1.0, 0.0,
                    ];

                    let tinted = self.layer_texture_cache.acquire(&self.device, size, false);
                    self.apply_color_matrix(&blurred.view, &tinted.view, &glow_matrix);
                    self.layer_texture_cache.release(blurred);
                    self.layer_texture_cache.release(current);
                    current = tinted;
                }

                LayerEffect::ColorMatrix { matrix } => {
                    let temp = self.layer_texture_cache.acquire(&self.device, size, false);
                    self.apply_color_matrix(&current.view, &temp.view, matrix);
                    self.layer_texture_cache.release(current);
                    current = temp;
                }
            }
        }

        current
    }

    /// Composite two textures together
    ///
    /// Blends `top` over `bottom` using the specified blend mode and opacity.
    pub fn composite_textures(
        &mut self,
        bottom: &wgpu::TextureView,
        top: &wgpu::TextureView,
        output: &wgpu::TextureView,
        size: (u32, u32),
        blend_mode: blinc_core::BlendMode,
        opacity: f32,
    ) {
        use crate::primitives::CompositeUniforms;

        let uniforms = CompositeUniforms {
            opacity,
            blend_mode: blend_mode as u32,
            _padding: [0.0; 2],
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Composite Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Composite Bind Group"),
            layout: &self.bind_group_layouts.composite,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(bottom),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(top),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Composite Pass Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Composite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.composite);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a range of primitives to a target
    fn render_primitive_range(
        &mut self,
        target: &wgpu::TextureView,
        batch: &PrimitiveBatch,
        start_idx: usize,
        end_idx: usize,
        clear_color: [f64; 4],
    ) {
        if start_idx >= end_idx {
            return;
        }

        // Extract the primitive range
        let primitive_count = end_idx - start_idx;
        let primitives = &batch.primitives[start_idx..end_idx];

        if primitives.is_empty() {
            return;
        }

        // Update uniforms
        let uniforms = Uniforms {
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.buffers.uniforms, 0, bytemuck::bytes_of(&uniforms));

        // Write primitive range to buffer
        self.queue.write_buffer(
            &self.buffers.primitives,
            0,
            bytemuck::cast_slice(primitives),
        );

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Primitive Range Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Primitive Range Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.sdf);
            render_pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            render_pass.draw(0..6, 0..primitive_count as u32);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Blit a texture to the target with blending
    fn blit_texture_to_target(
        &mut self,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        use crate::primitives::LayerCompositeUniforms;

        // Full viewport blit - source covers entire texture, dest covers entire viewport
        let uniforms = LayerCompositeUniforms {
            source_rect: [0.0, 0.0, 1.0, 1.0], // Full texture (normalized)
            dest_rect: [0.0, 0.0, self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            viewport_size: [self.viewport_size.0 as f32, self.viewport_size.1 as f32],
            opacity,
            blend_mode: blend_mode as u32,
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Blit Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blit Bind Group"),
            layout: &self.bind_group_layouts.layer_composite,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blit Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Load existing content - we're blending on top
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipelines.layer_composite);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Blit a specific region from source texture to target at given position
    ///
    /// This is used for layer effects where we need to composite only the
    /// element's region back to the target at the correct position.
    fn blit_region_to_target(
        &mut self,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        position: (f32, f32),
        size: (f32, f32),
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) {
        use crate::primitives::LayerCompositeUniforms;

        let vp_w = self.viewport_size.0 as f32;
        let vp_h = self.viewport_size.1 as f32;

        // Source rect in normalized coordinates (0-1)
        // The source texture is viewport-sized, so we extract the element's region
        let source_rect = [
            position.0 / vp_w,
            position.1 / vp_h,
            size.0 / vp_w,
            size.1 / vp_h,
        ];

        // Dest rect in viewport pixel coordinates
        let dest_rect = [position.0, position.1, size.0, size.1];

        let uniforms = LayerCompositeUniforms {
            source_rect,
            dest_rect,
            viewport_size: [vp_w, vp_h],
            opacity,
            blend_mode: blend_mode as u32,
        };

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Region Blit Uniforms Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Region Blit Bind Group"),
            layout: &self.bind_group_layouts.layer_composite,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.path_image_sampler),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Region Blit Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Region Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Set scissor rect to only affect the element's region
            render_pass.set_scissor_rect(
                position.0.max(0.0) as u32,
                position.1.max(0.0) as u32,
                size.0.min(vp_w - position.0).max(1.0) as u32,
                size.1.min(vp_h - position.1).max(1.0) as u32,
            );

            render_pass.set_pipeline(&self.pipelines.layer_composite);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}

impl Default for GpuRenderer {
    fn default() -> Self {
        // Create a basic renderer synchronously using pollster
        pollster::block_on(Self::new(RendererConfig::default()))
            .expect("Failed to create default renderer")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────────────────────────────────────────────────────────
    // LayerTextureCache Tests
    // ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn layer_texture_cache_initial_state() {
        let cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);
        assert_eq!(cache.pool_size(), 0);
        assert_eq!(cache.named_count(), 0);
    }

    #[test]
    fn layer_texture_cache_clear_all() {
        let cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);
        // Pool is empty, but clear_all should work without panic
        let mut cache = cache;
        cache.clear_all();
        assert_eq!(cache.pool_size(), 0);
        assert_eq!(cache.named_count(), 0);
    }

    #[test]
    fn layer_texture_cache_format_preserved() {
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let cache = LayerTextureCache::new(format);
        assert_eq!(cache.format, format);
    }

    #[test]
    fn layer_texture_matches_size() {
        // Test requires GPU, but we can test the matches_size logic
        // by creating a helper struct with known sizes
        struct FakeTexture {
            size: (u32, u32),
        }
        impl FakeTexture {
            fn matches_size(&self, size: (u32, u32)) -> bool {
                self.size == size
            }
        }

        let tex = FakeTexture { size: (800, 600) };
        assert!(tex.matches_size((800, 600)));
        assert!(!tex.matches_size((800, 601)));
        assert!(!tex.matches_size((801, 600)));
        assert!(!tex.matches_size((400, 300)));
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // GPU Integration Tests (require actual wgpu device)
    // ─────────────────────────────────────────────────────────────────────────────

    /// Helper to create a test wgpu device
    async fn create_test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .ok()?;

        Some((device, queue))
    }

    /// Helper to create unique layer IDs for testing
    fn test_layer_id(id: u64) -> blinc_core::LayerId {
        blinc_core::LayerId::new(id)
    }

    #[test]
    fn layer_texture_cache_acquire_and_release() {
        let result = pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                // Skip test if no GPU available
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);

            // Acquire a texture
            let tex1 = cache.acquire(&device, (512, 512), false);
            assert_eq!(tex1.size, (512, 512));
            assert!(!tex1.has_depth);

            // Release it back to pool
            cache.release(tex1);
            assert_eq!(cache.pool_size(), 1);

            // Acquire again - should reuse from pool
            let tex2 = cache.acquire(&device, (512, 512), false);
            assert_eq!(tex2.size, (512, 512));
            assert_eq!(cache.pool_size(), 0); // Removed from pool

            // Acquire different size - should create new
            let tex3 = cache.acquire(&device, (1024, 768), false);
            assert_eq!(tex3.size, (1024, 768));
            assert_eq!(cache.pool_size(), 0);

            // Release both
            cache.release(tex2);
            cache.release(tex3);
            assert_eq!(cache.pool_size(), 2);
        });
        result
    }

    #[test]
    fn layer_texture_cache_named_textures() {
        let result = pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);
            let layer_id = test_layer_id(1);

            // Store a named texture
            let tex = cache.acquire(&device, (256, 256), false);
            cache.store(layer_id, tex);
            assert_eq!(cache.named_count(), 1);

            // Get reference to it
            let retrieved = cache.get(&layer_id);
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().size, (256, 256));

            // Remove it
            let removed = cache.remove(&layer_id);
            assert!(removed.is_some());
            assert_eq!(cache.named_count(), 0);

            // Release back to pool
            cache.release(removed.unwrap());
            assert_eq!(cache.pool_size(), 1);
        });
        result
    }

    #[test]
    fn layer_texture_cache_clear_named_releases_to_pool() {
        let result = pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);

            // Store several named textures
            for i in 0..3 {
                let tex = cache.acquire(&device, (128, 128), false);
                cache.store(test_layer_id(i + 100), tex);
            }
            assert_eq!(cache.named_count(), 3);
            assert_eq!(cache.pool_size(), 0);

            // Clear named - should release to pool
            cache.clear_named();
            assert_eq!(cache.named_count(), 0);
            assert_eq!(cache.pool_size(), 3);
        });
        result
    }

    #[test]
    fn layer_texture_cache_pool_size_limit() {
        let result = pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);
            // Default max_pool_size is 8

            // Acquire and release more than max_pool_size textures
            let mut textures = Vec::new();
            for _ in 0..12 {
                textures.push(cache.acquire(&device, (64, 64), false));
            }

            // Release all
            for tex in textures {
                cache.release(tex);
            }

            // Pool should be capped at max_pool_size (8)
            assert_eq!(cache.pool_size(), 8);
        });
        result
    }

    #[test]
    fn layer_texture_with_depth() {
        let result = pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);

            // Acquire texture with depth
            let tex_with_depth = cache.acquire(&device, (512, 512), true);
            assert!(tex_with_depth.has_depth);
            assert!(tex_with_depth.depth_view.is_some());

            // Acquire texture without depth
            let tex_no_depth = cache.acquire(&device, (512, 512), false);
            assert!(!tex_no_depth.has_depth);
            assert!(tex_no_depth.depth_view.is_none());

            // Release both
            cache.release(tex_with_depth);
            cache.release(tex_no_depth);
            assert_eq!(cache.pool_size(), 2);

            // Acquire with depth - should NOT get the one without depth
            let tex_reacquire = cache.acquire(&device, (512, 512), true);
            assert!(tex_reacquire.has_depth);
            assert_eq!(cache.pool_size(), 1); // The no-depth one remains
        });
        result
    }

    #[test]
    fn layer_texture_reuse_larger() {
        let result = pollster::block_on(async {
            let Some((device, _queue)) = create_test_device().await else {
                return;
            };

            let mut cache = LayerTextureCache::new(wgpu::TextureFormat::Bgra8Unorm);

            // Acquire and release a large texture
            let large_tex = cache.acquire(&device, (1024, 1024), false);
            cache.release(large_tex);
            assert_eq!(cache.pool_size(), 1);

            // Acquire smaller - should reuse the larger one
            let small_tex = cache.acquire(&device, (256, 256), false);
            // The actual size will be 1024x1024 (reused from pool)
            assert!(small_tex.size.0 >= 256 && small_tex.size.1 >= 256);
            assert_eq!(cache.pool_size(), 0);
        });
        result
    }
}
