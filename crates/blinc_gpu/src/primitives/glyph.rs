//! Text glyph instance.

/// A GPU text glyph instance (matches shader `GlyphInstance` struct)
///
/// Memory layout:
/// - bounds: `vec4<f32>`       (16 bytes) - position and size
/// - uv_bounds: `vec4<f32>`    (16 bytes) - UV coordinates in atlas
/// - color: `vec4<f32>`        (16 bytes) - text color
/// - clip_bounds: `vec4<f32>`  (16 bytes) - clip region (x, y, width, height)
/// - clip_fade: `vec4<f32>`   (16 bytes) - overflow fade distances (top, right, bottom, left)
///   Total: 96 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuGlyph {
    /// Position and size (x, y, width, height)
    pub bounds: [f32; 4],
    /// UV coordinates in atlas (u_min, v_min, u_max, v_max)
    pub uv_bounds: [f32; 4],
    /// Text color (RGBA)
    pub color: [f32; 4],
    /// Clip bounds (x, y, width, height) - set to large values for no clip
    pub clip_bounds: [f32; 4],
    /// Overflow fade distances (top, right, bottom, left) in pixels
    pub clip_fade: [f32; 4],
    /// Flags: [is_color, path_header, node_slot, unused]. The path header is a
    /// one-based auxiliary row in the owning batch, or zero for no shape clip.
    /// is_color: 1.0 for color emoji (use color atlas), 0.0 for grayscale (use main atlas).
    /// node_slot: the animated node the glyph follows, as `slot + 1`, or zero for none.
    pub flags: [f32; 4],
}

impl Default for GpuGlyph {
    fn default() -> Self {
        Self {
            bounds: [0.0; 4],
            uv_bounds: [0.0, 0.0, 1.0, 1.0],
            color: [0.0, 0.0, 0.0, 1.0],
            // Default: no clip (large bounds that won't clip anything)
            clip_bounds: [-10000.0, -10000.0, 100000.0, 100000.0],
            clip_fade: [0.0; 4], // No fade (hard clip)
            flags: [0.0; 4],     // Not a color glyph by default
        }
    }
}

impl GpuGlyph {
    /// The animated node this glyph follows, if any.
    pub fn node_slot(&self) -> Option<u32> {
        (self.flags[2] >= 1.0).then(|| self.flags[2] as u32 - 1)
    }

    /// Follow the animated node in `slot`, or none.
    pub fn with_node_slot(mut self, slot: Option<u32>) -> Self {
        self.flags[2] = slot.map_or(0.0, |s| (s + 1) as f32);
        self
    }

    /// Set rectangular clip bounds for this glyph
    pub fn with_clip_rect(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.clip_bounds = [x, y, width, height];
        self
    }

    /// Clear clip bounds (no clipping)
    pub fn with_no_clip(mut self) -> Self {
        self.clip_bounds = [-10000.0, -10000.0, 100000.0, 100000.0];
        self.flags[1] = 0.0;
        self
    }
}
