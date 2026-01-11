//! Capture module for recording events and tree state.
//!
//! This module contains the core types for capturing:
//! - User interaction events (mouse, keyboard, scroll, etc.)
//! - Element tree snapshots (for debugging and replay)
//! - Primitive types (timestamps, points, rectangles)

mod events;
mod primitives;
mod tree;

pub use events::*;
pub use primitives::*;
pub use tree::*;
