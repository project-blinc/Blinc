//! Frame, glass, composite and path uniform blocks.

use super::ClipType;

/// Uniform buffer for viewport information
///
/// `_padding` carries two values the SDF shaders read: `[0]` is 1.0 while an overlay draws into
/// its multisampled target, and `[1]` is the frame clock's time in seconds. Build one with
/// [`Uniforms::new`] to have both set.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub viewport_size: [f32; 2],
    pub _padding: [f32; 2],
}

impl Uniforms {
    /// Uniforms for a `viewport_size` target, stamped with the frame clock's time.
    pub fn new(viewport_size: [f32; 2]) -> Self {
        Self {
            viewport_size,
            _padding: [0.0, blinc_core::FrameClock::now_secs()],
        }
    }

    /// With the multisampled-target flag set to `flag`.
    pub fn with_msaa(mut self, flag: f32) -> Self {
        self._padding[0] = flag;
        self
    }

    /// The frame clock's time in seconds when these were built.
    pub fn time(&self) -> f32 {
        self._padding[1]
    }
}

/// Uniform buffer for glass shader
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlassUniforms {
    pub viewport_size: [f32; 2],
    pub time: f32,
    /// Nonzero when the retained image applies the captured clip coverage.
    pub clip_in_image: f32,
    /// World frame used for backdrop UVs and blur distances.
    pub sample_size: [f32; 2],
    /// World coordinate of the target's top-left pixel.
    pub origin: [f32; 2],
}

/// Uniform buffer for compositor
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CompositeUniforms {
    pub opacity: f32,
    pub blend_mode: u32,
    pub _padding: [f32; 2],
}

/// Uniform buffer for layer composition
///
/// Layout matches LAYER_COMPOSITE_SHADER uniforms:
/// - source_rect: `vec4<f32>` (16 bytes) - Source rectangle in layer texture (normalized 0-1)
/// - dest_rect: `vec4<f32>` (16 bytes) - Destination rectangle in viewport (pixels)
/// - viewport_size: `vec2<f32>` (8 bytes)
/// - opacity: f32 (4 bytes)
/// - blend_mode: u32 (4 bytes)
/// - clip_bounds: `vec4<f32>` (16 bytes) - Clip region (x, y, width, height)
/// - clip_radius: `vec4<f32>` (16 bytes) - Corner radii (tl, tr, br, bl)
/// - clip_type: u32 (4 bytes) - 0=none, 1=rect
/// - perspective_d: f32 (4 bytes) - Perspective distance (0 = no 3D)
/// - sin_rx: f32 (4 bytes) - sin(rotate-x angle)
/// - cos_rx: f32 (4 bytes) - cos(rotate-x angle)
/// - sin_ry: f32 (4 bytes) - sin(rotate-y angle)
/// - cos_ry: f32 (4 bytes) - cos(rotate-y angle)
/// - sin_rz: f32 (4 bytes) - sin(rotate-z angle), used by the flat
///   composite path for in-plane rotation (motion-bound bake-and-blit)
/// - cos_rz: f32 (4 bytes) - cos(rotate-z angle)
///   Total: 112 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LayerCompositeUniforms {
    /// Source rectangle in layer texture (x, y, width, height) - normalized 0-1
    pub source_rect: [f32; 4],
    /// Destination rectangle in viewport (x, y, width, height) - pixels
    pub dest_rect: [f32; 4],
    /// Viewport size for coordinate conversion
    pub viewport_size: [f32; 2],
    /// Layer opacity (0.0 - 1.0)
    pub opacity: f32,
    /// Blend mode (see BlendMode enum)
    pub blend_mode: u32,
    /// Clip bounds (x, y, width, height) in pixels
    pub clip_bounds: [f32; 4],
    /// Clip corner radii (top-left, top-right, bottom-right, bottom-left)
    pub clip_radius: [f32; 4],
    /// Clip type: 0=none, 1=rect with optional rounded corners
    pub clip_type: u32,
    /// Perspective distance in physical pixels (0 = no 3D transform)
    pub perspective_d: f32,
    /// sin(rotate-x angle)
    pub sin_rx: f32,
    /// cos(rotate-x angle)
    pub cos_rx: f32,
    /// sin(rotate-y angle)
    pub sin_ry: f32,
    /// cos(rotate-y angle)
    pub cos_ry: f32,
    /// sin(rotate-z angle) — in-plane rotation applied to the flat
    /// (non-perspective) composite path. Lets motion-bound subtrees
    /// (e.g. cn::spinner's rotate_timeline) rotate the cached layer
    /// texture per-frame without re-baking. Identity = (0.0, 1.0).
    pub sin_rz: f32,
    /// cos(rotate-z angle) — see [`Self::sin_rz`].
    pub cos_rz: f32,
}

impl LayerCompositeUniforms {
    /// Create uniforms for full-layer composition at a specific position
    pub fn new(
        layer_size: (u32, u32),
        dest_x: f32,
        dest_y: f32,
        viewport_size: (f32, f32),
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) -> Self {
        Self {
            source_rect: [0.0, 0.0, 1.0, 1.0], // Full texture
            dest_rect: [dest_x, dest_y, layer_size.0 as f32, layer_size.1 as f32],
            viewport_size: [viewport_size.0, viewport_size.1],
            opacity,
            blend_mode: blend_mode as u32,
            clip_bounds: [0.0, 0.0, viewport_size.0, viewport_size.1], // No clipping
            clip_radius: [0.0, 0.0, 0.0, 0.0],
            clip_type: 0, // No clip
            perspective_d: 0.0,
            sin_rx: 0.0,
            cos_rx: 1.0,
            sin_ry: 0.0,
            cos_ry: 1.0,
            sin_rz: 0.0,
            cos_rz: 1.0,
        }
    }

