//! Render context for blinc_app
//!
//! Wraps the GPU rendering pipeline with a clean API.

use blinc_core::{
    Brush, Color, CornerRadius, DrawCommand, DrawContext, DrawContextExt, Rect, Stroke,
};
use blinc_gpu::{
    FontRegistry, GenericFont as GpuGenericFont, GpuGlyph, GpuImage, GpuImageInstance,
    GpuPaintContext, GpuPrimitive, GpuRenderer, ImageRenderingContext, PrimitiveBatch,
    TextAlignment, TextAnchor, TextRenderingContext,
};
use blinc_layout::div::{FontFamily, FontWeight, GenericFont, TextAlign, TextVerticalAlign};
use blinc_layout::prelude::*;
use blinc_layout::render_state::Overlay;
use blinc_layout::renderer::ElementType;
use blinc_svg::{RasterizedSvg, SvgDocument};
use lru::LruCache;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use crate::error::Result;

/// Maximum number of images to keep in cache (prevents unbounded memory growth)
const IMAGE_CACHE_CAPACITY: usize = 128;

/// Maximum number of parsed SVG documents to cache
const SVG_CACHE_CAPACITY: usize = 64;

/// Maximum number of rasterized SVG textures to cache
/// Key is (svg_hash, width, height, tint_hash) - separate textures for different sizes/tints
const RASTERIZED_SVG_CACHE_CAPACITY: usize = 64;

/// Internal render context that manages GPU resources and rendering
pub struct RenderContext {
    renderer: GpuRenderer,
    text_ctx: TextRenderingContext,
    image_ctx: ImageRenderingContext,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    sample_count: u32,
    // Single texture for glass backdrop (rendered to and sampled from)
    backdrop_texture: Option<CachedTexture>,
    // Cached MSAA texture for anti-aliased rendering
    msaa_texture: Option<CachedTexture>,
    // LRU cache for images (prevents unbounded memory growth)
    image_cache: LruCache<String, GpuImage>,
    // LRU cache for parsed SVG documents (avoids re-parsing)
    svg_cache: LruCache<u64, SvgDocument>,
    // LRU cache for rasterized SVG textures (CPU-rasterized with proper AA)
    rasterized_svg_cache: LruCache<u64, GpuImage>,
    // Scratch buffers for per-frame allocations (reused to avoid allocations)
    scratch_glyphs: Vec<GpuGlyph>,
    scratch_texts: Vec<TextElement>,
    scratch_svgs: Vec<SvgElement>,
    scratch_images: Vec<ImageElement>,
}

struct CachedTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

/// Text element data for rendering
struct TextElement {
    content: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    font_size: f32,
    color: [f32; 4],
    align: TextAlign,
    weight: FontWeight,
    /// Whether to use italic style
    italic: bool,
    /// Vertical alignment within bounding box
    v_align: TextVerticalAlign,
    /// Clip bounds from parent scroll container (x, y, width, height)
    clip_bounds: Option<[f32; 4]>,
    /// Motion opacity inherited from parent motion container
    motion_opacity: f32,
    /// Whether to wrap text at container bounds
    wrap: bool,
    /// Line height multiplier
    line_height: f32,
    /// Measured width (before layout constraints) - used to determine if wrap is needed
    measured_width: f32,
    /// Font family category
    font_family: FontFamily,
    /// Word spacing in pixels (0.0 = normal)
    word_spacing: f32,
    /// Z-index for rendering order (higher = on top)
    z_index: u32,
    /// Font ascender in pixels (distance from baseline to top)
    ascender: f32,
    /// Whether text has strikethrough decoration
    strikethrough: bool,
    /// Whether text has underline decoration
    underline: bool,
}

/// Image element data for rendering
struct ImageElement {
    source: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    object_fit: u8,
    object_position: [f32; 2],
    opacity: f32,
    border_radius: f32,
    tint: [f32; 4],
    /// Clip bounds from parent (x, y, width, height)
    clip_bounds: Option<[f32; 4]>,
    /// Clip corner radii (tl, tr, br, bl)
    clip_radius: [f32; 4],
    /// Which layer this image renders in
    layer: RenderLayer,
    /// Loading strategy: 0 = Eager (load immediately), 1 = Lazy (load when visible)
    loading_strategy: u8,
    /// Placeholder type: 0 = None, 1 = Color, 2 = Image, 3 = Skeleton
    placeholder_type: u8,
    /// Placeholder color [r, g, b, a]
    placeholder_color: [f32; 4],
    /// Z-layer index for interleaved rendering with primitives
    z_index: u32,
}

/// SVG element data for rendering
struct SvgElement {
    source: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// Tint color to apply to SVG fill/stroke
    tint: Option<blinc_core::Color>,
    /// Clip bounds from parent scroll container (x, y, width, height)
    clip_bounds: Option<[f32; 4]>,
    /// Motion opacity inherited from parent motion container
    motion_opacity: f32,
}

/// Debug bounds element for layout visualization
#[derive(Clone)]
struct DebugBoundsElement {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// Element type name for labeling
    element_type: String,
    /// Depth in the tree (for color coding)
    depth: u32,
}

impl RenderContext {
    /// Create a new render context
    pub(crate) fn new(
        renderer: GpuRenderer,
        text_ctx: TextRenderingContext,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        sample_count: u32,
    ) -> Self {
        let image_ctx = ImageRenderingContext::new(device.clone(), queue.clone());
        Self {
            renderer,
            text_ctx,
            image_ctx,
            device,
            queue,
            sample_count,
            backdrop_texture: None,
            msaa_texture: None,
            image_cache: LruCache::new(NonZeroUsize::new(IMAGE_CACHE_CAPACITY).unwrap()),
            svg_cache: LruCache::new(NonZeroUsize::new(SVG_CACHE_CAPACITY).unwrap()),
            rasterized_svg_cache: LruCache::new(
                NonZeroUsize::new(RASTERIZED_SVG_CACHE_CAPACITY).unwrap(),
            ),
            scratch_glyphs: Vec::with_capacity(1024), // Pre-allocate for typical text
            scratch_texts: Vec::with_capacity(64),    // Pre-allocate for text elements
            scratch_svgs: Vec::with_capacity(32),     // Pre-allocate for SVG elements
            scratch_images: Vec::with_capacity(32),   // Pre-allocate for image elements
        }
    }

    /// Render a layout tree to a texture view
    ///
    /// Handles everything automatically - glass, text, SVG, MSAA.
    pub fn render_tree(
        &mut self,
        tree: &RenderTree,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        // Get scale factor for HiDPI rendering
        let scale_factor = tree.scale_factor();

        // Create paint contexts for each layer with text rendering support
        let mut bg_ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);

        // Render layout layers (background and glass go to bg_ctx)
        tree.render_to_layer(&mut bg_ctx, RenderLayer::Background);
        tree.render_to_layer(&mut bg_ctx, RenderLayer::Glass);

        // Take the batch from bg_ctx before we can reuse text_ctx for fg_ctx
        let bg_batch = bg_ctx.take_batch();

