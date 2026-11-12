//! Creating a renderer: constructors for a window, a canvas or an offscreen target, and warming the pipelines.

use super::{
    GpuRenderer, RendererConfig, RendererError, apply_renderer_config_overrides,
    device_required_limits, log_renderer_config, requested_device_features,
};
use crate::path::PathVertex;
use std::sync::Arc;

impl GpuRenderer {
    /// Get the preferred backend for the current platform
    ///
    /// Using the primary backend instead of all backends reduces memory usage
    /// by avoiding initialization of multiple GPU driver stacks.
    pub(super) fn preferred_backends() -> wgpu::Backends {
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
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
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
            .map_err(|_| RendererError::AdapterNotFound)?;

        let required_limits = device_required_limits(&adapter);
        let config = apply_renderer_config_overrides(config, &required_limits);
        log_renderer_config(&config);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Blinc GPU Device"),
                required_features: requested_device_features(&adapter),
                required_limits,
                // MemoryUsage hint tells the driver to prefer lower memory over performance.
                // This helps reduce RSS on integrated GPUs (Apple Silicon) where GPU memory
                // is shared with CPU and counts against process memory.
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(RendererError::DeviceError)?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Non-sRGB for the same reason windowed surfaces prefer it below:
        // the shaders bake the gamma transfer at the source, so an sRGB
        // target treats already-encoded values as linear and encodes them
        // again. Offscreen captures used to come back lighter than the
        // same UI on screen.
        let texture_format = config
            .texture_format
            .unwrap_or(wgpu::TextureFormat::Bgra8Unorm);

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
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
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
            .map_err(|_| RendererError::AdapterNotFound)?;

        let required_limits = device_required_limits(&adapter);
        let config = apply_renderer_config_overrides(config, &required_limits);
        log_renderer_config(&config);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Blinc GPU Device"),
                required_features: requested_device_features(&adapter),
                required_limits,
                // MemoryUsage hint tells the driver to prefer lower memory over performance.
                // This helps reduce RSS on integrated GPUs (Apple Silicon) where GPU memory
                // is shared with CPU and counts against process memory.
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
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

        // Always prefer a non-sRGB surface format. Blinc's shaders
        // emit sRGB-encoded colors directly into the framebuffer
        // (text glyphs, SDF fills, layer composites all bake the
        // gamma transfer at the source), so an sRGB-format surface
        // would treat those already-encoded values as linear and
        // re-apply the curve on present — every colour visibly
        // washes out.
        //
        // This was originally `#[cfg(target_os = "macos")]`-only,
        // with non-macOS native falling through to sRGB "for now."
        // Linux users reported washed colours; the underlying
        // pipeline is the same on every native platform so the
        // selection should be too. WebGL2 was already covered by
        // the storage-buffer-limit heuristic below — folded into
        // this unconditional non-sRGB-first preference.
        //
        // The fallback (first available format) only kicks in on
        // platforms whose surface only advertises sRGB formats;
        // there's nothing better we can do there short of writing
        // a manual `pow(c, 2.2)` in every shader.
        let texture_format = config.texture_format.unwrap_or_else(|| {
            surface_caps
                .formats
                .iter()
                .find(|f| !f.is_srgb())
                .copied()
                .unwrap_or(surface_caps.formats[0])
        });
        tracing::info!(
            "Selected texture format: {:?} (sRGB: {})",
            texture_format,
            texture_format.is_srgb()
        );

        let renderer = Self::create_renderer(
            instance,
            adapter,
            device,
            queue,
            texture_format,
            config,
            (800, 600),
        )?;

        // Force driver to compile deferred Vulkan pipelines now,
        // before the first surface present. No-op on macOS/Windows.
        renderer.pre_warm_pipelines();

        Ok((renderer, surface))
    }

