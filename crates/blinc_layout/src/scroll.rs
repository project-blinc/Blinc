//! Scroll container element with webkit-style bounce physics
//!
//! Provides a scrollable container with smooth momentum and spring-based
//! edge bounce, similar to iOS/macOS native scroll behavior.
//!
//! # Example
//!
//! ```rust
//! use blinc_layout::prelude::*;
//!
//! let ui = scroll()
//!     .h(400.0)  // Viewport height
//!     .child(
//!         div().flex_col().gap(8.0)
//!             .child(text("Item 1"))
//!             .child(text("Item 2"))
//!             // ... many items that overflow
//!     )
//!     .on_scroll(|e| println!("Scrolled: {}", e.scroll_delta_y));
//! ```
//!
//! # Features
//!
//! - **Smooth momentum**: Continues scrolling after release with natural deceleration
//! - **Edge bounce**: Webkit-style spring animation when scrolling past edges
//! - **Glass-aware clipping**: Content clips properly even for glass/blur elements
//! - **FSM-based state**: Clear state machine for Idle, Scrolling, Decelerating, Bouncing

use std::sync::{Arc, Mutex};

use blinc_animation::{Spring, SpringConfig};
use blinc_core::{Brush, Shadow};

use crate::div::{Div, ElementBuilder, ElementTypeId};
use crate::element::RenderProps;
use crate::event_handler::{EventContext, EventHandlers};
use crate::stateful::{scroll_events, ScrollState, StateTransitions};
use crate::tree::{LayoutNodeId, LayoutTree};

// ============================================================================
// Scroll Configuration
// ============================================================================

/// Configuration for scroll behavior
#[derive(Debug, Clone, Copy)]
pub struct ScrollConfig {
    /// Enable bounce physics at edges (default: true)
    pub bounce_enabled: bool,
    /// Spring configuration for bounce animation
    pub bounce_spring: SpringConfig,
    /// Friction coefficient for deceleration (0.0-1.0, higher = more friction)
    pub friction: f32,
    /// Minimum velocity threshold for stopping (pixels/second)
    pub velocity_threshold: f32,
    /// Maximum overscroll distance as fraction of viewport (0.0-0.5)
    pub max_overscroll: f32,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            bounce_enabled: true,
            bounce_spring: SpringConfig::wobbly(),
            friction: 0.95,
            velocity_threshold: 0.5,
            max_overscroll: 0.3,
        }
    }
}

impl ScrollConfig {
    /// Create config with bounce disabled
    pub fn no_bounce() -> Self {
        Self {
            bounce_enabled: false,
            ..Default::default()
        }
    }

    /// Create config with stiff bounce (less wobbly)
    pub fn stiff_bounce() -> Self {
        Self {
            bounce_spring: SpringConfig::stiff(),
            ..Default::default()
        }
    }

    /// Create config with gentle bounce (more wobbly)
    pub fn gentle_bounce() -> Self {
        Self {
            bounce_spring: SpringConfig::gentle(),
            ..Default::default()
        }
    }
}

// ============================================================================
// Scroll Physics State
// ============================================================================

/// Internal physics state for scroll animation
#[derive(Clone)]
pub struct ScrollPhysics {
    /// Current scroll offset (negative = scrolled down)
    pub offset_y: f32,
    /// Current velocity (pixels per second)
    pub velocity_y: f32,
    /// Spring animator for bounce (None when not bouncing)
    pub spring: Option<Spring>,
    /// Current FSM state
    pub state: ScrollState,
    /// Content height (calculated from children)
    pub content_height: f32,
    /// Viewport height
    pub viewport_height: f32,
    /// Configuration
    pub config: ScrollConfig,
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        Self {
            offset_y: 0.0,
            velocity_y: 0.0,
            spring: None,
            state: ScrollState::Idle,
            content_height: 0.0,
            viewport_height: 0.0,
            config: ScrollConfig::default(),
        }
    }
}