        // Create foreground context with text rendering support
        let mut fg_ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);
        tree.render_to_layer(&mut fg_ctx, RenderLayer::Foreground);

        // Take the batch from fg_ctx before reusing text_ctx for text elements
        let mut fg_batch = fg_ctx.take_batch();

        // Collect text, SVG, and image elements
        let (texts, svgs, images) = self.collect_render_elements(tree);

        // Pre-load all images into cache before rendering
        self.preload_images(&images, width as f32, height as f32);

        // Prepare text glyphs
        let mut all_glyphs = Vec::new();
        for text in &texts {
            // Convert layout TextAlign to GPU TextAlignment
            let alignment = match text.align {
                TextAlign::Left => TextAlignment::Left,
                TextAlign::Center => TextAlignment::Center,
                TextAlign::Right => TextAlignment::Right,
            };

            // Vertical alignment:
            // - Center: Use TextAnchor::Center with y at vertical center of bounds.
            //   This ensures text appears visually centered (by cap-height) rather than
            //   mathematically centered by the full bounding box (which includes descenders).
            // - Top: Text is centered within its layout box (items_center works).
            // - Baseline: Position text so baseline aligns at the font's actual baseline.
            //   Using the actual ascender from font metrics ensures all fonts align by
            //   their true baseline regardless of font family.
            let (anchor, y_pos, use_layout_height) = match text.v_align {
                TextVerticalAlign::Center => {
                    (TextAnchor::Center, text.y + text.height / 2.0, false)
                }
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    // Use the actual font ascender for baseline positioning.
                    // This ensures each font aligns by its true baseline.
                    let baseline_y = text.y + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };

            // Determine wrap width: use clip bounds if available (parent constraint),
            // otherwise use the text element's own layout width
            let wrap_width = if text.wrap {
                if let Some(clip) = text.clip_bounds {
                    // clip[2] is the clip width - use it if smaller than text width
                    clip[2].min(text.width)
                } else {
                    text.width
                }
            } else {
                text.width
            };

            // Convert font family to GPU types
            let font_name = text.font_family.name.as_deref();
            let generic = to_gpu_generic_font(text.font_family.generic);
            let font_weight = text.weight.weight();

            // Only pass layout_height when we want centering within the box
            let layout_height = if use_layout_height {
                Some(text.height)
            } else {
                None
            };

            if let Ok(mut glyphs) = self.text_ctx.prepare_text_with_style(
                &text.content,
                text.x,
                y_pos,
                text.font_size,
                text.color,
                anchor,
                alignment,
                Some(wrap_width),
                text.wrap,
                font_name,
                generic,
                font_weight,
                text.italic,
                layout_height,
            ) {
                // Apply clip bounds to all glyphs if the text element has clip bounds
                if let Some(clip) = text.clip_bounds {
                    for glyph in &mut glyphs {
                        glyph.clip_bounds = clip;
                    }
                }
                all_glyphs.extend(glyphs);
            }
        }

        // SVGs are rendered as rasterized images (not tessellated paths) for better anti-aliasing
        // They will be rendered later via render_rasterized_svgs

        self.renderer.resize(width, height);

        let has_glass = bg_batch.glass_count() > 0;

        // Only allocate glass textures when glass is actually used
        if has_glass {
            self.ensure_glass_textures(width, height);
        }
        let use_msaa_overlay = self.sample_count > 1;

        // Background layer uses SDF rendering (shader-based AA, no MSAA needed)
        // Foreground layer (SVGs as tessellated paths) uses MSAA for smooth edges

        if has_glass {
            // Split images by layer: background images go behind glass (get blurred),
            // glass/foreground images render on top of glass (not blurred)
            let (bg_images, fg_images): (Vec<_>, Vec<_>) = images
                .iter()
                .partition(|img| img.layer == RenderLayer::Background);

            // Glass path - batched rendering for reduced command buffer overhead:
            // Steps 1-3 are batched into a single encoder submission
            {
                let backdrop = self.backdrop_texture.as_ref().unwrap();
                self.renderer.render_glass_frame(
                    target,
                    &backdrop.view,
                    (backdrop.width, backdrop.height),
                    &bg_batch,
                );
            }

            // Render background paths with MSAA for smooth edges on curved shapes like notch
            // (render_glass_frame uses 1x sampled path rendering, so we need MSAA overlay)
            if use_msaa_overlay && bg_batch.has_paths() {
                self.renderer
                    .render_paths_overlay_msaa(target, &bg_batch, self.sample_count);
            }

            // Step 4: Render background-layer images to target (separate for now - images use different pipeline)
            self.render_images_ref(target, &bg_images);

            // Step 5: Render glass/foreground-layer images (on top of glass, NOT blurred)
            self.render_images_ref(target, &fg_images);

            // Step 6: Render foreground and text
            // Use batch-based rendering when layer effects are present
            let has_layer_effects = fg_batch.has_layer_effects();
            if has_layer_effects {
                // Layer effects require batch-based rendering to process layer commands
                fg_batch.convert_glyphs_to_primitives();
                if !fg_batch.is_empty() {
                    self.renderer.render_overlay(target, &fg_batch);
                }
                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else if self.renderer.unified_text_rendering() {
                // Unified rendering: combine text glyphs with foreground primitives
                let unified_primitives = fg_batch.get_unified_foreground_primitives();
                if !unified_primitives.is_empty() {
                    self.render_unified(target, &unified_primitives);
                }

                // Render paths with MSAA for smooth edges (paths are not included in unified primitives)
                if use_msaa_overlay && fg_batch.has_paths() {
                    self.renderer
                        .render_paths_overlay_msaa(target, &fg_batch, self.sample_count);
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else {
                // Legacy rendering: separate foreground and text passes
                if !fg_batch.is_empty() {
                    if use_msaa_overlay {
                        self.renderer
                            .render_overlay_msaa(target, &fg_batch, self.sample_count);
                    } else {
                        self.renderer.render_overlay(target, &fg_batch);
                    }
                }

                // Step 7: Render text
                if !all_glyphs.is_empty() {
                    self.render_text(target, &all_glyphs);
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            }

            // Step 8: Render text decorations (strikethrough, underline)
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);
            for primitives in decorations_by_layer.values() {
                if !primitives.is_empty() {
                    self.render_unified(target, primitives);
                }
            }
        } else {
            // Simple path (no glass):
            // Background uses SDF rendering (no MSAA needed)
            // Foreground uses MSAA for smooth SVG edges

            // Render background directly to target
            // Use opaque black clear - transparent clear can cause issues with window surfaces
            self.renderer
                .render_with_clear(target, &bg_batch, [0.0, 0.0, 0.0, 1.0]);

            // Render background paths with MSAA for smooth edges on curved shapes like notch
            if use_msaa_overlay && bg_batch.has_paths() {
                self.renderer
                    .render_paths_overlay_msaa(target, &bg_batch, self.sample_count);
            }

            // Render images after background primitives
            self.render_images(target, &images, width as f32, height as f32);

            // Render foreground and text
            // Use batch-based rendering when layer effects are present to preserve
            // layer commands for effect processing
            let has_layer_effects = fg_batch.has_layer_effects();
            if has_layer_effects {
                // Layer effects require batch-based rendering to process layer commands
                // First convert glyphs to primitives so they're included in the batch
                fg_batch.convert_glyphs_to_primitives();

                // Use render_overlay which supports layer effect processing
                if !fg_batch.is_empty() {
                    self.renderer.render_overlay(target, &fg_batch);
                }
                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else if self.renderer.unified_text_rendering() {
                // Unified rendering: combine text glyphs with foreground primitives
                // This ensures text and shapes transform together during animations
                let unified_primitives = fg_batch.get_unified_foreground_primitives();
                if !unified_primitives.is_empty() {
                    self.render_unified(target, &unified_primitives);
                }

                // Render paths with MSAA for smooth edges (paths are not included in unified primitives)
                if use_msaa_overlay && fg_batch.has_paths() {
                    self.renderer
                        .render_paths_overlay_msaa(target, &fg_batch, self.sample_count);
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            } else {
                // Legacy rendering: separate foreground and text passes
                if !fg_batch.is_empty() {
                    if use_msaa_overlay {
                        self.renderer
                            .render_overlay_msaa(target, &fg_batch, self.sample_count);
                    } else {
                        self.renderer.render_overlay(target, &fg_batch);
                    }
                }

                // Render text
                if !all_glyphs.is_empty() {
                    self.render_text(target, &all_glyphs);
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }
            }

            // Render text decorations (strikethrough, underline)
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);
            for primitives in decorations_by_layer.values() {
                if !primitives.is_empty() {
                    self.render_unified(target, primitives);
                }
            }
        }

        // Return scratch buffers for reuse on next frame
        self.return_scratch_elements(texts, svgs, images);

        // Poll the device to free completed command buffers and prevent memory accumulation
        self.renderer.poll();

        Ok(())
    }

    /// Return element vectors to scratch pool for reuse
    #[inline]
    fn return_scratch_elements(
        &mut self,
        mut texts: Vec<TextElement>,
        mut svgs: Vec<SvgElement>,
        mut images: Vec<ImageElement>,
    ) {
        // Clear and keep capacity for reuse
        texts.clear();
        svgs.clear();
        images.clear();
        self.scratch_texts = texts;
        self.scratch_svgs = svgs;
        self.scratch_images = images;
    }

    /// Ensure glass-related textures exist and are the right size.
    /// Only called when glass elements are present in the scene.
    ///
    /// We use a single texture for both rendering and sampling (backdrop_texture).
    /// The texture is rendered at half resolution to save memory (blur doesn't need full res).
    fn ensure_glass_textures(&mut self, width: u32, height: u32) {
        // Use the same texture format as the renderer's pipelines
        let format = self.renderer.texture_format();

        // Use half resolution for glass backdrop - blur effect doesn't need full resolution
        // This saves 75% of texture memory (e.g., 2.5MB -> 0.6MB for 900x700 window)
        let backdrop_width = (width / 2).max(1);
        let backdrop_height = (height / 2).max(1);

        let needs_backdrop = self
            .backdrop_texture
            .as_ref()
            .map(|t| t.width != backdrop_width || t.height != backdrop_height)
            .unwrap_or(true);

        if needs_backdrop {
            // Single texture that can be both rendered to AND sampled from
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Glass Backdrop"),
                size: wgpu::Extent3d {
                    width: backdrop_width,
                    height: backdrop_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.backdrop_texture = Some(CachedTexture {
                texture,
                view,
                width: backdrop_width,
                height: backdrop_height,
            });
        }
    }

    /// Render text glyphs
    fn render_text(&mut self, target: &wgpu::TextureView, glyphs: &[GpuGlyph]) {
        if let (Some(atlas_view), Some(color_atlas_view)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            self.renderer.render_text(
                target,
                glyphs,
                atlas_view,
                color_atlas_view,
                self.text_ctx.sampler(),
            );
        }
    }

    /// Render SDF primitives and text glyphs in a unified pass
    ///
    /// This ensures text and shapes transform together during animations,
    /// preventing visual lag when parent containers have motion transforms.
    fn render_unified(&mut self, target: &wgpu::TextureView, primitives: &[GpuPrimitive]) {
        if primitives.is_empty() {
            return;
        }

        if let (Some(atlas_view), Some(color_atlas_view)) =
            (self.text_ctx.atlas_view(), self.text_ctx.color_atlas_view())
        {
            self.renderer.render_primitives_overlay_with_glyphs(
                target,
                primitives,
                atlas_view,
                color_atlas_view,
            );
        } else {
            // Fallback to regular rendering if no text atlases available
            self.renderer.render_primitives_overlay(target, primitives);
        }
    }

    /// Render text decorations for a specific z-layer
    fn render_text_decorations_for_layer(
        &mut self,
        target: &wgpu::TextureView,
        decorations_by_layer: &std::collections::HashMap<u32, Vec<GpuPrimitive>>,
        z_layer: u32,
    ) {
        if let Some(primitives) = decorations_by_layer.get(&z_layer) {
            if !primitives.is_empty() {
                self.renderer.render_primitives_overlay(target, primitives);
            }
        }
    }

    /// Render debug visualization overlays for text elements
    ///
    /// When `BLINC_DEBUG=text` (or `1`, `all`, `true`) is set, this renders:
    /// - Cyan: Text bounding box outline
    /// - Magenta: Baseline position
    /// - Green: Top of bounding box (ascender reference)
    /// - Yellow: Bottom of bounding box (descender reference)
    fn render_text_debug(&mut self, target: &wgpu::TextureView, texts: &[TextElement]) {
        let debug_primitives = generate_text_debug_primitives(texts);
        if !debug_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target, &debug_primitives);
        }
    }

    /// Render debug visualization overlays for all layout elements
    ///
    /// When `BLINC_DEBUG=layout` (or `all`) is set, this renders:
    /// - Semi-transparent colored rectangles for each element's bounding box
    /// - Colors cycle based on tree depth to distinguish nested elements
    fn render_layout_debug(&mut self, target: &wgpu::TextureView, tree: &RenderTree, scale: f32) {
        let debug_bounds = collect_debug_bounds(tree, scale);
        let debug_primitives = generate_layout_debug_primitives(&debug_bounds);
        if !debug_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target, &debug_primitives);
        }
    }

    /// Render debug visualization for motion/animations
    ///
    /// When `BLINC_DEBUG=motion` (or `all`) is set, this renders:
    /// - Top-right corner overlay showing animation stats
    /// - Number of active visual animations, layout animations, etc.
    fn render_motion_debug(
        &mut self,
        target: &wgpu::TextureView,
        tree: &RenderTree,
        width: u32,
        _height: u32,
    ) {
        let stats = tree.debug_stats();
        let mut debug_primitives = Vec::new();

        // Background for the debug panel
        let panel_width = 200.0;
        let panel_height = 100.0;
        let panel_x = width as f32 - panel_width - 10.0;
        let panel_y = 10.0;

        // Semi-transparent dark background
        debug_primitives.push(
            GpuPrimitive::rect(panel_x, panel_y, panel_width, panel_height)
                .with_color(0.1, 0.1, 0.15, 0.85)
                .with_corner_radius(6.0),
        );

        // Status indicator - green if any animations active
        let has_active = stats.visual_animation_count > 0
            || stats.layout_animation_count > 0
            || stats.animated_bounds_count > 0;

        let (r, g, b, a) = if has_active {
            (0.2, 0.9, 0.3, 1.0) // Green when animating
        } else {
            (0.4, 0.4, 0.5, 1.0) // Gray when idle
        };

        debug_primitives.push(
            GpuPrimitive::rect(panel_x + 10.0, panel_y + 12.0, 10.0, 10.0)
                .with_color(r, g, b, a)
                .with_corner_radius(5.0),
        );

        // Visual bars showing animation counts
        let bar_x = panel_x + 12.0;
        let bar_width = panel_width - 24.0;
        let bar_height = 6.0;

        // Visual animations bar (cyan)
        let visual_ratio = (stats.visual_animation_count as f32).min(10.0) / 10.0;
        if visual_ratio > 0.0 {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x, panel_y + 35.0, bar_width * visual_ratio, bar_height)
                    .with_color(0.0, 0.8, 0.9, 0.9)
                    .with_corner_radius(3.0),
            );
        }

        // Layout animations bar (magenta)
        let layout_ratio = (stats.layout_animation_count as f32).min(10.0) / 10.0;
        if layout_ratio > 0.0 {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x, panel_y + 50.0, bar_width * layout_ratio, bar_height)
                    .with_color(0.9, 0.2, 0.8, 0.9)
                    .with_corner_radius(3.0),
            );
        }

        // Animated bounds bar (yellow)
        let bounds_ratio = (stats.animated_bounds_count as f32).min(50.0) / 50.0;
        if bounds_ratio > 0.0 {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x, panel_y + 65.0, bar_width * bounds_ratio, bar_height)
                    .with_color(0.95, 0.85, 0.2, 0.9)
                    .with_corner_radius(3.0),
            );
        }

        // Scroll physics indicator (orange dots)
        let scroll_count = stats.scroll_physics_count.min(8);
        for i in 0..scroll_count {
            debug_primitives.push(
                GpuPrimitive::rect(bar_x + (i as f32 * 14.0), panel_y + 80.0, 8.0, 8.0)
                    .with_color(1.0, 0.6, 0.2, 0.9)
                    .with_corner_radius(4.0),
            );
        }

        if !debug_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target, &debug_primitives);
        }
    }

    /// Render images to the backdrop texture (for images that should be blurred by glass)
    fn render_images_to_backdrop(&mut self, images: &[&ImageElement]) {
        let Some(ref backdrop) = self.backdrop_texture else {
            return;
        };
        // Create a new view to avoid borrow conflicts
        let target = backdrop
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.render_images_ref(&target, images);
    }

    /// Pre-load images into cache (call before rendering)
    ///
    /// Images with lazy loading strategy are only loaded when visible in the viewport.
    /// A buffer zone extends the viewport to preload images that are about to become visible.
    fn preload_images(
        &mut self,
        images: &[ImageElement],
        viewport_width: f32,
        viewport_height: f32,
    ) {
        // Buffer zone: load images that are within 100px of becoming visible
        const VISIBILITY_BUFFER: f32 = 100.0;

        for image in images {
            // LruCache::contains also promotes to most-recently-used
            if self.image_cache.contains(&image.source) {
                continue;
            }

            // Check if lazy loading is enabled (loading_strategy == 1)
            if image.loading_strategy == 1 {
                // If image has clip bounds from a scroll container, use those for visibility check
                // The clip bounds represent the visible area of the parent scroll container
                let is_visible = if let Some([clip_x, clip_y, clip_w, clip_h]) = image.clip_bounds {
                    // Check if image intersects with its clip region (+ buffer for prefetching)
                    let clip_left = clip_x - VISIBILITY_BUFFER;
                    let clip_top = clip_y - VISIBILITY_BUFFER;
                    let clip_right = clip_x + clip_w + VISIBILITY_BUFFER;
                    let clip_bottom = clip_y + clip_h + VISIBILITY_BUFFER;

                    let image_right = image.x + image.width;
                    let image_bottom = image.y + image.height;

                    image.x < clip_right
                        && image_right > clip_left
                        && image.y < clip_bottom
                        && image_bottom > clip_top
                } else {
                    // No clip bounds - check against viewport
                    let viewport_left = -VISIBILITY_BUFFER;
                    let viewport_top = -VISIBILITY_BUFFER;
                    let viewport_right = viewport_width + VISIBILITY_BUFFER;
                    let viewport_bottom = viewport_height + VISIBILITY_BUFFER;

                    let image_right = image.x + image.width;
                    let image_bottom = image.y + image.height;

                    image.x < viewport_right
                        && image_right > viewport_left
                        && image.y < viewport_bottom
                        && image_bottom > viewport_top
                };

                if !is_visible {
                    // Skip loading - image is not yet visible
                    continue;
                }
            }

            // Try to load the image - use from_uri to handle emoji://, data:, and file paths
            let source = blinc_image::ImageSource::from_uri(&image.source);
            let image_data = match blinc_image::ImageData::load(source) {
                Ok(data) => data,
                Err(e) => {
                    tracing::debug!("Failed to load image '{}': {:?}", image.source, e);
                    continue; // Skip images that fail to load
                }
            };

            // Create GPU texture
            let gpu_image = self.image_ctx.create_image_labeled(
                image_data.pixels(),
                image_data.width(),
                image_data.height(),
                &image.source,
            );

            // LruCache::put evicts oldest entry if at capacity
            self.image_cache.put(image.source.clone(), gpu_image);
        }
    }

    /// Render images to target (images must be preloaded first)
    fn render_images(
        &mut self,
        target: &wgpu::TextureView,
        images: &[ImageElement],
        viewport_width: f32,
        viewport_height: f32,
    ) {
        use blinc_image::{calculate_fit_rects, src_rect_to_uv, ObjectFit, ObjectPosition};

        for image in images {
            // Get cached GPU image
            let gpu_image = self.image_cache.get(&image.source);

            // If image is not loaded and has a placeholder, render placeholder
            if gpu_image.is_none() && image.placeholder_type != 0 {
                // Placeholder type 1 = Color
                if image.placeholder_type == 1 {
                    // Render a solid color rectangle as placeholder
                    let color = blinc_core::Color::rgba(
                        image.placeholder_color[0],
                        image.placeholder_color[1],
                        image.placeholder_color[2],
                        image.placeholder_color[3],
                    );

                    // Create a simple rectangle for the placeholder
                    let mut ctx = GpuPaintContext::new(viewport_width, viewport_height);

                    let rect = blinc_core::Rect::new(image.x, image.y, image.width, image.height);

                    ctx.fill_rounded_rect(
                        rect,
                        blinc_core::CornerRadius::uniform(image.border_radius),
                        color,
                    );

                    let batch = ctx.take_batch();
                    self.renderer.render_overlay(target, &batch);
                }
                // TODO: Placeholder type 2 = Image (thumbnail), 3 = Skeleton (shimmer)
                continue;
            }

            let Some(gpu_image) = gpu_image else {
                continue; // Skip images that failed to load
            };

            // Convert object_fit byte to ObjectFit enum
            let object_fit = match image.object_fit {
                0 => ObjectFit::Cover,
                1 => ObjectFit::Contain,
                2 => ObjectFit::Fill,
                3 => ObjectFit::ScaleDown,
                4 => ObjectFit::None,
                _ => ObjectFit::Cover,
            };

            // Create ObjectPosition from array
            let object_position =
                ObjectPosition::new(image.object_position[0], image.object_position[1]);

            // Calculate fit rectangles
            let (src_rect, dst_rect) = calculate_fit_rects(
                gpu_image.width(),
                gpu_image.height(),
                image.width,
                image.height,
                object_fit,
                object_position,
            );

            // Convert src_rect to UV coordinates
            let src_uv = src_rect_to_uv(src_rect, gpu_image.width(), gpu_image.height());

            // Create GPU instance with proper positioning
            let mut instance = GpuImageInstance::new(
                image.x + dst_rect[0],
                image.y + dst_rect[1],
                dst_rect[2],
                dst_rect[3],
            )
            .with_src_uv(src_uv[0], src_uv[1], src_uv[2], src_uv[3])
            .with_tint(image.tint[0], image.tint[1], image.tint[2], image.tint[3])
            .with_border_radius(image.border_radius)
            .with_opacity(image.opacity);

            // Apply clip bounds if specified
            if let Some(clip) = image.clip_bounds {
                instance = instance.with_clip_rounded_rect_corners(
                    clip[0],
                    clip[1],
                    clip[2],
                    clip[3],
                    image.clip_radius[0],
                    image.clip_radius[1],
                    image.clip_radius[2],
                    image.clip_radius[3],
                );
            }

            // Render the image
            self.renderer
                .render_images(target, gpu_image.view(), &[instance]);
        }
    }

    /// Render images to target from references (images must be preloaded first)
    fn render_images_ref(&mut self, target: &wgpu::TextureView, images: &[&ImageElement]) {
        use blinc_image::{calculate_fit_rects, src_rect_to_uv, ObjectFit, ObjectPosition};

        for image in images {
            // Get cached GPU image
            let Some(gpu_image) = self.image_cache.get(&image.source) else {
                continue; // Skip images that failed to load
            };

            // Convert object_fit byte to ObjectFit enum
            let object_fit = match image.object_fit {
                0 => ObjectFit::Cover,
                1 => ObjectFit::Contain,
                2 => ObjectFit::Fill,
                3 => ObjectFit::ScaleDown,
                4 => ObjectFit::None,
                _ => ObjectFit::Cover,
            };

            // Create ObjectPosition from array
            let object_position =
                ObjectPosition::new(image.object_position[0], image.object_position[1]);

            // Calculate fit rectangles
            let (src_rect, dst_rect) = calculate_fit_rects(
                gpu_image.width(),
                gpu_image.height(),
                image.width,
                image.height,
                object_fit,
                object_position,
            );

            // Convert src_rect to UV coordinates
            let src_uv = src_rect_to_uv(src_rect, gpu_image.width(), gpu_image.height());

            // Create GPU instance with proper positioning
            let mut instance = GpuImageInstance::new(
                image.x + dst_rect[0],
                image.y + dst_rect[1],
                dst_rect[2],
                dst_rect[3],
            )
            .with_src_uv(src_uv[0], src_uv[1], src_uv[2], src_uv[3])
            .with_tint(image.tint[0], image.tint[1], image.tint[2], image.tint[3])
            .with_border_radius(image.border_radius)
            .with_opacity(image.opacity);

            // Apply clip bounds if specified
            if let Some(clip) = image.clip_bounds {
                instance = instance.with_clip_rounded_rect_corners(
                    clip[0],
                    clip[1],
                    clip[2],
                    clip[3],
                    image.clip_radius[0],
                    image.clip_radius[1],
                    image.clip_radius[2],
                    image.clip_radius[3],
                );
            }

            // Render the image
            self.renderer
                .render_images(target, gpu_image.view(), &[instance]);
        }
    }

    /// Render an SVG element with clipping and opacity support
    fn render_svg_element(&mut self, ctx: &mut GpuPaintContext, svg: &SvgElement) {
        // Skip completely transparent SVGs
        if svg.motion_opacity <= 0.001 {
            return;
        }

        // Skip SVGs completely outside their clip bounds
        if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
            let svg_right = svg.x + svg.width;
            let svg_bottom = svg.y + svg.height;
            let clip_right = clip_x + clip_w;
            let clip_bottom = clip_y + clip_h;

            // Check if SVG is completely outside clip bounds
            if svg.x >= clip_right
                || svg_right <= clip_x
                || svg.y >= clip_bottom
                || svg_bottom <= clip_y
            {
                return;
            }
        }

        // Hash the SVG source for cache lookup (faster than using string as key)
        let svg_hash = {
            let mut hasher = DefaultHasher::new();
            svg.source.hash(&mut hasher);
            hasher.finish()
        };

        // Try cache lookup first, parse only on miss
        let doc = if let Some(cached) = self.svg_cache.get(&svg_hash) {
            cached.clone()
        } else {
            let Ok(parsed) = SvgDocument::from_str(&svg.source) else {
                return;
            };
            self.svg_cache.put(svg_hash, parsed.clone());
            parsed
        };

        // Apply clipping if present
        if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
            ctx.push_clip(blinc_core::ClipShape::rect(Rect::new(
                clip_x, clip_y, clip_w, clip_h,
            )));
        }

        // Apply opacity if not fully opaque
        if svg.motion_opacity < 1.0 {
            ctx.push_opacity(svg.motion_opacity);
        }

        // Render the SVG with optional tint color override
        if let Some(tint) = svg.tint {
            // Render with tint - we need to modify the SVG commands
            self.render_svg_with_tint(ctx, &doc, svg.x, svg.y, svg.width, svg.height, tint);
        } else {
            doc.render_fit(ctx, Rect::new(svg.x, svg.y, svg.width, svg.height));
        }

        // Pop opacity if applied
        if svg.motion_opacity < 1.0 {
            ctx.pop_opacity();
        }

        // Pop clip if applied
        if svg.clip_bounds.is_some() {
            ctx.pop_clip();
        }
    }

    /// Render an SVG with a tint color applied to all fills and strokes
    fn render_svg_with_tint(
        &self,
        ctx: &mut GpuPaintContext,
        doc: &SvgDocument,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        tint: blinc_core::Color,
    ) {
        use blinc_svg::SvgDrawCommand;

        // Calculate scale to fit within bounds while maintaining aspect ratio
        let scale_x = width / doc.width;
        let scale_y = height / doc.height;
        let scale = scale_x.min(scale_y);

        // Center within bounds
        let scaled_width = doc.width * scale;
        let scaled_height = doc.height * scale;
        let offset_x = x + (width - scaled_width) / 2.0;
        let offset_y = y + (height - scaled_height) / 2.0;

        let commands = doc.commands();
        let tint_brush = Brush::Solid(tint);

        for cmd in commands {
            match cmd {
                SvgDrawCommand::FillPath { path, brush: _ } => {
                    let scaled = scale_and_translate_path(&path, offset_x, offset_y, scale);
                    ctx.fill_path(&scaled, tint_brush.clone());
                }
                SvgDrawCommand::StrokePath {
                    path,
                    stroke,
                    brush: _,
                } => {
                    let scaled = scale_and_translate_path(&path, offset_x, offset_y, scale);
                    let scaled_stroke = Stroke::new(stroke.width * scale)
                        .with_cap(stroke.cap)
                        .with_join(stroke.join);
                    ctx.stroke_path(&scaled, &scaled_stroke, tint_brush.clone());
                }
            }
        }
    }

    /// Render SVG elements using CPU rasterization for high-quality anti-aliased output
    ///
    /// This method rasterizes SVGs using resvg/tiny-skia and renders them as textures,
    /// providing much better anti-aliasing than tessellation-based path rendering.
    ///
    /// The `scale_factor` parameter is the display's DPI scale (e.g., 2.0 for Retina).
    /// SVGs are rasterized at physical pixel resolution for crisp rendering on HiDPI displays.
    fn render_rasterized_svgs(
        &mut self,
        target: &wgpu::TextureView,
        svgs: &[SvgElement],
        scale_factor: f32,
    ) {
        for svg in svgs {
            // Skip completely transparent SVGs
            if svg.motion_opacity <= 0.001 {
                continue;
            }

            // Skip SVGs completely outside their clip bounds
            if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
                let svg_right = svg.x + svg.width;
                let svg_bottom = svg.y + svg.height;
                let clip_right = clip_x + clip_w;
                let clip_bottom = clip_y + clip_h;

                if svg.x >= clip_right
                    || svg_right <= clip_x
                    || svg.y >= clip_bottom
                    || svg_bottom <= clip_y
                {
                    continue;
                }
            }

            // Rasterize at physical pixel resolution for HiDPI displays
            // svg.width/height are logical sizes, multiply by scale_factor for physical pixels
            let raster_width = ((svg.width * scale_factor).ceil() as u32).max(1);
            let raster_height = ((svg.height * scale_factor).ceil() as u32).max(1);

            // Compute cache key: hash of (svg_source, width, height, scale, tint)
            let cache_key = {
                let mut hasher = DefaultHasher::new();
                svg.source.hash(&mut hasher);
                raster_width.hash(&mut hasher);
                raster_height.hash(&mut hasher);
                // Include tint in cache key (hash as bits to handle f32)
                if let Some(tint) = &svg.tint {
                    tint.r.to_bits().hash(&mut hasher);
                    tint.g.to_bits().hash(&mut hasher);
                    tint.b.to_bits().hash(&mut hasher);
                    tint.a.to_bits().hash(&mut hasher);
                }
                hasher.finish()
            };

            // Check cache or rasterize on miss
            if self.rasterized_svg_cache.get(&cache_key).is_none() {
                // Rasterize the SVG
                let rasterized = if let Some(tint) = svg.tint {
                    RasterizedSvg::from_str_with_tint(
                        &svg.source,
                        raster_width,
                        raster_height,
                        tint,
                    )
                } else {
                    RasterizedSvg::from_str(&svg.source, raster_width, raster_height)
                };

                let rasterized = match rasterized {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("Failed to rasterize SVG: {}", e);
                        continue;
                    }
                };

                // Upload to GPU
                let gpu_image = GpuImage::from_rgba(
                    &self.device,
                    &self.queue,
                    rasterized.data(),
                    rasterized.width,
                    rasterized.height,
                    Some("Rasterized SVG"),
                );

                self.rasterized_svg_cache.put(cache_key, gpu_image);
            }

            // Get the cached GPU image
            let Some(gpu_image) = self.rasterized_svg_cache.get(&cache_key) else {
                continue;
            };

            // Create instance at SVG position
            let mut instance = GpuImageInstance::new(svg.x, svg.y, svg.width, svg.height)
                .with_opacity(svg.motion_opacity);

            // Apply clip bounds if specified
            if let Some([clip_x, clip_y, clip_w, clip_h]) = svg.clip_bounds {
                instance = instance.with_clip_rect(clip_x, clip_y, clip_w, clip_h);
            }

            // Render the rasterized SVG as an image
            self.renderer
                .render_images(target, gpu_image.view(), &[instance]);
        }
    }

    /// Collect text, SVG, and image elements from the render tree
    fn collect_render_elements(
        &mut self,
        tree: &RenderTree,
    ) -> (Vec<TextElement>, Vec<SvgElement>, Vec<ImageElement>) {
        self.collect_render_elements_with_state(tree, None)
    }

    /// Collect text, SVG, and image elements with motion state
    fn collect_render_elements_with_state(
        &mut self,
        tree: &RenderTree,
        render_state: Option<&blinc_layout::RenderState>,
    ) -> (Vec<TextElement>, Vec<SvgElement>, Vec<ImageElement>) {
        // Reuse scratch buffers - take them, clear, populate, and return
        // On next call they'll be reallocated if not returned
        let mut texts = std::mem::take(&mut self.scratch_texts);
        let mut svgs = std::mem::take(&mut self.scratch_svgs);
        let mut images = std::mem::take(&mut self.scratch_images);
        texts.clear();
        svgs.clear();
        images.clear();

        // Get the scale factor from the tree for DPI scaling
        let scale = tree.scale_factor();

        if let Some(root) = tree.root() {
            let mut z_layer = 0u32;
            self.collect_elements_recursive(
                tree,
                root,
                (0.0, 0.0),
                false,      // inside_glass
                false,      // inside_foreground
                None,       // No initial clip bounds
                None,       // No initial clip radius
                1.0,        // Initial motion opacity
                (0.0, 0.0), // Initial motion translate offset
                (1.0, 1.0), // Initial motion scale
                None,       // No initial motion scale center
                render_state,
                scale,
                &mut z_layer,
                &mut texts,
                &mut svgs,
                &mut images,
            );
        }

        // Sort texts by z_index (z_layer) to ensure correct rendering order with primitives
        texts.sort_by_key(|t| t.z_index);

        (texts, svgs, images)
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_elements_recursive(
        &self,
        tree: &RenderTree,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        inside_glass: bool,
        inside_foreground: bool,
        current_clip: Option<[f32; 4]>,
        current_clip_radius: Option<[f32; 4]>,
        inherited_motion_opacity: f32,
        inherited_motion_translate: (f32, f32),
        inherited_motion_scale: (f32, f32),
        // Center point for motion scale (in layout coordinates, before DPI scaling)
        // When a parent has motion scale, children should scale around the parent's center
        inherited_motion_scale_center: Option<(f32, f32)>,
        render_state: Option<&blinc_layout::RenderState>,
        scale: f32,
        z_layer: &mut u32,
        texts: &mut Vec<TextElement>,
        svgs: &mut Vec<SvgElement>,
        images: &mut Vec<ImageElement>,
    ) {
        use blinc_layout::Material;

        // Use animated bounds if this node has layout animation, otherwise use layout bounds
        // This ensures children are positioned correctly during layout animation transitions
        let Some(bounds) = tree.get_render_bounds(node, parent_offset) else {
            return;
        };

        let abs_x = bounds.x;
        let abs_y = bounds.y;

        // Get motion values for this node from RenderState (entry/exit animations)
        let motion_values = render_state.and_then(|rs| {
            // Try stable motion first, then node-based
            if let Some(render_node) = tree.get_render_node(node) {
                if let Some(ref stable_key) = render_node.props.motion_stable_id {
                    return rs.get_stable_motion_values(stable_key);
                }
            }
            rs.get_motion_values(node)
        });

        // Get motion bindings from RenderTree (continuous AnimatedValue animations)
        // NOTE: binding_transform (translate) is NOT added to effective_motion_translate
        // because it's already included in new_offset for child positioning (see line ~1250).
        // Only RenderState motion values need to be inherited through effective_motion_translate.
        let binding_scale = tree.get_motion_scale(node);
        let binding_opacity = tree.get_motion_opacity(node);

        // Calculate motion opacity for this node (combine both sources)
        let node_motion_opacity = motion_values
            .and_then(|m| m.opacity)
            .unwrap_or_else(|| binding_opacity.unwrap_or(1.0));

        // Get motion translate for this node from RenderState only
        // (binding translate is handled via new_offset in recursive calls)
        let node_motion_translate = motion_values
            .map(|m| m.resolved_translate())
            .unwrap_or((0.0, 0.0));

        // Get motion scale for this node from RenderState
        let node_motion_scale = motion_values
            .map(|m| m.resolved_scale())
            .unwrap_or((1.0, 1.0));

        // Combine with binding scale
        let binding_scale_values = binding_scale.unwrap_or((1.0, 1.0));

        // Combine with inherited values
        // NOTE: effective_motion_translate only includes RenderState motion values,
        // NOT binding transforms (which are already in the position via new_offset)
        let effective_motion_opacity = inherited_motion_opacity * node_motion_opacity;
        let effective_motion_translate = (
            inherited_motion_translate.0 + node_motion_translate.0,
            inherited_motion_translate.1 + node_motion_translate.1,
        );
        // Scale compounds multiplicatively (including binding scale)
        let effective_motion_scale = (
            inherited_motion_scale.0 * node_motion_scale.0 * binding_scale_values.0,
            inherited_motion_scale.1 * node_motion_scale.1 * binding_scale_values.1,
        );

        // Determine the motion scale center for children
        // If this node has motion scale (from RenderState or binding), use its center as the scale center
        // Otherwise, inherit the parent's scale center
        let this_node_has_scale = (node_motion_scale.0 - 1.0).abs() > 0.001
            || (node_motion_scale.1 - 1.0).abs() > 0.001
            || (binding_scale_values.0 - 1.0).abs() > 0.001
            || (binding_scale_values.1 - 1.0).abs() > 0.001;

        let effective_motion_scale_center = if this_node_has_scale {
            // This node has motion scale - compute its center in absolute layout coordinates
            let center_x = abs_x + bounds.width / 2.0;
            let center_y = abs_y + bounds.height / 2.0;
            Some((center_x, center_y))
        } else {
            // No scale on this node - inherit the parent's scale center
            inherited_motion_scale_center
        };

        // Skip if completely transparent
        if effective_motion_opacity <= 0.001 {
            return;
        }

        // Determine if this node is a glass element
        let is_glass = tree
            .get_render_node(node)
            .map(|n| matches!(n.props.material, Some(Material::Glass(_))))
            .unwrap_or(false);

        // Track if children should be considered inside glass
        let children_inside_glass = inside_glass || is_glass;

        // Check if this node clips its children (e.g., scroll containers)
        let clips_content = tree
            .get_render_node(node)
            .map(|n| n.props.clips_content)
            .unwrap_or(false);

        // Check if this node has an active layout animation (also needs clipping)
        // Layout animations need to clip children to animated bounds
        let has_layout_animation = tree.is_layout_animating(node);

        // Check if this is a Stack layer - if so, increment z_layer for proper z-ordering
        let is_stack_layer = tree
            .get_render_node(node)
            .map(|n| n.props.is_stack_layer)
            .unwrap_or(false);
        if is_stack_layer {
            *z_layer += 1;
        }

        // Update clip bounds for children if this node clips (either via clips_content or layout animation)
        // When a node clips, we INTERSECT its bounds with any existing clip
        // This ensures nested clipping works correctly (inner clips can't expand outer clips)
        let should_clip = clips_content || has_layout_animation;
        let (child_clip, child_clip_radius) = if should_clip {
            // For layout animation, use animated bounds for clipping
            // This ensures content is clipped to the animating size during transition
            let clip_bounds = if has_layout_animation {
                // Get animated bounds - these are the interpolated bounds during animation
                tree.get_render_bounds(node, parent_offset)
                    .map(|b| [b.x, b.y, b.width, b.height])
                    .unwrap_or([abs_x, abs_y, bounds.width, bounds.height])
            } else {
                [abs_x, abs_y, bounds.width, bounds.height]
            };
            let this_clip = clip_bounds;

            // Extract border radius from this node for rounded clipping
            // Order: top_left, top_right, bottom_right, bottom_left
            let this_clip_radius = tree.get_render_node(node).map(|n| {
                let r = &n.props.border_radius;
                [r.top_left, r.top_right, r.bottom_right, r.bottom_left]
            });

            let new_clip = if let Some(parent_clip) = current_clip {
                // Intersect: take the overlap of parent_clip and this_clip
                let x1 = parent_clip[0].max(this_clip[0]);
                let y1 = parent_clip[1].max(this_clip[1]);
                let x2 = (parent_clip[0] + parent_clip[2]).min(this_clip[0] + this_clip[2]);
                let y2 = (parent_clip[1] + parent_clip[3]).min(this_clip[1] + this_clip[3]);
                let w = (x2 - x1).max(0.0);
                let h = (y2 - y1).max(0.0);
                Some([x1, y1, w, h])
            } else {
                Some(this_clip)
            };

            // Use this node's border radius if it has one, otherwise inherit parent's
            let new_radius = if this_clip_radius
                .map(|r| r.iter().any(|&v| v > 0.0))
                .unwrap_or(false)
            {
                this_clip_radius
            } else {
                current_clip_radius
            };

            (new_clip, new_radius)
        } else {
            (current_clip, current_clip_radius)
        };

        if let Some(render_node) = tree.get_render_node(node) {
            // Determine effective layer: children inside glass render in Foreground
            let effective_layer = if inside_glass && !is_glass {
                RenderLayer::Foreground
            } else if is_glass {
                RenderLayer::Glass
            } else {
                render_node.props.layer
            };

            match &render_node.element_type {
                ElementType::Text(text_data) => {
                    // Apply DPI scale factor FIRST to match shape rendering order
                    // In render_with_motion, DPI scale is pushed at root level before any other transforms
                    // So we must: scale base positions first, then apply motion transforms
                    let base_x = abs_x * scale;
                    let base_y = abs_y * scale;
                    let base_width = bounds.width * scale;
                    let base_height = bounds.height * scale;

                    // Scale motion translate by DPI factor (motion values are in layout coordinates)
                    let scaled_motion_tx = effective_motion_translate.0 * scale;
                    let scaled_motion_ty = effective_motion_translate.1 * scale;

                    // Apply motion scale and translation
                    // When there's a motion scale center (from parent Motion container),
                    // we must scale around THAT center, not the text element's own center.
                    // This matches how shapes are rendered - the scale transform is pushed
                    // at the Motion container level and affects all children relative to
                    // the container's center.
                    let (scaled_x, scaled_y, scaled_width, scaled_height) =
                        if let Some((motion_center_x, motion_center_y)) =
                            effective_motion_scale_center
                        {
                            // Scale position around the motion container's center (in DPI-scaled coordinates)
                            let motion_center_x_scaled = motion_center_x * scale;
                            let motion_center_y_scaled = motion_center_y * scale;

                            // Calculate position relative to motion center
                            let rel_x = base_x - motion_center_x_scaled;
                            let rel_y = base_y - motion_center_y_scaled;

                            // Apply scale to relative position and size
                            let scaled_rel_x = rel_x * effective_motion_scale.0;
                            let scaled_rel_y = rel_y * effective_motion_scale.1;
                            let scaled_w = base_width * effective_motion_scale.0;
                            let scaled_h = base_height * effective_motion_scale.1;

                            // Apply motion translation and convert back to absolute position
                            let final_x = motion_center_x_scaled + scaled_rel_x + scaled_motion_tx;
                            let final_y = motion_center_y_scaled + scaled_rel_y + scaled_motion_ty;

                            (final_x, final_y, scaled_w, scaled_h)
                        } else {
                            // No motion scale center - just apply translation (no scale effect)
                            let final_x = base_x + scaled_motion_tx;
                            let final_y = base_y + scaled_motion_ty;
                            (final_x, final_y, base_width, base_height)
                        };

                    let scaled_font_size = text_data.font_size * effective_motion_scale.1 * scale;
                    let scaled_measured_width =
                        text_data.measured_width * effective_motion_scale.0 * scale;

                    // Scale clip bounds if present
                    let scaled_clip = current_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Log motion values if non-trivial (for debugging text/shape sync issues)
                    if effective_motion_translate.0.abs() > 0.1
                        || effective_motion_translate.1.abs() > 0.1
                        || (effective_motion_scale.0 - 1.0).abs() > 0.01
                        || (effective_motion_scale.1 - 1.0).abs() > 0.01
                    {
                        tracing::debug!(
                            "Text '{}': motion_translate=({:.1}, {:.1}), motion_scale=({:.2}, {:.2}), base=({:.1}, {:.1}), final=({:.1}, {:.1})",
                            text_data.content,
                            effective_motion_translate.0,
                            effective_motion_translate.1,
                            effective_motion_scale.0,
                            effective_motion_scale.1,
                            base_x,
                            base_y,
                            scaled_x,
                            scaled_y,
                        );
                    }
                    tracing::debug!(
                        "Text '{}': abs=({:.1}, {:.1}), size=({:.1}x{:.1}), font={:.1}, align={:?}, v_align={:?}, z_layer={}",
                        text_data.content,
                        scaled_x,
                        scaled_y,
                        scaled_width,
                        scaled_height,
                        scaled_font_size,
                        text_data.align,
                        text_data.v_align,
                        *z_layer
                    );

                    texts.push(TextElement {
                        content: text_data.content.clone(),
                        x: scaled_x,
                        y: scaled_y,
                        width: scaled_width,
                        height: scaled_height,
                        font_size: scaled_font_size,
                        color: text_data.color,
                        align: text_data.align,
                        weight: text_data.weight,
                        italic: text_data.italic,
                        v_align: text_data.v_align,
                        clip_bounds: scaled_clip,
                        motion_opacity: effective_motion_opacity,
                        wrap: text_data.wrap,
                        line_height: text_data.line_height,
                        measured_width: scaled_measured_width,
                        font_family: text_data.font_family.clone(),
                        word_spacing: text_data.word_spacing,
                        z_index: *z_layer,
                        ascender: text_data.ascender * effective_motion_scale.1 * scale,
                        strikethrough: text_data.strikethrough,
                        underline: text_data.underline,
                    });
                }
                ElementType::Svg(svg_data) => {
                    // Apply DPI scale factor FIRST to match shape rendering order
                    let base_x = abs_x * scale;
                    let base_y = abs_y * scale;
                    let base_width = bounds.width * scale;
                    let base_height = bounds.height * scale;

                    // Scale motion translate by DPI factor
                    let scaled_motion_tx = effective_motion_translate.0 * scale;
                    let scaled_motion_ty = effective_motion_translate.1 * scale;

                    // Apply motion scale and translation (same logic as Text)
                    let (scaled_x, scaled_y, scaled_width, scaled_height) =
                        if let Some((motion_center_x, motion_center_y)) =
                            effective_motion_scale_center
                        {
                            let motion_center_x_scaled = motion_center_x * scale;
                            let motion_center_y_scaled = motion_center_y * scale;

                            let rel_x = base_x - motion_center_x_scaled;
                            let rel_y = base_y - motion_center_y_scaled;

                            let scaled_rel_x = rel_x * effective_motion_scale.0;
                            let scaled_rel_y = rel_y * effective_motion_scale.1;
                            let scaled_w = base_width * effective_motion_scale.0;
                            let scaled_h = base_height * effective_motion_scale.1;

                            let final_x = motion_center_x_scaled + scaled_rel_x + scaled_motion_tx;
                            let final_y = motion_center_y_scaled + scaled_rel_y + scaled_motion_ty;

                            (final_x, final_y, scaled_w, scaled_h)
                        } else {
                            let final_x = base_x + scaled_motion_tx;
                            let final_y = base_y + scaled_motion_ty;
                            (final_x, final_y, base_width, base_height)
                        };

                    // Scale clip bounds if present
                    let scaled_clip = current_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    svgs.push(SvgElement {
                        source: svg_data.source.clone(),
                        x: scaled_x,
                        y: scaled_y,
                        width: scaled_width,
                        height: scaled_height,
                        tint: svg_data.tint,
                        clip_bounds: scaled_clip,
                        motion_opacity: effective_motion_opacity,
                    });
                }
                ElementType::Image(image_data) => {
                    // Apply DPI scale factor to image positions and sizes
                    let scaled_clip = current_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Scale clip radius by DPI factor (radius values are in layout coordinates)
                    let scaled_clip_radius = current_clip_radius
                        .map(|[tl, tr, br, bl]| [tl * scale, tr * scale, br * scale, bl * scale])
                        .unwrap_or([0.0; 4]);

                    images.push(ImageElement {
                        source: image_data.source.clone(),
                        x: abs_x * scale,
                        y: abs_y * scale,
                        width: bounds.width * scale,
                        height: bounds.height * scale,
                        object_fit: image_data.object_fit,
                        object_position: image_data.object_position,
                        opacity: image_data.opacity,
                        border_radius: image_data.border_radius * scale,
                        tint: image_data.tint,
                        clip_bounds: scaled_clip,
                        clip_radius: scaled_clip_radius,
                        layer: effective_layer,
                        loading_strategy: image_data.loading_strategy,
                        placeholder_type: image_data.placeholder_type,
                        placeholder_color: image_data.placeholder_color,
                        z_index: *z_layer,
                    });
                }
                // Canvas elements are rendered inline during tree traversal (in render_layer)
                ElementType::Canvas(_) => {}
                ElementType::Div => {}
                // StyledText: render text with inline styling using multiple TextElements
                ElementType::StyledText(styled_data) => {
                    // Apply DPI scale factor first
                    let base_x = abs_x * scale;
                    let base_y = abs_y * scale;
                    let base_width = bounds.width * scale;
                    let base_height = bounds.height * scale;

                    // Scale motion translate by DPI factor
                    let scaled_motion_tx = effective_motion_translate.0 * scale;
                    let scaled_motion_ty = effective_motion_translate.1 * scale;

                    // Apply motion scale and translation (same logic as Text)
                    let (scaled_x, scaled_y, scaled_width, scaled_height) =
                        if let Some((motion_center_x, motion_center_y)) =
                            effective_motion_scale_center
                        {
                            let motion_center_x_scaled = motion_center_x * scale;
                            let motion_center_y_scaled = motion_center_y * scale;

                            let rel_x = base_x - motion_center_x_scaled;
                            let rel_y = base_y - motion_center_y_scaled;

                            let scaled_rel_x = rel_x * effective_motion_scale.0;
                            let scaled_rel_y = rel_y * effective_motion_scale.1;
                            let scaled_w = base_width * effective_motion_scale.0;
                            let scaled_h = base_height * effective_motion_scale.1;

                            let final_x = motion_center_x_scaled + scaled_rel_x + scaled_motion_tx;
                            let final_y = motion_center_y_scaled + scaled_rel_y + scaled_motion_ty;

                            (final_x, final_y, scaled_w, scaled_h)
                        } else {
                            let final_x = base_x + scaled_motion_tx;
                            let final_y = base_y + scaled_motion_ty;
                            (final_x, final_y, base_width, base_height)
                        };

                    let scaled_font_size = styled_data.font_size * effective_motion_scale.1 * scale;
                    let scaled_clip = current_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

                    // Build non-overlapping segments from potentially overlapping spans
                    // This handles nested tags like <span color="red"><b>text</b></span>
                    let content = &styled_data.content;
                    let content_len = content.len();

                    // Get default styles from element config
                    let default_bold = styled_data.weight == FontWeight::Bold;
                    let default_italic = styled_data.italic;

                    // Collect all boundary positions where style might change
                    let mut boundaries: Vec<usize> = vec![0, content_len];
                    for span in &styled_data.spans {
                        if span.start < content_len {
                            boundaries.push(span.start);
                        }
                        if span.end <= content_len {
                            boundaries.push(span.end);
                        }
                    }
                    boundaries.sort();
                    boundaries.dedup();

                    // Build segments between boundaries
                    let mut segments: Vec<(usize, usize, [f32; 4], bool, bool, bool, bool)> =
                        Vec::new();

                    for window in boundaries.windows(2) {
                        let seg_start = window[0];
                        let seg_end = window[1];
                        if seg_start >= seg_end {
                            continue;
                        }

                        // Determine style for this segment by merging all overlapping spans
                        let mut color: Option<[f32; 4]> = None;
                        let mut bold = default_bold;
                        let mut italic = default_italic;
                        let mut underline = false;
                        let mut strikethrough = false;

                        for span in &styled_data.spans {
                            // Check if span overlaps this segment
                            if span.start <= seg_start && span.end >= seg_end {
                                // This span covers this segment - merge styles
                                if span.bold {
                                    bold = true;
                                }
                                if span.italic {
                                    italic = true;
                                }
                                if span.underline {
                                    underline = true;
                                }
                                if span.strikethrough {
                                    strikethrough = true;
                                }
                                // Use color if span has explicit color (not transparent)
                                if span.color[3] > 0.0 {
                                    color = Some(span.color);
                                }
                            }
                        }

                        let final_color = color.unwrap_or(styled_data.default_color);
                        segments.push((
                            seg_start,
                            seg_end,
                            final_color,
                            bold,
                            italic,
                            underline,
                            strikethrough,
                        ));
                    }

                    // Use consistent ascender from element for baseline alignment
                    let scaled_ascender = styled_data.ascender * scale;

                    // Calculate x offsets for each segment and push as TextElements
                    let mut x_offset = 0.0f32;
                    for (start, end, color, bold, italic, underline, strikethrough) in segments {
                        if start >= end || start >= content.len() {
                            continue;
                        }
                        let segment_text = &content[start..end.min(content.len())];
                        if segment_text.is_empty() {
                            continue;
                        }

                        // Measure segment width for positioning
                        let mut options = blinc_layout::text_measure::TextLayoutOptions::new();
                        options.font_name = styled_data.font_family.name.clone();
                        options.generic_font = styled_data.font_family.generic;
                        options.font_weight = if bold { 700 } else { 400 };
                        options.italic = italic;
                        let metrics = blinc_layout::text_measure::measure_text_with_options(
                            segment_text,
                            styled_data.font_size,
                            &options,
                        );
                        // Apply both DPI scale and motion scale to segment width
                        let segment_width = metrics.width * scale * effective_motion_scale.0;

                        texts.push(TextElement {
                            content: segment_text.to_string(),
                            x: scaled_x + x_offset,
                            y: scaled_y,
                            width: segment_width,
                            height: scaled_height,
                            font_size: scaled_font_size,
                            color,
                            align: TextAlign::Left, // Always left-align segments
                            weight: if bold {
                                FontWeight::Bold
                            } else {
                                FontWeight::Normal
                            },
                            italic,
                            v_align: styled_data.v_align,
                            clip_bounds: scaled_clip,
                            motion_opacity: effective_motion_opacity,
                            wrap: false, // Don't wrap individual segments
                            line_height: styled_data.line_height,
                            measured_width: segment_width,
                            font_family: styled_data.font_family.clone(),
                            word_spacing: 0.0,
                            z_index: *z_layer,
                            ascender: scaled_ascender * effective_motion_scale.1, // Scale ascender with motion
                            strikethrough,
                            underline,
                        });

                        x_offset += segment_width;
                    }
                }
            }
        }

        // Include scroll offset and motion offset when calculating child positions
        let scroll_offset = tree.get_scroll_offset(node);
        let static_motion_offset = tree
            .get_motion_transform(node)
            .map(|t| match t {
                blinc_core::Transform::Affine2D(a) => (a.elements[4], a.elements[5]),
                _ => (0.0, 0.0),
            })
            .unwrap_or((0.0, 0.0));

        let new_offset = (
            abs_x + scroll_offset.0 + static_motion_offset.0,
            abs_y + scroll_offset.1 + static_motion_offset.1,
        );
        for child_id in tree.layout().children(node) {
            self.collect_elements_recursive(
                tree,
                child_id,
                new_offset,
                children_inside_glass,
                inside_foreground,
                child_clip,
                child_clip_radius,
                effective_motion_opacity,
                effective_motion_translate,
                effective_motion_scale,
                effective_motion_scale_center,
                render_state,
                scale,
                z_layer,
                texts,
                svgs,
                images,
            );
        }
    }

    /// Get device arc
    pub fn device(&self) -> &Arc<wgpu::Device> {
        &self.device
    }

    /// Get queue arc
    pub fn queue(&self) -> &Arc<wgpu::Queue> {
        &self.queue
    }

    /// Get the shared font registry
    ///
    /// This can be used to share fonts between text measurement and rendering,
    /// ensuring consistent font loading and metrics.
    pub fn font_registry(&self) -> Arc<Mutex<FontRegistry>> {
        self.text_ctx.font_registry()
    }

    /// Get the texture format used by the renderer
    pub fn texture_format(&self) -> wgpu::TextureFormat {
        self.renderer.texture_format()
    }

    /// Render a layout tree with dynamic render state overlays
    ///
    /// This method renders:
    /// 1. The stable RenderTree (element hierarchy and layout)
    /// 2. RenderState overlays (cursors, selections, focus rings)
    ///
    /// The RenderState overlays are drawn on top of the tree without requiring
    /// tree rebuilds. This enables smooth cursor blinking and animations.
    pub fn render_tree_with_state(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        // First render the tree as normal
        self.render_tree(tree, width, height, target)?;

        // Then render overlays from RenderState
        self.render_overlays(render_state, width, height, target);

        Ok(())
    }

    /// Render a layout tree with motion animations from RenderState
    ///
    /// This method renders:
    /// 1. The RenderTree with motion animations applied (opacity, scale, translate)
    /// 2. RenderState overlays (cursors, selections, focus rings)
    ///
    /// Use this method when you have elements wrapped in motion() containers
    /// for enter/exit animations.
    pub fn render_tree_with_motion(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        // Get scale factor for HiDPI rendering
        let scale_factor = tree.scale_factor();

        // Create a single paint context for all layers with text rendering support
        let mut ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);

        // Render with motion animations applied (all layers to same context)
        tree.render_with_motion(&mut ctx, render_state);

        // Take the batch
        let batch = ctx.take_batch();

        // Collect text, SVG, and image elements WITH motion state
        let (texts, svgs, images) =
            self.collect_render_elements_with_state(tree, Some(render_state));

        // Pre-load all images into cache before rendering
        self.preload_images(&images, width as f32, height as f32);

        // Prepare text glyphs with z_layer information
        // Store (z_layer, glyphs) to enable interleaved rendering
        let mut glyphs_by_layer: std::collections::BTreeMap<u32, Vec<GpuGlyph>> =
            std::collections::BTreeMap::new();
        for text in &texts {
            // Skip text that's completely outside its clip bounds (visibility culling)
            // This prevents loading emoji fonts for off-screen text in scroll containers
            if let Some([clip_x, clip_y, clip_w, clip_h]) = text.clip_bounds {
                let text_right = text.x + text.width;
                let text_bottom = text.y + text.height;
                let clip_right = clip_x + clip_w;
                let clip_bottom = clip_y + clip_h;

                // Check if text is completely outside clip bounds
                if text.x >= clip_right
                    || text_right <= clip_x
                    || text.y >= clip_bottom
                    || text_bottom <= clip_y
                {
                    // Text is not visible, skip rendering entirely
                    continue;
                }
            }

            let alignment = match text.align {
                TextAlign::Left => TextAlignment::Left,
                TextAlign::Center => TextAlignment::Center,
                TextAlign::Right => TextAlignment::Right,
            };

            // Apply motion opacity to text color
            let color = if text.motion_opacity < 1.0 {
                [
                    text.color[0],
                    text.color[1],
                    text.color[2],
                    text.color[3] * text.motion_opacity,
                ]
            } else {
                text.color
            };

            // Determine wrap width:
            // 1. If clip bounds exist and are smaller than measured width, use clip width
            //    (this handles scroll containers where layout width isn't constrained)
            // 2. Otherwise, if layout width is smaller than measured, use layout width
            // 3. Otherwise, don't wrap (text fits naturally)
            let effective_width = if let Some(clip) = text.clip_bounds {
                // Use clip width if it constrains the text
                clip[2].min(text.width)
            } else {
                text.width
            };

            // Wrap if effective width is significantly smaller than measured width
            let needs_wrap = text.wrap && effective_width < text.measured_width - 2.0;

            // Always pass width for alignment - the layout engine needs max_width
            // to calculate center/right alignment offsets
            let wrap_width = Some(text.width);

            // Convert font family to GPU types
            let font_name = text.font_family.name.as_deref();
            let generic = to_gpu_generic_font(text.font_family.generic);
            let font_weight = text.weight.weight();

            // Map vertical alignment to text anchor
            let (anchor, y_pos, use_layout_height) = match text.v_align {
                TextVerticalAlign::Center => {
                    (TextAnchor::Center, text.y + text.height / 2.0, false)
                }
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    let baseline_y = text.y + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };
            let layout_height = if use_layout_height {
                Some(text.height)
            } else {
                None
            };

            if let Ok(glyphs) = self.text_ctx.prepare_text_with_style(
                &text.content,
                text.x,
                y_pos,
                text.font_size,
                color,
                anchor,
                alignment,
                wrap_width,
                needs_wrap,
                font_name,
                generic,
                font_weight,
                text.italic,
                layout_height,
            ) {
                // Apply clip bounds if present
                let mut glyphs = glyphs;
                if let Some(clip) = text.clip_bounds {
                    for glyph in &mut glyphs {
                        glyph.clip_bounds = clip;
                    }
                }
                // Group glyphs by their z_layer
                glyphs_by_layer
                    .entry(text.z_index)
                    .or_default()
                    .extend(glyphs);
            }
        }

        // SVGs are rendered as rasterized images (not tessellated paths) for better anti-aliasing
        // They will be rendered later via render_rasterized_svgs

        self.renderer.resize(width, height);

        let has_glass = batch.glass_count() > 0;
        let has_layer_effects_in_batch = batch.has_layer_effects();

        // Only allocate glass textures when glass is actually used
        if has_glass {
            self.ensure_glass_textures(width, height);
        }
        let use_msaa_overlay = self.sample_count > 1;

        if has_glass {
            // Glass path with layer effects support
            let (bg_images, fg_images): (Vec<_>, Vec<_>) = images
                .iter()
                .partition(|img| img.layer == RenderLayer::Background);

            if has_layer_effects_in_batch {
                // When we have layer effects, we need a more complex render path:
                // 1. Render backdrop for glass blur sampling
                // 2. Use render_with_clear which handles layer effects
                // 3. Render glass primitives on top
                let backdrop = self.backdrop_texture.as_ref().unwrap();

                // First render to backdrop texture for glass blur sampling
                self.renderer.render_to_backdrop(
                    &backdrop.view,
                    (backdrop.width, backdrop.height),
                    &batch,
                );

                // Then use render_with_clear which handles layer effects
                self.renderer
                    .render_with_clear(target, &batch, [0.0, 0.0, 0.0, 1.0]);

                // Finally render glass primitives on top
                if batch.glass_count() > 0 {
                    self.renderer.render_glass(target, &backdrop.view, &batch);
                }
            } else {
                // No layer effects, use optimized glass frame rendering
                let backdrop = self.backdrop_texture.as_ref().unwrap();
                self.renderer.render_glass_frame(
                    target,
                    &backdrop.view,
                    (backdrop.width, backdrop.height),
                    &batch,
                );
            }

            // Render paths with MSAA for smooth edges on curved shapes like notch
            // (render_glass_frame uses 1x sampled path rendering)
            if use_msaa_overlay && batch.has_paths() {
                self.renderer
                    .render_paths_overlay_msaa(target, &batch, self.sample_count);
            }

            self.render_images_ref(target, &bg_images);
            self.render_images_ref(target, &fg_images);

            // Collect all glyphs for glass path using scratch buffer to avoid allocation
            // (TODO: implement interleaved glass rendering)
            // Take ownership temporarily to avoid borrow conflict with self.render_text
            let mut scratch = std::mem::take(&mut self.scratch_glyphs);
            scratch.clear();
            for glyphs in glyphs_by_layer.values() {
                scratch.extend_from_slice(glyphs);
            }
            if !scratch.is_empty() {
                self.render_text(target, &scratch);
            }
            self.scratch_glyphs = scratch; // Restore for next frame

            // Render text decorations for glass path (all layers)
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);
            for primitives in decorations_by_layer.values() {
                if !primitives.is_empty() {
                    self.renderer.render_primitives_overlay(target, primitives);
                }
            }

            // Render SVGs as rasterized images for high-quality anti-aliasing
            if !svgs.is_empty() {
                self.render_rasterized_svgs(target, &svgs, scale_factor);
            }
        } else {
            // Simple path (no glass)
            // Pre-generate text decorations grouped by layer for interleaved rendering
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);

            let max_z = batch.max_z_layer();
            let max_text_z = glyphs_by_layer.keys().cloned().max().unwrap_or(0);
            let max_decoration_z = decorations_by_layer.keys().cloned().max().unwrap_or(0);
            let max_layer = max_z.max(max_text_z).max(max_decoration_z);
            let has_layer_effects = batch.has_layer_effects();

            if max_layer > 0 && !has_layer_effects {
                // Interleaved z-layer rendering for proper Stack z-ordering
                // Group images by z_index for interleaved rendering
                let mut images_by_layer: std::collections::BTreeMap<u32, Vec<&ImageElement>> =
                    std::collections::BTreeMap::new();
                for img in &images {
                    images_by_layer.entry(img.z_index).or_default().push(img);
                }
                let max_image_z = images_by_layer.keys().cloned().max().unwrap_or(0);
                let max_layer = max_layer.max(max_image_z);

                // First pass: render z_layer=0 primitives with clear
                let z0_primitives = batch.primitives_for_layer(0);
                // Create a temporary batch for z=0 (include paths - they don't have z-layer support)
                let mut z0_batch = PrimitiveBatch::new();
                z0_batch.primitives = z0_primitives;
                z0_batch.paths = batch.paths.clone();
                self.renderer
                    .render_with_clear(target, &z0_batch, [0.0, 0.0, 0.0, 1.0]);

                // Render paths with MSAA for smooth edges on curved shapes like notch
                if use_msaa_overlay && z0_batch.has_paths() {
                    self.renderer
                        .render_paths_overlay_msaa(target, &z0_batch, self.sample_count);
                }

                // Render z=0 images
                if let Some(z0_images) = images_by_layer.get(&0) {
                    self.render_images_ref(target, z0_images);
                }

                // Render subsequent layers interleaved (primitives and images)
                for z in 1..=max_layer {
                    // Render primitives for this layer
                    let layer_primitives = batch.primitives_for_layer(z);
                    if !layer_primitives.is_empty() {
                        self.renderer
                            .render_primitives_overlay(target, &layer_primitives);
                    }

                    // Render images for this layer
                    if let Some(layer_images) = images_by_layer.get(&z) {
                        self.render_images_ref(target, layer_images);
                    }
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }

                // Render foreground primitives on top (for .foreground() elements)
                if !batch.foreground_primitives.is_empty() {
                    self.renderer
                        .render_primitives_overlay(target, &batch.foreground_primitives);
                }

                // Render text on top (all z-layers)
                for z in 0..=max_layer {
                    if let Some(glyphs) = glyphs_by_layer.get(&z) {
                        if !glyphs.is_empty() {
                            self.render_text(target, glyphs);
                        }
                    }
                    self.render_text_decorations_for_layer(target, &decorations_by_layer, z);
                }
            } else {
                // No z-layers, use original fast path
                self.renderer
                    .render_with_clear(target, &batch, [0.0, 0.0, 0.0, 1.0]);

                // Render paths with MSAA for smooth edges on curved shapes like notch
                if use_msaa_overlay && batch.has_paths() {
                    self.renderer
                        .render_paths_overlay_msaa(target, &batch, self.sample_count);
                }

                self.render_images(target, &images, width as f32, height as f32);

                // Render foreground primitives on top of images (for .foreground() elements)
                if !batch.foreground_primitives.is_empty() {
                    self.renderer
                        .render_primitives_overlay(target, &batch.foreground_primitives);
                }

                // Render SVGs as rasterized images for high-quality anti-aliasing
                if !svgs.is_empty() {
                    self.render_rasterized_svgs(target, &svgs, scale_factor);
                }

                // Collect all glyphs for flat rendering
                let all_glyphs: Vec<_> = glyphs_by_layer.values().flatten().cloned().collect();
                if !all_glyphs.is_empty() {
                    self.render_text(target, &all_glyphs);
                }

                // Render text decorations (flat path - no z-layers)
                self.render_text_decorations_for_layer(target, &decorations_by_layer, 0);
            }
        }

        // Poll the device to free completed command buffers
        self.renderer.poll();

        // Render overlays from RenderState
        self.render_overlays(render_state, width, height, target);

        // Render debug visualization if enabled (BLINC_DEBUG=text|layout|all)
        let debug = DebugMode::from_env();
        if debug.text {
            self.render_text_debug(target, &texts);
        }
        if debug.layout {
            let scale = tree.scale_factor();
            self.render_layout_debug(target, tree, scale);
        }
        if debug.motion {
            self.render_motion_debug(target, tree, width, height);
        }

        // Return scratch buffers for reuse on next frame
        self.return_scratch_elements(texts, svgs, images);

        Ok(())
    }

    /// Render a tree on top of existing content (no clear)
    ///
    /// This is used for overlay trees (modals, toasts, dialogs) that render
    /// on top of the main UI without clearing it.
    pub fn render_overlay_tree_with_motion(
        &mut self,
        tree: &RenderTree,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) -> Result<()> {
        // Get scale factor for HiDPI rendering
        let scale_factor = tree.scale_factor();

        // Create a single paint context for all layers with text rendering support
        let mut ctx =
            GpuPaintContext::with_text_context(width as f32, height as f32, &mut self.text_ctx);

        // Render with motion animations applied (all layers to same context)
        tree.render_with_motion(&mut ctx, render_state);

        // Take the batch
        let batch = ctx.take_batch();

        // Collect text, SVG, and image elements WITH motion state
        let (texts, svgs, images) =
            self.collect_render_elements_with_state(tree, Some(render_state));

        // Pre-load all images into cache before rendering
        self.preload_images(&images, width as f32, height as f32);

        // Prepare text glyphs with z_layer information
        let mut glyphs_by_layer: std::collections::BTreeMap<u32, Vec<GpuGlyph>> =
            std::collections::BTreeMap::new();
        for text in &texts {
            let alignment = match text.align {
                TextAlign::Left => TextAlignment::Left,
                TextAlign::Center => TextAlignment::Center,
                TextAlign::Right => TextAlignment::Right,
            };

            // Apply motion opacity to text color
            let color = if text.motion_opacity < 1.0 {
                [
                    text.color[0],
                    text.color[1],
                    text.color[2],
                    text.color[3] * text.motion_opacity,
                ]
            } else {
                text.color
            };

            // Determine wrap width
            let effective_width = if let Some(clip) = text.clip_bounds {
                clip[2].min(text.width)
            } else {
                text.width
            };

            let needs_wrap = text.wrap && effective_width < text.measured_width - 2.0;
            let wrap_width = Some(text.width);
            let font_name = text.font_family.name.as_deref();
            let generic = to_gpu_generic_font(text.font_family.generic);
            let font_weight = text.weight.weight();

            let (anchor, y_pos, use_layout_height) = match text.v_align {
                TextVerticalAlign::Center => {
                    (TextAnchor::Center, text.y + text.height / 2.0, false)
                }
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    let baseline_y = text.y + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };
            let layout_height = if use_layout_height {
                Some(text.height)
            } else {
                None
            };

            if let Ok(glyphs) = self.text_ctx.prepare_text_with_style(
                &text.content,
                text.x,
                y_pos,
                text.font_size,
                color,
                anchor,
                alignment,
                wrap_width,
                needs_wrap,
                font_name,
                generic,
                font_weight,
                text.italic,
                layout_height,
            ) {
                let mut glyphs = glyphs;
                if let Some(clip) = text.clip_bounds {
                    for glyph in &mut glyphs {
                        glyph.clip_bounds = clip;
                    }
                }
                glyphs_by_layer
                    .entry(text.z_index)
                    .or_default()
                    .extend(glyphs);
            }
        }

        // SVGs are rendered as rasterized images (not tessellated paths) for better anti-aliasing
        // They will be rendered later via render_rasterized_svgs

        self.renderer.resize(width, height);

        // For overlay rendering, we DON'T have glass effects (overlays are simple)
        // Render primitives without clearing (LoadOp::Load)
        let max_z = batch.max_z_layer();
        let max_text_z = glyphs_by_layer.keys().cloned().max().unwrap_or(0);
        let max_layer = max_z.max(max_text_z);

        tracing::debug!(
            "render_overlay_tree: {} primitives, {} text layers, max_layer={}",
            batch.primitives.len(),
            glyphs_by_layer.len(),
            max_layer
        );

        // Render all layers using overlay mode (no clear)
        for z in 0..=max_layer {
            let layer_primitives = batch.primitives_for_layer(z);
            if !layer_primitives.is_empty() {
                tracing::debug!(
                    "render_overlay_tree: rendering {} primitives at z={}",
                    layer_primitives.len(),
                    z
                );
                self.renderer
                    .render_primitives_overlay(target, &layer_primitives);
            }

            if let Some(glyphs) = glyphs_by_layer.get(&z) {
                if !glyphs.is_empty() {
                    tracing::debug!(
                        "render_overlay_tree: rendering {} glyphs at z={}",
                        glyphs.len(),
                        z
                    );
                    self.render_text(target, glyphs);
                }
            }
        }

        // Images render on top
        self.render_images(target, &images, width as f32, height as f32);

        // Render foreground primitives on top of images (for .foreground() elements)
        if !batch.foreground_primitives.is_empty() {
            self.renderer
                .render_primitives_overlay(target, &batch.foreground_primitives);
        }

        // Poll the device to free completed command buffers
        self.renderer.poll();

        // Render layout debug for overlay tree if enabled
        let debug = DebugMode::from_env();
        if debug.layout {
            let scale = tree.scale_factor();
            self.render_layout_debug(target, tree, scale);
        }
        if debug.motion {
            self.render_motion_debug(target, tree, width, height);
        }

        // Return scratch buffers for reuse on next frame
        self.return_scratch_elements(texts, svgs, images);

        Ok(())
    }

    /// Render overlays from RenderState (cursors, selections, focus rings)
    fn render_overlays(
        &mut self,
        render_state: &blinc_layout::RenderState,
        width: u32,
        height: u32,
        target: &wgpu::TextureView,
    ) {
        let overlays = render_state.overlays();
        if overlays.is_empty() {
            return;
        }

        // Create a paint context for overlays
        let mut overlay_ctx = GpuPaintContext::new(width as f32, height as f32);

        for overlay in overlays {
            match overlay {
                Overlay::Cursor {
                    position,
                    size,
                    color,
                    opacity,
                } => {
                    if *opacity > 0.0 {
                        // Apply opacity to cursor color
                        let cursor_color =
                            Color::rgba(color.r, color.g, color.b, color.a * opacity);
                        overlay_ctx.execute_command(&DrawCommand::FillRect {
                            rect: Rect::new(position.0, position.1, size.0, size.1),
                            corner_radius: CornerRadius::default(),
                            brush: Brush::Solid(cursor_color),
                        });
                    }
                }
                Overlay::Selection { rects: _, color: _ } => {
                    // TODO: Re-enable for real-time text selection
                    // Disabled for now to avoid blue mask issue after modal close
                }
                Overlay::FocusRing {
                    position,
                    size,
                    radius,
                    color,
                    thickness,
                } => {
                    overlay_ctx.execute_command(&DrawCommand::StrokeRect {
                        rect: Rect::new(position.0, position.1, size.0, size.1),
                        corner_radius: CornerRadius::uniform(*radius),
                        stroke: Stroke::new(*thickness),
                        brush: Brush::Solid(*color),
                    });
                }
            }
        }

        // Render overlays as an overlay pass (on top of existing content)
        let overlay_batch = overlay_ctx.take_batch();
        if !overlay_batch.is_empty() {
            self.renderer.render_overlay(target, &overlay_batch);
        }
    }
}

