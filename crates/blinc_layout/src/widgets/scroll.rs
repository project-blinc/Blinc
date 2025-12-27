//! Scroll container widget with webkit-style bounce physics
//!
//! Provides a scrollable container with smooth momentum and spring-based
//! edge bounce, similar to iOS/macOS native scroll behavior.
//! Inherits ALL Div methods for full layout control via Deref.
//!
//! # Example
//!
//! ```rust
//! use blinc_layout::prelude::*;
//!
//! let ui = scroll()
//!     .h(400.0)  // Viewport height
//!     .rounded(16.0)
//!     .shadow_sm()
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
//! - **Inherits Div**: Full access to all Div methods for layout control

use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use blinc_animation::{Spring, SpringConfig};
use blinc_core::{Brush, Shadow};

use crate::div::{Div, ElementBuilder, ElementTypeId};
use crate::element::RenderProps;
use crate::event_handler::{EventContext, EventHandlers};
use crate::stateful::{scroll_events, ScrollState, StateTransitions};
use crate::tree::{LayoutNodeId, LayoutTree};

// ============================================================================
// Scroll Direction
// ============================================================================

/// Scroll direction for the container
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollDirection {
    /// Vertical scrolling only (default)
    #[default]
    Vertical,
    /// Horizontal scrolling only
    Horizontal,
    /// Both directions (free scroll)
    Both,
}

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
    /// Deceleration rate in pixels/second² (how fast momentum slows down)
    pub deceleration: f32,
    /// Minimum velocity threshold for stopping (pixels/second)
    pub velocity_threshold: f32,
    /// Maximum overscroll distance as fraction of viewport (0.0-0.5)
    pub max_overscroll: f32,
    /// Scroll direction
    pub direction: ScrollDirection,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            bounce_enabled: true,
            bounce_spring: SpringConfig::wobbly(),
            deceleration: 1500.0,     // Decelerate at 1500 px/s²
            velocity_threshold: 10.0, // Stop when below 10 px/s
            max_overscroll: 0.3,
            direction: ScrollDirection::Vertical,
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
    /// Current vertical scroll offset (negative = scrolled down)
    pub offset_y: f32,
    /// Current vertical velocity (pixels per second)
    pub velocity_y: f32,
    /// Current horizontal scroll offset (negative = scrolled right)
    pub offset_x: f32,
    /// Current horizontal velocity (pixels per second)
    pub velocity_x: f32,
    /// Spring animator for vertical bounce (None when not bouncing)
    pub spring_y: Option<Spring>,
    /// Spring animator for horizontal bounce (None when not bouncing)
    pub spring_x: Option<Spring>,
    /// Current FSM state
    pub state: ScrollState,
    /// Content height (calculated from children)
    pub content_height: f32,
    /// Viewport height
    pub viewport_height: f32,
    /// Content width (calculated from children)
    pub content_width: f32,
    /// Viewport width
    pub viewport_width: f32,
    /// Configuration
    pub config: ScrollConfig,
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        Self {
            offset_y: 0.0,
            velocity_y: 0.0,
            offset_x: 0.0,
            velocity_x: 0.0,
            spring_y: None,
            spring_x: None,
            state: ScrollState::Idle,
            content_height: 0.0,
            viewport_height: 0.0,
            content_width: 0.0,
            viewport_width: 0.0,
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

    /// Maximum vertical scroll offset (0 = top edge)
    pub fn min_offset_y(&self) -> f32 {
        0.0
    }

    /// Minimum vertical scroll offset (negative, at bottom edge)
    pub fn max_offset_y(&self) -> f32 {
        let scrollable = self.content_height - self.viewport_height;
        if scrollable > 0.0 {
            -scrollable
        } else {
            0.0
        }
    }

    /// Maximum horizontal scroll offset (0 = left edge)
    pub fn min_offset_x(&self) -> f32 {
        0.0
    }

    /// Minimum horizontal scroll offset (negative, at right edge)
    pub fn max_offset_x(&self) -> f32 {
        let scrollable = self.content_width - self.viewport_width;
        if scrollable > 0.0 {
            -scrollable
        } else {
            0.0
        }
    }

    /// Check if currently overscrolling vertically (past bounds)
    pub fn is_overscrolling_y(&self) -> bool {
        self.offset_y > self.min_offset_y() || self.offset_y < self.max_offset_y()
    }

    /// Check if currently overscrolling horizontally (past bounds)
    pub fn is_overscrolling_x(&self) -> bool {
        self.offset_x > self.min_offset_x() || self.offset_x < self.max_offset_x()
    }

    /// Check if currently overscrolling in any direction
    pub fn is_overscrolling(&self) -> bool {
        match self.config.direction {
            ScrollDirection::Vertical => self.is_overscrolling_y(),
            ScrollDirection::Horizontal => self.is_overscrolling_x(),
            ScrollDirection::Both => self.is_overscrolling_y() || self.is_overscrolling_x(),
        }
    }

    /// Get amount of vertical overscroll (positive at top, negative at bottom)
    pub fn overscroll_amount_y(&self) -> f32 {
        if self.offset_y > self.min_offset_y() {
            self.offset_y - self.min_offset_y()
        } else if self.offset_y < self.max_offset_y() {
            self.offset_y - self.max_offset_y()
        } else {
            0.0
        }
    }

    /// Get amount of horizontal overscroll (positive at left, negative at right)
    pub fn overscroll_amount_x(&self) -> f32 {
        if self.offset_x > self.min_offset_x() {
            self.offset_x - self.min_offset_x()
        } else if self.offset_x < self.max_offset_x() {
            self.offset_x - self.max_offset_x()
        } else {
            0.0
        }
    }

    /// Apply scroll delta (from user input)
    ///
    /// Note: On macOS, the system already applies momentum physics to scroll events,
    /// so we don't need to track velocity or apply our own momentum. We just apply
    /// the delta directly with bounds clamping.
    pub fn apply_scroll_delta(&mut self, delta_x: f32, delta_y: f32) {
        // Transition to scrolling state
        if let Some(new_state) = self.state.on_event(blinc_core::events::event_types::SCROLL) {
            self.state = new_state;
        }

        let old_offset_y = self.offset_y;

        // Apply vertical delta based on direction
        if matches!(
            self.config.direction,
            ScrollDirection::Vertical | ScrollDirection::Both
        ) {
            // If overscrolling, apply rubber-band resistance
            if self.is_overscrolling_y() && self.config.bounce_enabled {
                let resistance = 0.3; // 30% resistance when overscrolling
                self.offset_y += delta_y * resistance;
            } else {
                self.offset_y += delta_y;
            }

            // Clamp to bounds (or max overscroll if bounce enabled)
            if !self.config.bounce_enabled {
                self.offset_y = self
                    .offset_y
                    .clamp(self.max_offset_y(), self.min_offset_y());
            } else {
                // Clamp to max overscroll distance
                let max_over = self.viewport_height * self.config.max_overscroll;
                self.offset_y = self
                    .offset_y
                    .clamp(self.max_offset_y() - max_over, max_over);
            }

            tracing::trace!(
                "Scroll delta_y={:.1} offset: {:.1} -> {:.1}, bounds=({:.0}, {:.0}), content={:.0}, viewport={:.0}",
                delta_y, old_offset_y, self.offset_y, self.max_offset_y(), self.min_offset_y(),
                self.content_height, self.viewport_height
            );
        }

        // Apply horizontal delta based on direction
        if matches!(
            self.config.direction,
            ScrollDirection::Horizontal | ScrollDirection::Both
        ) {
            // If overscrolling, apply rubber-band resistance
            if self.is_overscrolling_x() && self.config.bounce_enabled {
                let resistance = 0.3;
                self.offset_x += delta_x * resistance;
            } else {
                self.offset_x += delta_x;
            }

            // Clamp to bounds (or max overscroll if bounce enabled)
            if !self.config.bounce_enabled {
                self.offset_x = self
                    .offset_x
                    .clamp(self.max_offset_x(), self.min_offset_x());
            } else {
                let max_over = self.viewport_width * self.config.max_overscroll;
                self.offset_x = self
                    .offset_x
                    .clamp(self.max_offset_x() - max_over, max_over);
            }
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
        // Start vertical bounce if needed
        if self.is_overscrolling_y()
            && matches!(
                self.config.direction,
                ScrollDirection::Vertical | ScrollDirection::Both
            )
        {
            let target = if self.offset_y > self.min_offset_y() {
                self.min_offset_y()
            } else {
                self.max_offset_y()
            };

            let mut spring = Spring::new(self.config.bounce_spring, self.offset_y);
            spring.set_target(target);
            self.spring_y = Some(spring);
        }

        // Start horizontal bounce if needed
        if self.is_overscrolling_x()
            && matches!(
                self.config.direction,
                ScrollDirection::Horizontal | ScrollDirection::Both
            )
        {
            let target = if self.offset_x > self.min_offset_x() {
                self.min_offset_x()
            } else {
                self.max_offset_x()
            };

            let mut spring = Spring::new(self.config.bounce_spring, self.offset_x);
            spring.set_target(target);
            self.spring_x = Some(spring);
        }

        if let Some(new_state) = self.state.on_event(scroll_events::HIT_EDGE) {
            self.state = new_state;
        }
    }

    /// Tick animation (called every frame)
    ///
    /// Returns true if still animating, false if settled
    ///
    /// Note: On macOS, the system provides momentum scrolling via continuous scroll events,
    /// so our tick function mainly handles bounce-back animation when overscrolled.
    pub fn tick(&mut self, dt: f32) -> bool {
        match self.state {
            ScrollState::Idle => false,

            ScrollState::Scrolling => {
                // Active scrolling is driven by events, not ticks
                // Check if we should start bounce (in case scroll ended while overscrolling)
                if self.is_overscrolling() && self.config.bounce_enabled {
                    self.start_bounce();
                    return true;
                }
                true
            }

            ScrollState::Decelerating => {
                // On macOS, the system provides momentum via scroll events
                // We don't need our own deceleration - just check for bounce
                if self.is_overscrolling() && self.config.bounce_enabled {
                    self.start_bounce();
                    return true;
                }

                // Settle to idle if not overscrolling
                if let Some(new_state) = self.state.on_event(scroll_events::SETTLED) {
                    self.state = new_state;
                }
                false
            }

            ScrollState::Bouncing => {
                let mut still_bouncing = false;

                // Tick vertical spring
                if let Some(ref mut spring) = self.spring_y {
                    spring.step(dt);
                    self.offset_y = spring.value();

                    if spring.is_settled() {
                        self.offset_y = spring.target();
                        self.spring_y = None;
                    } else {
                        still_bouncing = true;
                    }
                }

                // Tick horizontal spring
                if let Some(ref mut spring) = self.spring_x {
                    spring.step(dt);
                    self.offset_x = spring.value();

                    if spring.is_settled() {
                        self.offset_x = spring.target();
                        self.spring_x = None;
                    } else {
                        still_bouncing = true;
                    }
                }

                if !still_bouncing {
                    if let Some(new_state) = self.state.on_event(scroll_events::SETTLED) {
                        self.state = new_state;
                    }
                    return false;
                }

                true
            }
        }
    }

    /// Check if animation is active
    pub fn is_animating(&self) -> bool {
        self.state.is_active()
    }

    /// Set the scroll direction
    pub fn set_direction(&mut self, direction: ScrollDirection) {
        self.config.direction = direction;
        // Reset position when changing direction
        self.offset_x = 0.0;
        self.offset_y = 0.0;
        self.velocity_x = 0.0;
        self.velocity_y = 0.0;
        self.spring_x = None;
        self.spring_y = None;
        self.state = ScrollState::Idle;
    }
}

// ============================================================================
// Shared Physics Handle
// ============================================================================

/// Shared handle to scroll physics for external access
pub type SharedScrollPhysics = Arc<Mutex<ScrollPhysics>>;

// ============================================================================
// Scroll Render Info (for renderer)
// ============================================================================

/// Information about scroll state for rendering
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollRenderInfo {
    /// Current horizontal scroll offset (negative = scrolled right)
    pub offset_x: f32,
    /// Current vertical scroll offset (negative = scrolled down)
    pub offset_y: f32,
    /// Viewport width
    pub viewport_width: f32,
    /// Viewport height
    pub viewport_height: f32,
    /// Total content width
    pub content_width: f32,
    /// Total content height
    pub content_height: f32,
    /// Whether scroll animation is active
    pub is_animating: bool,
    /// Scroll direction
    pub direction: ScrollDirection,
}

// ============================================================================
// Scroll Element
// ============================================================================

/// A scrollable container element with bounce physics
///
/// Inherits all Div methods via Deref, so you have full layout control.
///
/// Wraps content in a clipped viewport with smooth scroll behavior.
pub struct Scroll {
    /// Inner div for layout properties
    inner: Div,
    /// Child content (single child expected, typically a container div)
    content: Option<Box<dyn ElementBuilder>>,
    /// Shared physics state
    physics: SharedScrollPhysics,
    /// Event handlers
    handlers: EventHandlers,
}

// Deref to Div gives Scroll ALL Div methods for reading
impl Deref for Scroll {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Scroll {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
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
            // Use overflow_scroll to allow children to be laid out at natural size
            // (not constrained to viewport). We handle visual clipping ourselves.
            // Also set items_start to ensure child starts at top-left (not centered/stretched)
            inner: Div::new().overflow_scroll().items_start(),
            content: None,
            physics,
            handlers,
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: ScrollConfig) -> Self {
        let physics = Arc::new(Mutex::new(ScrollPhysics::new(config)));
        let handlers = Self::create_internal_handlers(Arc::clone(&physics));

        Self {
            inner: Div::new().overflow_scroll().items_start(),
            content: None,
            physics,
            handlers,
        }
    }

    /// Create with external shared physics (for state persistence)
    pub fn with_physics(physics: SharedScrollPhysics) -> Self {
        let handlers = Self::create_internal_handlers(Arc::clone(&physics));

        Self {
            inner: Div::new().overflow_scroll().items_start(),
            content: None,
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
                physics
                    .lock()
                    .unwrap()
                    .apply_scroll_delta(ctx.scroll_delta_x, ctx.scroll_delta_y);
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

    /// Set deceleration rate in pixels/second²
    pub fn deceleration(self, decel: f32) -> Self {
        self.physics.lock().unwrap().config.deceleration = decel.max(0.0);
        self
    }

    /// Set bounce spring configuration
    pub fn spring(self, config: SpringConfig) -> Self {
        self.physics.lock().unwrap().config.bounce_spring = config;
        self
    }

    /// Set scroll direction
    pub fn direction(self, direction: ScrollDirection) -> Self {
        self.physics.lock().unwrap().config.direction = direction;
        self
    }

    /// Set to vertical-only scrolling
    pub fn vertical(self) -> Self {
        self.direction(ScrollDirection::Vertical)
    }

    /// Set to horizontal-only scrolling
    pub fn horizontal(self) -> Self {
        self.direction(ScrollDirection::Horizontal)
    }

    /// Set to free scrolling (both directions)
    pub fn both_directions(self) -> Self {
        self.direction(ScrollDirection::Both)
    }

    // =========================================================================
    // Content
    // =========================================================================

    /// Set the scrollable content
    ///
    /// Typically a single container div with the actual content.
    pub fn content(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.content = Some(Box::new(child));
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
    pub fn apply_scroll_delta(&self, delta_x: f32, delta_y: f32) {
        self.physics
            .lock()
            .unwrap()
            .apply_scroll_delta(delta_x, delta_y);
    }

    /// Called when scroll gesture ends
    pub fn on_scroll_gesture_end(&self) {
        self.physics.lock().unwrap().on_scroll_end();
    }

    /// Tick animation (returns true if still animating)
    pub fn tick(&self, dt: f32) -> bool {
        self.physics.lock().unwrap().tick(dt)
    }

    // =========================================================================
    // Builder methods that return Self (shadow Div methods for fluent API)
    // =========================================================================

    pub fn w(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).w(px);
        self.physics.lock().unwrap().viewport_width = px;
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).h(px);
        self.physics.lock().unwrap().viewport_height = px;
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).size(w, h);
        {
            let mut physics = self.physics.lock().unwrap();
            physics.viewport_width = w;
            physics.viewport_height = h;
        }
        self
    }

    pub fn w_full(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).w_full();
        self
    }

    pub fn h_full(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).h_full();
        self
    }

    pub fn w_fit(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).w_fit();
        self
    }

    pub fn h_fit(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).h_fit();
        self
    }

    pub fn p(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).p(px);
        self
    }

    pub fn px(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).px(px);
        self
    }

    pub fn py(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).py(px);
        self
    }

    pub fn m(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).m(px);
        self
    }

    pub fn mx(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).mx(px);
        self
    }

    pub fn my(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).my(px);
        self
    }

    pub fn gap(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).gap(px);
        self
    }

    pub fn flex_row(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).flex_row();
        self
    }

    pub fn flex_col(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).flex_col();
        self
    }

    pub fn flex_grow(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).flex_grow();
        self
    }

    pub fn items_center(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).items_center();
        self
    }

    pub fn items_start(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).items_start();
        self
    }

    pub fn items_end(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).items_end();
        self
    }

    pub fn justify_center(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_center();
        self
    }

    pub fn justify_start(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_start();
        self
    }

    pub fn justify_end(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_end();
        self
    }

    pub fn justify_between(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).justify_between();
        self
    }

    pub fn bg(mut self, color: impl Into<Brush>) -> Self {
        self.inner = std::mem::take(&mut self.inner).background(color);
        self
    }

    pub fn rounded(mut self, radius: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).rounded(radius);
        self
    }

    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow(shadow);
        self
    }

    pub fn shadow_sm(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow_sm();
        self
    }

    pub fn shadow_md(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow_md();
        self
    }

    pub fn shadow_lg(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).shadow_lg();
        self
    }

    pub fn transform(mut self, transform: blinc_core::Transform) -> Self {
        self.inner = std::mem::take(&mut self.inner).transform(transform);
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).opacity(opacity);
        self
    }

    pub fn overflow_clip(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).overflow_clip();
        self
    }

    pub fn overflow_visible(mut self) -> Self {
        self.inner = std::mem::take(&mut self.inner).overflow_visible();
        self
    }

    /// Add scrollable child content (alias for content())
    pub fn child(self, child: impl ElementBuilder + 'static) -> Self {
        self.content(child)
    }

    // Event handlers
    pub fn on_scroll<F>(mut self, handler: F) -> Self
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.handlers.on_scroll(handler);
        self
    }

    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_click(handler);
        self
    }

    pub fn on_hover_enter<F>(mut self, handler: F) -> Self
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_hover_enter(handler);
        self
    }

    pub fn on_hover_leave<F>(mut self, handler: F) -> Self
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_hover_leave(handler);
        self
    }

    pub fn on_mouse_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_mouse_down(handler);
        self
    }

    pub fn on_mouse_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_mouse_up(handler);
        self
    }

    pub fn on_focus<F>(mut self, handler: F) -> Self
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_focus(handler);
        self
    }

    pub fn on_blur<F>(mut self, handler: F) -> Self
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_blur(handler);
        self
    }

    pub fn on_key_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_key_down(handler);
        self
    }

    pub fn on_key_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_key_up(handler);
        self
    }
}

