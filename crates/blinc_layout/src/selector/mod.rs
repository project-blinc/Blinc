//! Element selector and programmatic control API
//!
//! This module provides high-performance element selection and manipulation:
//!
//! - `ElementRegistry` - O(1) lookup of elements by string ID
//! - `ScrollRef` - Programmatic scroll control for scroll containers
//! - `ElementHandle` - Query result with bounds, events, signals, state access
//! - `ScrollOptions` - Configuration for scroll-into-view behavior
//! - `query()` - Global function to query elements from event handlers
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
//!
//! // From event handlers, use the global query function:
//! div().on_click(|_| {
//!     if let Some(handle) = query("my-target") {
//!         handle.scroll_into_view();
//!     }
//! })
//! ```

mod div_ref;
mod handle;
mod input_ref;
mod registry;
mod scroll_ref;

use std::sync::Arc;

use blinc_core::BlincContextState;

pub use div_ref::{DivRef, DivRefInner};
pub use handle::{ElementEvent, ElementHandle, MotionHandle};
pub use input_ref::InputRef;
pub use registry::ElementRegistry;

/// Shared element registry for thread-safe access
pub type SharedElementRegistry = Arc<ElementRegistry>;

/// Query an element by ID from event handlers
///
/// This is the primary way to access elements programmatically from within
/// event handler closures. It uses the global `BlincContextState` to access
/// the element registry.
///
/// Returns `Some(ElementHandle)` if the element exists, `None` otherwise.
///
/// # Example
///
/// ```rust,ignore
/// use blinc_layout::selector::query;
///
/// div().on_click(|_| {
///     if let Some(handle) = query("my-element") {
///         handle.scroll_into_view();
///         handle.focus();
///     }
/// })
/// ```
pub fn query(id: &str) -> Option<ElementHandle<()>> {
    let ctx = BlincContextState::try_get()?;
    let registry: Arc<ElementRegistry> = ctx.element_registry()?;
    Some(ElementHandle::new(id, registry))
}

/// Query a motion animation by its stable key
///
/// Returns a `MotionHandle` that can be used to check the animation state.
/// Use this to determine if a parent motion animation has settled before
/// rendering child content with hover effects.
///
/// # Example
///
/// ```rust,ignore
/// use blinc_layout::selector::query_motion;
///
/// // Inside a Stateful on_state callback:
/// let motion = query_motion("dialog-content");
/// if motion.is_settled() {
///     // Safe to render with hover effects
///     container.merge(interactive_button());
/// } else {
///     // Render static version during animation
///     container.merge(static_button());
/// }
/// ```
pub fn query_motion(key: &str) -> MotionHandle {
    MotionHandle::new(key)
}
pub use scroll_ref::{
    PendingScroll, ScrollRef, SharedScrollRefInner, TriggerCallback, use_scroll_ref,
    use_scroll_ref_keyed,
};

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
