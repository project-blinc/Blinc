//! Render context for blinc_app
//!
//! Wraps the GPU rendering pipeline with a clean API.

use blinc_core::Rect;
use blinc_gpu::{
    GpuGlyph, GpuImage, GpuImageInstance, GpuPaintContext, GpuRenderer, ImageRenderingContext,
    TextAlignment, TextAnchor, TextRenderingContext,
};
use blinc_layout::div::{FontWeight, TextAlign, TextVerticalAlign};
use blinc_layout::prelude::*;
use blinc_layout::renderer::ElementType;
use blinc_svg::SvgDocument;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::Result;

/// Internal render context that manages GPU resources and rendering
pub struct RenderContext {
    renderer: GpuRenderer,
    text_ctx: TextRenderingContext,
    image_ctx: ImageRenderingContext,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    sample_count: u32,
    // Cached textures for glass rendering
    pre_glass_texture: Option<CachedTexture>,
    backdrop_texture: Option<CachedTexture>,
    // Cached MSAA texture for anti-aliased rendering
    msaa_texture: Option<CachedTexture>,
    // Cached images by source
    image_cache: HashMap<String, GpuImage>,
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
    /// Vertical alignment within bounding box
    v_align: TextVerticalAlign,
    /// Clip bounds from parent scroll container (x, y, width, height)
    clip_bounds: Option<[f32; 4]>,
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
            pre_glass_texture: None,
            backdrop_texture: None,
            msaa_texture: None,
            image_cache: HashMap::new(),
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
        // Create paint contexts for each layer
        let mut bg_ctx = GpuPaintContext::new(width as f32, height as f32);
        let mut fg_ctx = GpuPaintContext::new(width as f32, height as f32);

        // Render layout layers
        tree.render_to_layer(&mut bg_ctx, RenderLayer::Background);
        tree.render_to_layer(&mut bg_ctx, RenderLayer::Glass);
        tree.render_to_layer(&mut fg_ctx, RenderLayer::Foreground);

        // Collect text, SVG, and image elements
        let (texts, svgs, images) = self.collect_render_elements(tree);

        // DEBUG: Uncomment to draw text element bounds for debugging layout issues
        // #[cfg(debug_assertions)]
        // {
        //     use blinc_core::{Brush, Color, CornerRadius, DrawCommand, Stroke};
        //     for text in &texts {
        //         // Red rectangle: text element bounds from Taffy
        //         bg_ctx.execute_command(&DrawCommand::StrokeRect {
        //             rect: Rect::new(text.x, text.y, text.width, text.height),
        //             corner_radius: CornerRadius::default(),
        //             stroke: Stroke::new(1.0),
        //             brush: Brush::Solid(Color::rgba(1.0, 0.0, 0.0, 0.8)),
        //         });
        //         // Blue dot: center point for vertical centering
        //         let cx = text.x + text.width / 2.0;
        //         let cy = text.y + text.height / 2.0;
        //         bg_ctx.execute_command(&DrawCommand::FillRect {
        //             rect: Rect::new(cx - 3.0, cy - 3.0, 6.0, 6.0),
        //             corner_radius: CornerRadius::default(),
        //             brush: Brush::Solid(Color::rgba(0.0, 0.0, 1.0, 0.9)),
        //         });
        //         // Green dot: left edge marker
        //         bg_ctx.execute_command(&DrawCommand::FillRect {
        //             rect: Rect::new(text.x - 2.0, cy - 2.0, 4.0, 4.0),
        //             corner_radius: CornerRadius::default(),
        //             brush: Brush::Solid(Color::rgba(0.0, 1.0, 0.0, 0.9)),
        //         });
        //     }
        // }

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

