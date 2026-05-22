//! Blinc Application Delegate
//!
//! The main entry point for Blinc applications.

use blinc_gpu::{FontRegistry, GpuRenderer, RendererConfig, TextRenderingContext};
use blinc_layout::prelude::*;
use blinc_layout::RenderTree;
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
            max_primitives: 10_000,
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

    /// Update the global mouse-pressed state (for `@flow` `builtin(pressure)`).
    pub fn set_cursor_pressed(&mut self, pressed: bool) {
        self.ctx.set_cursor_pressed(pressed);
    }

    /// Set the current render target texture for blend mode two-pass compositing.
    pub fn set_blend_target(&mut self, texture: &wgpu::Texture) {
        self.ctx.set_blend_target(texture);
    }

    /// Clear the blend target texture reference after rendering.
    pub fn clear_blend_target(&mut self) {
        self.ctx.clear_blend_target();
    }

    /// Whether the last render frame contained @flow shader elements.
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

        let (renderer, surface) = GpuRenderer::with_canvas(canvas, renderer_config)
            .await
            .map_err(|e| BlincError::GpuInit(e.to_string()))?;

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
