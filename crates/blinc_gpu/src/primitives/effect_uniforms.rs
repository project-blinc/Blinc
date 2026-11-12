//! Uniform blocks for layer effects: blur, colour matrix, drop shadow, glow and mask image.

/// Uniforms for Kawase blur shader
///
/// Memory layout (32 bytes total, padded to 16-byte alignment):
/// - texel_size: `vec2<f32>` (8 bytes) - inverse texture size (1/width, 1/height)
/// - radius: `f32` (4 bytes) - blur radius
/// - iteration: `u32` (4 bytes) - current pass iteration
/// - blur_alpha: `u32` (4 bytes) - whether to blur alpha (1) or preserve it (0)
/// - _pad1, _pad2, _pad3: `f32` (12 bytes) - padding to 32 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlurUniforms {
    /// Inverse texture size (1/width, 1/height)
    pub texel_size: [f32; 2],
    /// Blur radius
    pub radius: f32,
    /// Current iteration (0, 1, 2, ...) for multi-pass blur
    pub iteration: u32,
    /// Whether to blur alpha channel (1 = blur alpha, 0 = preserve original alpha)
    /// Use blur_alpha=0 for element blur (preserves corner radius)
    /// Use blur_alpha=1 for shadow blur (creates soft edges)
    pub blur_alpha: u32,
    /// Padding for 16-byte alignment
    pub _pad1: f32,
    pub _pad2: f32,
    pub _pad3: f32,
}

/// Uniforms for color matrix shader
///
/// Memory layout (80 bytes total):
/// - row0-row3: 4 x `vec4<f32>` (64 bytes) - 4x4 matrix rows
/// - offset: `vec4<f32>` (16 bytes) - offset/bias for each channel
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorMatrixUniforms {
    /// Row 0: [m0, m1, m2, m3] - R coefficients
    pub row0: [f32; 4],
    /// Row 1: [m5, m6, m7, m8] - G coefficients
    pub row1: [f32; 4],
    /// Row 2: [m10, m11, m12, m13] - B coefficients
    pub row2: [f32; 4],
    /// Row 3: [m15, m16, m17, m18] - A coefficients
    pub row3: [f32; 4],
    /// Offset: [m4, m9, m14, m19] - bias for R, G, B, A
    pub offset: [f32; 4],
}

impl Default for ColorMatrixUniforms {
    fn default() -> Self {
        // Identity matrix (no color transformation)
        Self {
            row0: [1.0, 0.0, 0.0, 0.0],
            row1: [0.0, 1.0, 0.0, 0.0],
            row2: [0.0, 0.0, 1.0, 0.0],
            row3: [0.0, 0.0, 0.0, 1.0],
            offset: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

impl ColorMatrixUniforms {
    /// Create from a 4x5 matrix (row-major, 20 elements)
    pub fn from_matrix(matrix: &[f32; 20]) -> Self {
        Self {
            row0: [matrix[0], matrix[1], matrix[2], matrix[3]],
            row1: [matrix[5], matrix[6], matrix[7], matrix[8]],
            row2: [matrix[10], matrix[11], matrix[12], matrix[13]],
            row3: [matrix[15], matrix[16], matrix[17], matrix[18]],
            offset: [matrix[4], matrix[9], matrix[14], matrix[19]],
        }
    }
}

/// Uniforms for drop shadow shader
///
/// Memory layout (48 bytes total):
/// - offset: `vec2<f32>` (8 bytes) - shadow offset in pixels
/// - blur_radius: `f32` (4 bytes) - blur radius
/// - spread: `f32` (4 bytes) - spread (expand/contract)
/// - color: `vec4<f32>` (16 bytes) - shadow color RGBA
/// - texel_size: `vec2<f32>` (8 bytes) - inverse texture size
/// - _pad: `vec2<f32>` (8 bytes) - padding for alignment
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DropShadowUniforms {
    /// Shadow offset in pixels (x, y)
    pub offset: [f32; 2],
    /// Blur radius
    pub blur_radius: f32,
    /// Spread (positive expands, negative contracts)
    pub spread: f32,
    /// Shadow color (RGBA)
    pub color: [f32; 4],
    /// Inverse texture size (1/width, 1/height)
    pub texel_size: [f32; 2],
    /// Padding for 16-byte alignment
    pub _pad: [f32; 2],
}

impl Default for DropShadowUniforms {
    fn default() -> Self {
        Self {
            offset: [4.0, 4.0],
            blur_radius: 8.0,
            spread: 0.0,
            color: [0.0, 0.0, 0.0, 0.5], // 50% black
            texel_size: [1.0 / 800.0, 1.0 / 600.0],
            _pad: [0.0, 0.0],
        }
    }
}

/// Uniforms for the glow effect shader
///
/// Memory layout:
/// - color: `vec4<f32>` (16 bytes) - glow color RGBA
/// - blur: `f32` (4 bytes) - blur softness
/// - range: `f32` (4 bytes) - glow range
/// - opacity: `f32` (4 bytes) - glow opacity
/// - _pad0: `f32` (4 bytes) - padding
/// - texel_size: `vec2<f32>` (8 bytes) - inverse texture size
/// - _pad1: `vec2<f32>` (8 bytes) - padding for alignment
///   Total: 48 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlowUniforms {
    /// Glow color (RGBA)
    pub color: [f32; 4],
    /// Blur softness (affects falloff smoothness)
    pub blur: f32,
    /// Glow range (how far the glow extends)
    pub range: f32,
    /// Glow opacity (0-1)
    pub opacity: f32,
    /// Padding for alignment
    pub _pad0: f32,
    /// Inverse texture size (1/width, 1/height)
    pub texel_size: [f32; 2],
    /// Padding for 16-byte alignment
    pub _pad1: [f32; 2],
}

impl Default for GlowUniforms {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0, 1.0], // White glow
            blur: 8.0,
            range: 4.0,
            opacity: 1.0,
            _pad0: 0.0,
            texel_size: [1.0 / 800.0, 1.0 / 600.0],
            _pad1: [0.0, 0.0],
        }
    }
}

/// Uniforms for the mask image effect shader
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaskImageUniforms {
    /// 0 = alpha, 1 = luminance
    pub mask_mode: u32,
    /// Reserve the shader's aligned vec3 and trailing structure padding.
    pub _pad: [f32; 7],
}
