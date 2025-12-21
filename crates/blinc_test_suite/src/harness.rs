//! Test harness for visual tests
//!
//! Provides infrastructure for running visual tests, including:
//! - GPU context initialization
//! - Offscreen rendering to PNG files
//! - Reference image comparison

use anyhow::{Context, Result};
use blinc_core::Size;
use blinc_gpu::{GpuPaintContext, GpuRenderer, PrimitiveBatch, RendererConfig};
use image::{ImageBuffer, Rgba, RgbaImage};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Result of a visual test
#[derive(Debug)]
pub enum TestResult {
    /// Test passed
    Passed,
    /// Test passed but reference image was created/updated
    PassedWithNewReference,
    /// Test failed with difference percentage
    Failed { difference: f32, diff_path: PathBuf },
    /// Test skipped (e.g., no GPU available)
    Skipped { reason: String },
}

impl TestResult {
    pub fn is_passed(&self) -> bool {
        matches!(
            self,
            TestResult::Passed | TestResult::PassedWithNewReference
        )
    }
}

/// Context for a single test
pub struct TestContext {
    /// Paint context for drawing
    pub paint_ctx: GpuPaintContext,
    /// Viewport size
    pub size: Size,
    /// Test name
    pub name: String,
    /// Output directory for reference images
    pub output_dir: PathBuf,
}

impl TestContext {
    /// Create a new test context
    pub fn new(name: &str, width: f32, height: f32) -> Self {
        Self {
            paint_ctx: GpuPaintContext::new(width, height),
            size: Size::new(width, height),
            name: name.to_string(),
            output_dir: PathBuf::from("test_output"),
        }
    }

    /// Get the paint context for drawing
    pub fn ctx(&mut self) -> &mut GpuPaintContext {
        &mut self.paint_ctx
    }

    /// Get the primitive batch after drawing
    pub fn batch(&self) -> &PrimitiveBatch {
        self.paint_ctx.batch()
    }

    /// Take the batch, consuming the recorded primitives
    pub fn take_batch(&mut self) -> PrimitiveBatch {
        self.paint_ctx.take_batch()
    }

    /// Clear and reset the paint context
    pub fn clear(&mut self) {
        self.paint_ctx = GpuPaintContext::new(self.size.width, self.size.height);
    }
}

/// Test harness for running visual tests
pub struct TestHarness {
    /// GPU renderer (wrapped in RefCell for interior mutability)
    renderer: RefCell<GpuRenderer>,
    /// wgpu device
    device: Arc<wgpu::Device>,
    /// wgpu queue
    queue: Arc<wgpu::Queue>,
    /// Output directory for test results
    output_dir: PathBuf,
    /// Reference image directory
    reference_dir: PathBuf,
    /// Default viewport size
    default_size: Size,
    /// Difference threshold for visual comparison (0.0-1.0)
    threshold: f32,
    /// MSAA sample count
    sample_count: u32,
}