            // Pass width for alignment (center/right) but wrapping is disabled by default
            // in prepare_text_with_options (it uses LineBreakMode::None internally)
            //
            // Vertical alignment:
            // - Center: Use TextAnchor::Center with y at vertical center of bounds.
            //   This ensures text appears visually centered (by cap-height) rather than
            //   mathematically centered by the full bounding box (which includes descenders).
            // - Top: Use TextAnchor::Top with y at top of bounds. Used for multi-line text
            //   like text areas where content flows from top.
            let (anchor, y_pos) = match text.v_align {
                TextVerticalAlign::Center => (TextAnchor::Center, text.y + text.height / 2.0),
                TextVerticalAlign::Top => (TextAnchor::Top, text.y),
            };
            if let Ok(mut glyphs) = self.text_ctx.prepare_text_with_options(
                &text.content,
                text.x,
                y_pos,
                text.font_size,
                text.color,
                anchor,
                alignment,
                Some(text.width),  // Width for alignment (wrapping disabled internally)
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

        // Render SVGs to foreground context
        for (source, x, y, w, h) in &svgs {
            if let Ok(doc) = SvgDocument::from_str(source) {
                doc.render_fit(&mut fg_ctx, Rect::new(*x, *y, *w, *h));
            }
        }

        // Take batches
        let bg_batch = bg_ctx.take_batch();
        let fg_batch = fg_ctx.take_batch();

        self.renderer.resize(width, height);
        self.ensure_textures(width, height);

        let has_glass = bg_batch.glass_count() > 0;
        let use_msaa_overlay = self.sample_count > 1;

        // Background layer uses SDF rendering (shader-based AA, no MSAA needed)
        // Foreground layer (SVGs as tessellated paths) uses MSAA for smooth edges

        if has_glass {
            // Split images by layer: background images go behind glass (get blurred),
            // glass/foreground images render on top of glass (not blurred)
            let (bg_images, fg_images): (Vec<_>, Vec<_>) = images
                .iter()
                .partition(|img| img.layer == RenderLayer::Background);

            // Glass path:
            // 1. Render background primitives to pre-glass texture
            // 2. Render background-layer images to pre-glass texture (will be blurred by glass)
            // 3. Copy to backdrop for glass sampling
            // 4. Render background primitives to target
            // 5. Render background-layer images to target
            // 6. Render glass with backdrop blur onto target
            // 7. Render glass/foreground-layer images (on top of glass, not blurred)
            // 8. Render foreground primitives (with MSAA if enabled)
            // 9. Render text

            // Step 1: Render background primitives to pre-glass texture
            {
                let pre_glass_view = &self.pre_glass_texture.as_ref().unwrap().view;
                self.renderer
                    .render_with_clear(pre_glass_view, &bg_batch, [0.0, 0.0, 0.0, 0.0]);
            }

            // Step 2: Render background-layer images to pre-glass texture (so glass can blur them)
            self.render_images_to_pre_glass(&bg_images);

            // Step 3: Copy pre-glass to backdrop for glass sampling
            {
                let pre_glass_tex = &self.pre_glass_texture.as_ref().unwrap().texture;
                let backdrop_tex = &self.backdrop_texture.as_ref().unwrap().texture;
                self.copy_texture(pre_glass_tex, backdrop_tex, width, height);
            }

            // Step 4: Render background primitives to target
            self.renderer
                .render_with_clear(target, &bg_batch, [0.0, 0.0, 0.0, 1.0]);

            // Step 5: Render background-layer images to target
            self.render_images_ref(target, &bg_images);

            // Step 6: Render glass with backdrop blur onto target
            {
                let backdrop_view = &self.backdrop_texture.as_ref().unwrap().view;
                self.renderer.render_glass(target, backdrop_view, &bg_batch);
            }

            // Step 7: Render glass/foreground-layer images (on top of glass, NOT blurred)
            self.render_images_ref(target, &fg_images);

            // Step 8: Render foreground primitives with MSAA for smooth SVG edges
            if !fg_batch.is_empty() {
                if use_msaa_overlay {
                    self.renderer
                        .render_overlay_msaa(target, &fg_batch, self.sample_count);
                } else {
                    self.renderer.render_overlay(target, &fg_batch);
                }
            }

            // Step 9: Render text
            if !all_glyphs.is_empty() {
                self.render_text(target, &all_glyphs);
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
        }

        Ok(())
    }

    /// Ensure internal textures exist and are the right size
    fn ensure_textures(&mut self, width: u32, height: u32) {
        // Use the same texture format as the renderer's pipelines
        let format = self.renderer.texture_format();

        let needs_pre_glass = self
            .pre_glass_texture
            .as_ref()
            .map(|t| t.width != width || t.height != height)
            .unwrap_or(true);

        if needs_pre_glass {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Pre-Glass Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.pre_glass_texture = Some(CachedTexture {
                texture,
                view,
                width,
                height,
            });
        }

        let needs_backdrop = self
            .backdrop_texture
            .as_ref()
            .map(|t| t.width != width || t.height != height)
            .unwrap_or(true);

        if needs_backdrop {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Glass Backdrop"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.backdrop_texture = Some(CachedTexture {
                texture,
                view,
                width,
                height,
            });
        }