impl ScrollPhysics {
    /// Create new physics with given config
    pub fn new(config: ScrollConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// Maximum scroll offset (0 = top edge)
    pub fn min_offset(&self) -> f32 {
        0.0
    }

    /// Minimum scroll offset (negative, at bottom edge)
    pub fn max_offset(&self) -> f32 {
        let scrollable = self.content_height - self.viewport_height;
        if scrollable > 0.0 {
            -scrollable
        } else {
            0.0
        }
    }

    /// Check if currently overscrolling (past bounds)
    pub fn is_overscrolling(&self) -> bool {
        self.offset_y > self.min_offset() || self.offset_y < self.max_offset()
    }

    /// Get amount of overscroll (positive at top, negative at bottom)
    pub fn overscroll_amount(&self) -> f32 {
        if self.offset_y > self.min_offset() {
            self.offset_y - self.min_offset()
        } else if self.offset_y < self.max_offset() {
            self.offset_y - self.max_offset()
        } else {
            0.0
        }
    }

    /// Apply scroll delta (from user input)
    pub fn apply_scroll_delta(&mut self, delta_y: f32) {
        // Transition to scrolling state
        if let Some(new_state) = self.state.on_event(blinc_core::events::event_types::SCROLL) {
            self.state = new_state;
        }

        // If overscrolling, apply rubber-band resistance
        if self.is_overscrolling() && self.config.bounce_enabled {
            let resistance = 0.5; // 50% resistance when overscrolling
            self.offset_y += delta_y * resistance;
        } else {
            self.offset_y += delta_y;
        }

        // Track velocity from delta (assuming 16ms frame time)
        self.velocity_y = delta_y * 60.0; // Approximate velocity

        // Clamp overscroll if bounce disabled
        if !self.config.bounce_enabled {
            self.offset_y = self.offset_y.clamp(self.max_offset(), self.min_offset());
        } else {
            // Clamp to max overscroll
            let max_over = self.viewport_height * self.config.max_overscroll;
            self.offset_y = self.offset_y.clamp(self.max_offset() - max_over, max_over);
        }
    }

    /// Called when scroll gesture ends - start momentum/bounce
    pub fn on_scroll_end(&mut self) {
        if let Some(new_state) = self
            .state
            .on_event(blinc_core::events::event_types::SCROLL_END)
        {
            self.state = new_state;
        }

        // If overscrolling, start bounce immediately
        if self.is_overscrolling() && self.config.bounce_enabled {
            self.start_bounce();
        }
    }

    /// Start bounce animation to return to bounds
    fn start_bounce(&mut self) {
        let target = if self.offset_y > self.min_offset() {
            self.min_offset()
        } else {
            self.max_offset()
        };

        let mut spring = Spring::new(self.config.bounce_spring, self.offset_y);
        spring.set_target(target);
        self.spring = Some(spring);

        if let Some(new_state) = self.state.on_event(scroll_events::HIT_EDGE) {
            self.state = new_state;
        }
    }

    /// Tick animation (called every frame)
    ///
    /// Returns true if still animating, false if settled
    pub fn tick(&mut self, dt: f32) -> bool {
        match self.state {
            ScrollState::Idle => false,

            ScrollState::Scrolling => {
                // Active scrolling is driven by events, not ticks
                true
            }

            ScrollState::Decelerating => {
                // Apply friction
                self.velocity_y *= self.config.friction;
                self.offset_y += self.velocity_y * dt;

                // Check if hit edge
                if self.is_overscrolling() && self.config.bounce_enabled {
                    self.start_bounce();
                    return true;
                }

                // Clamp if no bounce
                if !self.config.bounce_enabled {
                    self.offset_y = self.offset_y.clamp(self.max_offset(), self.min_offset());
                }

                // Check if settled
                if self.velocity_y.abs() < self.config.velocity_threshold {
                    self.velocity_y = 0.0;
                    if let Some(new_state) = self.state.on_event(scroll_events::SETTLED) {
                        self.state = new_state;
                    }
                    return false;
                }

                true
            }

            ScrollState::Bouncing => {
                if let Some(ref mut spring) = self.spring {
                    spring.step(dt);
                    self.offset_y = spring.value();

                    if spring.is_settled() {
                        self.offset_y = spring.target();
                        self.spring = None;
                        if let Some(new_state) = self.state.on_event(scroll_events::SETTLED) {
                            self.state = new_state;
                        }
                        return false;
                    }
                    true
                } else {
                    // No spring, shouldn't happen, settle
                    if let Some(new_state) = self.state.on_event(scroll_events::SETTLED) {
                        self.state = new_state;
                    }
                    false
                }
            }
        }
    }

    /// Check if animation is active
    pub fn is_animating(&self) -> bool {
        self.state.is_active()
    }
}

// ============================================================================
// Shared Physics Handle
// ============================================================================

/// Shared handle to scroll physics for external access
pub type SharedScrollPhysics = Arc<Mutex<ScrollPhysics>>;

// ============================================================================
// Scroll Element
// ============================================================================

/// A scrollable container element with bounce physics
///
/// Wraps content in a clipped viewport with smooth scroll behavior.
pub struct Scroll {
    /// Inner div for layout properties
    inner: Div,
    /// Child content (single child expected, typically a container div)
    child: Option<Box<dyn ElementBuilder>>,
    /// Shared physics state
    physics: SharedScrollPhysics,
    /// Event handlers
    handlers: EventHandlers,
}

impl Default for Scroll {
    fn default() -> Self {
        Self::new()
    }
}

impl Scroll {
    /// Create a new scroll container
    pub fn new() -> Self {
        let physics = Arc::new(Mutex::new(ScrollPhysics::default()));
        let handlers = Self::create_internal_handlers(Arc::clone(&physics));

        Self {
            inner: Div::new().overflow_clip(),
            child: None,
            physics,
            handlers,
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: ScrollConfig) -> Self {
        let physics = Arc::new(Mutex::new(ScrollPhysics::new(config)));
        let handlers = Self::create_internal_handlers(Arc::clone(&physics));

        Self {
            inner: Div::new().overflow_clip(),
            child: None,
            physics,
            handlers,
        }
    }

    /// Create with external shared physics (for state persistence)
    pub fn with_physics(physics: SharedScrollPhysics) -> Self {
        let handlers = Self::create_internal_handlers(Arc::clone(&physics));

        Self {
            inner: Div::new().overflow_clip(),
            child: None,
            physics,
            handlers,
        }
    }

    /// Create internal event handlers that update physics state
    fn create_internal_handlers(physics: SharedScrollPhysics) -> EventHandlers {
        let mut handlers = EventHandlers::new();

        // Internal handler that applies scroll delta to physics
        handlers.on_scroll({
            let physics = Arc::clone(&physics);
            move |ctx| {
                physics.lock().unwrap().apply_scroll_delta(ctx.scroll_delta_y);
            }
        });

        handlers
    }

    /// Get the shared physics handle
    pub fn physics(&self) -> SharedScrollPhysics {
        Arc::clone(&self.physics)
    }

    /// Get current scroll offset
    pub fn offset_y(&self) -> f32 {
        self.physics.lock().unwrap().offset_y
    }

    /// Get current scroll state
    pub fn state(&self) -> ScrollState {
        self.physics.lock().unwrap().state
    }

    // =========================================================================
    // Configuration
    // =========================================================================

    /// Enable or disable bounce physics (default: enabled)
    pub fn bounce(self, enabled: bool) -> Self {
        self.physics.lock().unwrap().config.bounce_enabled = enabled;
        self
    }

    /// Disable bounce physics
    pub fn no_bounce(self) -> Self {
        self.bounce(false)
    }

    /// Set friction coefficient (0.0-1.0)
    pub fn friction(self, friction: f32) -> Self {
        self.physics.lock().unwrap().config.friction = friction.clamp(0.0, 1.0);
        self
    }

    /// Set bounce spring configuration
    pub fn spring(self, config: SpringConfig) -> Self {
        self.physics.lock().unwrap().config.bounce_spring = config;
        self
    }

    // =========================================================================
    // Size
    // =========================================================================

    /// Set viewport width
    pub fn w(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).w(px);
        self
    }

