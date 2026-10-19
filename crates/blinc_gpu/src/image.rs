//! Image texture management for GPU rendering
//!
//! Manages GPU textures for images and provides rendering support.

use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Color space tag for a compressed texture upload.
///
/// BC1 and BC3 come in two wgpu variants — sRGB-decoded-on-sample
/// for color slots (diffuse, emissive) and linear for non-color
/// slots. BC4 / BC5 only exist as linear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressedColorSpace {
    /// sRGB encoding on disk; hardware decodes to linear when
    /// sampled. Use for diffuse / base color / emissive.
    Srgb,
    /// Linear encoding both on disk and in the shader. Use for
    /// normal, MR, occlusion.
    Linear,
}

/// A GPU image texture ready for rendering
pub struct GpuImage {
    /// The GPU texture
    texture: wgpu::Texture,
    /// Texture view for sampling
    view: wgpu::TextureView,
    /// Image width
    width: u32,
    /// Image height
    height: u32,
}

impl GpuImage {
    /// Create a GPU image from RGBA pixel data (linear encoding).
    pub fn from_rgba(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        width: u32,
        height: u32,
        label: Option<&str>,
    ) -> Self {
        Self::from_rgba_with_format(
            device,
            queue,
            pixels,
            wgpu::TextureFormat::Rgba8Unorm,
            width,
            height,
            label,
        )
    }

    /// Create a GPU image from sRGB-encoded RGBA pixel data.
    ///
    /// Use for diffuse / base-color / emissive textures. The sampler
    /// decodes sRGB to linear on read, so shader math sees linear
    /// values. Without this, uploading sRGB-authored assets as
    /// `Rgba8Unorm` double-applies gamma (too bright) or not at all
    /// (too bright / washed) depending on the downstream pipeline —
    /// PNG/JPEG assets from glTF are sRGB-encoded by convention.
    pub fn from_rgba_srgb(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        width: u32,
        height: u32,
        label: Option<&str>,
    ) -> Self {
        Self::from_rgba_with_format(
            device,
            queue,
            pixels,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            width,
            height,
            label,
        )
    }

