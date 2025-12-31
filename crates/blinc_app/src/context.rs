//! Render context for blinc_app
//!
//! Wraps the GPU rendering pipeline with a clean API.

use blinc_core::{Brush, Color, CornerRadius, DrawCommand, Rect, Stroke};
use blinc_gpu::{
    FontRegistry, GenericFont as GpuGenericFont, GpuGlyph, GpuImage, GpuImageInstance,
    GpuPaintContext, GpuPrimitive, GpuRenderer, ImageRenderingContext, PrimitiveBatch,
    TextAlignment, TextAnchor, TextRenderingContext,
};
use blinc_layout::div::{FontFamily, FontWeight, GenericFont, TextAlign, TextVerticalAlign};
use blinc_layout::prelude::*;
use blinc_layout::render_state::Overlay;
use blinc_layout::renderer::ElementType;
use blinc_svg::SvgDocument;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use crate::error::Result;

/// Maximum number of images to keep in cache (prevents unbounded memory growth)
const IMAGE_CACHE_CAPACITY: usize = 128;

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
    // Scratch buffers for per-frame allocations (reused to avoid allocations)
    scratch_glyphs: Vec<GpuGlyph>,
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
            scratch_glyphs: Vec::with_capacity(1024), // Pre-allocate for typical text
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
        let fg_batch = fg_ctx.take_batch();

        // Collect text, SVG, and image elements
        let (texts, svgs, images) = self.collect_render_elements(tree);

        // Pre-load all images into cache before rendering
        self.preload_images(&images);

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
                TextVerticalAlign::Center => (TextAnchor::Center, text.y + text.height / 2.0, false),
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
            let layout_height = if use_layout_height { Some(text.height) } else { None };

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

        // Render SVGs to a new foreground context (svg_ctx)
        // We need a fresh context since fg_ctx's batch was already taken
        let mut svg_ctx = GpuPaintContext::new(width as f32, height as f32);
        for (source, x, y, w, h) in &svgs {
            if let Ok(doc) = SvgDocument::from_str(source) {
                doc.render_fit(&mut svg_ctx, Rect::new(*x, *y, *w, *h));
            }
        }

        // Merge SVG batch into foreground batch
        let mut fg_batch = fg_batch;
        fg_batch.merge(svg_ctx.take_batch());

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

            // Step 4: Render background-layer images to target (separate for now - images use different pipeline)
            self.render_images_ref(target, &bg_images);

            // Step 5: Render glass/foreground-layer images (on top of glass, NOT blurred)
            self.render_images_ref(target, &fg_images);

            // Step 6: Render foreground primitives with MSAA for smooth SVG edges
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

            // Step 8: Render text decorations (strikethrough, underline)
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);
            for primitives in decorations_by_layer.values() {
                if !primitives.is_empty() {
                    self.renderer.render_primitives_overlay(target, primitives);
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

            // Render images after background primitives
            self.render_images(target, &images);

            // Render foreground with MSAA for smooth SVG edges
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

            // Render text decorations (strikethrough, underline)
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);
            for primitives in decorations_by_layer.values() {
                if !primitives.is_empty() {
                    self.renderer.render_primitives_overlay(target, primitives);
                }
            }
        }

        // Poll the device to free completed command buffers and prevent memory accumulation
        self.renderer.poll();

        Ok(())
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
    fn preload_images(&mut self, images: &[ImageElement]) {
        for image in images {
            // LruCache::contains also promotes to most-recently-used
            if self.image_cache.contains(&image.source) {
                continue;
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
    fn render_images(&mut self, target: &wgpu::TextureView, images: &[ImageElement]) {
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

    /// Collect text, SVG, and image elements from the render tree
    fn collect_render_elements(
        &self,
        tree: &RenderTree,
    ) -> (
        Vec<TextElement>,
        Vec<(String, f32, f32, f32, f32)>,
        Vec<ImageElement>,
    ) {
        self.collect_render_elements_with_state(tree, None)
    }

    /// Collect text, SVG, and image elements with motion state
    fn collect_render_elements_with_state(
        &self,
        tree: &RenderTree,
        render_state: Option<&blinc_layout::RenderState>,
    ) -> (
        Vec<TextElement>,
        Vec<(String, f32, f32, f32, f32)>,
        Vec<ImageElement>,
    ) {
        let mut texts = Vec::new();
        let mut svgs = Vec::new();
        let mut images = Vec::new();

        // Get the scale factor from the tree for DPI scaling
        let scale = tree.scale_factor();

        if let Some(root) = tree.root() {
            let mut z_layer = 0u32;
            self.collect_elements_recursive(
                tree,
                root,
                (0.0, 0.0),
                false,
                None, // No initial clip
                1.0,  // Initial motion opacity
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

    fn collect_elements_recursive(
        &self,
        tree: &RenderTree,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        inside_glass: bool,
        current_clip: Option<[f32; 4]>,
        inherited_motion_opacity: f32,
        render_state: Option<&blinc_layout::RenderState>,
        scale: f32,
        z_layer: &mut u32,
        texts: &mut Vec<TextElement>,
        svgs: &mut Vec<(String, f32, f32, f32, f32)>,
        images: &mut Vec<ImageElement>,
    ) {
        use blinc_layout::Material;

        let Some(bounds) = tree.layout().get_bounds(node, parent_offset) else {
            return;
        };

        let abs_x = bounds.x;
        let abs_y = bounds.y;

        // Calculate motion opacity for this node (from RenderState)
        let node_motion_opacity = render_state
            .and_then(|rs| rs.get_motion_values(node))
            .and_then(|m| m.opacity)
            .unwrap_or(1.0);

        // Combine with inherited opacity
        let effective_motion_opacity = inherited_motion_opacity * node_motion_opacity;

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

        // Check if this is a Stack layer - if so, increment z_layer for proper z-ordering
        let is_stack_layer = tree
            .get_render_node(node)
            .map(|n| n.props.is_stack_layer)
            .unwrap_or(false);
        if is_stack_layer {
            *z_layer += 1;
        }

        // Update clip bounds for children if this node clips
        // When a node clips, we INTERSECT its bounds with any existing clip
        // This ensures nested clipping works correctly (inner clips can't expand outer clips)
        let child_clip = if clips_content {
            let this_clip = [abs_x, abs_y, bounds.width, bounds.height];
            if let Some(parent_clip) = current_clip {
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
            }
        } else {
            current_clip
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
                    // Apply DPI scale factor to positions and sizes
                    let scaled_x = abs_x * scale;
                    let scaled_y = abs_y * scale;
                    let scaled_width = bounds.width * scale;
                    let scaled_height = bounds.height * scale;
                    let scaled_font_size = text_data.font_size * scale;
                    let scaled_measured_width = text_data.measured_width * scale;

                    // Scale clip bounds if present
                    let scaled_clip = current_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

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
                        ascender: text_data.ascender * scale,
                        strikethrough: text_data.strikethrough,
                        underline: text_data.underline,
                    });
                }
                ElementType::Svg(svg_data) => {
                    // Apply DPI scale factor to SVG positions and sizes
                    svgs.push((
                        svg_data.source.clone(),
                        abs_x * scale,
                        abs_y * scale,
                        bounds.width * scale,
                        bounds.height * scale,
                    ));
                }
                ElementType::Image(image_data) => {
                    // Apply DPI scale factor to image positions and sizes
                    let scaled_clip = current_clip
                        .map(|[cx, cy, cw, ch]| [cx * scale, cy * scale, cw * scale, ch * scale]);

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
                        clip_radius: [0.0; 4],
                        layer: effective_layer,
                    });
                }
                // Canvas elements are rendered inline during tree traversal (in render_layer)
                ElementType::Canvas(_) => {}
                ElementType::Div => {}
                // StyledText is a future optimization - currently handled as multiple Text elements
                ElementType::StyledText(_) => {}
            }
        }

        // Include scroll offset and motion offset when calculating child positions
        let scroll_offset = tree.get_scroll_offset(node);
        let motion_offset = tree
            .get_motion_transform(node)
            .map(|t| match t {
                blinc_core::Transform::Affine2D(a) => (a.elements[4], a.elements[5]),
                _ => (0.0, 0.0),
            })
            .unwrap_or((0.0, 0.0));
        let new_offset = (
            abs_x + scroll_offset.0 + motion_offset.0,
            abs_y + scroll_offset.1 + motion_offset.1,
        );
        for child_id in tree.layout().children(node) {
            self.collect_elements_recursive(
                tree,
                child_id,
                new_offset,
                children_inside_glass,
                child_clip,
                effective_motion_opacity,
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
        self.preload_images(&images);

        // Prepare text glyphs with z_layer information
        // Store (z_layer, glyphs) to enable interleaved rendering
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
                TextVerticalAlign::Center => (TextAnchor::Center, text.y + text.height / 2.0, false),
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    let baseline_y = text.y + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };
            let layout_height = if use_layout_height { Some(text.height) } else { None };

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

        // Render SVGs
        let mut svg_ctx = GpuPaintContext::new(width as f32, height as f32);
        for (source, x, y, w, h) in &svgs {
            if let Ok(doc) = SvgDocument::from_str(source) {
                doc.render_fit(&mut svg_ctx, Rect::new(*x, *y, *w, *h));
            }
        }

        // Merge SVG batch into main batch
        let mut batch = batch;
        batch.merge(svg_ctx.take_batch());

        self.renderer.resize(width, height);

        let has_glass = batch.glass_count() > 0;

        // Only allocate glass textures when glass is actually used
        if has_glass {
            self.ensure_glass_textures(width, height);
        }
        let use_msaa_overlay = self.sample_count > 1;

        if has_glass {
            // Glass path
            let (bg_images, fg_images): (Vec<_>, Vec<_>) = images
                .iter()
                .partition(|img| img.layer == RenderLayer::Background);

            {
                let backdrop = self.backdrop_texture.as_ref().unwrap();
                self.renderer.render_glass_frame(
                    target,
                    &backdrop.view,
                    (backdrop.width, backdrop.height),
                    &batch,
                );
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
        } else {
            // Simple path (no glass)
            // Pre-generate text decorations grouped by layer for interleaved rendering
            let decorations_by_layer = generate_text_decoration_primitives_by_layer(&texts);

            let max_z = batch.max_z_layer();
            let max_text_z = glyphs_by_layer.keys().cloned().max().unwrap_or(0);
            let max_decoration_z = decorations_by_layer.keys().cloned().max().unwrap_or(0);
            let max_layer = max_z.max(max_text_z).max(max_decoration_z);

            if max_layer > 0 {
                // Interleaved z-layer rendering for proper Stack z-ordering
                // First pass: render z_layer=0 primitives with clear
                let z0_primitives = batch.primitives_for_layer(0);
                if !z0_primitives.is_empty() {
                    // Create a temporary batch for z=0
                    let mut z0_batch = PrimitiveBatch::new();
                    z0_batch.primitives = z0_primitives;
                    self.renderer
                        .render_with_clear(target, &z0_batch, [0.0, 0.0, 0.0, 1.0]);
                } else {
                    // Still need to clear even if no z=0 primitives
                    let empty_batch = PrimitiveBatch::new();
                    self.renderer
                        .render_with_clear(target, &empty_batch, [0.0, 0.0, 0.0, 1.0]);
                }

                // Render z=0 text and decorations
                if let Some(glyphs) = glyphs_by_layer.get(&0) {
                    if !glyphs.is_empty() {
                        self.render_text(target, glyphs);
                    }
                }
                self.render_text_decorations_for_layer(target, &decorations_by_layer, 0);

                // Render subsequent layers interleaved
                for z in 1..=max_layer {
                    // Render primitives for this layer
                    let layer_primitives = batch.primitives_for_layer(z);
                    if !layer_primitives.is_empty() {
                        self.renderer
                            .render_primitives_overlay(target, &layer_primitives);
                    }

                    // Render text for this layer
                    if let Some(glyphs) = glyphs_by_layer.get(&z) {
                        if !glyphs.is_empty() {
                            self.render_text(target, glyphs);
                        }
                    }

                    // Render text decorations for this layer
                    self.render_text_decorations_for_layer(target, &decorations_by_layer, z);
                }

                // Images render on top (existing behavior)
                self.render_images(target, &images);
            } else {
                // No z-layers, use original fast path
                self.renderer
                    .render_with_clear(target, &batch, [0.0, 0.0, 0.0, 1.0]);

                self.render_images(target, &images);

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

        // Render debug visualization if enabled (BLINC_DEBUG=text)
        let debug = DebugMode::from_env();
        if debug.text {
            self.render_text_debug(target, &texts);
        }

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
        self.preload_images(&images);

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
                TextVerticalAlign::Center => (TextAnchor::Center, text.y + text.height / 2.0, false),
                TextVerticalAlign::Top => (TextAnchor::Top, text.y, true),
                TextVerticalAlign::Baseline => {
                    let baseline_y = text.y + text.ascender;
                    (TextAnchor::Baseline, baseline_y, false)
                }
            };
            let layout_height = if use_layout_height { Some(text.height) } else { None };

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

        // Render SVGs
        let mut svg_ctx = GpuPaintContext::new(width as f32, height as f32);
        for (source, x, y, w, h) in &svgs {
            if let Ok(doc) = SvgDocument::from_str(source) {
                doc.render_fit(&mut svg_ctx, Rect::new(*x, *y, *w, *h));
            }
        }

        // Merge SVG batch into main batch
        let mut batch = batch;
        batch.merge(svg_ctx.take_batch());

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
        self.render_images(target, &images);

        // Poll the device to free completed command buffers
        self.renderer.poll();

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
                Overlay::Selection { rects, color } => {
                    for (x, y, w, h) in rects {
                        overlay_ctx.execute_command(&DrawCommand::FillRect {
                            rect: Rect::new(*x, *y, *w, *h),
                            corner_radius: CornerRadius::default(),
                            brush: Brush::Solid(*color),
                        });
                    }
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
/// - `text` or `1`: Show text bounding boxes and baselines
/// - `all`: Show all debug visualizations
#[derive(Clone, Copy)]
pub struct DebugMode {
    /// Show text bounding boxes and baseline indicators
    pub text: bool,
}

impl DebugMode {
    /// Check environment variable and return debug mode configuration
    pub fn from_env() -> Self {
        let debug_text = std::env::var("BLINC_DEBUG")
            .map(|v| {
                let v = v.to_lowercase();
                v == "1" || v == "text" || v == "all" || v == "true"
            })
            .unwrap_or(false);

        Self { text: debug_text }
    }

    /// Check if any debug mode is enabled
    pub fn any_enabled(&self) -> bool {
        self.text
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