    /// Set viewport height
    pub fn h(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).h(px);
        self.physics.lock().unwrap().viewport_height = px;
        self
    }

    /// Set viewport size
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).size(w, h);
        self.physics.lock().unwrap().viewport_height = h;
        self
    }

    /// Set width to 100%
    pub fn w_full(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).w_full();
        self
    }

    /// Set height to 100%
    pub fn h_full(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).h_full();
        self
    }

    // =========================================================================
    // Visual
    // =========================================================================

    /// Set background color
    pub fn bg(mut self, color: impl Into<Brush>) -> Self {
        self.inner = std::mem::take(&mut self.inner).background(color);
        self
    }

    /// Set corner radius
    pub fn rounded(mut self, radius: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).rounded(radius);
        self
    }

    /// Apply shadow
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow(shadow);
        self
    }

    // =========================================================================
    // Content
    // =========================================================================

    /// Set the scrollable content
    ///
    /// Typically a single container div with the actual content.
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.child = Some(Box::new(child));
        self
    }

    // =========================================================================
    // Event Handlers
    // =========================================================================

    /// Register a scroll event handler
    ///
    /// Called during active scrolling with scroll delta in EventContext.
    pub fn on_scroll<F>(mut self, handler: F) -> Self
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.handlers.on_scroll(handler);
        self
    }

    // =========================================================================
    // Internal
    // =========================================================================

    /// Update content height (called by renderer after layout)
    pub fn set_content_height(&self, height: f32) {
        self.physics.lock().unwrap().content_height = height;
    }

    /// Apply scroll delta (called by event router)
    pub fn apply_scroll_delta(&self, delta_y: f32) {
        self.physics.lock().unwrap().apply_scroll_delta(delta_y);
    }

    /// Called when scroll gesture ends
    pub fn on_scroll_gesture_end(&self) {
        self.physics.lock().unwrap().on_scroll_end();
    }

    /// Tick animation (returns true if still animating)
    pub fn tick(&self, dt: f32) -> bool {
        self.physics.lock().unwrap().tick(dt)
    }
}