    fn from_rgba_with_format(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        label: Option<&str>,
    ) -> Self {
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label,
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
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            pixels,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            texture,
            view,
            width,
            height,
        }
    }

    /// Upload RGBA pixel data, compressing to BC1/BC3 when the
    /// device supports `TEXTURE_COMPRESSION_BC` and the
    /// `bc-encode` feature is enabled. Otherwise falls back to
    /// [`Self::from_rgba`] or [`Self::from_rgba_srgb`] depending on
    /// `is_srgb`.
    ///
    /// Intended for one-time-uploaded, many-frames-read images (the
    /// 2D image widget cache, CSS mask images). BC1 for opaque,
    /// BC3 for images with meaningful alpha — decided by
    /// [`crate::bc_encode::is_effectively_opaque`]. Minimum 4×4
    /// dimensions for block coverage; smaller images skip BC and
    /// go through the uncompressed path.
    ///
    /// The encode happens inline — caller owns whatever thread the
    /// latency lives on. Budget ~50-150 ms per 2K × 2K texture on
    /// native; linear fallback when the feature is off means zero
    /// added cost for callers who never opt in.
    #[cfg(feature = "bc-encode")]
    #[allow(clippy::too_many_arguments)]
    pub fn from_rgba_maybe_compressed(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        width: u32,
        height: u32,
        is_srgb: bool,
        has_bc_support: bool,
        label: Option<&str>,
    ) -> Self {
        // BC formats quantize in 4×4 blocks, so wgpu's
        // `Device::create_texture` rejects any texture whose
        // dimensions aren't multiples of 4 for the BC formats. Most
        // call sites already filter on alignment, but keep the
        // check here as defense-in-depth — a missed guard upstream
        // should fall back to Rgba8, not panic the render loop.
        //
        // Also bail if pixel length doesn't match width*height*4 —
        // the encoder's debug_assert would fire otherwise, and in
        // release builds we'd silently corrupt.
        let can_compress = has_bc_support
            && width >= 4
            && height >= 4
            && width % 4 == 0
            && height % 4 == 0
            && pixels.len() == (width as usize) * (height as usize) * 4;
        if can_compress {
            let td = crate::bc_encode::encode_auto(pixels, width, height);
            let color_space = if is_srgb {
                CompressedColorSpace::Srgb
            } else {
                CompressedColorSpace::Linear
            };
            // `td` was just produced above; `with_bytes` can only
            // return None after a `drop_cpu_bytes()` call, which
            // nothing between construction and here performs.
            return td
                .with_bytes(|bytes| {
                    Self::from_compressed(
                        device,
                        queue,
                        bytes,
                        td.format,
                        color_space,
                        td.width,
                        td.height,
                        label,
                    )
                })
                .expect("freshly encoded TextureData retains its CPU bytes");
        }
        if is_srgb {
            Self::from_rgba_srgb(device, queue, pixels, width, height, label)
        } else {
            Self::from_rgba(device, queue, pixels, width, height, label)
        }
    }

    /// Feature-disabled variant — mirrors the signature so call
    /// sites can pin their dispatch logic regardless of whether
    /// the downstream build has `bc-encode` turned on.
    #[cfg(not(feature = "bc-encode"))]
    #[allow(clippy::too_many_arguments)]
    pub fn from_rgba_maybe_compressed(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        width: u32,
        height: u32,
        is_srgb: bool,
        _has_bc_support: bool,
        label: Option<&str>,
    ) -> Self {
        if is_srgb {
            Self::from_rgba_srgb(device, queue, pixels, width, height, label)
        } else {
            Self::from_rgba(device, queue, pixels, width, height, label)
        }
    }

    /// Slot intent for a compressed upload — determines whether the
    /// sRGB variant of the matching wgpu `TextureFormat` is used.
    ///
    /// - `Color` for diffuse / base-color / emissive (sRGB-encoded
    ///   on disk, sampled as sRGB so the shader sees linear values).
    /// - `Linear` for normal maps, metallic-roughness, occlusion, and
    ///   anything that already stores linear values.
    pub fn compressed_color_space(color: bool) -> CompressedColorSpace {
        if color {
            CompressedColorSpace::Srgb
        } else {
            CompressedColorSpace::Linear
        }
    }

    /// Create a GPU image from block-compressed pixel data.
    ///
    /// `pixels` is the packed BC block buffer — 8 bytes per 4×4
    /// block for BC1/BC4 and 16 bytes per 4×4 block for BC3/BC5.
    /// See `blinc_core::TexturePixelFormat` for the format's byte
    /// layout; the caller is responsible for producing bytes in
    /// that shape.
    ///
    /// `width` and `height` must round up to a multiple of 4 for
    /// block coverage — fractional edge blocks are the encoder's
    /// responsibility to pad.
    #[allow(clippy::too_many_arguments)]
    pub fn from_compressed(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        format: blinc_core::TexturePixelFormat,
        color_space: CompressedColorSpace,
        width: u32,
        height: u32,
        label: Option<&str>,
    ) -> Self {
        use blinc_core::TexturePixelFormat as P;
        let wgpu_format = match (format, color_space) {
            (P::Rgba8, _) => {
                // Fallback: caller asked for compressed but passed
                // Rgba8. Treat as the uncompressed path.
                return Self::from_rgba(device, queue, pixels, width, height, label);
            }
            (P::Bc1, CompressedColorSpace::Srgb) => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
            (P::Bc1, CompressedColorSpace::Linear) => wgpu::TextureFormat::Bc1RgbaUnorm,
            (P::Bc3, CompressedColorSpace::Srgb) => wgpu::TextureFormat::Bc3RgbaUnormSrgb,
            (P::Bc3, CompressedColorSpace::Linear) => wgpu::TextureFormat::Bc3RgbaUnorm,
            // BC4 and BC5 are single/dual-channel linear formats —
            // no sRGB variant exists in wgpu for them. Color-space
            // argument is accepted for uniformity but ignored.
            (P::Bc4, _) => wgpu::TextureFormat::Bc4RUnorm,
            (P::Bc5, _) => wgpu::TextureFormat::Bc5RgUnorm,
        };

        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label,
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            pixels,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            texture,
            view,
            width,
            height,
        }
    }

    /// Get the texture view for binding
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Get image dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get image width
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get image height
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Get the underlying texture
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
}

