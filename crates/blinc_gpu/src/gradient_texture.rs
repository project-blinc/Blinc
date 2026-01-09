//! Gradient texture cache for multi-stop gradient support
//!
//! This module provides a 1D texture-based gradient rasterization system
//! that enables paths to use gradients with more than 2 color stops.
//!
//! The approach follows standard techniques used in Skia and Cairo:
//! - Gradients are rasterized to a 256-wide 1D RGBA texture
//! - The shader samples from this texture using the gradient parameter t
//! - A placeholder texture is used for 2-stop gradients (fast path)

use blinc_core::{Color, GradientStop};

/// How gradient colors are spread outside the gradient's defined range
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpreadMode {
    /// Clamp to edge colors (default)
    #[default]
    Pad,
    /// Repeat the gradient pattern
    Repeat,
    /// Mirror/reflect the gradient pattern
    Reflect,
}

/// Width of the gradient lookup texture
pub const GRADIENT_TEXTURE_WIDTH: u32 = 256;

/// Rasterized gradient data ready for GPU upload
pub struct RasterizedGradient {
    /// RGBA pixel data (256 * 4 bytes)
    pub pixels: [u8; GRADIENT_TEXTURE_WIDTH as usize * 4],
    /// Number of color stops in the original gradient
    pub stop_count: usize,
}

impl RasterizedGradient {
    /// Rasterize a gradient with multiple stops into a 256-wide texture
    pub fn from_stops(stops: &[GradientStop], spread: SpreadMode) -> Self {
        let mut pixels = [0u8; GRADIENT_TEXTURE_WIDTH as usize * 4];

        if stops.is_empty() {
            // All transparent
            return Self {
                pixels,
                stop_count: 0,
            };
        }

        if stops.len() == 1 {
            // Single color fills entire texture
            let c = &stops[0].color;
            let r = (c.r * 255.0).clamp(0.0, 255.0) as u8;
            let g = (c.g * 255.0).clamp(0.0, 255.0) as u8;
            let b = (c.b * 255.0).clamp(0.0, 255.0) as u8;
            let a = (c.a * 255.0).clamp(0.0, 255.0) as u8;

            for i in 0..GRADIENT_TEXTURE_WIDTH as usize {
                pixels[i * 4] = r;
                pixels[i * 4 + 1] = g;
                pixels[i * 4 + 2] = b;
                pixels[i * 4 + 3] = a;
            }

            return Self {
                pixels,
                stop_count: 1,
            };
        }

        // Rasterize multi-stop gradient
        for i in 0..GRADIENT_TEXTURE_WIDTH as usize {
            let t = i as f32 / (GRADIENT_TEXTURE_WIDTH - 1) as f32;

            // Apply spread mode
            let t = apply_spread_mode(t, spread);

            // Find the two stops that bracket t
            let color = sample_gradient(stops, t);

            pixels[i * 4] = (color.r * 255.0).clamp(0.0, 255.0) as u8;
            pixels[i * 4 + 1] = (color.g * 255.0).clamp(0.0, 255.0) as u8;
            pixels[i * 4 + 2] = (color.b * 255.0).clamp(0.0, 255.0) as u8;
            pixels[i * 4 + 3] = (color.a * 255.0).clamp(0.0, 255.0) as u8;
        }

        Self {
            pixels,
            stop_count: stops.len(),
        }
    }

    /// Create a simple 2-stop gradient
    pub fn two_stop(start: Color, end: Color) -> Self {
        let stops = [
            GradientStop {
                offset: 0.0,
                color: start,
            },
            GradientStop {
                offset: 1.0,
                color: end,
            },
        ];
        Self::from_stops(&stops, SpreadMode::Pad)
    }
}

/// Apply spread mode to a gradient parameter
fn apply_spread_mode(t: f32, spread: SpreadMode) -> f32 {
    match spread {
        SpreadMode::Pad => t.clamp(0.0, 1.0),
        SpreadMode::Repeat => t.fract().abs(),
        SpreadMode::Reflect => {
            let t_mod = t.abs() % 2.0;
            if t_mod > 1.0 {
                2.0 - t_mod
            } else {
                t_mod
            }
        }
    }
}

