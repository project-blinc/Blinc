//! Element selector and programmatic control API
//!
//! This module provides high-performance element selection and manipulation:
//!
//! - `ElementRegistry` - O(1) lookup of elements by string ID
//! - `ScrollRef` - Programmatic scroll control for scroll containers
//! - `ElementHandle` - Query result with bounds, events, signals, state access
//! - `ScrollOptions` - Configuration for scroll-into-view behavior
//!
//! # Example
//!
//! ```rust,ignore
//! use blinc_layout::prelude::*;
//!
//! // Assign IDs to elements
//! div()
//!     .id("my-container")
//!     .child(
//!         scroll()
//!             .bind(&scroll_ref)
//!             .child(items.iter().map(|i| div().id(format!("item-{}", i.id))))
//!     )
//!
//! // Later: scroll to element
//! scroll_ref.scroll_to("item-42");
//! ```

mod handle;
mod registry;
mod scroll_ref;

pub use handle::{ElementEvent, ElementHandle};
pub use registry::ElementRegistry;
pub use scroll_ref::{PendingScroll, ScrollRef, SharedScrollRefInner, TriggerCallback};

/// Options for scroll-into-view behavior
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollOptions {
    /// How to animate the scroll
    pub behavior: ScrollBehavior,
    /// Vertical alignment within the viewport
    pub block: ScrollBlock,
    /// Horizontal alignment within the viewport
    pub inline: ScrollInline,
}

impl Default for ScrollOptions {
    fn default() -> Self {
        Self {
            behavior: ScrollBehavior::Auto,
            block: ScrollBlock::Nearest,
            inline: ScrollInline::Nearest,
        }
    }
}

/// Scroll animation behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollBehavior {
    /// Instant scroll (no animation)
    #[default]
    Auto,
    /// Smooth animated scroll
    Smooth,
}

/// Vertical scroll alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollBlock {
    /// Align to top of viewport
    Start,
    /// Align to center of viewport
    Center,
    /// Align to bottom of viewport
    End,
    /// Scroll minimum distance to make visible
    #[default]
    Nearest,
}

/// Horizontal scroll alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollInline {
    /// Align to left of viewport
    Start,
    /// Align to center of viewport
    Center,
    /// Align to right of viewport
    End,
    /// Scroll minimum distance to make visible
    #[default]
    Nearest,
}
