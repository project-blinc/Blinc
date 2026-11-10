//! Primitive, fill, glass and clip kind codes shared with the shaders.

/// Primitive types (must match shader constants)
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrimitiveType {
    #[default]
    Rect = 0,
    Circle = 1,
    Ellipse = 2,
    Shadow = 3,
    InnerShadow = 4,
    CircleShadow = 5,
    CircleInnerShadow = 6,
    /// Text glyph - samples from atlas texture using gradient_params as UV bounds
    Text = 7,
    /// Notch: rounded rect with concave corners and optional top/bottom edge
    /// modifiers (scoop, bulge, v-cut, v-peak). SDF-composed in the fragment
    /// shader so notches inherit free AA, transforms, layer clipping, and
    /// shadow from the primary pipeline — no tessellation required.
    ///
    /// Repurposed fields (only valid when prim_type == Notch):
    ///
    /// - `perspective` → top modifier [type, width, height_or_depth, corner_radius]
    /// - `sdf_3d`      → bottom modifier [type, width, height_or_depth, corner_radius]
    /// - `light`       → per-corner type flags [TL, TR, BR, BL]
    ///   (0.0 = sharp/convex, 1.0 = concave)
    ///
    /// Modifier type codes (stored in the `type` slot as f32):
    ///   0 = none   1 = scoop   2 = bulge   3 = v-cut   4 = v-peak
    ///
    /// Notches cannot use the 3D pipeline (they reuse those slots); if you
    /// need 3D rotation on a notched shape, fall back to `Path` for that
    /// draw call.
    Notch = 8,
    /// Mesh triangle. Used to route tessellated path fills through
    /// the SDF primitive stream so their submission order interleaves
    /// naturally with text and other SDF primitives — text rendered
    /// after a shape fill paints on top, matching the caller's draw
    /// order instead of ending up behind every path draw (which is
    /// what the separate tessellated-path pipeline does on its own).
    ///
    /// Each primitive represents ONE triangle. The three corners live
    /// in the shared `aux_data` storage buffer starting at
    /// `border[2]` (the same `aux_offset` slot the 3D compound shape
    /// primitives reuse), packed as:
    ///
    /// - `aux_data[offset + 0] = (v0.x, v0.y, v1.x, v1.y)`
    /// - `aux_data[offset + 1] = (v2.x, v2.y, 0, 0)`
    ///
    /// `bounds` is pre-computed to the triangle AABB so the vertex
    /// shader still emits a tight quad and the fragment shader's
    /// point-in-triangle test only runs over covered pixels.
    Mesh = 9,
}

/// Corner style for notch primitives — matches `blinc_layout::notch::CornerStyle`
/// but lives here so the GPU side can encode it without depending on layout.
pub const NOTCH_CORNER_SHARP_OR_CONVEX: f32 = 0.0;

pub const NOTCH_CORNER_CONCAVE: f32 = 1.0;

/// Top/bottom edge modifier codes for notch primitives.
pub const NOTCH_MOD_NONE: f32 = 0.0;

pub const NOTCH_MOD_SCOOP: f32 = 1.0;

pub const NOTCH_MOD_BULGE: f32 = 2.0;

pub const NOTCH_MOD_CUT: f32 = 3.0;

pub const NOTCH_MOD_PEAK: f32 = 4.0;

/// Pipeline category for multi-pipeline SDF dispatch.
///
/// Primitives are sorted by category before GPU upload so each
/// specialized pipeline draws a contiguous range of instances.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SdfPipelineCategory {
    /// Rect, Circle, Ellipse — 2D shapes with borders, gradients, filters
    Core = 0,
    /// Shadow, InnerShadow, CircleShadow, CircleInnerShadow
    Shadow = 1,
    /// 3D raymarched shapes (shape_type > 0 in perspective.w)
    Sdf3D = 2,
    /// Notch (prim_type 8) — concave corners + edge modifiers
    Notch = 3,
    /// Text (prim_type 7) — handled by TEXT_SHADER pipeline
    Text = 4,
    /// A Core primitive that uses none of the optional features: solid fill, no mask, CSS
    /// filter, inline shadow, 3D tilt or shape clip. Drawn by the core shader compiled
    /// without those blocks. See [`GpuPrimitive::is_simple_core`](super::GpuPrimitive::is_simple_core).
    CoreSimple = 5,
    /// A `CoreSimple` rect or circle with nothing but circular corners, a uniform border and
    /// a rounded rect clip. Drawn by the compact `sdf_simple.wgsl`. See
    /// [`GpuPrimitive::is_plain_core`](super::GpuPrimitive::is_plain_core).
    CorePlain = 6,
}

/// Fill types (must match shader constants)
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillType {
    #[default]
    Solid = 0,
    LinearGradient = 1,
    RadialGradient = 2,
    ConicGradient = 3,
}

/// Glass material types (must match shader constants)
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlassType {
    UltraThin = 0,
    Thin = 1,
    #[default]
    Regular = 2,
    Thick = 3,
    Chrome = 4,
    /// Simple frosted glass - pure blur + tint, no liquid glass effects
    Simple = 5,
}

/// Clip types for primitives
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipType {
    /// No clipping
    #[default]
    None = 0,
    /// Rectangular clip (with optional rounded corners)
    Rect = 1,
    /// Circular clip
    Circle = 2,
    /// Elliptical clip
    Ellipse = 3,
    /// Polygon clip (winding number test via aux_data vertices)
    Polygon = 4,
}
