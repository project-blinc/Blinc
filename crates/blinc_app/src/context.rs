//! Render context for blinc_app
//!
//! Wraps the GPU rendering pipeline with a clean API.

use blinc_core::Rect;
use blinc_gpu::{GpuGlyph, GpuPaintContext, GpuRenderer, TextRenderingContext};
use blinc_layout::prelude::*;
use blinc_layout::renderer::ElementType;
use blinc_svg::SvgDocument;
use blinc_text::TextAnchor;
use std::sync::Arc;

use crate::error::Result;

/// Internal render context that manages GPU resources and rendering
pub struct RenderContext {
    renderer: GpuRenderer,
    text_ctx: TextRenderingContext,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    sample_count: u32,
    // Cached textures for glass rendering
    pre_glass_texture: Option<CachedTexture>,
    backdrop_texture: Option<CachedTexture>,
    // Cached MSAA texture for anti-aliased rendering
    msaa_texture: Option<CachedTexture>,
}

struct CachedTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
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
        Self {
            renderer,
            text_ctx,
            device,
            queue,
            sample_count,
            pre_glass_texture: None,
            backdrop_texture: None,
            msaa_texture: None,
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

        // Collect text and SVG elements
        let (texts, svgs) = self.collect_render_elements(tree);

        // Prepare text glyphs
        let mut all_glyphs = Vec::new();
        for (content, x, y, _w, h, font_size, color) in &texts {
            if let Ok(glyphs) = self.text_ctx.prepare_text_with_anchor(
                content,
                *x,
                *y + *h / 2.0,
                *font_size,
                *color,
                TextAnchor::Center,
            ) {
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
            // Glass path:
            // 1. Render background to pre-glass texture (for backdrop capture)
            // 2. Copy to backdrop for glass sampling
            // 3. Render background to target
            // 4. Render glass on top with backdrop blur
            // 5. Render foreground (with MSAA if enabled)
            // 6. Render text

            let pre_glass_view = &self.pre_glass_texture.as_ref().unwrap().view;
            let pre_glass_tex = &self.pre_glass_texture.as_ref().unwrap().texture;
            let backdrop_tex = &self.backdrop_texture.as_ref().unwrap().texture;
            let backdrop_view = &self.backdrop_texture.as_ref().unwrap().view;

            // Step 1: Render background to pre-glass texture
            // Use transparent clear - the UI elements provide their own background colors
            self.renderer
                .render_with_clear(pre_glass_view, &bg_batch, [0.0, 0.0, 0.0, 0.0]);

            // Step 2: Copy pre-glass to backdrop for glass sampling
            self.copy_texture(pre_glass_tex, backdrop_tex, width, height);

            // Step 3: Render background to target
            // Use opaque black clear for window surfaces
            self.renderer
                .render_with_clear(target, &bg_batch, [0.0, 0.0, 0.0, 1.0]);

            // Step 4: Render glass with backdrop blur onto target
            self.renderer.render_glass(target, backdrop_view, &bg_batch);

            // Step 5: Render foreground with MSAA for smooth SVG edges
            if !fg_batch.is_empty() {
                if use_msaa_overlay {
                    self.renderer
                        .render_overlay_msaa(target, &fg_batch, self.sample_count);
                } else {
                    self.renderer.render_overlay(target, &fg_batch);
                }
            }

            // Step 6: Render text
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

    /// Collect text and SVG elements from the render tree
    fn collect_render_elements(
        &self,
        tree: &RenderTree,
    ) -> (
        Vec<(String, f32, f32, f32, f32, f32, [f32; 4])>,
        Vec<(String, f32, f32, f32, f32)>,
    ) {
        let mut texts = Vec::new();
        let mut svgs = Vec::new();

        if let Some(root) = tree.root() {
            self.collect_elements_recursive(tree, root, (0.0, 0.0), &mut texts, &mut svgs);
        }

        (texts, svgs)
    }

    fn collect_elements_recursive(
        &self,
        tree: &RenderTree,
        node: LayoutNodeId,
        parent_offset: (f32, f32),
        texts: &mut Vec<(String, f32, f32, f32, f32, f32, [f32; 4])>,
        svgs: &mut Vec<(String, f32, f32, f32, f32)>,
    ) {
        let Some(bounds) = tree.layout().get_bounds(node, parent_offset) else {
            return;
        };

        let abs_x = bounds.x;
        let abs_y = bounds.y;

        if let Some(render_node) = tree.get_render_node(node) {
            match &render_node.element_type {
                ElementType::Text(text_data) => {
                    texts.push((
                        text_data.content.clone(),
                        abs_x,
                        abs_y,
                        bounds.width,
                        bounds.height,
                        text_data.font_size,
                        text_data.color,
                    ));
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
                ElementType::Div => {}
            }
        }

        let new_offset = (abs_x, abs_y);
        for child_id in tree.layout().children(node) {
            self.collect_elements_recursive(tree, child_id, new_offset, texts, svgs);
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