        // Note: MSAA textures for overlay rendering are created internally by
        // render_overlay_msaa, so we don't need to cache them here.
    }

    /// Copy one texture to another
    fn copy_texture(&self, src: &wgpu::Texture, dst: &wgpu::Texture, width: u32, height: u32) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Backdrop Copy Encoder"),
            });

        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: src,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: dst,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render text glyphs
    fn render_text(&mut self, target: &wgpu::TextureView, glyphs: &[GpuGlyph]) {
        if let Some(atlas_view) = self.text_ctx.atlas_view() {
            self.renderer
                .render_text(target, glyphs, atlas_view, self.text_ctx.sampler());
        }
    }

    /// Render images to the pre-glass texture (for images that should be blurred by glass)
    fn render_images_to_pre_glass(&mut self, images: &[&ImageElement]) {
        let Some(ref pre_glass) = self.pre_glass_texture else {
            return;
        };
        // Create a new view to avoid borrow conflicts
        let target = pre_glass
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.render_images_ref(&target, images);
    }

    /// Pre-load images into cache (call before rendering)
    fn preload_images(&mut self, images: &[ImageElement]) {
        for image in images {
            if self.image_cache.contains_key(&image.source) {
                continue;
            }

            // Try to load the image
            let image_data = match blinc_image::ImageData::load(blinc_image::ImageSource::File(
                image.source.clone().into(),
            )) {
                Ok(data) => data,
                Err(_) => continue, // Skip images that fail to load
            };

            // Create GPU texture
            let gpu_image = self.image_ctx.create_image_labeled(
                image_data.pixels(),
                image_data.width(),
                image_data.height(),
                &image.source,
            );

            self.image_cache.insert(image.source.clone(), gpu_image);
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
        let mut texts = Vec::new();
        let mut svgs = Vec::new();
        let mut images = Vec::new();

        if let Some(root) = tree.root() {
            self.collect_elements_recursive(
                tree,
                root,
                (0.0, 0.0),
                false,
                None, // No initial clip
                &mut texts,
                &mut svgs,
                &mut images,
            );
        }

        (texts, svgs, images)
    }

    fn collect_elements_recursive(
        &self,
        tree: &RenderTree,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        inside_glass: bool,
        current_clip: Option<[f32; 4]>,
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

        // Update clip bounds for children if this node clips
        let child_clip = if clips_content {
            Some([abs_x, abs_y, bounds.width, bounds.height])
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
                    // Debug: Log text positioning at DEBUG level for visibility
                    tracing::debug!(
                        "Text '{}': abs=({:.1}, {:.1}), size=({:.1}x{:.1}), font={:.1}, align={:?}, v_align={:?}",
                        text_data.content,
                        abs_x,
                        abs_y,
                        bounds.width,
                        bounds.height,
                        text_data.font_size,
                        text_data.align,
                        text_data.v_align
                    );
                    texts.push(TextElement {
                        content: text_data.content.clone(),
                        x: abs_x,
                        y: abs_y,
                        width: bounds.width,
                        height: bounds.height,
                        font_size: text_data.font_size,
                        color: text_data.color,
                        align: text_data.align,
                        weight: text_data.weight,
                        v_align: text_data.v_align,
                        clip_bounds: current_clip,
                    });
                }
                ElementType::Svg(svg_data) => {
                    svgs.push((
                        svg_data.source.clone(),
                        abs_x,
                        abs_y,
                        bounds.width,
                        bounds.height,
                    ));
                }
                ElementType::Image(image_data) => {
                    images.push(ImageElement {
                        source: image_data.source.clone(),
                        x: abs_x,
                        y: abs_y,
                        width: bounds.width,
                        height: bounds.height,
                        object_fit: image_data.object_fit,
                        object_position: image_data.object_position,
                        opacity: image_data.opacity,
                        border_radius: image_data.border_radius,
                        tint: image_data.tint,
                        clip_bounds: current_clip,
                        clip_radius: [0.0; 4],
                        layer: effective_layer,
                    });
                }
                ElementType::Div => {}
            }
        }

        // Include scroll offset when calculating child positions
        let scroll_offset = tree.get_scroll_offset(node);
        let new_offset = (abs_x + scroll_offset.0, abs_y + scroll_offset.1);
        for child_id in tree.layout().children(node) {
            self.collect_elements_recursive(
                tree,
                child_id,
                new_offset,
                children_inside_glass,
                child_clip,
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

    /// Get the texture format used by the renderer
    pub fn texture_format(&self) -> wgpu::TextureFormat {
        self.renderer.texture_format()
    }
}