    /// Create a new renderer with a `<canvas>` element on `wasm32`.
    ///
    /// Mirrors [`Self::with_surface`] but takes a
    /// [`web_sys::HtmlCanvasElement`] instead of a raw-window-handle
    /// type, because `HtmlCanvasElement` doesn't (and can't) implement
    /// `HasWindowHandle` / `HasDisplayHandle` — the browser exposes its
    /// surface through `wgpu::SurfaceTarget::Canvas` instead.
    ///
    /// The texture format is selected from the browser-reported surface
    /// capabilities, preferring an sRGB format. WebGPU's canonical
    /// preferred format on Chrome is `Bgra8UnormSrgb`, but Safari
    /// Technology Preview reports only `Rgba8UnormSrgb` — the
    /// `find(is_srgb)` lookup handles both.
    ///
    /// # Browser availability
    ///
    /// Requires WebGPU (Chrome ≥ 113, Edge ≥ 113, Safari Technology
    /// Preview, Firefox Nightly with the WebGPU flag). The `web` feature
    /// also enables the `webgl` backend so wgpu can fall back to WebGL2
    /// where WebGPU isn't available, but the fallback path will reject
    /// some Blinc shader features (storage buffers in particular).
    #[cfg(target_arch = "wasm32")]
    pub async fn with_canvas(
        canvas: web_sys::HtmlCanvasElement,
        config: RendererConfig,
    ) -> Result<(Self, wgpu::Surface<'static>), RendererError> {
        Self::with_web_target(wgpu::SurfaceTarget::Canvas(canvas), config).await
    }

