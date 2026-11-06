//! Blinc Application Delegate
//!
//! The main entry point for Blinc applications.

use blinc_gpu::{FontRegistry, GpuRenderer, RendererConfig, TextRenderingContext};
use blinc_layout::RenderTree;
use blinc_layout::prelude::*;
use std::sync::{Arc, Mutex};

use crate::context::RenderContext;
use crate::error::{BlincError, Result};

/// Blinc application configuration
#[derive(Clone, Debug)]
pub struct BlincConfig {
    /// Maximum primitives per batch
    pub max_primitives: usize,
    /// Maximum glass primitives per batch
    pub max_glass_primitives: usize,
    /// Maximum glyphs per batch
    pub max_glyphs: usize,
    /// MSAA sample count (1, 2, 4, or 8)
    pub sample_count: u32,
}

impl Default for BlincConfig {
    fn default() -> Self {
        Self {
            max_primitives: 20_000,
            max_glass_primitives: 1_000,
            max_glyphs: 50_000,
            sample_count: 4, // 4x MSAA for path anti-aliasing
        }
    }
}

/// The main Blinc application
///
/// This is the primary interface for rendering Blinc UI.
/// It handles all GPU initialization and provides a clean API.
///
/// # Example
///
/// ```ignore
/// use blinc_app::prelude::*;
///
/// let app = BlincApp::new()?;
///
/// let ui = div()
///     .w(400.0).h(300.0)
///     .child(text("Hello!").size(24.0));
///
/// // Render to a texture - handles everything automatically
/// app.render(&ui, target_view, 400.0, 300.0)?;
/// ```
pub struct BlincApp {
    ctx: RenderContext,
    config: BlincConfig,
}

