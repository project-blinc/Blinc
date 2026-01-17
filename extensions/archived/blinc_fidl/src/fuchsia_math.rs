//! Stub types for fuchsia.math FIDL library
//!
//! These provide API compatibility for cross-compilation.
//! On actual Fuchsia, use the generated fidl_fuchsia_math crate.

/// Size with unsigned integer dimensions
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SizeU {
    /// Width
    pub width: u32,
    /// Height
    pub height: u32,
}

/// Size with floating point dimensions
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SizeF {
    /// Width
    pub width: f32,
    /// Height
    pub height: f32,
}

/// Rectangle with unsigned integer dimensions
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RectU {
    /// X position
    pub x: u32,
    /// Y position
    pub y: u32,
    /// Width
    pub width: u32,
    /// Height
    pub height: u32,
}

/// Rectangle with floating point dimensions
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RectF {
    /// X position
    pub x: f32,
    /// Y position
    pub y: f32,
    /// Width
    pub width: f32,
    /// Height
    pub height: f32,
}

/// Insets (padding/margins)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InsetsF {
    /// Top inset
    pub top: f32,
    /// Right inset
    pub right: f32,
    /// Bottom inset
    pub bottom: f32,
    /// Left inset
    pub left: f32,
}

/// 2D vector
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    /// X component
    pub x: f32,
    /// Y component
    pub y: f32,
}

/// 3x3 transformation matrix (row-major)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    /// Matrix elements
    pub matrix: [f32; 9],
}

impl Default for Mat3 {
    fn default() -> Self {
        Self {
            matrix: [
                1.0, 0.0, 0.0,
                0.0, 1.0, 0.0,
                0.0, 0.0, 1.0,
            ],
        }
    }
}
