//! Text rendering support for blinc_gpu
//!
//! This module provides integration between blinc_text's TextRenderer
//! and the GPU rendering pipeline.

use blinc_text::{
    ColorSpan, FontRegistry, GenericFont, LayoutOptions, TextAlignment, TextAnchor, TextRenderer,
};
use std::sync::{Arc, Mutex};

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
    /// Glyph atlas texture (grayscale, created on demand)
    atlas_texture: Option<wgpu::Texture>,
    /// Glyph atlas texture view
    atlas_view: Option<wgpu::TextureView>,
    /// Color glyph atlas texture (RGBA for emoji, created on demand)
    color_atlas_texture: Option<wgpu::Texture>,
    /// Color glyph atlas texture view
    color_atlas_view: Option<wgpu::TextureView>,
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

        // Create initial empty atlases (will be populated as glyphs are used)
        let renderer = TextRenderer::new();

        // Grayscale atlas for regular text
        let (gray_width, gray_height) = renderer.atlas_dimensions();
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph Atlas Texture"),
            size: wgpu::Extent3d {
                width: gray_width,
                height: gray_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // RGBA color atlas for emoji
        let (color_width, color_height) = renderer.color_atlas_dimensions();
        let color_atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Color Glyph Atlas Texture"),
            size: wgpu::Extent3d {
                width: color_width,
                height: color_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let color_atlas_view =
            color_atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            renderer,
            device,
            queue,
            atlas_texture: Some(atlas_texture),
            atlas_view: Some(atlas_view),
            color_atlas_texture: Some(color_atlas_texture),
            color_atlas_view: Some(color_atlas_view),
            sampler,
        }
    }

    /// Load the default font from a file path
    pub fn load_font(&mut self, path: &std::path::Path) -> Result<(), blinc_text::TextError> {
        self.renderer.load_default_font(path)
    }

    /// Preload fonts by name (call at startup for fonts your app uses)
    /// This ensures fonts are cached before render time.
    pub fn preload_fonts(&mut self, names: &[&str]) {
        self.renderer.preload_fonts(names);
    }

    /// Preload fonts with specific weights and styles
    /// Each spec is (font_name, weight, italic)
    /// Weight: 400 = normal, 700 = bold
    pub fn preload_fonts_with_styles(&mut self, specs: &[(&str, u16, bool)]) {
        self.renderer.preload_fonts_with_styles(specs);
    }

    /// Preload generic font variants with specific weights
    pub fn preload_generic_styles(&mut self, generic: GenericFont, weights: &[u16], italic: bool) {
        self.renderer
            .preload_generic_styles(generic, weights, italic);
    }

    /// Load the default font from data
    pub fn load_font_data(&mut self, data: Vec<u8>) -> Result<(), blinc_text::TextError> {
        self.renderer.load_default_font_data(data)
    }

    /// Load font data into the registry (for use by the rendering system)
    ///
    /// This adds fonts to the registry where they can be found by name
    /// during text rendering. Returns the number of font faces loaded.
    ///
    /// Use this instead of `load_font_data` when you want fonts to be
    /// available for regular text rendering (not just as a default fallback).
    pub fn load_font_data_to_registry(&mut self, data: Vec<u8>) -> usize {
        self.renderer.load_font_data_to_registry(data)
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
            false, // no wrap for simple text
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
    /// * `width` - Optional width for alignment/wrapping (if None, text is positioned at x)
    /// * `wrap` - Whether to wrap text at width boundary
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
        wrap: bool,
    ) -> Result<Vec<GpuGlyph>, blinc_text::TextError> {
        self.prepare_text_full(text, x, y, font_size, color, anchor, alignment, width, wrap)
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
        self.prepare_text_with_font(
            text,
            x,
            y,
            font_size,
            color,
            anchor,
            alignment,
            width,
            wrap,
            None,
            GenericFont::System,
        )
    }

    /// Prepare text with a specific font family
    ///
    /// # Arguments
    /// * `text` - The text string to render
    /// * `x` - X position
    /// * `y` - Y position
    /// * `font_size` - Font size in pixels
    /// * `color` - RGBA color as [r, g, b, a] in 0.0-1.0 range
    /// * `anchor` - Vertical anchor (Top, Center, Baseline)
    /// * `alignment` - Horizontal alignment (Left, Center, Right)
    /// * `width` - Optional width for alignment/wrapping
    /// * `wrap` - Whether to wrap text at width boundary
    /// * `font_name` - Optional font name (e.g., "Fira Code", "Inter")
    /// * `generic` - Generic font category for fallback
    pub fn prepare_text_with_font(
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
        font_name: Option<&str>,
        generic: GenericFont,
    ) -> Result<Vec<GpuGlyph>, blinc_text::TextError> {
        self.prepare_text_with_style(
            text, x, y, font_size, color, anchor, alignment, width, wrap, font_name, generic, 400,
            false, None,
        )
    }

    /// Prepare text with a specific font family, weight, and style
    ///
    /// # Arguments
    /// * `text` - The text string to render
    /// * `x` - X position
    /// * `y` - Y position
    /// * `font_size` - Font size in pixels
    /// * `color` - RGBA color as [r, g, b, a] in 0.0-1.0 range
    /// * `anchor` - Vertical anchor (Top, Center, Baseline)
    /// * `alignment` - Horizontal alignment (Left, Center, Right)
    /// * `width` - Optional width for alignment/wrapping
    /// * `wrap` - Whether to wrap text at width boundary
    /// * `font_name` - Optional font name (e.g., "Fira Code", "Inter")
    /// * `generic` - Generic font category for fallback
    /// * `weight` - Font weight (100-900, 400=normal, 700=bold)
    /// * `italic` - Whether to use italic variant
    /// * `layout_height` - Optional layout-assigned height for vertical centering
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_text_with_style(
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
        font_name: Option<&str>,
        generic: GenericFont,
        weight: u16,
        italic: bool,
        layout_height: Option<f32>,
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

        let prepared = self.renderer.prepare_text_with_style(
            text, font_size, color, &options, font_name, generic, weight, italic,
        )?;

        // Determine the number of lines from the prepared text
        // If there's only 1 line, use glyph_extent for more accurate centering
        // For multi-line text, use total_text_height to center all lines properly
        let glyph_extent = prepared.ascender - prepared.descender;
        let is_multiline = prepared.height > glyph_extent * 1.5; // Heuristic: more than 1.5x single line
        let centering_height = if is_multiline {
            prepared.height
        } else {
            glyph_extent
        };

        let y_offset = match anchor {
            TextAnchor::Top => {
                // Center glyphs within the layout-assigned height (if provided).
                // This ensures items_center() on parent works correctly - text is
                // centered within its bounding box regardless of font metrics.
                if let Some(lh) = layout_height {
                    // Center text within the actual layout height
                    // Use glyph_extent for single-line, total height for multi-line
                    y + (lh - centering_height) / 2.0
                } else {
                    // No layout height provided - render glyphs at top without centering.
                    // This is used for baseline alignment where we want the natural
                    // baseline position (ascender from top) to determine alignment.
                    y
                }
            }
            TextAnchor::Center => {
                // Center text so the visual center of glyphs aligns with y.
                // User provides y at the vertical center of the bounding box.
                // We want glyph center to align with that.
                y - centering_height / 2.0
            }
            TextAnchor::Baseline => {
                // Position text so its baseline aligns EXACTLY with user's y.
                // The caller provides y as the desired baseline position.
                // Baseline is at `ascender` from the top of the glyph bounding box.
                // So glyph_top = y - ascender.
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
                // Set is_color flag for emoji glyphs
                flags: [if g.is_color { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            })
            .collect();

        // Update atlas textures if dirty
        if self.renderer.atlas_is_dirty() {
            self.update_atlas_texture();
            self.renderer.mark_atlas_clean();
        }
        if self.renderer.color_atlas_is_dirty() {
            self.update_color_atlas_texture();
            self.renderer.mark_color_atlas_clean();
        }

        Ok(glyphs)
    }

    /// Prepare styled text with multiple color spans
    ///
    /// This renders text as a single unit but applies different colors to different ranges.
    /// Each ColorSpan specifies a byte range and color.
    pub fn prepare_styled_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_size: f32,
        default_color: [f32; 4],
        color_spans: &[ColorSpan],
        anchor: TextAnchor,
        font_name: Option<&str>,
        generic: GenericFont,
        layout_height: Option<f32>,
    ) -> Result<Vec<GpuGlyph>, blinc_text::TextError> {
        let mut options = LayoutOptions::default();
        options.anchor = anchor;
        options.line_break = blinc_text::LineBreakMode::None;

        let prepared = self.renderer.prepare_styled_text(
            text,
            font_size,
            default_color,
            color_spans,
            &options,
            font_name,
            generic,
        )?;

        // Determine if this is single-line or multi-line text
        // For single-line, use glyph_extent for more accurate centering
        let glyph_extent = prepared.ascender - prepared.descender;
        let is_multiline = prepared.height > glyph_extent * 1.5;
        let centering_height = if is_multiline {
            prepared.height
        } else {
            glyph_extent
        };

        let y_offset = match anchor {
            TextAnchor::Top => {
                if let Some(lh) = layout_height {
                    y + (lh - centering_height) / 2.0
                } else {
                    y // No centering for baseline alignment
                }
            }
            TextAnchor::Center => y - centering_height / 2.0,
            TextAnchor::Baseline => y - prepared.ascender,
        };

        let glyphs = prepared
            .glyphs
            .iter()
            .map(|g| GpuGlyph {
                bounds: [
                    g.bounds[0] + x,
                    g.bounds[1] + y_offset,
                    g.bounds[2],
                    g.bounds[3],
                ],
                uv_bounds: g.uv_bounds,
                color: g.color,
                clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
                flags: [if g.is_color { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            })
            .collect();

        if self.renderer.atlas_is_dirty() {
            self.update_atlas_texture();
            self.renderer.mark_atlas_clean();
        }
        if self.renderer.color_atlas_is_dirty() {
            self.update_color_atlas_texture();
            self.renderer.mark_color_atlas_clean();
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

    /// Get the color atlas texture view (RGBA for emoji)
    pub fn color_atlas_view(&self) -> Option<&wgpu::TextureView> {
        self.color_atlas_view.as_ref()
    }

    /// Get the sampler
    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    /// Get the shared font registry
    ///
    /// This can be used to share the font registry with other components
    /// like text measurement, ensuring consistent font loading and metrics.
    pub fn font_registry(&self) -> Arc<Mutex<FontRegistry>> {
        self.renderer.font_registry()
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

    /// Update the GPU color atlas texture from the TextRenderer's color atlas
    fn update_color_atlas_texture(&mut self) {
        let (width, height) = self.renderer.color_atlas_dimensions();
        let pixels = self.renderer.color_atlas_pixels();

        // Create or recreate texture if size changed
        let needs_create = match &self.color_atlas_texture {
            Some(tex) => tex.width() != width || tex.height() != height,
            None => true,
        };

        if needs_create {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Color Glyph Atlas Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb, // RGBA for color emoji
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.color_atlas_texture = Some(texture);
            self.color_atlas_view = Some(view);
        }

        // Upload pixel data (RGBA = 4 bytes per pixel)
        if let Some(texture) = &self.color_atlas_texture {
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
                    bytes_per_row: Some(width * 4), // 4 bytes per pixel for RGBA
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