/// Convert layout's GenericFont to GPU's GenericFont
fn to_gpu_generic_font(generic: GenericFont) -> GpuGenericFont {
    match generic {
        GenericFont::System => GpuGenericFont::System,
        GenericFont::Monospace => GpuGenericFont::Monospace,
        GenericFont::Serif => GpuGenericFont::Serif,
        GenericFont::SansSerif => GpuGenericFont::SansSerif,
    }
}

/// Debug mode flags for visual debugging
///
/// Set environment variable `BLINC_DEBUG` to enable debug visualization:
/// - `text`: Show text bounding boxes and baselines
/// - `layout`: Show all element bounding boxes (useful for debugging hit-testing)
/// - `motion`: Show active animation stats overlay
/// - `all` or `1` or `true`: Show all debug visualizations
#[derive(Clone, Copy)]
pub struct DebugMode {
    /// Show text bounding boxes and baseline indicators
    pub text: bool,
    /// Show all element bounding boxes
    pub layout: bool,
    /// Show motion/animation debug info
    pub motion: bool,
}

impl DebugMode {
    /// Check environment variable and return debug mode configuration
    pub fn from_env() -> Self {
        let debug_value = std::env::var("BLINC_DEBUG")
            .map(|v| v.to_lowercase())
            .unwrap_or_default();

        let all = debug_value == "all" || debug_value == "1" || debug_value == "true";
        let text = all || debug_value == "text";
        let layout = all || debug_value == "layout";
        let motion = all || debug_value == "motion";

        Self {
            text,
            layout,
            motion,
        }
    }