impl TestHarness {
    /// Create a new test harness with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(TestHarnessConfig::default())
    }

    /// Create a new test harness with custom configuration
    pub fn with_config(config: TestHarnessConfig) -> Result<Self> {
        let renderer_config = RendererConfig {
            max_primitives: config.max_primitives,
            max_glass_primitives: config.max_glass_primitives,
            max_glyphs: config.max_glyphs,
            sample_count: config.sample_count,
            texture_format: Some(wgpu::TextureFormat::Rgba8Unorm),
        };

        let renderer = pollster::block_on(GpuRenderer::new(renderer_config))
            .context("Failed to create GPU renderer")?;

        // Get device and queue from renderer
        let device = renderer.device_arc();
        let queue = renderer.queue_arc();

        // Create output directories
        std::fs::create_dir_all(&config.output_dir).context("Failed to create output directory")?;
        std::fs::create_dir_all(&config.reference_dir)
            .context("Failed to create reference directory")?;

        Ok(Self {
            renderer: RefCell::new(renderer),
            device,
            queue,
            output_dir: config.output_dir,
            reference_dir: config.reference_dir,
            default_size: config.default_size,
            threshold: config.threshold,
            sample_count: config.sample_count,
        })
    }

    /// Create a test context with default size
    pub fn create_context(&self, name: &str) -> TestContext {
        self.create_context_with_size(name, self.default_size.width, self.default_size.height)
    }

    /// Create a test context with custom size
    pub fn create_context_with_size(&self, name: &str, width: f32, height: f32) -> TestContext {
        let mut ctx = TestContext::new(name, width, height);
        ctx.output_dir = self.output_dir.clone();
        ctx
    }

    /// Create an offscreen texture for rendering (multisampled if sample_count > 1)
    fn create_render_texture(&self, width: u32, height: u32, sample_count: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Test Render Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }

    /// Create a resolve texture for MSAA (always 1x sampled, used for readback)
    fn create_resolve_texture(&self, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Test Resolve Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    /// Create a buffer for reading back texture data
    fn create_readback_buffer(&self, width: u32, height: u32) -> wgpu::Buffer {
        // Each pixel is 4 bytes (RGBA8)
        // Row alignment must be 256 bytes for wgpu
        let bytes_per_row = Self::padded_bytes_per_row(width);
        let buffer_size = (bytes_per_row * height) as u64;

        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Test Readback Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    }

    /// Calculate padded bytes per row (must be multiple of 256 for wgpu)
    fn padded_bytes_per_row(width: u32) -> u32 {
        let unpadded = width * 4; // 4 bytes per pixel (RGBA)
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        ((unpadded + align - 1) / align) * align
    }

    /// Render a batch to a PNG file
    pub fn render_to_png(
        &self,
        batch: &PrimitiveBatch,
        width: u32,
        height: u32,
        path: &Path,
    ) -> Result<()> {
        // Determine which texture to copy from based on MSAA
        let copy_texture: wgpu::Texture;

        if self.sample_count > 1 {
            // MSAA path: render to multisampled texture, resolve to single-sampled
            let msaa_texture = self.create_render_texture(width, height, self.sample_count);
            let msaa_view = msaa_texture.create_view(&wgpu::TextureViewDescriptor::default());

            let resolve_texture = self.create_resolve_texture(width, height);
            let resolve_view = resolve_texture.create_view(&wgpu::TextureViewDescriptor::default());

            // Render with MSAA resolve
            {
                let mut renderer = self.renderer.borrow_mut();
                renderer.resize(width, height);
                renderer.render_msaa(&msaa_view, &resolve_view, batch, [1.0, 1.0, 1.0, 1.0]);
            }

            copy_texture = resolve_texture;
        } else {
            // Non-MSAA path: render directly to single-sampled texture
            let texture = self.create_resolve_texture(width, height);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            {
                let mut renderer = self.renderer.borrow_mut();
                renderer.resize(width, height);
                renderer.render_with_clear(&view, batch, [1.0, 1.0, 1.0, 1.0]);
            }

            copy_texture = texture;
        }

        // Create readback buffer
        let buffer = self.create_readback_buffer(width, height);
        let bytes_per_row = Self::padded_bytes_per_row(width);

        // Copy texture to buffer
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Texture Copy Encoder"),
            });

        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &copy_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        // Map buffer and read pixels
        let buffer_slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .context("Failed to receive buffer map result")?
            .context("Failed to map buffer")?;

        // Read the mapped buffer
        let data = buffer_slice.get_mapped_range();

        // Create image from buffer data (removing row padding)
        let mut img: RgbaImage = ImageBuffer::new(width, height);
        for y in 0..height {
            let row_start = (y * bytes_per_row) as usize;
            let row_end = row_start + (width * 4) as usize;
            let row_data = &data[row_start..row_end];

            for x in 0..width {
                let i = (x * 4) as usize;
                img.put_pixel(
                    x,
                    y,
                    Rgba([
                        row_data[i],
                        row_data[i + 1],
                        row_data[i + 2],
                        row_data[i + 3],
                    ]),
                );
            }
        }

        drop(data);
        buffer.unmap();

        // Save as PNG
        img.save(path).context("Failed to save PNG")?;

        Ok(())
    }

    /// Compare two images and return the difference ratio (0.0 = identical, 1.0 = completely different)
    pub fn compare_images(img1: &RgbaImage, img2: &RgbaImage) -> f32 {
        if img1.dimensions() != img2.dimensions() {
            return 1.0; // Different sizes = completely different
        }

        let (width, height) = img1.dimensions();
        let total_pixels = (width * height) as f64;
        let mut diff_sum = 0.0;

        for y in 0..height {
            for x in 0..width {
                let p1 = img1.get_pixel(x, y);
                let p2 = img2.get_pixel(x, y);

                // Calculate per-channel difference
                let r_diff = (p1[0] as f64 - p2[0] as f64).abs() / 255.0;
                let g_diff = (p1[1] as f64 - p2[1] as f64).abs() / 255.0;
                let b_diff = (p1[2] as f64 - p2[2] as f64).abs() / 255.0;
                let a_diff = (p1[3] as f64 - p2[3] as f64).abs() / 255.0;

                // Average of all channels
                diff_sum += (r_diff + g_diff + b_diff + a_diff) / 4.0;
            }
        }

        (diff_sum / total_pixels) as f32
    }

    /// Generate a diff image highlighting differences between two images
    pub fn generate_diff_image(img1: &RgbaImage, img2: &RgbaImage) -> Option<RgbaImage> {
        if img1.dimensions() != img2.dimensions() {
            return None;
        }

        let (width, height) = img1.dimensions();
        let mut diff = ImageBuffer::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let p1 = img1.get_pixel(x, y);
                let p2 = img2.get_pixel(x, y);

                // Calculate difference magnitude
                let r_diff = (p1[0] as i32 - p2[0] as i32).abs();
                let g_diff = (p1[1] as i32 - p2[1] as i32).abs();
                let b_diff = (p1[2] as i32 - p2[2] as i32).abs();
                let max_diff = r_diff.max(g_diff).max(b_diff);

                // Highlight differences in red, show matching pixels dimmed
                if max_diff > 2 {
                    // Threshold for "different"
                    diff.put_pixel(x, y, Rgba([255, 0, 0, 255])); // Red for different
                } else {
                    // Dimmed version of original
                    diff.put_pixel(x, y, Rgba([p1[0] / 3, p1[1] / 3, p1[2] / 3, 128]));
                }
            }
        }

        Some(diff)
    }

    /// Run a test and save output as PNG
    pub fn run_test<F>(&self, name: &str, test_fn: F) -> Result<TestResult>
    where
        F: FnOnce(&mut TestContext),
    {
        self.run_test_with_size(
            name,
            self.default_size.width,
            self.default_size.height,
            test_fn,
        )
    }

    /// Run a test with custom size and save output as PNG
    pub fn run_test_with_size<F>(
        &self,
        name: &str,
        width: f32,
        height: f32,
        test_fn: F,
    ) -> Result<TestResult>
    where
        F: FnOnce(&mut TestContext),
    {
        let mut ctx = self.create_context_with_size(name, width, height);
        test_fn(&mut ctx);

        let batch = ctx.take_batch();
        let output_path = self.output_path(name);
        let reference_path = self.reference_path(name);

        tracing::info!(
            "Test '{}': {} primitives, {} glass, {} glyphs",
            name,
            batch.primitive_count(),
            batch.glass_count(),
            batch.glyph_count()
        );

        // Render to PNG
        self.render_to_png(&batch, width as u32, height as u32, &output_path)?;
        tracing::info!("Rendered test '{}' to {:?}", name, output_path);

        // Compare with reference if it exists
        if reference_path.exists() {
            let output_img = image::open(&output_path)
                .context("Failed to open output image")?
                .to_rgba8();
            let reference_img = image::open(&reference_path)
                .context("Failed to open reference image")?
                .to_rgba8();

            let difference = Self::compare_images(&output_img, &reference_img);

            if difference <= self.threshold {
                tracing::info!("Test '{}' PASSED (diff: {:.4}%)", name, difference * 100.0);
                Ok(TestResult::Passed)
            } else {
                // Generate diff image
                let diff_path = self.diff_path(name);
                if let Some(diff_img) = Self::generate_diff_image(&output_img, &reference_img) {
                    diff_img.save(&diff_path).ok();
                }
                tracing::warn!(
                    "Test '{}' FAILED (diff: {:.4}%, threshold: {:.4}%)",
                    name,
                    difference * 100.0,
                    self.threshold * 100.0
                );
                Ok(TestResult::Failed {
                    difference,
                    diff_path,
                })
            }
        } else {
            // No reference exists - copy output as new reference
            std::fs::copy(&output_path, &reference_path)
                .context("Failed to create reference image")?;
            tracing::info!(
                "Test '{}' created new reference at {:?}",
                name,
                reference_path
            );
            Ok(TestResult::PassedWithNewReference)
        }
    }

    /// Get the reference image path for a test
    pub fn reference_path(&self, name: &str) -> PathBuf {
        self.reference_dir.join(format!("{}.png", name))
    }

    /// Get the output image path for a test
    pub fn output_path(&self, name: &str) -> PathBuf {
        self.output_dir.join(format!("{}.png", name))
    }

    /// Get the diff image path for a test
    pub fn diff_path(&self, name: &str) -> PathBuf {
        self.output_dir.join(format!("{}_diff.png", name))
    }
}

