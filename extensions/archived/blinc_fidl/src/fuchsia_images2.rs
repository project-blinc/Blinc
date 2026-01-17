//! Stub types for fuchsia.images2 FIDL library
//!
//! These provide API compatibility for cross-compilation.
//! On actual Fuchsia, use the generated fidl_fuchsia_images2 crate.

/// Pixel format for images
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum PixelFormat {
    /// Invalid/unknown format
    Invalid = 0,
    /// 32-bit RGBA (R in lowest byte)
    R8G8B8A8 = 1,
    /// 32-bit BGRA (B in lowest byte)
    B8G8R8A8 = 101,
    /// YUV420 planar
    I420 = 102,
    /// NV12 (Y plane + interleaved UV)
    Nv12 = 103,
    /// YUY2 packed
    Yuy2 = 104,
    /// RGB565
    R5G6B5 = 105,
    /// RGB888
    R8G8B8 = 106,
    /// Luminance (grayscale) 8-bit
    L8 = 107,
    /// RG88 (two-channel)
    R8G8 = 108,
    /// Single-channel 8-bit
    R8 = 109,
    /// 16-bit per channel RGBA
    R16G16B16A16Float = 110,
    /// 10-bit per channel RGBA (2-bit alpha)
    A2R10G10B10 = 111,
    /// 10-bit per channel BGRA (2-bit alpha)
    A2B10G10R10 = 112,
}

impl Default for PixelFormat {
    fn default() -> Self {
        Self::Invalid
    }
}

/// Color space for images
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ColorSpace {
    /// Unknown color space
    Unknown = 0,
    /// sRGB color space
    Srgb = 1,
    /// BT.601 NTSC
    Rec601Ntsc = 2,
    /// BT.601 PAL
    Rec601Pal = 3,
    /// BT.709 (HD video)
    Rec709 = 4,
    /// BT.2020 (UHD video)
    Rec2020 = 5,
    /// BT.2100 HLG
    Rec2100Hlg = 6,
    /// BT.2100 PQ
    Rec2100Pq = 7,
    /// Pass-through (no conversion)
    Passthrough = 8,
}

impl Default for ColorSpace {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Image format combining pixel format and color space
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageFormat {
    /// Pixel format
    pub pixel_format: PixelFormat,
    /// Color space
    pub color_space: ColorSpace,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bytes per row (stride)
    pub bytes_per_row: u32,
}