    /// Check if any debug mode is enabled
    pub fn any_enabled(&self) -> bool {
        self.text || self.layout || self.motion
    }
}

/// Generate text decoration primitives (strikethrough and underline) grouped by z-layer
///
/// Creates decoration lines for text elements that have:
/// - strikethrough: horizontal line through the middle of the text
/// - underline: horizontal line below the text baseline
///
/// Returns a HashMap of z_index -> primitives for interleaved rendering with text
fn generate_text_decoration_primitives_by_layer(
    texts: &[TextElement],
) -> std::collections::HashMap<u32, Vec<GpuPrimitive>> {
    let mut primitives_by_layer: std::collections::HashMap<u32, Vec<GpuPrimitive>> =
        std::collections::HashMap::new();

    for text in texts {
        if !text.strikethrough && !text.underline {
            continue;
        }

        // Calculate text width for decorations
        let decoration_width = if text.wrap && text.measured_width > text.width {
            text.width
        } else {
            text.measured_width.min(text.width)
        };

        // Skip if there's no meaningful width
        if decoration_width <= 0.0 {
            continue;
        }

        // Line thickness scales with font size (roughly 1/14th of font size, minimum 1px)
        let line_thickness = (text.font_size / 14.0).max(1.0).min(3.0);

        let layer_primitives = primitives_by_layer.entry(text.z_index).or_default();

        // Calculate the actual baseline Y position based on vertical alignment
        // This must match the text rendering logic to position decorations correctly
        //
        // glyph_extent = ascender - descender (where descender is negative)
        // Typical descender is about -20% of ascender, so glyph_extent ≈ ascender * 1.2
        let descender_approx = -text.ascender * 0.2;
        let glyph_extent = text.ascender - descender_approx;

        let baseline_y = match text.v_align {
            TextVerticalAlign::Center => {
                // GPU: y_pos = text.y + text.height / 2.0, then y_offset = y_pos - glyph_extent / 2.0
                // Glyph top is at: text.y + text.height/2 - glyph_extent/2
                // Baseline is at: glyph_top + ascender
                let glyph_top = text.y + text.height / 2.0 - glyph_extent / 2.0;
                glyph_top + text.ascender
            }
            TextVerticalAlign::Top => {
                // GPU: y_pos = text.y, y_offset = y + (layout_height - glyph_extent) / 2.0
                // Glyph top is at: text.y + (text.height - glyph_extent) / 2.0
                // Baseline is at: glyph_top + ascender
                let glyph_top = text.y + (text.height - glyph_extent) / 2.0;
                glyph_top + text.ascender
            }
            TextVerticalAlign::Baseline => {
                // GPU: y_pos = text.y + ascender, y_offset = y_pos - ascender = text.y
                // Glyph top is at: text.y
                // Baseline is at: text.y + ascender
                text.y + text.ascender
            }
        };

        // Strikethrough: draw line through the center of lowercase letters (x-height center)
        if text.strikethrough {
            // x-height is typically ~50% of ascender, center of x-height is ~25% above baseline
            let strikethrough_y = baseline_y - text.ascender * 0.35;
            let mut strike_rect = GpuPrimitive::rect(
                text.x,
                strikethrough_y - line_thickness / 2.0,
                decoration_width,
                line_thickness,
            )
            .with_color(text.color[0], text.color[1], text.color[2], text.color[3]);

            // Apply clip bounds from text element if present
            if let Some(clip) = text.clip_bounds {
                strike_rect = strike_rect.with_clip_rect(clip[0], clip[1], clip[2], clip[3]);
            }
            layer_primitives.push(strike_rect);
        }

        // Underline: draw line just below the baseline (at text bottom)
        if text.underline {
            // Underline position: just below baseline, snapping to text bottom
            let underline_y = baseline_y + text.ascender * 0.05;
            let mut underline_rect = GpuPrimitive::rect(
                text.x,
                underline_y - line_thickness / 2.0,
                decoration_width,
                line_thickness,
            )
            .with_color(text.color[0], text.color[1], text.color[2], text.color[3]);

            // Apply clip bounds from text element if present
            if let Some(clip) = text.clip_bounds {
                underline_rect = underline_rect.with_clip_rect(clip[0], clip[1], clip[2], clip[3]);
            }
            layer_primitives.push(underline_rect);
        }
    }

    primitives_by_layer
}