/// Configuration for test harness
#[derive(Debug, Clone)]
pub struct TestHarnessConfig {
    /// Output directory for test results
    pub output_dir: PathBuf,
    /// Reference image directory
    pub reference_dir: PathBuf,
    /// Default viewport size
    pub default_size: Size,
    /// Difference threshold for visual comparison (0.0-1.0)
    pub threshold: f32,
    /// Maximum primitives per batch
    pub max_primitives: usize,
    /// Maximum glass primitives per batch
    pub max_glass_primitives: usize,
    /// Maximum glyphs per batch
    pub max_glyphs: usize,
    /// MSAA sample count
    pub sample_count: u32,
}

impl Default for TestHarnessConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("test_output"),
            reference_dir: PathBuf::from("test_output/references"),
            default_size: Size::new(400.0, 300.0),
            threshold: 0.001, // 0.1% difference allowed
            max_primitives: 10_000,
            max_glass_primitives: 1_000,
            max_glyphs: 50_000,
            sample_count: 4, // 4x MSAA for smooth edges
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::Color;
    use blinc_core::DrawContext;

    #[test]
    #[ignore] // Requires GPU
    fn test_harness_creation() {
        let harness = TestHarness::new().unwrap();
        assert_eq!(harness.default_size, Size::new(400.0, 300.0));
    }

    #[test]
    #[ignore] // Requires GPU
    fn test_context_creation() {
        let harness = TestHarness::new().unwrap();
        let ctx = harness.create_context("test");
        assert_eq!(ctx.name, "test");
        assert_eq!(ctx.size, Size::new(400.0, 300.0));
    }

    #[test]
    #[ignore] // Requires GPU
    fn test_render_to_png() {
        let harness = TestHarness::new().unwrap();
        let result = harness
            .run_test("simple_rect", |ctx| {
                ctx.ctx().fill_rect(
                    blinc_core::Rect::new(100.0, 100.0, 200.0, 100.0),
                    8.0.into(),
                    Color::BLUE.into(),
                );
            })
            .unwrap();

        assert!(result.is_passed());
    }
}
