//! Geometric primitives
//!
//! Core types are re-exported from blinc_core for unified type system.
//! blinc_paint-specific convenience types are defined here.

// Re-export core types
pub use blinc_core::{CornerRadius, Point, Rect, Shadow, Size};

use crate::Color;

/// A circle
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Circle {
    pub center: Point,
    pub radius: f32,
}

impl Circle {
    pub const fn new(center: Point, radius: f32) -> Self {
        Self { center, radius }
    }

    pub fn contains(&self, point: Point) -> bool {
        let dx = point.x - self.center.x;
        let dy = point.y - self.center.y;
        (dx * dx + dy * dy) <= (self.radius * self.radius)
    }
}

/// An ellipse
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Ellipse {
    pub center: Point,
    pub radius_x: f32,
    pub radius_y: f32,
}

/// A rounded rectangle (convenience type combining Rect + CornerRadius)
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct RoundedRect {
    pub rect: Rect,
    pub corner_radius: CornerRadius,
}

impl RoundedRect {
    pub fn new(rect: Rect, corner_radius: CornerRadius) -> Self {
        Self {
            rect,
            corner_radius,
        }
    }

    pub fn uniform(rect: Rect, radius: f32) -> Self {
        Self {
            rect,
            corner_radius: radius.into(),
        }
    }
}

/// Shadow presets for convenience
pub mod shadow_presets {
    use super::*;

    /// Small shadow (1px offset, 2px blur)
    pub fn sm() -> Shadow {
        Shadow::new(0.0, 1.0, 2.0, Color::BLACK.with_alpha(0.1))
    }

    /// Medium shadow (4px offset, 6px blur)
    pub fn md() -> Shadow {
        Shadow {
            offset_x: 0.0,
            offset_y: 4.0,
            blur: 6.0,
            spread: -1.0,
            color: Color::BLACK.with_alpha(0.1),
        }
    }

    /// Large shadow (10px offset, 15px blur)
    pub fn lg() -> Shadow {
        Shadow {
            offset_x: 0.0,
            offset_y: 10.0,
            blur: 15.0,
            spread: -3.0,
            color: Color::BLACK.with_alpha(0.1),
        }
    }
}