    /// Create uniforms for sub-region composition
    pub fn with_source_rect(
        source_rect: [f32; 4],
        dest_rect: [f32; 4],
        viewport_size: (f32, f32),
        opacity: f32,
        blend_mode: blinc_core::BlendMode,
    ) -> Self {
        Self {
            source_rect,
            dest_rect,
            viewport_size: [viewport_size.0, viewport_size.1],
            opacity,
            blend_mode: blend_mode as u32,
            clip_bounds: [0.0, 0.0, viewport_size.0, viewport_size.1], // No clipping
            clip_radius: [0.0, 0.0, 0.0, 0.0],
            clip_type: 0, // No clip
            perspective_d: 0.0,
            sin_rx: 0.0,
            cos_rx: 1.0,
            sin_ry: 0.0,
            cos_ry: 1.0,
            sin_rz: 0.0,
            cos_rz: 1.0,
        }
    }

    /// Set clip region with optional rounded corners
    pub fn with_clip(mut self, bounds: [f32; 4], radius: [f32; 4]) -> Self {
        self.clip_bounds = bounds;
        self.clip_radius = radius;
        self.clip_type = 1;
        self
    }
}

/// Uniform buffer for path rendering
/// Layout matches shader struct exactly:
/// - viewport_size: `vec2<f32>` (8 bytes)
/// - opacity: f32 (4 bytes)
/// - _pad0: f32 (4 bytes)
/// - transform_row0: `vec4<f32>` (16 bytes)
/// - transform_row1: `vec4<f32>` (16 bytes)
/// - transform_row2: `vec4<f32>` (16 bytes)
/// - clip_bounds: `vec4<f32>` (16 bytes) - (x, y, width, height) or (cx, cy, rx, ry)
/// - clip_radius: `vec4<f32>` (16 bytes) - corner radii (tl, tr, br, bl) or (rx, ry, 0, 0)
/// - clip_type: `u32` (4 bytes) - 0=none, 1=rect, 2=circle, 3=ellipse
/// - use_gradient_texture: `u32` (4 bytes) - 0=use vertex colors, 1=sample gradient texture
/// - use_image_texture: `u32` (4 bytes) - 0=no image, 1=sample image texture
/// - use_glass_effect: `u32` (4 bytes) - 0=no glass, 1=glass effect on path
/// - image_uv_bounds: `vec4<f32>` (16 bytes) - (u_min, v_min, u_max, v_max) for image UV mapping
/// - glass_params: `vec4<f32>` (16 bytes) - (blur_radius, saturation, tint_strength, opacity)
/// - glass_tint: `vec4<f32>` (16 bytes) - glass tint color RGBA
///   Total: 160 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PathUniforms {
    pub viewport_size: [f32; 2],
    pub opacity: f32,
    pub _pad0: f32,
    /// 3x3 transform matrix stored as 3 vec4s (xyz used, w is padding)
    pub transform: [[f32; 4]; 3],
    /// Clip bounds: (x, y, width, height) for rects, (cx, cy, rx, ry) for circles/ellipses
    pub clip_bounds: [f32; 4],
    /// Clip corner radii for rounded rects: (top-left, top-right, bottom-right, bottom-left)
    /// For circles/ellipses: (rx, ry, 0, 0)
    pub clip_radius: [f32; 4],
    /// Clip type: 0=none, 1=rect, 2=circle, 3=ellipse
    pub clip_type: u32,
    /// Use gradient texture: 0=use vertex colors (2-stop fast path), 1=sample gradient texture
    pub use_gradient_texture: u32,
    /// Use image texture: 0=no image, 1=sample image texture for brush
    pub use_image_texture: u32,
    /// Use glass effect: 0=no glass, 1=glass effect on path (requires mask texture)
    pub use_glass_effect: u32,
    /// Image UV bounds for mapping: (u_min, v_min, u_max, v_max)
    pub image_uv_bounds: [f32; 4],
    /// Glass parameters: (blur_radius, saturation, tint_strength, opacity)
    pub glass_params: [f32; 4],
    /// Glass tint color (RGBA)
    pub glass_tint: [f32; 4],
}

impl Default for PathUniforms {
    fn default() -> Self {
        Self {
            viewport_size: [800.0, 600.0],
            opacity: 1.0,
            _pad0: 0.0,
            // Identity matrix (row-major: row0 = [1,0,0], row1 = [0,1,0], row2 = [0,0,1])
            transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
            ],
            // Default: no clipping (huge bounds)
            clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
            clip_radius: [0.0; 4],
            clip_type: ClipType::None as u32,
            use_gradient_texture: 0, // Default: use vertex colors (2-stop fast path)
            use_image_texture: 0,    // Default: no image texture
            use_glass_effect: 0,     // Default: no glass effect
            image_uv_bounds: [0.0, 0.0, 1.0, 1.0], // Default: full UV range
            glass_params: [20.0, 1.0, 0.5, 0.9], // Default: blur=20, sat=1, tint=0.5, opacity=0.9
            glass_tint: [1.0, 1.0, 1.0, 0.3], // Default: white with 30% alpha
        }
    }
}
