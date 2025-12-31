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
            sample_count: 4,
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
        };

        let renderer = pollster::block_on(GpuRenderer::new(renderer_config))
            .map_err(|e| BlincError::GpuInit(e.to_string()))?;

        let device = renderer.device_arc();
        let queue = renderer.queue_arc();

        let mut text_ctx = TextRenderingContext::new(device.clone(), queue.clone());

        // Try to load a default system font
        #[cfg(target_os = "macos")]
        {
            let font_path = std::path::Path::new("/System/Library/Fonts/Helvetica.ttc");
            if font_path.exists() {
                if let Ok(data) = std::fs::read(font_path) {
                    let _ = text_ctx.load_font_data(data);
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            let font_paths = [
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
            ];
            for path in &font_paths {
                if let Ok(data) = std::fs::read(path) {
                    let _ = text_ctx.load_font_data(data);
                    break;
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            let font_path = "C:\\Windows\\Fonts\\segoeui.ttf";
            if let Ok(data) = std::fs::read(font_path) {
                let _ = text_ctx.load_font_data(data);
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
        text_ctx.preload_generic_styles(blinc_gpu::GenericFont::SansSerif, &[400, 700], false);
        text_ctx.preload_generic_styles(blinc_gpu::GenericFont::SansSerif, &[400, 700], true);
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

    /// Get the texture format used by the renderer's pipelines
    ///
    /// This should match the format used for the surface configuration
    /// to avoid format mismatches.
    pub fn texture_format(&self) -> wgpu::TextureFormat {
        self.ctx.texture_format()
    }

    /// Get the shared font registry
    ///
    /// This can be used to share fonts between text measurement and rendering,
    /// ensuring consistent font loading and metrics.
    pub fn font_registry(&self) -> Arc<Mutex<FontRegistry>> {
        self.ctx.font_registry()
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
        };

        let (renderer, surface) =
            pollster::block_on(GpuRenderer::with_surface(window, renderer_config))
                .map_err(|e| BlincError::GpuInit(e.to_string()))?;

        let device = renderer.device_arc();
        let queue = renderer.queue_arc();

        let mut text_ctx = TextRenderingContext::new(device.clone(), queue.clone());

        // Try to load a default system font
        #[cfg(target_os = "macos")]
        {
            let font_path = std::path::Path::new("/System/Library/Fonts/Helvetica.ttc");
            if font_path.exists() {
                if let Ok(data) = std::fs::read(font_path) {
                    let _ = text_ctx.load_font_data(data);
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            let font_paths = [
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
            ];
            for path in &font_paths {
                if let Ok(data) = std::fs::read(path) {
                    let _ = text_ctx.load_font_data(data);
                    break;
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            let font_path = "C:\\Windows\\Fonts\\segoeui.ttf";
            if let Ok(data) = std::fs::read(font_path) {
                let _ = text_ctx.load_font_data(data);
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
        text_ctx.preload_generic_styles(blinc_gpu::GenericFont::SansSerif, &[400, 700], false);
        text_ctx.preload_generic_styles(blinc_gpu::GenericFont::SansSerif, &[400, 700], true);
        text_ctx.preload_generic_styles(blinc_gpu::GenericFont::Monospace, &[400, 700], false);

        let ctx = RenderContext::new(renderer, text_ctx, device, queue, config.sample_count);
        let app = Self { ctx, config };

        Ok((app, surface))
    }
}