/// Generate debug primitives for text elements
///
/// Creates visual overlays showing:
/// - Bounding box outline (cyan)
/// - Baseline position (magenta line)
/// - Ascender line (green, at top of bounding box)
/// - Descender line (yellow, at bottom of bounding box)
fn generate_text_debug_primitives(texts: &[TextElement]) -> Vec<GpuPrimitive> {
    let mut primitives = Vec::new();

    for text in texts {
        // Determine the actual text width for debug visualization:
        // - For non-wrapped text: use measured_width (actual rendered text width)
        // - For wrapped text: use layout width (container constrains the text)
        let debug_width = if text.wrap && text.measured_width > text.width {
            // Text is wrapping - use container width
            text.width
        } else {
            // Single line - use actual measured width (clamped to layout width)
            text.measured_width.min(text.width)
        };

        // Bounding box outline (cyan, semi-transparent)
        let bbox = GpuPrimitive::rect(text.x, text.y, debug_width, text.height)
            .with_color(0.0, 0.0, 0.0, 0.0) // Transparent fill
            .with_border(1.0, 0.0, 1.0, 1.0, 0.7); // Cyan border
        primitives.push(bbox);

        // Baseline indicator (magenta horizontal line)
        // The baseline is at y + ascender
        let baseline_y = text.y + text.ascender;
        let baseline = GpuPrimitive::rect(text.x, baseline_y - 0.5, debug_width, 1.0)
            .with_color(1.0, 0.0, 1.0, 0.6); // Magenta
        primitives.push(baseline);

        // Ascender line indicator (green, at top of text)
        // For v_baseline texts, this shows where the ascender sits
        let ascender_line = GpuPrimitive::rect(text.x, text.y - 0.5, debug_width, 1.0)
            .with_color(0.0, 1.0, 0.0, 0.4); // Green, more transparent
        primitives.push(ascender_line);

        // Descender line (yellow, at bottom of bounding box)
        let descender_y = text.y + text.height;
        let descender_line = GpuPrimitive::rect(text.x, descender_y - 0.5, debug_width, 1.0)
            .with_color(1.0, 1.0, 0.0, 0.4); // Yellow
        primitives.push(descender_line);
    }

    primitives
}

