//! Path building and representation
//!
//! Core types are re-exported from blinc_core for unified type system.
//! PathBuilder provides a fluent API for path construction.

// Re-export core types
pub use blinc_core::{Path, PathCommand, Point};

/// Builder for constructing paths with fluent API
///
/// PathBuilder provides a more ergonomic API compared to Path's chainable methods,
/// maintaining cursor state for relative operations.
pub struct PathBuilder {
    path: Path,
    current: Point,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self {
            path: Path::new(),
            current: Point::ZERO,
        }
    }

    pub fn move_to(mut self, x: f32, y: f32) -> Self {
        self.path = self.path.move_to(x, y);
        self.current = Point::new(x, y);
        self
    }

    pub fn line_to(mut self, x: f32, y: f32) -> Self {
        self.path = self.path.line_to(x, y);
        self.current = Point::new(x, y);
        self
    }

    pub fn quad_to(mut self, cx: f32, cy: f32, x: f32, y: f32) -> Self {
        self.path = self.path.quad_to(cx, cy, x, y);
        self.current = Point::new(x, y);
        self
    }

    pub fn cubic_to(mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) -> Self {
        self.path = self.path.cubic_to(c1x, c1y, c2x, c2y, x, y);
        self.current = Point::new(x, y);
        self
    }

    pub fn close(mut self) -> Self {
        self.path = self.path.close();
        self
    }

    pub fn build(self) -> Path {
        self.path
    }

    /// Get the current cursor position
    pub fn current_position(&self) -> Point {
        self.current
    }
}

impl Default for PathBuilder {
    fn default() -> Self {
        Self::new()
    }
}