/// Sample a gradient at parameter t
fn sample_gradient(stops: &[GradientStop], t: f32) -> Color {
    if stops.is_empty() {
        return Color::TRANSPARENT;
    }

    if t <= stops[0].offset {
        return stops[0].color;
    }

    if t >= stops[stops.len() - 1].offset {
        return stops[stops.len() - 1].color;
    }

    // Find bracketing stops
    for i in 0..stops.len() - 1 {
        let s0 = &stops[i];
        let s1 = &stops[i + 1];

        if t >= s0.offset && t <= s1.offset {
            // Interpolate between these stops
            let range = s1.offset - s0.offset;
            if range < 0.0001 {
                return s0.color;
            }

            let local_t = (t - s0.offset) / range;
            return lerp_color(&s0.color, &s1.color, local_t);
        }
    }

    // Fallback
    stops[stops.len() - 1].color
}

/// Linear interpolation between two colors
fn lerp_color(a: &Color, b: &Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// GPU gradient texture cache
pub struct GradientTextureCache {
    /// The gradient texture (256x1 RGBA)
    pub texture: wgpu::Texture,
    /// Texture view for binding
    pub view: wgpu::TextureView,
    /// Sampler for gradient lookups
    pub sampler: wgpu::Sampler,
    /// Whether the texture contains valid gradient data
    pub has_gradient: bool,
}

impl GradientTextureCache {
    /// Create a new gradient texture cache with a placeholder texture
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Gradient Texture"),
            size: wgpu::Extent3d {
                width: GRADIENT_TEXTURE_WIDTH,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D1,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Gradient Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Initialize with a white-to-white gradient (placeholder)
        let placeholder = RasterizedGradient::two_stop(Color::WHITE, Color::WHITE);
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &placeholder.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(GRADIENT_TEXTURE_WIDTH * 4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: GRADIENT_TEXTURE_WIDTH,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        Self {
            texture,
            view,
            sampler,
            has_gradient: false,
        }
    }

    /// Upload a rasterized gradient to the GPU texture
    pub fn upload(&mut self, queue: &wgpu::Queue, gradient: &RasterizedGradient) {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &gradient.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(GRADIENT_TEXTURE_WIDTH * 4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: GRADIENT_TEXTURE_WIDTH,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        self.has_gradient = gradient.stop_count > 2;
    }

    /// Clear the gradient texture (sets to placeholder)
    pub fn clear(&mut self, queue: &wgpu::Queue) {
        let placeholder = RasterizedGradient::two_stop(Color::WHITE, Color::WHITE);
        self.upload(queue, &placeholder);
        self.has_gradient = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_two_stop_gradient() {
        let gradient = RasterizedGradient::two_stop(Color::BLACK, Color::WHITE);
        assert_eq!(gradient.stop_count, 2);

        // First pixel should be black
        assert_eq!(gradient.pixels[0], 0); // R
        assert_eq!(gradient.pixels[1], 0); // G
        assert_eq!(gradient.pixels[2], 0); // B
        assert_eq!(gradient.pixels[3], 255); // A

        // Last pixel should be white
        let last_idx = (GRADIENT_TEXTURE_WIDTH as usize - 1) * 4;
        assert_eq!(gradient.pixels[last_idx], 255); // R
        assert_eq!(gradient.pixels[last_idx + 1], 255); // G
        assert_eq!(gradient.pixels[last_idx + 2], 255); // B
        assert_eq!(gradient.pixels[last_idx + 3], 255); // A
    }

    #[test]
    fn test_multi_stop_gradient() {
        let stops = vec![
            GradientStop {
                offset: 0.0,
                color: Color::RED,
            },
            GradientStop {
                offset: 0.5,
                color: Color::GREEN,
            },
            GradientStop {
                offset: 1.0,
                color: Color::BLUE,
            },
        ];

        let gradient = RasterizedGradient::from_stops(&stops, SpreadMode::Pad);
        assert_eq!(gradient.stop_count, 3);

        // First pixel should be red
        assert!(gradient.pixels[0] > 200); // R
        assert!(gradient.pixels[1] < 50); // G
        assert!(gradient.pixels[2] < 50); // B

        // Middle pixel should be greenish
        let mid_idx = 128 * 4;
        assert!(gradient.pixels[mid_idx + 1] > gradient.pixels[mid_idx]); // G > R

        // Last pixel should be blue
        let last_idx = 255 * 4;
        assert!(gradient.pixels[last_idx] < 50); // R
        assert!(gradient.pixels[last_idx + 1] < 50); // G
        assert!(gradient.pixels[last_idx + 2] > 200); // B
    }
}