/// Collect all element bounds from the render tree for debug visualization
fn collect_debug_bounds(tree: &RenderTree, scale: f32) -> Vec<DebugBoundsElement> {
    let mut bounds = Vec::new();

    if let Some(root) = tree.root() {
        collect_debug_bounds_recursive(tree, root, (0.0, 0.0), 0, scale, &mut bounds);
    }

    bounds
}

/// Recursively collect bounds from all nodes
fn collect_debug_bounds_recursive(
    tree: &RenderTree,
    node: LayoutNodeId,
    parent_offset: (f32, f32),
    depth: u32,
    scale: f32,
    bounds: &mut Vec<DebugBoundsElement>,
) {
    use blinc_layout::renderer::ElementType;

    let Some(node_bounds) = tree.layout().get_bounds(node, parent_offset) else {
        return;
    };

    // Determine element type name
    let element_type = tree
        .get_render_node(node)
        .map(|n| match &n.element_type {
            ElementType::Div => "Div".to_string(),
            ElementType::Text(_) => "Text".to_string(),
            ElementType::StyledText(_) => "StyledText".to_string(),
            ElementType::Image(_) => "Image".to_string(),
            ElementType::Svg(_) => "Svg".to_string(),
            ElementType::Canvas(_) => "Canvas".to_string(),
        })
        .unwrap_or_else(|| "Unknown".to_string());

    // Add this element's bounds (with DPI scaling)
    bounds.push(DebugBoundsElement {
        x: node_bounds.x * scale,
        y: node_bounds.y * scale,
        width: node_bounds.width * scale,
        height: node_bounds.height * scale,
        element_type,
        depth,
    });

    // Get scroll offset for this node (scroll containers offset their children)
    let scroll_offset = tree.get_scroll_offset(node);

    // Calculate new offset for children (including scroll offset)
    let new_offset = (
        node_bounds.x + scroll_offset.0,
        node_bounds.y + scroll_offset.1,
    );

    // Recurse into children
    for child in tree.layout().children(node) {
        collect_debug_bounds_recursive(tree, child, new_offset, depth + 1, scale, bounds);
    }
}

