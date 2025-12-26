//! Text rendering support for blinc_gpu
//!
//! This module provides integration between blinc_text's TextRenderer
//! and the GPU rendering pipeline.

use blinc_text::{LayoutOptions, TextAlignment, TextAnchor, TextRenderer};
use std::sync::Arc;

use crate::primitives::GpuGlyph;

/// Text measurement result with full metrics
#[derive(Debug, Clone, Copy, Default)]
pub struct TextMeasurement {
    /// Width in pixels
    pub width: f32,
    /// Height in pixels
    pub height: f32,
    /// Ascender in pixels (distance from baseline to top)
    pub ascender: f32,
    /// Descender in pixels (distance from baseline to bottom, typically negative)
    pub descender: f32,
}

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

    /// Prepare text for GPU rendering with default top anchor
    ///
    /// Returns a list of GPU glyphs ready for rendering.
    /// The y coordinate represents the top of the text bounding box.
    pub fn prepare_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: [f32; 4],
    ) -> Result<Vec<GpuGlyph>, blinc_text::TextError> {
        self.prepare_text_with_anchor(text, x, y, font_size, color, TextAnchor::Top)
    }

    /// Prepare text for GPU rendering with specified anchor
    ///
    /// Returns a list of GPU glyphs ready for rendering.
    /// The anchor determines how the y coordinate is interpreted:
    /// - Top: y is the top of the text bounding box
    /// - Center: y is the vertical center of the text
    /// - Baseline: y is the text baseline
    pub fn prepare_text_with_anchor(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: [f32; 4],
        anchor: TextAnchor,
    ) -> Result<Vec<GpuGlyph>, blinc_text::TextError> {
        self.prepare_text_with_options(
            text,
            x,
            y,
            font_size,
            color,
            anchor,
            TextAlignment::Left,
            None,
        )
    }

    /// Prepare text for GPU rendering with full options
    ///
    /// Returns a list of GPU glyphs ready for rendering.
    ///
    /// # Arguments
    /// * `text` - The text string to render
    /// * `x` - X position (left edge for Left align, center for Center, right for Right)
    /// * `y` - Y position (interpreted based on anchor)
    /// * `font_size` - Font size in pixels
    /// * `color` - RGBA color as [r, g, b, a] in 0.0-1.0 range
    /// * `anchor` - Vertical anchor (Top, Center, Baseline)
    /// * `alignment` - Horizontal alignment (Left, Center, Right)
    /// * `width` - Optional width for alignment (if None, text is positioned at x)
    pub fn prepare_text_with_options(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: [f32; 4],
        anchor: TextAnchor,
        alignment: TextAlignment,
        width: Option<f32>,
    ) -> Result<Vec<GpuGlyph>, blinc_text::TextError> {
        // For regular text, disable wrapping (LineBreakMode::None)
        // This allows width to be used for alignment only
        self.prepare_text_full(
            text, x, y, font_size, color, anchor, alignment, width, false,
        )
    }

    /// Prepare text with full control over wrapping behavior
    ///
    /// * `wrap` - If true, text wraps at width boundary. If false, text stays on single line.
    pub fn prepare_text_full(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        color: [f32; 4],
        anchor: TextAnchor,
        alignment: TextAlignment,
        width: Option<f32>,
        wrap: bool,
    ) -> Result<Vec<GpuGlyph>, blinc_text::TextError> {
        let mut options = LayoutOptions::default();
        options.anchor = anchor;
        options.alignment = alignment;
        if let Some(w) = width {
            options.max_width = Some(w);
        }
        // Disable wrapping unless explicitly requested
        if !wrap {
            options.line_break = blinc_text::LineBreakMode::None;
        }
        let prepared = self
            .renderer
            .prepare_text(text, font_size, color, &options)?;

        let y_offset = match anchor {
            TextAnchor::Top => y,
            TextAnchor::Center => {
                // For optical centering, center based on cap-height region (baseline to cap top)
                // rather than full glyph bounds (which includes descenders)
                // Cap height is approximately ascender (the baseline is at y = ascender in layout coords)
                // The visual center of text without descenders is at: baseline - cap_height/2
                // In layout coordinates: ascender - (ascender * 0.7) / 2 = ascender * 0.65
                //
                // But we want to center the cap-height region at user's y
                // Cap top is at glyph_min_y (≈0), cap bottom is at baseline (≈ascender)
                // So cap center is at ascender / 2
                let cap_center = prepared.ascender / 2.0;
                y - cap_center
            }
            TextAnchor::Baseline => {
                // Baseline is at y = ascender from the top of the em box
                // We want baseline at user's y, so offset = y - ascender
                y - prepared.ascender
            }
        };

        // Calculate x offset based on alignment
        // When max_width is set, the layout engine already aligns glyphs within that width.
        // We just need to add the container's x position as base offset.
        // When no width is provided, we manually apply alignment offset.
        let x_offset = if width.is_some() {
            // Layout engine already aligned within max_width, just offset by container x
            x
        } else {
            // No container width - just position at x (left-aligned)
            x
        };

        // Convert to GPU glyphs with position offset
        let glyphs = prepared
            .glyphs
            .iter()
            .map(|g| GpuGlyph {
                bounds: [
                    g.bounds[0] + x_offset,
                    g.bounds[1] + y_offset,
                    g.bounds[2],
                    g.bounds[3],
                ],
                uv_bounds: g.uv_bounds,
                color: g.color,
                // Default: no clip (will be set by caller if needed)
                clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
            })
            .collect();

        // Update atlas texture if dirty
        if self.renderer.atlas_is_dirty() {
            self.update_atlas_texture();
            self.renderer.mark_atlas_clean();
        }

        Ok(glyphs)
    }

    /// Measure text dimensions without rendering
    ///
    /// Returns (width, height) in pixels. This uses the actual font metrics
    /// for accurate measurement.
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> (f32, f32) {
        let options = LayoutOptions::default();
        match self
            .renderer
            .prepare_text(text, font_size, [0.0; 4], &options)
        {
            Ok(prepared) => (prepared.width, prepared.height),
            Err(_) => {
                // Fallback to estimation if font not loaded
                let char_count = text.chars().count() as f32;
                let width = char_count * font_size * 0.55;
                let height = font_size * 1.2;
                (width, height)
            }
        }
    }

    /// Measure text dimensions with full metrics
    ///
    /// Returns TextMeasurement with width, height, ascender, and descender.
    pub fn measure_text_full(&mut self, text: &str, font_size: f32) -> TextMeasurement {
        let options = LayoutOptions::default();
        match self
            .renderer
            .prepare_text(text, font_size, [0.0; 4], &options)
        {
            Ok(prepared) => TextMeasurement {
                width: prepared.width,
                height: prepared.height,
                ascender: prepared.ascender,
                descender: prepared.descender,
            },
            Err(_) => {
                // Fallback to estimation if font not loaded
                let char_count = text.chars().count() as f32;
                TextMeasurement {
                    width: char_count * font_size * 0.55,
                    height: font_size * 1.2,
                    ascender: font_size * 0.8,
                    descender: font_size * -0.2,
                }
            }
        }
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