/// GPU image instance data for batched rendering
///
/// Memory layout (matches shader ImageInstance):
/// - `dst_rect`: `vec4<f32>` (16 bytes) - destination rectangle
/// - `src_uv`: `vec4<f32>` (16 bytes) - source UV coordinates
/// - `tint`: `vec4<f32>` (16 bytes) - tint color
/// - `params`: `vec4<f32>` (16 bytes) - border_radius, opacity, border_width, packed_border_color
/// - `clip_bounds`: `vec4<f32>` (16 bytes) - clip region
/// - `clip_radius`: `vec4<f32>` (16 bytes) - clip corner radii
/// - `filter_a`: `vec4<f32>` (16 bytes) - grayscale, invert, sepia, hue_rotate_rad
/// - `filter_b`: `vec4<f32>` (16 bytes) - brightness, contrast, saturate, unused
/// - `transform`: `vec4<f32>` (16 bytes) - 2x2 affine matrix [a, b, c, d]
/// - `clip2_bounds`: `vec4<f32>` (16 bytes) - secondary sharp clip (scroll boundary)
/// - `mask_params`: `vec4<f32>` (16 bytes) - mask gradient geometry
/// - `mask_info`: `vec4<f32>` (16 bytes) - mask type and alpha values
///   Total: 192 bytes
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuImageInstance {
    /// Destination rectangle (x, y, width, height) in screen pixels
    pub dst_rect: [f32; 4],
    /// Source UV rectangle (u_min, v_min, u_max, v_max) normalized 0-1
    pub src_uv: [f32; 4],
    /// Tint color (RGBA)
    pub tint: [f32; 4],
    /// Parameters: (border_radius, opacity, border_width, packed_border_color)
    pub params: [f32; 4],
    /// Clip bounds (x, y, width, height) - set to large negative x for no clip
    pub clip_bounds: [f32; 4],
    /// Clip corner radii (top-left, top-right, bottom-right, bottom-left)
    pub clip_radius: [f32; 4],
    /// CSS filter A (grayscale, invert, sepia, hue_rotate_rad)
    pub filter_a: [f32; 4],
    /// CSS filter B (brightness, contrast, saturate, unused)
    pub filter_b: [f32; 4],
    /// 2x2 CSS affine transform [a, b, c, d] applied around quad center.
    /// Identity = [1, 0, 0, 1]. Supports rotation, scale, and skew.
    pub transform: [f32; 4],
    /// Secondary clip bounds (x, y, width, height) — sharp rect, no radius.
    /// Used for scroll container boundaries separate from the primary rounded clip.
    /// Set to large negative x for no clip.
    pub clip2_bounds: [f32; 4],
    /// Mask gradient params: linear=(x1,y1,x2,y2), radial=(cx,cy,r,0) in OBB space
    pub mask_params: [f32; 4],
    /// Mask info: [mask_type, start_alpha, end_alpha, 0] (0=none, 1=linear, 2=radial)
    pub mask_info: [f32; 4],
    /// ABI metadata: clip header pointer, alpha-mask flag, mapping header pointer, reserved.
    /// Pointers are one more than their auxiliary row index; zeros keep native drawing.
    pub abi_clip: [f32; 4],
}

impl Default for GpuImageInstance {
    fn default() -> Self {
        Self {
            dst_rect: [0.0, 0.0, 100.0, 100.0],
            src_uv: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
            params: [0.0, 1.0, 0.0, 0.0], // border_radius=0, opacity=1, border_width=0, border_color=0
            // Default: no clip (large negative value disables clipping)
            clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
            clip_radius: [0.0; 4],
            // Default filter: identity (no effect)
            filter_a: [0.0, 0.0, 0.0, 0.0], // grayscale=0, invert=0, sepia=0, hue_rotate=0
            filter_b: [1.0, 1.0, 1.0, 0.0], // brightness=1, contrast=1, saturate=1, unused=0
            // Default transform: identity (no rotation, scale, or skew)
            transform: [1.0, 0.0, 0.0, 1.0], // [a, b, c, d] = identity
            // Default: no secondary clip
            clip2_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
            // Default: no mask gradient
            mask_params: [0.0; 4],
            mask_info: [0.0; 4],
            abi_clip: [0.0; 4],
        }
    }
}