    /// Create a renderer for a canvas transferred to a dedicated worker.
    #[cfg(target_arch = "wasm32")]
    pub async fn with_offscreen_canvas(
        canvas: web_sys::OffscreenCanvas,
        config: RendererConfig,
    ) -> Result<(Self, wgpu::Surface<'static>), RendererError> {
        Self::with_web_target(wgpu::SurfaceTarget::OffscreenCanvas(canvas), config).await
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) async fn with_web_target(
        target: wgpu::SurfaceTarget<'static>,
        config: RendererConfig,
    ) -> Result<(Self, wgpu::Surface<'static>), RendererError> {
        // Browsers can expose `navigator.gpu` and still return no adapter (a GPU outside the
        // browser's WebGPU allowlist), and wgpu then never reaches WebGL2. Probe first, with no
        // surface so the canvas stays free to take a WebGL2 context.
        let webgpu = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });
        let has_webgpu = webgpu
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .is_ok();
        let instance = if has_webgpu {
            webgpu
        } else {
            tracing::info!("No WebGPU adapter; using the WebGL2 backend");
            wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::GL,
                ..Default::default()
            })
        };

        let surface = instance
            .create_surface(target)
            .map_err(RendererError::SurfaceError)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| RendererError::AdapterNotFound)?;

        // Check that the adapter supports storage buffers in vertex
        // shaders — Blinc's SDF pipeline requires this (the primitives
        // buffer is `var<storage, read>` accessed from both vertex and
        // fragment stages). Skip on wasm32: the WebGPU spec guarantees
        // storage buffer support, but wgpu's downlevel report can be
        // wrong when the GL fallback adapter is selected (WebGL2 lacks
        // VERTEX_STORAGE, producing a false negative even though the
        // browser's WebGPU backend supports it).
        #[cfg(not(target_arch = "wasm32"))]
        {
            let downlevel = adapter.get_downlevel_capabilities();
            if !downlevel
                .flags
                .contains(wgpu::DownlevelFlags::VERTEX_STORAGE)
            {
                return Err(RendererError::ShaderError(
                    "GPU adapter does not support storage buffers in vertex shaders \
                     (VERTEX_STORAGE). Blinc requires this feature for its SDF \
                     rendering pipeline."
                        .to_string(),
                ));
            }
        }

        let required_limits = device_required_limits(&adapter);
        let config = apply_renderer_config_overrides(config, &required_limits);
        log_renderer_config(&config);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Blinc GPU Device (Web)"),
                required_features: requested_device_features(&adapter),
                required_limits,
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(RendererError::DeviceError)?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_caps = surface.get_capabilities(&adapter);
        tracing::debug!(
            "Web surface capabilities - formats: {:?}",
            surface_caps.formats
        );
        tracing::debug!(
            "Web surface capabilities - alpha modes: {:?}",
            surface_caps.alpha_modes
        );

        // On WebGPU (Chrome/Safari), prefer sRGB — the browser pipeline
        // expects sRGB output. On WebGL2 (GL adapter, no storage buffers),
        // prefer non-sRGB — shaders output sRGB-encoded colors directly,
        // and an sRGB surface would apply gamma encoding again (washed out).
        let is_gl_adapter = adapter.limits().max_storage_buffers_per_shader_stage == 0;
        let texture_format = config.texture_format.unwrap_or_else(|| {
            if is_gl_adapter {
                surface_caps
                    .formats
                    .iter()
                    .find(|f| !f.is_srgb())
                    .copied()
                    .unwrap_or(surface_caps.formats[0])
            } else {
                surface_caps
                    .formats
                    .iter()
                    .find(|f| f.is_srgb())
                    .copied()
                    .unwrap_or(surface_caps.formats[0])
            }
        });
        tracing::info!(
            "Web surface texture format: {:?} (sRGB: {}, GL adapter: {})",
            texture_format,
            texture_format.is_srgb(),
            is_gl_adapter
        );

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

    /// Create a new renderer with an existing wgpu instance and surface
    ///
    /// This is useful for platforms like Android where the surface is created
    /// from a native window handle before the renderer is initialized.
    pub async fn with_instance_and_surface(
        instance: wgpu::Instance,
        surface: &wgpu::Surface<'_>,
        config: RendererConfig,
    ) -> Result<Self, RendererError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| RendererError::AdapterNotFound)?;

        let required_limits = device_required_limits(&adapter);
        let config = apply_renderer_config_overrides(config, &required_limits);
        log_renderer_config(&config);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Blinc GPU Device"),
                required_features: requested_device_features(&adapter),
                required_limits,
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(RendererError::DeviceError)?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_caps = surface.get_capabilities(&adapter);
        tracing::debug!("Surface capabilities - formats: {:?}", surface_caps.formats);

        // Select texture format based on platform
        let texture_format = config.texture_format.unwrap_or_else(|| {
            // On Android, prefer non-sRGB format to match macOS behavior
            // Using sRGB causes colors to appear washed out because the GPU
            // applies automatic gamma correction
            surface_caps
                .formats
                .iter()
                .find(|f| !f.is_srgb())
                .copied()
                .unwrap_or(surface_caps.formats[0])
        });
        tracing::info!("Surface formats available: {:?}", surface_caps.formats);
        tracing::info!("Selected texture format: {:?}", texture_format);

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

    /// Pre-compile the eagerly-created render pipelines by issuing
    /// dummy draws against a throwaway texture.
    ///
    /// Vulkan drivers defer shader compilation to first-draw, so the
    /// first frame in a Linux app pays the cost of compiling 8+ SDF
    /// shaders + path/clear_quad sequentially on the main thread. On
    /// 8GB-RAM laptops the user reported this as "huge delay before
    /// the initial route renders". Pre-warming pushes that cost into
    /// renderer construction (before the window is visible), trading
    /// a slightly slower `with_surface` for a snappy first paint.
    ///
    /// Only pipelines whose required bind groups exist at this point
    /// are pre-warmed (SDF family + path + clear_quad). text_overlay,
    /// composite_overlay, and layer_composite need per-call bind
    /// groups created at first use, so they're left to JIT-compile on
    /// the first real frame — those are individually cheap. MSAA
    /// variants are created lazily for the active sample_count and
    /// aren't pre-warmed here either.
    ///
    /// Opt out with `BLINC_PIPELINE_PREWARM=0`. No-op on non-Linux
    /// (Metal/DX12 don't defer compilation this way).
    #[cfg(target_os = "linux")]
    pub(super) fn pre_warm_pipelines(&self) {
        if std::env::var("BLINC_PIPELINE_PREWARM").as_deref() == Ok("0") {
            tracing::debug!("Pipeline pre-warm disabled via BLINC_PIPELINE_PREWARM=0");
            return;
        }

        let start = std::time::Instant::now();

        let throwaway = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Pipeline Pre-Warm Throwaway"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = throwaway.create_view(&wgpu::TextureViewDescriptor::default());

        // Dummy vertex buffer for pipelines that declare a vertex
        // buffer layout (Path always; SDF when VERTEX_STORAGE is
        // unavailable and the renderer falls back to instance VBs).
        // Validation requires slot 0 to be bound to a real buffer
        // matching the declared stride — without it, `pass.draw(…)`
        // panics with "requires vertex buffer 0 to be set" and the
        // pre-warm aborts the whole renderer construction. 1 KiB of
        // zeros covers every draw count we issue here (≤ 3 vertices
        // × 120 B per PathVertex = 360 B, comfortably less than 1024).
        let dummy_vb = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Pipeline Pre-Warm Dummy VB"),
            size: 1024,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Pipeline Pre-Warm Encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Pipeline Pre-Warm Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // clear_quad — no bind group, no vertex buffer required.
            pass.set_pipeline(&self.pipelines.clear_quad);
            pass.draw(0..3, 0..1);

            // SDF family shares `bind_groups.sdf` at slot 0. The
            // pipelines only declare a vertex buffer when
            // `has_vertex_storage` is false (instance-stepped VB
            // fallback) — bind the dummy in that case so validation
            // accepts the draw on hosts without storage in vertex
            // shaders (older Intel iGPUs, some Mesa configs).
            pass.set_bind_group(0, &self.bind_groups.sdf, &[]);
            if !self.has_vertex_storage {
                pass.set_vertex_buffer(0, dummy_vb.slice(..));
            }
            for sdf_pipeline in [
                &self.pipelines.sdf_core,
                &self.pipelines.sdf_core_simple,
                &self.pipelines.sdf_plain,
                &self.pipelines.sdf_shadow,
                &self.pipelines.sdf_3d,
                &self.pipelines.sdf_notch,
                &self.pipelines.sdf_core_overlay,
                &self.pipelines.sdf_core_simple_overlay,
                &self.pipelines.sdf_plain_overlay,
                &self.pipelines.sdf_shadow_overlay,
                &self.pipelines.sdf_3d_overlay,
                &self.pipelines.sdf_notch_overlay,
            ] {
                pass.set_pipeline(sdf_pipeline);
                pass.draw(0..6, 0..1);
            }

            // Path family always declares a vertex buffer layout —
            // bind the dummy unconditionally before drawing.
            pass.set_bind_group(0, &self.bind_groups.path, &[]);
            pass.set_vertex_buffer(0, dummy_vb.slice(..));
            for path_pipeline in [&self.pipelines.path, &self.pipelines.path_overlay] {
                pass.set_pipeline(path_pipeline);
                pass.draw(0..3, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // Block until the GPU has actually finished compiling and
        // running these. Without this the work stays queued and the
        // first real frame still pays the compile cost.
        let _ = self.device.poll(wgpu::PollType::Wait);

        tracing::info!(
            "Vulkan pipeline pre-warm complete in {:.1} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }

    /// No-op pre-warm for non-Linux targets — Metal (macOS),
    /// DX12 (Windows), and WebGPU compile shaders eagerly at
    /// pipeline creation, so there's nothing to warm up.
    #[cfg(not(target_os = "linux"))]
    pub(super) fn pre_warm_pipelines(&self) {}

    /// Create a new surface for an additional window.
    ///
    /// Uses the existing wgpu Instance to create a surface that can be
    /// configured and rendered to using the shared device and queue.
    /// This is used for multi-window support.
    pub fn create_surface<W>(&self, window: Arc<W>) -> Result<wgpu::Surface<'static>, RendererError>
    where
        W: raw_window_handle::HasWindowHandle
            + raw_window_handle::HasDisplayHandle
            + Send
            + Sync
            + 'static,
    {
        self.instance
            .create_surface(wgpu::SurfaceTarget::from(window))
            .map_err(RendererError::SurfaceError)
    }
}
