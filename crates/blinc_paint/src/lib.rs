//! Blinc Paint/Canvas API
//!
//! A 2D drawing API for custom graphics, similar to HTML Canvas or Skia.
//! All core types are unified with blinc_core for seamless integration
//! with the GPU renderer.
//!
//! # Features
//!
//! - Path drawing (lines, curves, arcs)
//! - Shape primitives (rect, circle, rounded rect)
//! - Fills and strokes with colors, gradients
//! - Text rendering
//! - DrawContext implementation for GPU rendering
//!
//! # Example
//!
//! ```ignore
//! use blinc_paint::{PaintContext, Color, Rect};
//! use blinc_core::DrawContext;
//!
//! let mut ctx = PaintContext::new(800.0, 600.0);
//!
//! // Canvas-like API
//! ctx.fill_rect_xywh(10.0, 20.0, 100.0, 50.0, Color::BLUE);
//! ctx.translate(50.0, 50.0);
//! ctx.fill_rounded_rect_xywh(0.0, 0.0, 80.0, 40.0, 8.0, Color::RED);
//!
//! // Or use the DrawContext API directly
//! ctx.fill_rect(Rect::new(0.0, 0.0, 50.0, 25.0), 4.0.into(), Color::GREEN.into());
//!
//! // Get commands for GPU execution
//! let commands = ctx.take_commands();
//! ```

pub mod context;
pub mod gradient;
pub mod path;
pub mod primitives;

// Re-export modules
pub mod color {
    //! Color types - re-exported from blinc_core
    pub use blinc_core::Color;
}

// ─────────────────────────────────────────────────────────────────────────────
// Core type re-exports from blinc_core (unified type system)
// ─────────────────────────────────────────────────────────────────────────────

pub use blinc_core::{
    // Brushes and fills
    Brush,
    // Colors
    Color,
    // Corner radius
    CornerRadius,
    // Draw context trait
    DrawCommand,
    DrawContext,
    DrawContextExt,
    // Gradients
    Gradient,
    GradientStop,
    // Strokes
    LineCap,
    LineJoin,
    // Paths
    Path,
    PathCommand,
    // Geometry
    Point,
    Rect,
    // Shadows
    Shadow,
    Size,
    Stroke,
    // Text
    TextStyle,
    // Transforms
    Transform,
};

// ─────────────────────────────────────────────────────────────────────────────
// blinc_paint specific exports
// ─────────────────────────────────────────────────────────────────────────────

pub use context::PaintContext;
pub use path::PathBuilder;
pub use primitives::{shadow_presets, Circle, Ellipse, RoundedRect};