impl GpuImageInstance {
    /// Create a new image instance with no transformations
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            dst_rect: [x, y, width, height],
            ..Default::default()
        }
    }

    /// Set the source UV coordinates for cropping
    pub fn with_src_uv(mut self, u_min: f32, v_min: f32, u_max: f32, v_max: f32) -> Self {
        self.src_uv = [u_min, v_min, u_max, v_max];
        self
    }

    /// Set a tint color
    pub fn with_tint(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.tint = [r, g, b, a];
        self
    }

    /// Set border radius for rounded corners
    pub fn with_border_radius(mut self, radius: f32) -> Self {
        self.params[0] = radius;
        self
    }

    /// Set opacity
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.params[1] = opacity;
        self
    }

    /// Set border (rendered in the image shader for perfect transform alignment).
    /// params\[2\] = border_width, params\[3\] = RGBA packed as u32 bitcast to f32.
    pub fn with_image_border(mut self, width: f32, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.params[2] = width;
        let ru = (r.clamp(0.0, 1.0) * 255.0).round() as u32;
        let gu = (g.clamp(0.0, 1.0) * 255.0).round() as u32;
        let bu = (b.clamp(0.0, 1.0) * 255.0).round() as u32;
        let au = (a.clamp(0.0, 1.0) * 255.0).round() as u32;
        self.params[3] = f32::from_bits((ru << 24) | (gu << 16) | (bu << 8) | au);
        self
    }

    /// Set full 2x2 affine transform [a, b, c, d] applied around quad center.
    /// Supports rotation, scale, and skew. Identity = [1, 0, 0, 1].
    pub fn with_transform(mut self, a: f32, b: f32, c: f32, d: f32) -> Self {
        self.transform = [a, b, c, d];
        self
    }

    /// Set rectangular clip region
    pub fn with_clip_rect(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.clip_bounds = [x, y, width, height];
        self.clip_radius = [0.0; 4];
        self
    }

    /// Set rounded rectangular clip region with uniform radius
    pub fn with_clip_rounded_rect(
        mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
    ) -> Self {
        self.clip_bounds = [x, y, width, height];
        self.clip_radius = [radius; 4];
        self
    }

    /// Set rounded rectangular clip region with per-corner radii
    #[allow(clippy::too_many_arguments)]
    pub fn with_clip_rounded_rect_corners(
        mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        tl: f32,
        tr: f32,
        br: f32,
        bl: f32,
    ) -> Self {
        self.clip_bounds = [x, y, width, height];
        self.clip_radius = [tl, tr, br, bl];
        self
    }

    /// Clear clip region (no clipping)
    pub fn with_no_clip(mut self) -> Self {
        self.clip_bounds = [-10000.0, -10000.0, 100000.0, 100000.0];
        self.clip_radius = [0.0; 4];
        self
    }

    /// Set secondary sharp clip (scroll container boundary, no radius)
    pub fn with_clip2_rect(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.clip2_bounds = [x, y, width, height];
        self
    }

    /// Set CSS filter parameters
    /// filter_a = (grayscale, invert, sepia, hue_rotate_rad)
    /// filter_b = (brightness, contrast, saturate, 0)
    pub fn with_filter(mut self, filter_a: [f32; 4], filter_b: [f32; 4]) -> Self {
        self.filter_a = filter_a;
        self.filter_b = filter_b;
        self
    }

    /// Get border radius
    pub fn border_radius(&self) -> f32 {
        self.params[0]
    }

    /// Get opacity
    pub fn opacity(&self) -> f32 {
        self.params[1]
    }
}

/// Image rendering context
pub struct ImageRenderingContext {
    /// Device reference
    device: Arc<wgpu::Device>,
    /// Queue reference
    queue: Arc<wgpu::Queue>,
    /// Image sampler (linear filtering)
    sampler_linear: wgpu::Sampler,
    /// Image sampler (nearest filtering, for pixel art)
    sampler_nearest: wgpu::Sampler,
}

impl ImageRenderingContext {
    /// Create a new image rendering context
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        let sampler_linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Image Sampler (Linear)"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let sampler_nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Image Sampler (Nearest)"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            device,
            queue,
            sampler_linear,
            sampler_nearest,
        }
    }

    /// Create a GPU image from RGBA data
    pub fn create_image(&self, pixels: &[u8], width: u32, height: u32) -> GpuImage {
        GpuImage::from_rgba(&self.device, &self.queue, pixels, width, height, None)
    }

    /// Create a GPU image with a label
    pub fn create_image_labeled(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
        label: &str,
    ) -> GpuImage {
        GpuImage::from_rgba(
            &self.device,
            &self.queue,
            pixels,
            width,
            height,
            Some(label),
        )
    }

    /// Create a labeled GPU image that is BC-compressed when
    /// `has_bc_support` is true (and the `bc-encode` feature is
    /// built in), otherwise falls back to the uncompressed upload.
    /// Thin wrapper over [`GpuImage::from_rgba_maybe_compressed`]
    /// so callers don't need to reach into raw device/queue.
    ///
    /// `is_srgb` selects between `Rgba8UnormSrgb` /
    /// `Bc{1,3}RgbaUnormSrgb` (for color images the sampler should
    /// decode sRGB→linear) and the linear variants. The 2D image
    /// widget cache today treats image bytes as linear, matching
    /// [`Self::create_image_labeled`]; pass `false` to keep parity.
    pub fn create_image_maybe_compressed(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
        is_srgb: bool,
        has_bc_support: bool,
        label: &str,
    ) -> GpuImage {
        GpuImage::from_rgba_maybe_compressed(
            &self.device,
            &self.queue,
            pixels,
            width,
            height,
            is_srgb,
            has_bc_support,
            Some(label),
        )
    }

    /// Get the linear sampler
    pub fn sampler_linear(&self) -> &wgpu::Sampler {
        &self.sampler_linear
    }

    /// Get the nearest sampler
    pub fn sampler_nearest(&self) -> &wgpu::Sampler {
        &self.sampler_nearest
    }

    /// Get the device
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Get the queue
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}