impl BlincApp {
    /// Create a new Blinc application with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(BlincConfig::default())
    }

    /// Create a new Blinc application from an existing render context
    ///
    /// This is used internally for platform-specific initialization (Android, iOS)
    /// where the GPU setup is done differently.
    pub(crate) fn from_context(ctx: RenderContext, config: BlincConfig) -> Self {
        Self { ctx, config }
    }

    /// Create a new Blinc application with custom configuration
    pub fn with_config(config: BlincConfig) -> Result<Self> {
        // Create renderer with sample_count=1 for SDF pipelines.
        // MSAA is handled separately via render_overlay_msaa for foreground paths.
        let renderer_config = RendererConfig {
            max_primitives: config.max_primitives,
            max_glass_primitives: config.max_glass_primitives,
            max_glyphs: config.max_glyphs,
            sample_count: 1, // SDF pipelines always use single-sampled textures
            texture_format: None,
            unified_text_rendering: true,
            ..RendererConfig::default()
        };

        let renderer = pollster::block_on(GpuRenderer::new(renderer_config))
            .map_err(|e| BlincError::GpuInit(e.to_string()))?;

        let device = renderer.device_arc();
        let queue = renderer.queue_arc();

        let mut text_ctx = TextRenderingContext::new(device.clone(), queue.clone());

        // Load system default font
        for font_path in crate::system_font_paths() {
            let path = std::path::Path::new(font_path);
            if path.exists()
                && let Ok(data) = std::fs::read(path)
            {
                let _ = text_ctx.load_font_data(data);
                break;
            }
        }

        // Preload common fonts that apps might use
        // This ensures fonts are cached before render time
        text_ctx.preload_fonts(&[
            "Inter",
            "Fira Code",
            "Menlo",
            "SF Mono",
            "SF Pro",
            "Roboto",
            "Consolas",
            "Monaco",
            "Source Code Pro",
            "JetBrains Mono",
        ]);

        // Preload generic font weights (for system fallback fonts)
        text_ctx.preload_generic_styles(
            blinc_gpu::GenericFont::SansSerif,
            &[400, 600, 700, 800, 900],
            false,
        );
        text_ctx.preload_generic_styles(
            blinc_gpu::GenericFont::SansSerif,
            &[400, 600, 700, 800, 900],
            true,
        );
        text_ctx.preload_generic_styles(blinc_gpu::GenericFont::Monospace, &[400, 700], false);

        let ctx = RenderContext::new(renderer, text_ctx, device, queue, config.sample_count);

        Ok(Self { ctx, config })
    }

    /// Render a UI element tree to a texture
    ///
    /// This handles everything automatically:
    /// - Computes layout
    /// - Renders background elements
    /// - Renders glass elements with backdrop blur
    /// - Renders foreground elements on top
    /// - Renders text at layout-computed positions
    /// - Renders SVG icons at layout-computed positions
    /// - Applies MSAA if configured
    ///
    /// # Arguments
    ///
    /// * `element` - The root UI element (created with `div()`, etc.)
    /// * `target` - The texture view to render to
    /// * `width` - Viewport width in pixels
    /// * `height` - Viewport height in pixels
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ui = div().w(400.0).h(300.0)
    ///     .flex_col().gap(4.0)
    ///     .child(
    ///         div().glass().rounded(16.0)
    ///             .child(text("Hello!").size(24.0))
    ///     );
    ///
    /// app.render(&ui, &target_view, 400.0, 300.0)?;
    /// ```
    pub fn render<E: ElementBuilder>(
        &mut self,
        element: &E,
        target: &wgpu::TextureView,
        width: f32,
        height: f32,
    ) -> Result<()> {
        let mut tree = RenderTree::from_element(element);
        tree.compute_layout(width, height);
        self.ctx
            .render_tree(&tree, width as u32, height as u32, target)
    }

    /// Render `element` into a freshly-allocated offscreen
    /// `wgpu::Texture` in the renderer's configured `texture_format`.
    ///
    /// Routes through the plain [`Self::render`] path: static layout
    /// and paint only. **Springs, motion containers, transform
    /// animations, the motion-subtree bake / overlay system, and the
    /// compositor fast path DO NOT RUN.** Use this for screenshots,
    /// debug captures, image-diff visual regression — anything where
    /// the frame is the same regardless of time `t`.
    ///
    /// For animated content (the Sketch + Player + Timeline →
    /// video-export pipeline) use
    /// [`Self::render_to_texture_with_motion`] instead and manage
    /// your own [`RenderState`] across frames so spring physics and
    /// motion-bake caches persist between calls.
    ///
    /// The texture is created with `RENDER_ATTACHMENT | COPY_SRC` so
    /// callers can either feed it into a follow-up GPU pass (a custom
    /// `run_gpu_pass`, a `blinc_media` video encoder that consumes
    /// `wgpu::Texture` directly) or read back to CPU memory via
    /// `wgpu::CommandEncoder::copy_texture_to_buffer`.
    ///
    /// See [`Self::render_to_rgba8`] for the CPU-pixel convenience that
    /// adds the readback + swizzle for `image` / `ffmpeg-next` feeds.
    pub fn render_to_texture<E: ElementBuilder>(
        &mut self,
        element: &E,
        width: u32,
        height: u32,
    ) -> Result<wgpu::Texture> {
        let texture = self.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("blinc_app::render_to_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture_format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.render(element, &view, width as f32, height as f32)?;
        Ok(texture)
    }

    /// Render `element` and read the framebuffer back as tightly-packed
    /// RGBA8 pixels.
    ///
    /// Always returns `width * height * 4` bytes in `[R, G, B, A, ...]`
    /// order regardless of the renderer's internal pixel format —
    /// `BGRA8UnormSrgb` (the common surface format on macOS / Windows)
    /// is swizzled on the way out, and the wgpu copy-row padding
    /// (`COPY_BYTES_PER_ROW_ALIGNMENT`) is unwound so callers see no
    /// stride.
    ///
    /// Higher-level companion to [`Self::render_to_texture`] for the
    /// "I just want bytes" use cases — `image::RgbaImage::from_raw`
    /// for PNG export, `ffmpeg-next` for MP4 / WebM encoding,
    /// pixel-diff visual regression tests. For GPU-side composition
    /// (custom passes, video-encoder buffers that take `wgpu::Texture`
    /// directly) prefer `render_to_texture` to avoid the device →
    /// host round-trip.
    ///
    /// Blocks until the GPU finishes the render and the readback
    /// buffer maps. Throughput on a single thread is bound by frame
    /// render cost plus the read-after-write fence; for a sustained
    /// export pipeline run multiple `BlincApp` instances in parallel
    /// or pipeline frame N+1's render with frame N's readback.
    pub fn render_to_rgba8<E: ElementBuilder>(
        &mut self,
        element: &E,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        let texture = self.render_to_texture(element, width, height)?;
        self.read_texture_to_rgba8(&texture, width, height)
    }

    /// Render `element` into an offscreen texture using the
    /// motion-aware render path — same one the windowed runner uses
    /// per frame.
    ///
    /// Routes through [`Self::render_tree_with_motion`], so springs,
    /// motion containers, transform animations, the motion-subtree
    /// bake / overlay system, and the compositor fast path all run.
    /// The caller owns the [`RenderState`] and is responsible for:
    ///
    /// 1. **Ticking the `AnimationScheduler`** (held inside the
    ///    `RenderState`) forward by the right `dt` before each frame —
    ///    `state.animations().lock().unwrap().tick(dt_ms)` for a
    ///    deterministic export, or seeking a [`blinc_animation::Timeline`]
    ///    that the tree reads from.
    /// 2. **Reusing the same `RenderState` across frames** so
    ///    `stable_motions`, spring physics, and the motion-bake cache
    ///    accumulate properly. Creating a fresh `RenderState` per
    ///    frame resets motion physics and discards the cache — every
    ///    frame becomes a cold start.
    /// 3. **Setting the viewport** via
    ///    `state.set_viewport(0., 0., w, h)` once before the first
    ///    frame so viewport culling matches the output size.
    ///
    /// Composes into the headless video-export loop:
    ///
    /// ```ignore
    /// let mut state = RenderState::new(scheduler);
    /// state.set_viewport(Rect::new(0., 0., w as f32, h as f32));
    /// let dt_ms = 1000.0 / fps;
    /// for f in 0..total_frames {
    ///     scheduler.lock().unwrap().tick(dt_ms);
    ///     let tree = build_tree_for_frame(f);
    ///     let frame = app.render_to_texture_with_motion(&tree, &mut state, w, h)?;
    ///     encoder.push_frame(f, &frame);
    /// }
    /// ```
    ///
    /// `tree` is taken `&mut` because `render_tree_with_motion`
    /// internally mutates lazy state on the tree (motion-derived
    /// caches, primitive batches). Pre-build via
    /// `RenderTree::from_element` so layout costs are paid once per
    /// frame rather than inside the renderer.
    pub fn render_to_texture_with_motion(
        &mut self,
        tree: &RenderTree,
        render_state: &RenderState,
        width: u32,
        height: u32,
    ) -> Result<wgpu::Texture> {
        let texture = self.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("blinc_app::render_to_texture_with_motion"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture_format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // `try_fast_paint = false` — the fast path is only correct
        // when a previous full paint primed the cache against the
        // SAME surface texture. Offscreen export always renders into
        // a fresh texture, so the cache is stale by definition. The
        // full walker path is the right (and only correct) call here.
        self.ctx.render_tree_with_motion_opt(
            tree,
            render_state,
            width,
            height,
            &view,
            None,
            false,
        )?;
        Ok(texture)
    }

    /// Motion-aware companion to [`Self::render_to_rgba8`]. See
    /// [`Self::render_to_texture_with_motion`] for the caller-managed
    /// `RenderState` / scheduler-ticking contract.
    pub fn render_to_rgba8_with_motion(
        &mut self,
        tree: &RenderTree,
        render_state: &RenderState,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        let texture = self.render_to_texture_with_motion(tree, render_state, width, height)?;
        self.read_texture_to_rgba8(&texture, width, height)
    }

    /// Copy `texture` back to a tightly-packed `Vec<u8>` of
    /// `width * height * 4` RGBA bytes. Used by both
    /// [`Self::render_to_rgba8`] and [`Self::render_to_rgba8_with_motion`];
    /// also useful on its own when callers built their own offscreen
    /// `wgpu::Texture` (e.g. via `render_to_texture` followed by a
    /// custom `run_gpu_pass`) and just want pixels at the end.
    ///
    /// Handles the COPY_BYTES_PER_ROW_ALIGNMENT padding-strip and the
    /// `BGRA8` → `RGBA8` swizzle so callers see no stride and a
    /// consistent component order regardless of the renderer's
    /// internal format.
    pub fn read_texture_to_rgba8(
        &self,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        // wgpu requires `bytes_per_row` aligned to
        // `COPY_BYTES_PER_ROW_ALIGNMENT` (256 on most adapters). The
        // unpadded payload is `width * 4`; pad up, copy, then strip
        // the padding row-by-row on the way out.
        let unpadded_bpr = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        #[allow(clippy::manual_div_ceil)]
        let padded_bpr = unpadded_bpr.div_ceil(align) * align;
        let readback_size = (padded_bpr as u64) * (height as u64);
        let readback = self.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("blinc_app::read_texture_to_rgba8 readback"),
            size: readback_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blinc_app::read_texture_to_rgba8 copy"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bpr),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue().submit(std::iter::once(encoder.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = self.device().poll(wgpu::PollType::Wait);
        rx.recv()
            .map_err(|_| BlincError::Other("readback channel dropped".to_string()))?
            .map_err(|e| BlincError::Other(format!("readback map failed: {e:?}")))?;

        let data = slice.get_mapped_range();
        let swizzle_bgra = matches!(
            self.texture_format(),
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );

        let mut out = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for y in 0..height as usize {
            let row_start = y * padded_bpr as usize;
            let row = &data[row_start..row_start + unpadded_bpr as usize];
            if swizzle_bgra {
                for chunk in row.as_chunks::<4>().0 {
                    out.push(chunk[2]);
                    out.push(chunk[1]);
                    out.push(chunk[0]);
                    out.push(chunk[3]);
                }
            } else {
                out.extend_from_slice(row);
            }
        }
        drop(data);
        readback.unmap();
        Ok(out)
    }

    /// Render a pre-computed render tree
    ///
    /// Use this when you want to compute layout once and render multiple times.
    pub fn render_tree(
        &mut self,
        tree: &RenderTree,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.ctx.render_tree(tree, width, height, target)
    }

    /// Render a pre-computed render tree with dynamic render state
    ///
    /// This method renders the stable tree structure and overlays any dynamic
    /// elements from RenderState (cursor, selections, animated properties).
    ///
    /// The tree structure is only rebuilt when elements are added/removed.
    /// The RenderState is updated every frame for animations and cursor blink.
    pub fn render_tree_with_state(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.ctx
            .render_tree_with_state(tree, render_state, width, height, target)
    }

    /// Render a pre-computed render tree with motion animations
    ///
    /// This method renders elements with enter/exit animations applied:
    /// - opacity fading
    /// - scale transformations
    /// - translation animations
    ///
    /// Use this when you have elements wrapped in motion() containers.
    pub fn render_tree_with_motion(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.ctx
            .render_tree_with_motion(tree, render_state, width, height, target)
    }

    /// Render with motion, with optional compositor fast-path.
    /// Forwards to [`RenderContext::render_tree_with_motion_opt`].
    /// `try_fast_paint=true` lets the renderer skip the paint walker
    /// when only motion bindings changed this frame and the cached
    /// `PrimitiveBatch` from the last full paint can be patched in
    /// place.
    #[allow(clippy::too_many_arguments)]
    pub fn render_tree_with_motion_opt(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        target: &wgpu::TextureView,
        target_texture: Option<&wgpu::Texture>,
        width: u32,
        height: u32,
        try_fast_paint: bool,
    ) -> Result<()> {
        self.ctx.render_tree_with_motion_opt(
            tree,
            render_state,
            width,
            height,
            target,
            target_texture,
            try_fast_paint,
        )
    }

    /// Whether the renderer has a usable cached `PrimitiveBatch` from
    /// the most recent full paint. Phase-4 fast-path gate.
    pub fn has_render_cache(&self) -> bool {
        self.ctx.has_render_cache()
    }

    /// Drop the cached `PrimitiveBatch`. Forces the next paint to
    /// take the full walker path and repopulate the cache.
    pub fn invalidate_render_cache(&mut self) {
        self.ctx.invalidate_render_cache();
    }

    pub fn invalidate_render_cache_tagged(&mut self, source: &'static str) {
        self.ctx.invalidate_render_cache_tagged(source);
    }

    /// Drop cached `LayerTexture`s for newly-mounted composite-promoted
    /// subtrees so the next paint re-bakes against fresh GPU `aux_data`.
    /// See [`crate::context::RenderContext::invalidate_overlay_mount_textures`]
    /// for the rationale + the cn-context-menu invisible-on-first-mount
    /// bug it patches.
    pub fn invalidate_overlay_mount_textures(&mut self, keys: &[u64]) {
        self.ctx.invalidate_overlay_mount_textures(keys);
    }

    /// Drain every cached composite / motion-subtree layer texture.
    /// See [`crate::context::RenderContext::drain_all_layer_textures`]
    /// for the rationale — use only when EVERY texture is provably
    /// stale (viewport pan/zoom on a canvas-bearing page,
    /// post-resize), not on routine state changes.
    pub fn drain_all_layer_textures(&mut self, source: &'static str) {
        self.ctx.drain_all_layer_textures(source);
    }

    /// Set the alpha used when clearing the main render target.
    ///
    /// The desktop runner calls this before each window's render so that
    /// transparent windows (whose wgpu surface is configured with a
    /// premultiplied/postmultiplied alpha mode) get a fully clear
    /// surface. Default is `1.0` (opaque).
    pub fn set_clear_alpha(&mut self, alpha: f32) {
        self.ctx.set_clear_alpha(alpha);
    }

    /// Render an overlay tree on top of existing content (no clear)
    ///
    /// This is used for rendering modal/dialog/toast overlays on top of the main UI.
    /// Unlike `render_tree_with_motion`, this method does NOT clear the render target,
    /// preserving whatever was rendered before.
    pub fn render_overlay_tree_with_motion(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.ctx
            .render_overlay_tree_with_motion(tree, render_state, width, height, target)
    }

    /// Get the render context for advanced usage
    pub fn context(&mut self) -> &mut RenderContext {
        &mut self.ctx
    }

    /// Re-run generic font family preloading (sans-serif, monospace)
    /// so the family→weight mapping binds to any fonts loaded since
    /// the last call. On web, fonts are loaded after `with_canvas`
    /// creates the text context, so the initial `preload_generic_styles`
    /// runs against an empty registry. This method re-runs the same
    /// calls so `.monospace()` / `.serif()` / `.sans_serif()` resolve
    /// to the correct loaded font bytes.
    pub fn refresh_generic_font_styles(&mut self) {
        // Clear cached negative lookups from before fonts were loaded.
        // Without this, the first preload attempt (which ran against an
        // empty registry in `with_canvas`) caches "not found" for every
        // generic family+weight combo, and subsequent preload calls hit
        // the cache and return the stale "not found" without retrying.
        self.ctx.text_ctx.invalidate_generic_font_cache();

        self.ctx.text_ctx.preload_generic_styles(
            blinc_gpu::GenericFont::SansSerif,
            &[100, 200, 300, 400, 500, 600, 700, 800, 900],
            false,
        );
        self.ctx.text_ctx.preload_generic_styles(
            blinc_gpu::GenericFont::SansSerif,
            &[100, 200, 300, 400, 500, 600, 700, 800, 900],
            true,
        );
        self.ctx.text_ctx.preload_generic_styles(
            blinc_gpu::GenericFont::Monospace,
            &[400, 700],
            false,
        );
    }

    /// Update the current cursor position in physical pixels (for @flow pointer input)
    pub fn set_cursor_position(&mut self, x: f32, y: f32) {
        self.ctx.set_cursor_position(x, y);
    }

    /// Set the current render target texture for blend mode two-pass compositing.
    pub fn set_blend_target(&mut self, texture: &wgpu::Texture) {
        self.ctx.set_blend_target(texture);
    }

    /// Clear the blend target texture reference after rendering.
    pub fn clear_blend_target(&mut self) {
        self.ctx.clear_blend_target();
    }

    /// Whether the last render frame asked for another one: a captured skeleton is animating.
    pub fn has_active_flows(&self) -> bool {
        self.ctx.has_active_flows()
    }

    /// Get the configuration
    pub fn config(&self) -> &BlincConfig {
        &self.config
    }

    /// Get the wgpu device
    pub fn device(&self) -> &Arc<wgpu::Device> {
        self.ctx.device()
    }

    /// Get the wgpu queue
    pub fn queue(&self) -> &Arc<wgpu::Queue> {
        self.ctx.queue()
    }

    /// Whether the GPU adapter supports storage buffers.
    pub fn has_storage_buffers(&self) -> bool {
        self.ctx.has_storage_buffers()
    }

    /// Create a new wgpu surface for an additional window.
    ///
    /// Uses the existing GPU instance to create a surface that shares
    /// the device and queue with the primary renderer. For multi-window support.
    pub fn create_surface_for_window<W>(
        &self,
        window: std::sync::Arc<W>,
    ) -> std::result::Result<wgpu::Surface<'static>, blinc_gpu::RendererError>
    where
        W: raw_window_handle::HasWindowHandle
            + raw_window_handle::HasDisplayHandle
            + Send
            + Sync
            + 'static,
    {
        self.ctx.create_surface(window)
    }

    /// Create a new wgpu surface from platform raw handles.
    ///
    /// Android uses this to attach a freshly-created `ANativeWindow` to the
    /// existing renderer/device after the app returns from the background.
    ///
    /// # Safety
    ///
    /// The caller must uphold [`wgpu::Instance::create_surface_unsafe`]'s
    /// handle validity and lifetime requirements.
    pub unsafe fn create_surface_unsafe(
        &self,
        target: wgpu::SurfaceTargetUnsafe,
    ) -> std::result::Result<wgpu::Surface<'static>, blinc_gpu::RendererError> {
        unsafe { self.ctx.create_surface_unsafe(target) }
    }

    /// Get the texture format used by the renderer's pipelines
    ///
    /// This should match the format used for the surface configuration
    /// to avoid format mismatches.
    pub fn texture_format(&self) -> wgpu::TextureFormat {
        self.ctx.texture_format()
    }

    /// The adapter the renderer was initialized against. Needed for
    /// `Surface::get_capabilities` to negotiate format / alpha mode /
    /// present mode against what the OS compositor actually exposes.
    pub fn adapter(&self) -> &wgpu::Adapter {
        self.ctx.adapter()
    }

    /// Get the shared font registry
    ///
    /// This can be used to share fonts between text measurement and rendering,
    /// ensuring consistent font loading and metrics.
    pub fn font_registry(&self) -> Arc<Mutex<FontRegistry>> {
        self.ctx.font_registry()
    }

    /// Load font data into the text rendering registry
    ///
    /// This adds fonts that will be available for text rendering.
    /// Returns the number of font faces loaded.
    ///
    /// Use this to load bundled fonts on platforms (like iOS) where
    /// system fonts aren't directly accessible via file paths.
    pub fn load_font_data_to_registry(&mut self, data: Vec<u8>) -> usize {
        self.ctx.load_font_data_to_registry(data)
    }

    /// Create a new Blinc application with a window surface
    ///
    /// This creates a GPU renderer optimized for the given window and returns
    /// both the application and the wgpu surface for rendering.
    ///
    /// # Arguments
    ///
    /// * `window` - The window to create a surface for (must implement raw-window-handle traits)
    /// * `config` - Optional configuration (uses defaults if None)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (app, surface) = BlincApp::with_window(window_arc, None)?;
    /// ```
    #[cfg(feature = "windowed")]
    pub fn with_window<W>(
        window: Arc<W>,
        config: Option<BlincConfig>,
    ) -> Result<(Self, wgpu::Surface<'static>)>
    where
        W: raw_window_handle::HasWindowHandle
            + raw_window_handle::HasDisplayHandle
            + Send
            + Sync
            + 'static,
    {
        let config = config.unwrap_or_default();

        let renderer_config = RendererConfig {
            max_primitives: config.max_primitives,
            max_glass_primitives: config.max_glass_primitives,
            max_glyphs: config.max_glyphs,
            sample_count: 1,
            texture_format: None,
            unified_text_rendering: true,
            ..RendererConfig::default()
        };

        let (renderer, surface) =
            pollster::block_on(GpuRenderer::with_surface(window, renderer_config))
                .map_err(|e| BlincError::GpuInit(e.to_string()))?;

        let device = renderer.device_arc();
        let queue = renderer.queue_arc();

        let mut text_ctx = TextRenderingContext::new(device.clone(), queue.clone());

        // Load system default font
        for font_path in crate::system_font_paths() {
            let path = std::path::Path::new(font_path);
            if path.exists() {
                if let Ok(data) = std::fs::read(path) {
                    let _ = text_ctx.load_font_data(data);
                    break;
                }
            }
        }

        // Preload common fonts that apps might use
        // This ensures fonts are cached before render time
        text_ctx.preload_fonts(&[
            "Inter",
            "Fira Code",
            "Menlo",
            "SF Mono",
            "SF Pro",
            "Roboto",
            "Consolas",
            "Monaco",
            "Source Code Pro",
            "JetBrains Mono",
        ]);

        // Preload generic font weights (for system fallback fonts)
        text_ctx.preload_generic_styles(
            blinc_gpu::GenericFont::SansSerif,
            &[400, 600, 700, 800, 900],
            false,
        );
        text_ctx.preload_generic_styles(
            blinc_gpu::GenericFont::SansSerif,
            &[400, 600, 700, 800, 900],
            true,
        );
        text_ctx.preload_generic_styles(blinc_gpu::GenericFont::Monospace, &[400, 700], false);

        let ctx = RenderContext::new(renderer, text_ctx, device, queue, config.sample_count);
        let app = Self { ctx, config };

        Ok((app, surface))
    }

    /// Async sibling of [`Self::with_window`] for the web target.
    ///
    /// `with_window` blocks on `pollster::block_on(GpuRenderer::with_surface(...))`,
    /// which cannot run on the browser main thread — wasm-bindgen-futures
    /// requires the entire async chain to be `await`ed back into the JS
    /// event loop. This sibling preserves the same shape but is `async`
    /// the whole way through, calling [`GpuRenderer::with_canvas`] from
    /// `blinc_gpu` instead of `with_surface`.
    ///
    /// **No system-font loading.** On the web there's no filesystem to
    /// scan for `.ttf` paths. Apps must call
    /// [`Self::load_font_data_to_registry`] explicitly with bundled or
    /// fetched font bytes after `with_canvas` returns. The Phase 6
    /// rollout adds an async preload helper in `blinc_platform_web`.
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    pub async fn with_canvas(
        canvas: web_sys::HtmlCanvasElement,
        config: Option<BlincConfig>,
    ) -> Result<(Self, wgpu::Surface<'static>)> {
        Self::with_web_canvas(Some(canvas), None, config).await
    }

    /// Create the web renderer inside a dedicated worker.
    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    pub async fn with_offscreen_canvas(
        canvas: web_sys::OffscreenCanvas,
        config: Option<BlincConfig>,
    ) -> Result<(Self, wgpu::Surface<'static>)> {
        Self::with_web_canvas(None, Some(canvas), config).await
    }

    #[cfg(all(feature = "web", target_arch = "wasm32"))]
    async fn with_web_canvas(
        canvas: Option<web_sys::HtmlCanvasElement>,
        offscreen: Option<web_sys::OffscreenCanvas>,
        config: Option<BlincConfig>,
    ) -> Result<(Self, wgpu::Surface<'static>)> {
        let config = config.unwrap_or_default();

        let renderer_config = RendererConfig {
            max_primitives: config.max_primitives,
            max_glass_primitives: config.max_glass_primitives,
            max_glyphs: config.max_glyphs,
            sample_count: 1,
            texture_format: None,
            unified_text_rendering: true,
            ..RendererConfig::default()
        };

        let result = match canvas {
            Some(canvas) => GpuRenderer::with_canvas(canvas, renderer_config).await,
            None => {
                GpuRenderer::with_offscreen_canvas(
                    offscreen.expect("web canvas supplied"),
                    renderer_config,
                )
                .await
            }
        };
        let (renderer, surface) = result.map_err(|e| BlincError::GpuInit(e.to_string()))?;

        let device = renderer.device_arc();
        let queue = renderer.queue_arc();

        let mut text_ctx = TextRenderingContext::new(device.clone(), queue.clone());

        // Generic-font preload mirrors the desktop path so common
        // weights are cached before the first frame. The actual font
        // bytes have to be supplied separately by the caller via
        // `load_font_data()` — these calls only register the
        // generic-family → weight mappings so the shaper knows
        // which families to resolve `font-family: sans-serif` /
        // `monospace` to. Without them, CSS `font-family: monospace`
        // falls back to the first registered font (usually Arial)
        // instead of the intended monospace font.
        text_ctx.preload_generic_styles(
            blinc_gpu::GenericFont::SansSerif,
            &[100, 200, 300, 400, 500, 600, 700, 800, 900],
            false,
        );
        text_ctx.preload_generic_styles(
            blinc_gpu::GenericFont::SansSerif,
            &[100, 200, 300, 400, 500, 600, 700, 800, 900],
            true,
        );
        text_ctx.preload_generic_styles(blinc_gpu::GenericFont::Monospace, &[400, 700], false);

        let ctx = RenderContext::new(renderer, text_ctx, device, queue, config.sample_count);
        let app = Self { ctx, config };

        Ok((app, surface))
    }
}