/// Generate debug primitives for layout element bounds
///
/// Creates visual overlays showing:
/// - Colored outlines for each element's bounding box
/// - Colors cycle based on tree depth (red, green, blue, yellow, cyan, magenta)
fn generate_layout_debug_primitives(bounds: &[DebugBoundsElement]) -> Vec<GpuPrimitive> {
    let mut primitives = Vec::new();

    // Color palette for different depths (cycling)
    let colors: [(f32, f32, f32); 6] = [
        (1.0, 0.3, 0.3), // Red
        (0.3, 1.0, 0.3), // Green
        (0.3, 0.3, 1.0), // Blue
        (1.0, 1.0, 0.3), // Yellow
        (0.3, 1.0, 1.0), // Cyan
        (1.0, 0.3, 1.0), // Magenta
    ];

    for elem in bounds {
        // Skip very small elements (likely invisible)
        if elem.width < 1.0 || elem.height < 1.0 {
            continue;
        }

        let (r, g, b) = colors[(elem.depth as usize) % colors.len()];
        let alpha = 0.5; // Semi-transparent outline

        // Draw outline only (transparent fill with colored border)
        let rect = GpuPrimitive::rect(elem.x, elem.y, elem.width, elem.height)
            .with_color(0.0, 0.0, 0.0, 0.0) // Transparent fill
            .with_border(1.0, r, g, b, alpha); // Colored border

        primitives.push(rect);
    }

    primitives
}