impl ElementBuilder for Scroll {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Build the viewport container
        let viewport_id = self.inner.build(tree);

        // Build child if present
        if let Some(ref child) = self.child {
            let child_id = child.build(tree);
            tree.add_child(viewport_id, child_id);
        }

        viewport_id
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        // Return slice to child if present
        if let Some(ref child) = self.child {
            std::slice::from_ref(child)
        } else {
            &[]
        }
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div // Scroll is a specialized div
    }

    fn event_handlers(&self) -> Option<&EventHandlers> {
        if self.handlers.is_empty() {
            None
        } else {
            Some(&self.handlers)
        }
    }

    fn scroll_info(&self) -> Option<ScrollRenderInfo> {
        let physics = self.physics.lock().unwrap();
        Some(ScrollRenderInfo {
            offset_y: physics.offset_y,
            viewport_height: physics.viewport_height,
            content_height: physics.content_height,
            is_animating: physics.is_animating(),
        })
    }
}

// ============================================================================
// Scroll Render Info (for renderer)
// ============================================================================

/// Information about scroll state for rendering
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollRenderInfo {
    /// Current scroll offset (negative = scrolled down)
    pub offset_y: f32,
    /// Viewport height
    pub viewport_height: f32,
    /// Total content height
    pub content_height: f32,
    /// Whether scroll animation is active
    pub is_animating: bool,
}

// ============================================================================
// Convenience Constructor
// ============================================================================

/// Create a new scroll container with default bounce physics
///
/// # Example
///
/// ```rust
/// use blinc_layout::prelude::*;
///
/// let scrollable = scroll()
///     .h(400.0)
///     .child(div().flex_col().gap(8.0));
/// ```
pub fn scroll() -> Scroll {
    Scroll::new()
}

/// Create a scroll container with bounce disabled
pub fn scroll_no_bounce() -> Scroll {
    Scroll::with_config(ScrollConfig::no_bounce())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_physics_basic() {
        let mut physics = ScrollPhysics::default();
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        assert_eq!(physics.min_offset(), 0.0);
        assert_eq!(physics.max_offset(), -600.0); // 1000 - 400

        // Apply scroll
        physics.apply_scroll_delta(-50.0);
        assert_eq!(physics.offset_y, -50.0);
        assert_eq!(physics.state, ScrollState::Scrolling);
    }

    #[test]
    fn test_scroll_physics_overscroll() {
        let mut physics = ScrollPhysics::default();
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        // Scroll past top
        physics.apply_scroll_delta(50.0);
        assert!(physics.is_overscrolling());
        assert!(physics.overscroll_amount() > 0.0);
    }

    #[test]
    fn test_scroll_physics_bounce() {
        let mut physics = ScrollPhysics::default();
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        // Overscroll at top
        physics.offset_y = 50.0;
        physics.state = ScrollState::Scrolling;

        // End scroll gesture
        physics.on_scroll_end();

        // Should be bouncing back
        assert_eq!(physics.state, ScrollState::Bouncing);
        assert!(physics.spring.is_some());

        // Tick until settled
        for _ in 0..120 {
            if !physics.tick(1.0 / 60.0) {
                break;
            }
        }

        assert_eq!(physics.state, ScrollState::Idle);
        assert!((physics.offset_y - 0.0).abs() < 1.0);
    }

    #[test]
    fn test_scroll_physics_no_bounce() {
        let config = ScrollConfig::no_bounce();
        let mut physics = ScrollPhysics::new(config);
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        // Try to overscroll
        physics.apply_scroll_delta(100.0);

        // Should be clamped
        assert_eq!(physics.offset_y, 0.0);
    }

    #[test]
    fn test_scroll_deceleration() {
        let mut physics = ScrollPhysics::default();
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        // Start scrolling with velocity
        physics.apply_scroll_delta(-50.0);
        physics.on_scroll_end();

        assert_eq!(physics.state, ScrollState::Decelerating);

        // Should decelerate over time
        let initial_velocity = physics.velocity_y;
        physics.tick(1.0 / 60.0);

        assert!(physics.velocity_y.abs() < initial_velocity.abs());
    }

    #[test]
    fn test_scroll_element_builder() {
        use crate::text::text;

        let s = scroll()
            .h(400.0)
            .rounded(8.0)
            .child(text("Hello"));

        let mut tree = LayoutTree::new();
        let _node = s.build(&mut tree);

        assert!(s.scroll_info().is_some());
    }
}
