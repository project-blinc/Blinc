//! Text rendering support for blinc_gpu
//!
//! This module provides integration between blinc_text's TextRenderer
//! and the GPU rendering pipeline.

use blinc_text::{LayoutOptions, TextRenderer};
use std::sync::Arc;

use crate::primitives::GpuGlyph;

/// Text rendering context that manages font, atlas, and glyph preparation
pub struct TextRenderingContext {
    /// The text renderer (font, atlas, rasterizer)
    renderer: TextRenderer,
    /// GPU device for texture creation
    device: Arc<wgpu::Device>,
    /// GPU queue for texture upload
    queue: Arc<wgpu::Queue>,
    /// Glyph atlas texture (created on demand)
    atlas_texture: Option<wgpu::Texture>,
    /// Glyph atlas texture view
    atlas_view: Option<wgpu::TextureView>,
    /// Sampler for the atlas
    sampler: wgpu::Sampler,
}

impl TextRenderingContext {
    /// Create a new text rendering context
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        // Use Nearest filtering for sharp, pixel-perfect text at 1:1 scale
        // Linear filtering causes blur when glyphs are rendered at exact pixel positions
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glyph Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            renderer: TextRenderer::new(),
            device,
            queue,
            atlas_texture: None,
            atlas_view: None,
            sampler,
        }
    }

    /// Load the default font from a file path
    pub fn load_font(&mut self, path: &std::path::Path) -> Result<(), blinc_text::TextError> {
        self.renderer.load_default_font(path)
    }

    /// Load the default font from data
    pub fn load_font_data(&mut self, data: Vec<u8>) -> Result<(), blinc_text::TextError> {
        self.renderer.load_default_font_data(data)
    }

    /// Set the default font
    pub fn set_font(&mut self, font: blinc_text::FontFace) {
        self.renderer.set_default_font(font);
    }

    /// Prepare text for GPU rendering
    ///
    /// Returns a list of GPU glyphs ready for rendering
    pub fn prepare_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: [f32; 4],
    ) -> Result<Vec<GpuGlyph>, blinc_text::TextError> {
        let options = LayoutOptions::default();
        let prepared = self.renderer.prepare_text(text, font_size, color, &options)?;

        // Convert to GPU glyphs with position offset
        let glyphs = prepared
            .glyphs
            .iter()
            .map(|g| GpuGlyph {
                bounds: [g.bounds[0] + x, g.bounds[1] + y, g.bounds[2], g.bounds[3]],
                uv_bounds: g.uv_bounds,
                color: g.color,
            })
            .collect();

        // Update atlas texture if dirty
        if self.renderer.atlas_is_dirty() {
            self.update_atlas_texture();
            self.renderer.mark_atlas_clean();
        }

        Ok(glyphs)
    }

    /// Get the atlas texture view (creates it if needed)
    pub fn atlas_view(&self) -> Option<&wgpu::TextureView> {
        self.atlas_view.as_ref()
    }

    /// Get the sampler
    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    /// Update the GPU atlas texture from the TextRenderer's atlas
    fn update_atlas_texture(&mut self) {
        let (width, height) = self.renderer.atlas_dimensions();
        let pixels = self.renderer.atlas_pixels();

        // Create or recreate texture if size changed
        let needs_create = match &self.atlas_texture {
            Some(tex) => tex.width() != width || tex.height() != height,
            None => true,
        };

        if needs_create {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Glyph Atlas Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.atlas_texture = Some(texture);
            self.atlas_view = Some(view);
        }

        // Upload pixel data
        if let Some(texture) = &self.atlas_texture {
            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}