/// Scale and translate a path for SVG rendering with tint
fn scale_and_translate_path(
    path: &blinc_core::Path,
    x: f32,
    y: f32,
    scale: f32,
) -> blinc_core::Path {
    use blinc_core::{PathCommand, Point, Vec2};

    if scale == 1.0 && x == 0.0 && y == 0.0 {
        return path.clone();
    }

    let transform_point = |p: Point| -> Point { Point::new(p.x * scale + x, p.y * scale + y) };

    let new_commands: Vec<PathCommand> = path
        .commands()
        .iter()
        .map(|cmd| match cmd {
            PathCommand::MoveTo(p) => PathCommand::MoveTo(transform_point(*p)),
            PathCommand::LineTo(p) => PathCommand::LineTo(transform_point(*p)),
            PathCommand::QuadTo { control, end } => PathCommand::QuadTo {
                control: transform_point(*control),
                end: transform_point(*end),
            },
            PathCommand::CubicTo {
                control1,
                control2,
                end,
            } => PathCommand::CubicTo {
                control1: transform_point(*control1),
                control2: transform_point(*control2),
                end: transform_point(*end),
            },
            PathCommand::ArcTo {
                radii,
                rotation,
                large_arc,
                sweep,
                end,
            } => PathCommand::ArcTo {
                radii: Vec2::new(radii.x * scale, radii.y * scale),
                rotation: *rotation,
                large_arc: *large_arc,
                sweep: *sweep,
                end: transform_point(*end),
            },
            PathCommand::Close => PathCommand::Close,
        })
        .collect();

    blinc_core::Path::from_commands(new_commands)
}