impl ElementBuilder for Scroll {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Build the viewport container
        let viewport_id = self.inner.build(tree);

        // Build child if present
        if let Some(ref child) = self.content {
            let child_id = child.build(tree);
            tree.add_child(viewport_id, child_id);
        }

        viewport_id
    }

    fn render_props(&self) -> RenderProps {
        let mut props = self.inner.render_props();
        // Scroll containers always clip their children
        props.clips_content = true;
        props
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        // Return slice to child if present
        if let Some(ref child) = self.content {
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
            offset_x: physics.offset_x,
            offset_y: physics.offset_y,
            viewport_width: physics.viewport_width,
            viewport_height: physics.viewport_height,
            content_width: physics.content_width,
            content_height: physics.content_height,
            is_animating: physics.is_animating(),
            direction: physics.config.direction,
        })
    }

    fn scroll_physics(&self) -> Option<SharedScrollPhysics> {
        Some(Arc::clone(&self.physics))
    }
}

// ============================================================================
// Convenience Constructor
// ============================================================================

/// Create a new scroll container with default bounce physics
///
/// The scroll container inherits ALL Div methods, so you have full layout control.
///
/// # Example
///
/// ```rust
/// use blinc_layout::prelude::*;
///
/// let scrollable = scroll()
///     .h(400.0)
///     .rounded(16.0)
///     .shadow_sm()
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

        assert_eq!(physics.min_offset_y(), 0.0);
        assert_eq!(physics.max_offset_y(), -600.0); // 1000 - 400

        // Apply scroll (vertical)
        physics.apply_scroll_delta(0.0, -50.0);
        assert_eq!(physics.offset_y, -50.0);
        assert_eq!(physics.state, ScrollState::Scrolling);
    }

    #[test]
    fn test_scroll_physics_overscroll() {
        let mut physics = ScrollPhysics::default();
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        // Scroll past top (vertical)
        physics.apply_scroll_delta(0.0, 50.0);
        assert!(physics.is_overscrolling_y());
        assert!(physics.overscroll_amount_y() > 0.0);
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
        assert!(physics.spring_y.is_some());

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

        // Try to overscroll (vertical)
        physics.apply_scroll_delta(0.0, 100.0);

        // Should be clamped
        assert_eq!(physics.offset_y, 0.0);
    }

    #[test]
    fn test_scroll_settling() {
        let mut physics = ScrollPhysics::default();
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        // Start scrolling (vertical)
        physics.apply_scroll_delta(0.0, -50.0);
        assert_eq!(physics.state, ScrollState::Scrolling);
        assert_eq!(physics.offset_y, -50.0);

        // End scroll gesture
        physics.on_scroll_end();

        // Should be in decelerating state
        assert_eq!(physics.state, ScrollState::Decelerating);

        // Tick should settle to idle (since macOS provides momentum)
        let still_animating = physics.tick(1.0 / 60.0);

        // Should settle immediately since we're not overscrolling
        assert!(!still_animating);
        assert_eq!(physics.state, ScrollState::Idle);
    }

    #[test]
    fn test_scroll_element_builder() {
        use crate::text::text;

        let s = scroll().h(400.0).rounded(8.0).child(text("Hello"));

        let mut tree = LayoutTree::new();
        let _node = s.build(&mut tree);

        assert!(s.scroll_info().is_some());
    }
}
