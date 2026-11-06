//! Scroll container widget with optional webkit-style bounce physics
//!
//! Provides a scrollable container with smooth momentum and spring-based
//! edge bounce, similar to iOS/macOS native scroll behavior.
//! Inherits ALL Div methods for full layout control via Deref.
//!
//! # Example
//!
//! ```rust,ignore
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
//! - **Optional edge bounce**: Webkit-style spring animation when explicitly enabled
//! - **Glass-aware clipping**: Content clips properly even for glass/blur elements
//! - **FSM-based state**: Clear state machine for Idle, Scrolling, Decelerating, Bouncing
//! - **Inherits Div**: Full access to all Div methods for layout control

use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, Weak};

use blinc_animation::{AnimationScheduler, Spring, SpringConfig, SpringId};
use blinc_core::{Brush, Shadow};

use crate::div::{Div, ElementBuilder, ElementTypeId};
use crate::element::RenderProps;
use crate::event_handler::{EventContext, EventHandlers};
use crate::selector::ScrollRef;
use crate::stateful::{ScrollState, StateTransitions, scroll_events};
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
// Scrollbar Types
// ============================================================================

/// Scrollbar visibility modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollbarVisibility {
    /// Always show scrollbar (like classic Windows style)
    Always,
    /// Show scrollbar only when hovering over the scroll area
    Hover,
    /// Show when scrolling, auto-dismiss after inactivity (like macOS)
    #[default]
    Auto,
    /// Never show scrollbar (content still scrollable)
    Never,
}

/// Scrollbar interaction state (FSM for scrollbar UI)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollbarState {
    /// Scrollbar is not interacted with
    #[default]
    Idle,
    /// Mouse is hovering over the scrollbar track
    TrackHovered,
    /// Mouse is hovering over the scrollbar thumb
    ThumbHovered,
    /// Scrollbar thumb is being dragged
    Dragging,
    /// Scrollbar is visible due to active scrolling
    Scrolling,
    /// Scrollbar is fading out (auto-dismiss)
    FadingOut,
}

impl ScrollbarState {
    /// Check if the scrollbar should be visible
    pub fn is_visible(&self) -> bool {
        !matches!(self, ScrollbarState::Idle)
    }

    /// Check if the scrollbar is being actively interacted with
    pub fn is_interacting(&self) -> bool {
        matches!(
            self,
            ScrollbarState::ThumbHovered | ScrollbarState::Dragging
        )
    }

    /// Get opacity value for the scrollbar (0.0 to 1.0)
    pub fn opacity(&self) -> f32 {
        match self {
            ScrollbarState::Idle => 0.0,
            ScrollbarState::FadingOut => 0.3, // Partial visibility during fade
            ScrollbarState::TrackHovered => 0.6,
            ScrollbarState::Scrolling => 0.7,
            ScrollbarState::ThumbHovered => 0.9,
            ScrollbarState::Dragging => 1.0,
        }
    }
}

/// Size presets for scrollbar
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollbarSize {
    /// Thin scrollbar (4px)
    Thin,
    /// Normal scrollbar (6px)
    #[default]
    Normal,
    /// Wide scrollbar (10px)
    Wide,
}

impl ScrollbarSize {
    /// Get the width in pixels
    pub fn width(&self) -> f32 {
        match self {
            ScrollbarSize::Thin => 4.0,
            ScrollbarSize::Normal => 6.0,
            ScrollbarSize::Wide => 10.0,
        }
    }
}

/// Configuration for scrollbar appearance and behavior
#[derive(Debug, Clone, Copy)]
pub struct ScrollbarConfig {
    /// Visibility mode
    pub visibility: ScrollbarVisibility,
    /// Scrollbar size preset (or use custom_width)
    pub size: ScrollbarSize,
    /// Custom width override (takes precedence over size)
    pub custom_width: Option<f32>,
    /// Thumb color (RGBA)
    pub thumb_color: [f32; 4],
    /// Thumb color when hovered
    pub thumb_hover_color: [f32; 4],
    /// Track color (RGBA)
    pub track_color: [f32; 4],
    /// Corner radius for thumb (fraction of width, 0.0-0.5)
    pub corner_radius: f32,
    /// Padding from edge of scroll container
    pub edge_padding: f32,
    /// Auto-dismiss delay in seconds (for Auto visibility mode)
    pub auto_dismiss_delay: f32,
    /// Minimum thumb length in pixels
    pub min_thumb_length: f32,
}

impl Default for ScrollbarConfig {
    fn default() -> Self {
        Self {
            visibility: ScrollbarVisibility::Auto,
            size: ScrollbarSize::Normal,
            custom_width: None,
            // Semi-transparent gray thumb
            thumb_color: [0.5, 0.5, 0.5, 0.5],
            thumb_hover_color: [0.6, 0.6, 0.6, 0.8],
            // Very subtle track
            track_color: [0.5, 0.5, 0.5, 0.1],
            corner_radius: 0.5, // Fully rounded by default
            edge_padding: 2.0,
            auto_dismiss_delay: 1.5, // 1.5 seconds like macOS
            min_thumb_length: 30.0,
        }
    }
}

impl ScrollbarConfig {
    /// Create config with always-visible scrollbar
    pub fn always_visible() -> Self {
        Self {
            visibility: ScrollbarVisibility::Always,
            ..Default::default()
        }
    }

    /// Create config with hover-triggered scrollbar
    pub fn show_on_hover() -> Self {
        Self {
            visibility: ScrollbarVisibility::Hover,
            ..Default::default()
        }
    }

    /// Create config with hidden scrollbar
    pub fn hidden() -> Self {
        Self {
            visibility: ScrollbarVisibility::Never,
            ..Default::default()
        }
    }

    /// Get the actual scrollbar width
    pub fn width(&self) -> f32 {
        self.custom_width.unwrap_or_else(|| self.size.width())
    }
}

// ============================================================================
// Scroll Configuration
// ============================================================================

/// Configuration for scroll behavior
#[derive(Debug, Clone, Copy)]
pub struct ScrollConfig {
    /// Enable bounce physics at edges (default: false)
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
    /// Scrollbar configuration
    pub scrollbar: ScrollbarConfig,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            bounce_enabled: false,
            // iOS-like elastic snap-back: very stiff critically-damped spring
            // Critical damping = 2 * sqrt(stiffness * mass) = 2 * sqrt(3000) ≈ 109.5
            // Using damping = 110 (slightly overdamped) for fast snap with no rebound
            bounce_spring: SpringConfig::new(3000.0, 110.0, 1.0),
            deceleration: 1500.0, // Decelerate at 1500 px/s² (shared with desktop/web)
            velocity_threshold: 3.0, // Stop when below 3 px/s
            // Safety cap on visible overscroll as a fraction of viewport.
            // The apparent-stretch curve in `apply_scroll_delta` tapers
            // sub-linearly and reaches the cap only under sustained
            // dragging; 12 % matches native desktop feel (macOS / iOS
            // trackpad scroll stretches less than a sixth of the viewport
            // under normal use).
            max_overscroll: 0.12,
            direction: ScrollDirection::Vertical,
            scrollbar: ScrollbarConfig::default(),
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

    /// Create config with bounce enabled
    pub fn bouncy() -> Self {
        Self {
            bounce_enabled: true,
            ..Default::default()
        }
    }

    /// Create config with stiff bounce (less wobbly)
    pub fn stiff_bounce() -> Self {
        Self {
            bounce_enabled: true,
            bounce_spring: SpringConfig::stiff(),
            ..Default::default()
        }
    }

    /// Create config with gentle bounce (more wobbly)
    pub fn gentle_bounce() -> Self {
        Self {
            bounce_enabled: true,
            bounce_spring: SpringConfig::gentle(),
            ..Default::default()
        }
    }
}

// ============================================================================
// Scroll Physics State
// ============================================================================

/// Internal physics state for scroll animation
pub struct ScrollPhysics {
    /// Current vertical scroll offset (negative = scrolled down)
    pub offset_y: f32,
    /// Current vertical velocity (pixels per second)
    pub velocity_y: f32,
    /// Current horizontal scroll offset (negative = scrolled right)
    pub offset_x: f32,
    /// Current horizontal velocity (pixels per second)
    pub velocity_x: f32,
    /// Spring ID for vertical bounce (None when not bouncing)
    spring_y: Option<SpringId>,
    /// Spring ID for horizontal bounce (None when not bouncing)
    spring_x: Option<SpringId>,
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
    /// Weak reference to animation scheduler for spring management
    scheduler: Weak<Mutex<AnimationScheduler>>,

    // =========================================================================
    // Scrollbar State
    // =========================================================================
    /// Current scrollbar interaction state
    pub scrollbar_state: ScrollbarState,
    /// Current scrollbar opacity (0.0 to 1.0, animated)
    pub scrollbar_opacity: f32,
    /// Target scrollbar opacity (for animation)
    scrollbar_target_opacity: f32,
    /// Whether the scroll area is being hovered
    pub area_hovered: bool,
    /// Time accumulator since last scroll activity (seconds)
    pub idle_time: f32,
    /// Vertical scrollbar thumb drag start position (mouse Y)
    pub thumb_drag_start_y: f32,
    /// Horizontal scrollbar thumb drag start position (mouse X)
    pub thumb_drag_start_x: f32,
    /// Scroll offset at drag start (Y)
    pub thumb_drag_start_scroll_y: f32,
    /// Scroll offset at drag start (X)
    pub thumb_drag_start_scroll_x: f32,
    /// Spring ID for scrollbar opacity animation
    scrollbar_opacity_spring: Option<SpringId>,
    /// Last scroll event time in milliseconds (for velocity calculation and
    /// the idle-based rebound trigger in `check_idle_bounce`).
    last_scroll_time: Option<f64>,
    /// Marks the window between finger-lift and the matching momentum-
    /// cycle end. Platforms with OS momentum (notably macOS trackpads)
    /// deliver two scroll-end signals per gesture: the first at finger-
    /// lift, the second when the post-release momentum has finished.
    /// `on_scroll_end` uses this flag to tell them apart. Also gates the
    /// mid-momentum rebound trigger: a stretched-past-edge delta only
    /// fires the spring while `in_momentum_mode` is set (i.e. during the
    /// post-release momentum stream, not during active user drag).
    in_momentum_mode: bool,
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
            scheduler: Weak::new(),
            // Scrollbar state
            scrollbar_state: ScrollbarState::Idle,
            scrollbar_opacity: 0.0,
            scrollbar_target_opacity: 0.0,
            area_hovered: false,
            idle_time: 0.0,
            thumb_drag_start_y: 0.0,
            thumb_drag_start_x: 0.0,
            thumb_drag_start_scroll_y: 0.0,
            thumb_drag_start_scroll_x: 0.0,
            scrollbar_opacity_spring: None,
            last_scroll_time: None,
            in_momentum_mode: false,
        }
    }
}

/// Result of scrollbar hit testing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarHitResult {
    /// Not over any scrollbar
    None,
    /// Over vertical scrollbar track (but not thumb)
    VerticalTrack,
    /// Over vertical scrollbar thumb
    VerticalThumb,
    /// Over horizontal scrollbar track (but not thumb)
    HorizontalTrack,
    /// Over horizontal scrollbar thumb
    HorizontalThumb,
}

impl ScrollPhysics {
    /// Create new physics with given config
    pub fn new(config: ScrollConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// Create new physics with scheduler for animation-driven bounce
    pub fn with_scheduler(
        config: ScrollConfig,
        scheduler: &Arc<Mutex<AnimationScheduler>>,
    ) -> Self {
        Self {
            config,
            scheduler: Arc::downgrade(scheduler),
            ..Default::default()
        }
    }

    /// Whether edge-bounce (the overscroll spring) is active.
    ///
    /// Hard-disabled on mobile targets for now: momentum and the strict
    /// edge clamp stay, but the rubber-band stretch + spring rebound are
    /// suppressed so touch scrolling stops flat at the edge. Reading the
    /// config through this accessor (instead of the field directly) means
    /// an app enabling bounce still gets a plain clamp on Android/iOS.
    #[inline]
    fn bounce_enabled(&self) -> bool {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            false
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            self.config.bounce_enabled
        }
    }

    /// Set the animation scheduler (for spring-based bounce animation)
    pub fn set_scheduler(&mut self, scheduler: &Arc<Mutex<AnimationScheduler>>) {
        self.scheduler = Arc::downgrade(scheduler);
    }

    /// Maximum vertical scroll offset (0 = top edge)
    pub fn min_offset_y(&self) -> f32 {
        0.0
    }

    /// Minimum vertical scroll offset (negative, at bottom edge)
    pub fn max_offset_y(&self) -> f32 {
        let scrollable = self.content_height - self.viewport_height;
        if scrollable > 0.0 { -scrollable } else { 0.0 }
    }

    /// Maximum horizontal scroll offset (0 = left edge)
    pub fn min_offset_x(&self) -> f32 {
        0.0
    }

    /// Minimum horizontal scroll offset (negative, at right edge)
    pub fn max_offset_x(&self) -> f32 {
        let scrollable = self.content_width - self.viewport_width;
        if scrollable > 0.0 { -scrollable } else { 0.0 }
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
    ///
    /// When bouncing, we ignore momentum scroll events - the spring animation
    /// takes over and drives the position back to bounds.
    pub fn apply_scroll_delta(&mut self, delta_x: f32, delta_y: f32) {
        // While the rebound spring is still animating, drop the delta —
        // the spring is driving the return and we don't want momentum
        // events to fight it. Once the spring settles (state → Idle), we
        // let deltas through but `in_momentum_mode` forces strict bounds
        // clamping in the clamp branches below so residual momentum
        // can't re-overscroll the just-rebounded edge.
        if self.state == ScrollState::Bouncing {
            return;
        }

        // Transition state machine
        if let Some(new_state) = self.state.on_event(blinc_core::events::event_types::SCROLL) {
            self.state = new_state;
        }

        // Stamp last activity. `apply_touch_scroll_delta` already does
        // this with a caller-supplied time, but the bare desktop path
        // (mouse wheel, scrollbar drag) didn't — which left
        // `check_idle_bounce` unable to detect quiescence and pinned
        // the FSM in `Scrolling` forever.
        self.last_scroll_time = Some(crate::widgets::text_input::elapsed_ms() as f64);

        let old_offset_y = self.offset_y;

        // Apply vertical delta based on direction
        if matches!(
            self.config.direction,
            ScrollDirection::Vertical | ScrollDirection::Both
        ) {
            let overscroll = self.overscroll_amount_y();
            let pushing_further =
                (overscroll > 0.0 && delta_y > 0.0) || (overscroll < 0.0 && delta_y < 0.0);
            let pulling_back =
                (overscroll > 0.0 && delta_y < 0.0) || (overscroll < 0.0 && delta_y > 0.0);

            // While the user is still dragging, content follows the delta
            // directly (with rubber-band resistance when pushing further past
            // the edge). On release, the `on_scroll_end` spring drives the
            // rebound back to the edge — we ignore any platform-generated
            // pull-back momentum deltas so the spring is the sole source of
            // return motion. That keeps the behaviour identical on every
            // platform regardless of whether the OS emits its own momentum.
            if self.is_overscrolling_y() && self.bounce_enabled() && pulling_back {
                // Platform-generated pull-back momentum — let the spring
                // handle the return. Applying these deltas would race the
                // spring and cause a double-rebound.
            } else if self.is_overscrolling_y() && self.bounce_enabled() && pushing_further {
                // Asymptotic rubber-band resistance. The rate at which
                // additional overscroll is accepted falls off as the
                // current stretch grows, so the first delta past the
                // edge feels light but sustained dragging plateaus.
                // Equivalent (via the instantaneous slope of
                // `b(x) = (1 - 1/(x·c/d + 1)) · d`) to d/dx b(x) =
                // c / (1 + x·c/d)². With c = 0.55 the initial
                // resistance matches the old linear curve; beyond that
                // the taper is much gentler.
                const C: f32 = 0.55;
                let d = self.viewport_height.max(1.0);
                let x = overscroll.abs();
                let k = 1.0 + x * C / d;
                let resistance = C / (k * k);
                self.offset_y += delta_y * resistance;
            } else {
                self.offset_y += delta_y;
            }

            // Clamp to bounds (or max overscroll if bounce enabled).
            // `in_momentum_mode` forces strict clamping so residual OS
            // momentum that streams after a rebound can't stretch the
            // content again — user scrolls into the content area still
            // go through because they don't need the overscroll region.
            if !self.bounce_enabled() || self.in_momentum_mode {
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
                "scroll_phys delta_y={:.1} offset: {:.1} -> {:.1}, bounds=({:.0}, {:.0}), content={:.0}, viewport={:.0}",
                delta_y,
                old_offset_y,
                self.offset_y,
                self.max_offset_y(),
                self.min_offset_y(),
                self.content_height,
                self.viewport_height
            );
        }

        // Apply horizontal delta based on direction
        if matches!(
            self.config.direction,
            ScrollDirection::Horizontal | ScrollDirection::Both
        ) {
            let overscroll = self.overscroll_amount_x();
            let pushing_further =
                (overscroll > 0.0 && delta_x > 0.0) || (overscroll < 0.0 && delta_x < 0.0);
            let pulling_back =
                (overscroll > 0.0 && delta_x < 0.0) || (overscroll < 0.0 && delta_x > 0.0);

            // Pull-back deltas are ignored — spring drives the rebound. See
            // Y-axis branch for the full rationale.
            if self.is_overscrolling_x() && self.bounce_enabled() && pulling_back {
                // Ignored — spring handles return.
            } else if self.is_overscrolling_x() && self.bounce_enabled() && pushing_further {
                // Asymptotic rubber-band resistance — see Y-axis branch.
                const C: f32 = 0.55;
                let d = self.viewport_width.max(1.0);
                let x = overscroll.abs();
                let k = 1.0 + x * C / d;
                let resistance = C / (k * k);
                self.offset_x += delta_x * resistance;
            } else {
                self.offset_x += delta_x;
            }

            // Clamp to bounds. See Y-axis branch for the rationale on
            // the `in_momentum_mode` strict-clamp.
            if !self.bounce_enabled() || self.in_momentum_mode {
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

        // No mid-momentum rebound — when `in_momentum_mode` is set the
        // clamp branches above strip any overscroll directly, so residual
        // momentum simply stops the content at the edge without triggering
        // a second spring. The finger-lift rebound in `on_scroll_end` is
        // the sole source of return motion.
    }

    /// Apply scroll delta from touch input with velocity tracking
    ///
    /// This is used on mobile platforms where we need to track velocity
    /// ourselves for momentum scrolling (unlike macOS which provides it).
    ///
    /// `current_time` is in milliseconds (from elapsed_ms()).
    pub fn apply_touch_scroll_delta(&mut self, delta_x: f32, delta_y: f32, current_time: f64) {
        // New-gesture safety clear. If there's been a pause since the last
        // scroll event, treat this as the start of a fresh user gesture —
        // drop any lingering momentum flags that might still be set from a
        // prior cycle whose end signal never arrived. Without this, a
        // momentum-ended event that winit/the OS neglects to deliver (or
        // a momentum stream interrupted by a new gesture) leaves
        // `in_momentum_mode` latched and subsequent scrolls are clamped
        // to zero overscroll forever.
        if let Some(last_time) = self.last_scroll_time
            && current_time - last_time > 100.0
            && self.in_momentum_mode
        {
            self.in_momentum_mode = false;
        }
        // Calculate velocity from delta and time since last event
        if let Some(last_time) = self.last_scroll_time {
            let dt_seconds = ((current_time - last_time) / 1000.0) as f32;
            if dt_seconds > 0.0 && dt_seconds < 0.5 {
                // Smooth velocity using exponential moving average.
                // Weight the most recent sample heavily so a quick flick's
                // release velocity reflects the *end* of the gesture, not an
                // average dragged down by its slow start. At 0.3 the EMA lagged
                // ~0.1s (worse at low frame rates, where touch deltas batch
                // per render frame), so momentum launched far slower than the
                // finger was actually moving — the "heavy/slow coast".
                let alpha = 0.6; // Smoothing factor (recent-weighted)
                let instant_vx = delta_x / dt_seconds;
                let instant_vy = delta_y / dt_seconds;
                self.velocity_x = self.velocity_x * (1.0 - alpha) + instant_vx * alpha;
                self.velocity_y = self.velocity_y * (1.0 - alpha) + instant_vy * alpha;
            }
        } else {
            // First event of a fresh gesture (the previous end nulled
            // `last_scroll_time`). Clear `in_momentum_mode` here: it exists for
            // macOS's TWO scroll-end calls per gesture (finger-lift, then a
            // separate momentum-end). Touch fires scroll-end only ONCE, so the
            // flag was left latched `true` after the first flick and made the
            // NEXT flick's finger-lift look like a momentum-end — `on_scroll_end`
            // then early-returned without launching momentum, so every other
            // swipe produced no coast. Resetting it at gesture start makes each
            // touch flick a clean finger-lift.
            self.in_momentum_mode = false;
            // Initialize velocity from delta assuming 16ms frame
            self.velocity_x = delta_x * 60.0;
            self.velocity_y = delta_y * 60.0;
        }
        self.last_scroll_time = Some(current_time);

        // Apply the delta using normal scroll logic
        self.apply_scroll_delta(delta_x, delta_y);
    }

    /// Record that a scroll event just arrived. Used by `check_idle_bounce`
    /// to detect when the user has stopped scrolling on inputs that don't
    /// deliver an explicit "end" signal (classic mouse wheel, many Windows /
    /// Linux trackpad drivers). Touch and macOS-trackpad paths update
    /// `last_scroll_time` themselves via `apply_touch_scroll_delta` /
    /// `on_scroll_end`, so this is a no-op in those flows.
    pub fn note_scroll_event(&mut self, current_time_ms: f64) {
        self.last_scroll_time = Some(current_time_ms);
    }

    /// Settle a quiescent `Scrolling` state. Called once per frame from
    /// `RenderTree::tick_scroll_physics`.
    ///
    /// Inputs that emit an explicit `ScrollPhase::Ended` (macOS trackpad,
    /// touch) already call `on_scroll_end` / `on_gesture_end` synchronously,
    /// so this only fires for mouse-wheel / no-phase inputs that would
    /// otherwise leave the FSM latched in `Scrolling` forever — which kept
    /// `tick()` returning `true` and pinned a continuous redraw loop at
    /// 80–90% CPU after every wheel event.
    ///
    /// After the idle threshold, two cases:
    /// - Overscrolled & bounce enabled → fire the rebound spring.
    /// - Otherwise → transition straight to `Idle` so the redraw loop stops.
    pub fn check_idle_bounce(&mut self, current_time_ms: f64) {
        if self.state != ScrollState::Scrolling {
            return;
        }
        let Some(last) = self.last_scroll_time else {
            return;
        };
        if current_time_ms - last <= 200.0 {
            return;
        }
        if self.is_overscrolling() && self.bounce_enabled() {
            self.on_scroll_end();
        } else {
            self.last_scroll_time = None;
            self.state = ScrollState::Idle;
        }
    }

    /// Called on scroll-end. Platforms with OS momentum (notably macOS
    /// trackpads) deliver this twice per gesture — once at finger-lift,
    /// once at the end of the separate momentum cycle. `in_momentum_mode`
    /// distinguishes them:
    /// - First call (`in_momentum_mode == false`): finger-lift. If content
    ///   is stretched, fire the rebound spring. Set `in_momentum_mode = true`
    ///   so the overscroll clamp goes strict for the follow-on momentum.
    /// - Second call (`in_momentum_mode == true`): momentum phase is done.
    ///   Clear `in_momentum_mode` so follow-up gestures can overscroll again.
    pub fn on_scroll_end(&mut self) {
        if self.in_momentum_mode {
            // Momentum phase ended — re-enable the rubber-band for the
            // next gesture.
            self.in_momentum_mode = false;
            self.last_scroll_time = None;
            return;
        }

        // Finger-lift. Transition out of the active scroll state.
        if let Some(new_state) = self
            .state
            .on_event(blinc_core::events::event_types::SCROLL_END)
        {
            self.state = new_state;
        }
        self.last_scroll_time = None;
        self.in_momentum_mode = true;

        // If stretched, fire the spring. Subsequent momentum deltas
        // (drops when state == Bouncing, clamped at edge when the spring
        // settles and state flips back to Idle) can't fight the rebound.
        if self.is_overscrolling() && self.bounce_enabled() {
            self.start_bounce();
            return;
        }

        // No stretch — if the touch path left us with real velocity, let
        // the Decelerating state run the momentum.
        let has_velocity = self.velocity_x.abs() > self.config.velocity_threshold
            || self.velocity_y.abs() > self.config.velocity_threshold;
        if has_velocity
            && let Some(new_state) = self
                .state
                .on_event(blinc_core::events::event_types::SCROLL_END)
        {
            self.state = new_state;
        }
    }

    /// Called when scroll gesture ends (finger lifted from trackpad)
    ///
    /// If overscrolling, start bounce immediately for snappy feedback.
    /// The user expects the elastic snap-back to begin the moment they release.
    pub fn on_gesture_end(&mut self) {
        // If overscrolling, start bounce immediately on finger lift
        // This gives snappy iOS-like feedback where release = snap back
        if self.is_overscrolling() && self.bounce_enabled() {
            self.start_bounce();
        }
    }

    /// Stop any active scroll animation — momentum deceleration, bounce
    /// spring, rebound — and pin the offset where it currently is.
    ///
    /// Intended for the pointer-down handler: when the user taps on a
    /// scrolling list that's still coasting, the expected behaviour
    /// (iOS / every native toolkit) is that the tap cancels the
    /// deceleration so the user can grab and re-scroll immediately.
    /// Without this, a tap lands on whatever is under the cursor a
    /// few frames later (after the list has coasted further) — feels
    /// unresponsive.
    pub fn cancel_active_animation(&mut self) {
        self.cancel_springs();
        self.velocity_x = 0.0;
        self.velocity_y = 0.0;
        self.in_momentum_mode = false;
        self.state = ScrollState::Idle;
    }

    /// Cancel any active bounce springs
    fn cancel_springs(&mut self) {
        if let Some(scheduler) = self.scheduler.upgrade() {
            let scheduler = scheduler.lock().unwrap();
            if let Some(id) = self.spring_y.take() {
                scheduler.remove_spring(id);
            }
            if let Some(id) = self.spring_x.take() {
                scheduler.remove_spring(id);
            }
        } else {
            // No scheduler, just clear the IDs
            self.spring_y = None;
            self.spring_x = None;
        }
    }

    /// Start bounce animation to return to bounds
    fn start_bounce(&mut self) {
        // Don't restart if already bouncing - this prevents resetting the spring
        // which would cause vibration/jitter
        if self.state == ScrollState::Bouncing {
            return;
        }

        // Get scheduler - if not available, can't animate
        let Some(scheduler_arc) = self.scheduler.upgrade() else {
            // No scheduler - just snap to bounds immediately
            if self.is_overscrolling_y() {
                self.offset_y = if self.offset_y > self.min_offset_y() {
                    self.min_offset_y()
                } else {
                    self.max_offset_y()
                };
            }
            if self.is_overscrolling_x() {
                self.offset_x = if self.offset_x > self.min_offset_x() {
                    self.min_offset_x()
                } else {
                    self.max_offset_x()
                };
            }
            return;
        };

        let scheduler = scheduler_arc.lock().unwrap();

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
            let spring_id = scheduler.add_spring(spring);
            self.spring_y = Some(spring_id);
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
            let spring_id = scheduler.add_spring(spring);
            self.spring_x = Some(spring_id);
        }

        drop(scheduler); // Release lock before state transition

        // Transition directly to `Bouncing`. The state machine only has
        // `Scrolling`/`Decelerating → HIT_EDGE → Bouncing` mappings; firing
        // `HIT_EDGE` from `Idle` would be a no-op, leaving us with a live
        // spring but the tick returning `false` for Idle — content would sit
        // permanently at the overscrolled offset with no animation driving
        // it back. That happened on macOS where the momentum-phase `Ended`
        // can arrive after our cooldown has already expired to `Idle`.
        self.state = ScrollState::Bouncing;
    }

    /// Update scroll offsets from animation scheduler springs
    ///
    /// Returns true if still animating, false if settled.
    /// This reads spring values from the AnimationScheduler which ticks them
    /// on a background thread at 120fps.
    ///
    /// `dt` is delta time in seconds since last tick.
    pub fn tick(&mut self, dt: f32) -> bool {
        match self.state {
            ScrollState::Idle => false,

            ScrollState::Scrolling => {
                // Active desktop/free scrolling is driven by scroll events,
                // not by per-frame physics. Returning true here made one
                // wheel/trackpad event start a full redraw chain until the
                // idle timeout settled the state; repeated free scrolling
                // kept large apps rendering at vsync between input events.
                //
                // The exception is an explicit bouncy scroll that is already
                // overscrolled. Inputs without a reliable end phase still need
                // a short tick window so `check_idle_bounce` can fire the
                // rebound spring even when the scrollbar is hidden.
                self.bounce_enabled() && self.is_overscrolling()
            }

            ScrollState::Decelerating => {
                // Apply momentum scrolling (for touch devices)
                // On macOS, the system provides momentum via scroll events instead.

                // Exponential decay — matches the real-world feel of a
                // flick coasting to a stop. Removes a *fraction* of
                // velocity per frame (not a constant `deceleration * dt`,
                // which felt draggy near the end: the same absolute
                // slowdown at 20 px/s as at 2000 px/s), so the tail glides
                // out while high velocities still shed quickly.
                //
                // The config's `deceleration` field (px/s²) is the single
                // knob, reinterpreted as a decay-rate driver: `k =
                // deceleration / V_REF` is 1/τ of the exponential (s⁻¹);
                // `V_REF = 500 px/s`, so the default `deceleration = 1500`
                // is k = 3 → ≈0.951 friction per 60Hz frame.
                const V_REF: f32 = 500.0;
                let k = self.config.deceleration / V_REF;
                let friction = (-k * dt).exp();

                // Position update via the CLOSED-FORM integral of the
                // decay over this frame — ∫₀^dt v·e^(−k·s) ds =
                // (v/k)(1 − e^(−k·dt)) = (v/k)(1 − friction) — NOT the
                // explicit-Euler `v·dt`. The analytic form is exact and
                // frame-rate-independent: the offset lands exactly on the
                // decay curve for the elapsed wall-clock time, so an
                // uneven or dropped frame advances by exactly the right
                // distance and never shows a step. `v·dt` instead moves a
                // distance that scales linearly with each frame's jitter,
                // which reads as stutter in the coast — worst over
                // remote/RDP where frame delivery isn't perfectly even.
                // Falls back to `v·dt` when k≈0 (deceleration disabled →
                // constant-velocity glide).
                let (dx, dy) = if k > 1e-4 {
                    let travel = (1.0 - friction) / k;
                    (self.velocity_x * travel, self.velocity_y * travel)
                } else {
                    (self.velocity_x * dt, self.velocity_y * dt)
                };

                // Check if we would hit bounds
                let new_offset_y = self.offset_y + dy;
                let new_offset_x = self.offset_x + dx;

                self.velocity_x *= friction;
                self.velocity_y *= friction;

                // Check if we've hit edge bounds
                let hit_edge_y =
                    new_offset_y > self.min_offset_y() || new_offset_y < self.max_offset_y();
                let hit_edge_x =
                    new_offset_x > self.min_offset_x() || new_offset_x < self.max_offset_x();

                if hit_edge_y || hit_edge_x {
                    // Hit edge - clamp and start bounce
                    self.offset_y = new_offset_y.clamp(self.max_offset_y(), self.min_offset_y());
                    self.offset_x = new_offset_x.clamp(self.max_offset_x(), self.min_offset_x());
                    self.velocity_x = 0.0;
                    self.velocity_y = 0.0;
                    if let Some(new_state) = self.state.on_event(scroll_events::SETTLED) {
                        self.state = new_state;
                    }
                    return false;
                }

                // Update position
                self.offset_y = new_offset_y;
                self.offset_x = new_offset_x;

                // Check if velocity is below threshold
                let stopped = self.velocity_x.abs() < self.config.velocity_threshold
                    && self.velocity_y.abs() < self.config.velocity_threshold;

                if stopped {
                    self.velocity_x = 0.0;
                    self.velocity_y = 0.0;
                    if let Some(new_state) = self.state.on_event(scroll_events::SETTLED) {
                        self.state = new_state;
                    }
                    return false;
                }

                true // Still decelerating
            }

            ScrollState::Bouncing => {
                // Read spring values from scheduler (scheduler ticks them on background thread)
                let Some(scheduler_arc) = self.scheduler.upgrade() else {
                    // No scheduler - snap to bounds and settle
                    self.offset_y = self
                        .offset_y
                        .clamp(self.max_offset_y(), self.min_offset_y());
                    self.offset_x = self
                        .offset_x
                        .clamp(self.max_offset_x(), self.min_offset_x());
                    self.state = ScrollState::Idle;
                    return false;
                };

                let scheduler = scheduler_arc.lock().unwrap();
                let mut still_bouncing = false;

                // Settle when value is within a pixel of target regardless
                // of the spring's internal velocity — the spring's
                // `is_settled` guard requires both |value-target|<ε AND
                // |velocity|<ε, which can fail the second check even once
                // the pixel-space effect is indistinguishable from target
                // (numerical residue, tick-rate mismatch between the bg
                // scheduler thread and the main tick). Without this
                // override the scroll state stays latched in Bouncing,
                // which drops subsequent scroll deltas.
                const SETTLE_EPS: f32 = 0.5;

                // Read vertical spring value
                if let Some(spring_id) = self.spring_y
                    && let Some(spring) = scheduler.get_spring(spring_id)
                {
                    let at_target = (spring.value() - spring.target()).abs() < SETTLE_EPS;
                    if spring.is_settled() || at_target {
                        self.offset_y = spring.target();
                    } else {
                        self.offset_y = spring.value();
                        still_bouncing = true;
                    }
                }

                // Read horizontal spring value
                if let Some(spring_id) = self.spring_x
                    && let Some(spring) = scheduler.get_spring(spring_id)
                {
                    let at_target = (spring.value() - spring.target()).abs() < SETTLE_EPS;
                    if spring.is_settled() || at_target {
                        self.offset_x = spring.target();
                    } else {
                        self.offset_x = spring.value();
                        still_bouncing = true;
                    }
                }

                drop(scheduler);

                if !still_bouncing {
                    self.cancel_springs();
                    self.state = ScrollState::Idle;
                    return false;
                }

                true
            }
        }
    }

    /// Check if a scroll animation is active.
    ///
    /// User-driven `Scrolling` is input-paced and should not by itself
    /// keep the frame loop alive.
    pub fn is_animating(&self) -> bool {
        matches!(
            self.state,
            ScrollState::Decelerating | ScrollState::Bouncing
        )
    }

    /// Set the scroll direction
    pub fn set_direction(&mut self, direction: ScrollDirection) {
        self.config.direction = direction;
        // Reset position when changing direction
        self.offset_x = 0.0;
        self.offset_y = 0.0;
        self.velocity_x = 0.0;
        self.velocity_y = 0.0;
        self.cancel_springs();
        self.state = ScrollState::Idle;
    }

    /// Animate scroll to a target offset using spring physics
    ///
    /// This provides smooth animated scrolling instead of instant jumps.
    /// The spring configuration uses a snappy feel for quick but smooth transitions.
    pub fn scroll_to_animated(&mut self, target_x: f32, target_y: f32) {
        // Cancel any existing springs
        self.cancel_springs();

        // Get scheduler - if not available, just snap to target
        let Some(scheduler_arc) = self.scheduler.upgrade() else {
            self.offset_x = target_x;
            self.offset_y = target_y;
            return;
        };

        let mut scheduler = scheduler_arc.lock().unwrap();

        // Use a snappy spring for scroll animations - fast but smooth
        let scroll_spring_config = SpringConfig::new(400.0, 30.0, 1.0);

        // Animate vertical scroll if direction supports it
        if matches!(
            self.config.direction,
            ScrollDirection::Vertical | ScrollDirection::Both
        ) && (self.offset_y - target_y).abs() > 0.5
        {
            let mut spring = Spring::new(scroll_spring_config, self.offset_y);
            spring.set_target(target_y);
            let spring_id = scheduler.add_spring(spring);
            self.spring_y = Some(spring_id);
        } else if matches!(
            self.config.direction,
            ScrollDirection::Vertical | ScrollDirection::Both
        ) {
            self.offset_y = target_y;
        }

        // Animate horizontal scroll if direction supports it
        if matches!(
            self.config.direction,
            ScrollDirection::Horizontal | ScrollDirection::Both
        ) && (self.offset_x - target_x).abs() > 0.5
        {
            let mut spring = Spring::new(scroll_spring_config, self.offset_x);
            spring.set_target(target_x);
            let spring_id = scheduler.add_spring(spring);
            self.spring_x = Some(spring_id);
        } else if matches!(
            self.config.direction,
            ScrollDirection::Horizontal | ScrollDirection::Both
        ) {
            self.offset_x = target_x;
        }

        drop(scheduler);

        // Transition to bouncing state to read spring values in tick()
        if self.spring_x.is_some() || self.spring_y.is_some() {
            self.state = ScrollState::Bouncing;
        }
    }

    // =========================================================================
    // Scrollbar State Methods
    // =========================================================================

    /// Called when scroll area is hovered
    pub fn on_area_hover_enter(&mut self) {
        self.area_hovered = true;
        self.update_scrollbar_visibility();
    }

    /// Called when scroll area hover ends
    pub fn on_area_hover_leave(&mut self) {
        self.area_hovered = false;
        // Only hide if not dragging
        if self.scrollbar_state != ScrollbarState::Dragging {
            self.update_scrollbar_visibility();
        }
    }

    /// Called when scrollbar track is hovered
    pub fn on_scrollbar_track_hover(&mut self) {
        if self.scrollbar_state != ScrollbarState::Dragging {
            self.scrollbar_state = ScrollbarState::TrackHovered;
            self.update_scrollbar_visibility();
        }
    }

    /// Called when scrollbar thumb is hovered
    pub fn on_scrollbar_thumb_hover(&mut self) {
        if self.scrollbar_state != ScrollbarState::Dragging {
            self.scrollbar_state = ScrollbarState::ThumbHovered;
            self.update_scrollbar_visibility();
        }
    }

    /// Called when scrollbar hover ends (mouse leaves track/thumb)
    pub fn on_scrollbar_hover_leave(&mut self) {
        if self.scrollbar_state != ScrollbarState::Dragging {
            self.scrollbar_state = if self.area_hovered {
                ScrollbarState::Scrolling
            } else {
                ScrollbarState::Idle
            };
            self.update_scrollbar_visibility();
        }
    }

    /// Called when scrollbar thumb drag starts
    ///
    /// # Arguments
    /// * `mouse_x` - Current mouse X position
    /// * `mouse_y` - Current mouse Y position
    pub fn on_scrollbar_drag_start(&mut self, mouse_x: f32, mouse_y: f32) {
        self.scrollbar_state = ScrollbarState::Dragging;
        self.thumb_drag_start_x = mouse_x;
        self.thumb_drag_start_y = mouse_y;
        self.thumb_drag_start_scroll_x = self.offset_x;
        self.thumb_drag_start_scroll_y = self.offset_y;
        self.update_scrollbar_visibility();
    }

    /// Called during scrollbar thumb drag
    ///
    /// # Arguments
    /// * `mouse_x` - Current mouse X position
    /// * `mouse_y` - Current mouse Y position
    ///
    /// # Returns
    /// New scroll offset (x, y) that should be applied
    pub fn on_scrollbar_drag(&mut self, mouse_x: f32, mouse_y: f32) -> (f32, f32) {
        let delta_x = mouse_x - self.thumb_drag_start_x;
        let delta_y = mouse_y - self.thumb_drag_start_y;

        // Calculate thumb travel range and scroll range
        let (thumb_travel_x, scroll_range_x) = self.thumb_travel_x();
        let (thumb_travel_y, scroll_range_y) = self.thumb_travel_y();

        // Convert thumb drag delta to scroll delta
        let scroll_delta_x = if thumb_travel_x > 0.0 {
            (delta_x / thumb_travel_x) * scroll_range_x
        } else {
            0.0
        };
        let scroll_delta_y = if thumb_travel_y > 0.0 {
            (delta_y / thumb_travel_y) * scroll_range_y
        } else {
            0.0
        };

        // Calculate new scroll position (clamped to bounds)
        let new_x = (self.thumb_drag_start_scroll_x - scroll_delta_x)
            .clamp(self.max_offset_x(), self.min_offset_x());
        let new_y = (self.thumb_drag_start_scroll_y - scroll_delta_y)
            .clamp(self.max_offset_y(), self.min_offset_y());

        (new_x, new_y)
    }

    /// Called when scrollbar thumb drag ends
    pub fn on_scrollbar_drag_end(&mut self) {
        self.scrollbar_state = if self.area_hovered {
            ScrollbarState::Scrolling
        } else {
            ScrollbarState::FadingOut
        };
        self.idle_time = 0.0;
        self.update_scrollbar_visibility();
    }

    /// Called when scroll activity occurs (shows scrollbar in Auto mode)
    pub fn on_scroll_activity(&mut self) {
        self.idle_time = 0.0;
        if self.scrollbar_state == ScrollbarState::Idle
            || self.scrollbar_state == ScrollbarState::FadingOut
        {
            self.scrollbar_state = ScrollbarState::Scrolling;
        }
        self.update_scrollbar_visibility();
    }

    /// Update scrollbar visibility based on current state and config
    fn update_scrollbar_visibility(&mut self) {
        let target = match self.config.scrollbar.visibility {
            ScrollbarVisibility::Always => 1.0,
            ScrollbarVisibility::Never => 0.0,
            ScrollbarVisibility::Hover => {
                if self.area_hovered || self.scrollbar_state.is_interacting() {
                    self.scrollbar_state.opacity()
                } else {
                    0.0
                }
            }
            ScrollbarVisibility::Auto => {
                if self.scrollbar_state == ScrollbarState::Idle {
                    0.0
                } else {
                    self.scrollbar_state.opacity()
                }
            }
        };

        // Early-out when the requested target hasn't actually moved.
        // `update_scrollbar_visibility` is invoked every time the
        // scrollbar state machine ticks (`tick_scrollbar`'s FadingOut
        // transition, the Scrolling auto-detect inside the same fn,
        // wheel input, etc.). The previous code unconditionally
        // dropped the old spring and added a new one — each `add_spring`
        // calls `notify_active`, which fires the windowed runner's
        // wake_callback and flips `frame_dirty=true`. The cn_demo
        // scrollbar (Auto visibility) was rapidly toggling
        // FadingOut↔Idle while the spring settled towards 0, calling
        // this fn every frame, creating a new spring every frame,
        // pinging the wake callback every frame, and pinning CPU at
        // 30 % forever even though nothing visually changed once the
        // opacity reached 0.
        if (self.scrollbar_target_opacity - target).abs() < f32::EPSILON {
            return;
        }
        self.scrollbar_target_opacity = target;

        // Animate opacity using spring if scheduler available
        if let Some(scheduler_arc) = self.scheduler.upgrade() {
            let mut scheduler = scheduler_arc.lock().unwrap();

            // Remove existing spring if any
            if let Some(spring_id) = self.scrollbar_opacity_spring.take() {
                scheduler.remove_spring(spring_id);
            }

            // Create new spring for smooth opacity transition
            if (self.scrollbar_opacity - target).abs() > 0.01 {
                let spring_config = SpringConfig::new(300.0, 25.0, 1.0); // Gentle animation
                let mut spring = Spring::new(spring_config, self.scrollbar_opacity);
                spring.set_target(target);
                let spring_id = scheduler.add_spring(spring);
                self.scrollbar_opacity_spring = Some(spring_id);
            } else {
                self.scrollbar_opacity = target;
            }
        } else {
            // No scheduler - instant transition
            self.scrollbar_opacity = target;
        }
    }

    /// Tick scrollbar animation (call from main tick)
    ///
    /// # Arguments
    /// * `dt` - Delta time in seconds
    ///
    /// # Returns
    /// true if scrollbar animation is still active
    pub fn tick_scrollbar(&mut self, dt: f32) -> bool {
        let mut animating = false;

        // Auto-detect scrolling activity from physics state
        // This shows the scrollbar even if scroll events don't go through handlers
        if (self.state == ScrollState::Scrolling || self.state == ScrollState::Bouncing)
            && (self.scrollbar_state == ScrollbarState::Idle
                || self.scrollbar_state == ScrollbarState::FadingOut)
        {
            self.scrollbar_state = ScrollbarState::Scrolling;
            self.idle_time = 0.0;
            self.update_scrollbar_visibility();
        }

        // Update idle timer for auto-dismiss
        if self.scrollbar_state == ScrollbarState::Scrolling
            || self.scrollbar_state == ScrollbarState::FadingOut
        {
            self.idle_time += dt;

            // Check for auto-dismiss timeout
            // For Auto mode: fade after inactivity regardless of hover (unless interacting)
            // For Hover mode: don't fade while hovered
            let should_fade = match self.config.scrollbar.visibility {
                ScrollbarVisibility::Auto => {
                    self.idle_time >= self.config.scrollbar.auto_dismiss_delay
                        && !self.scrollbar_state.is_interacting()
                }
                ScrollbarVisibility::Hover => {
                    self.idle_time >= self.config.scrollbar.auto_dismiss_delay
                        && !self.area_hovered
                        && !self.scrollbar_state.is_interacting()
                }
                _ => false,
            };

            if should_fade && self.scrollbar_state != ScrollbarState::FadingOut {
                self.scrollbar_state = ScrollbarState::FadingOut;
                self.update_scrollbar_visibility();
            }

            // Transition to Idle when fully faded
            if self.scrollbar_state == ScrollbarState::FadingOut && self.scrollbar_opacity < 0.01 {
                self.scrollbar_state = ScrollbarState::Idle;
                self.scrollbar_opacity = 0.0;
            }
        }

        // Read opacity spring value if animating
        if let Some(scheduler_arc) = self.scheduler.upgrade() {
            let scheduler = scheduler_arc.lock().unwrap();
            if let Some(spring_id) = self.scrollbar_opacity_spring
                && let Some(spring) = scheduler.get_spring(spring_id)
            {
                self.scrollbar_opacity = spring.value();
                if !spring.is_settled() {
                    animating = true;
                } else {
                    self.scrollbar_opacity = spring.target();
                }
            }
        }

        animating
    }

    /// Calculate thumb dimensions for vertical scrollbar
    ///
    /// # Returns
    /// (thumb_height, thumb_y_position) in pixels
    pub fn thumb_dimensions_y(&self) -> (f32, f32) {
        let viewport = self.viewport_height;
        let content = self.content_height.max(viewport);
        let scroll_ratio = viewport / content;

        // Calculate thumb height (with minimum)
        let thumb_height = (scroll_ratio * viewport)
            .max(self.config.scrollbar.min_thumb_length)
            .min(viewport - self.config.scrollbar.edge_padding * 2.0);

        // Calculate thumb position
        let max_scroll = (content - viewport).max(0.0);
        let scroll_progress = if max_scroll > 0.0 {
            (-self.offset_y / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let track_height = viewport - self.config.scrollbar.edge_padding * 2.0;
        let max_thumb_travel = track_height - thumb_height;
        let thumb_y = self.config.scrollbar.edge_padding + (scroll_progress * max_thumb_travel);

        (thumb_height, thumb_y)
    }

    /// Calculate thumb dimensions for horizontal scrollbar
    ///
    /// # Returns
    /// (thumb_width, thumb_x_position) in pixels
    pub fn thumb_dimensions_x(&self) -> (f32, f32) {
        let viewport = self.viewport_width;
        let content = self.content_width.max(viewport);
        let scroll_ratio = viewport / content;

        // Calculate thumb width (with minimum)
        let thumb_width = (scroll_ratio * viewport)
            .max(self.config.scrollbar.min_thumb_length)
            .min(viewport - self.config.scrollbar.edge_padding * 2.0);

        // Calculate thumb position
        let max_scroll = (content - viewport).max(0.0);
        let scroll_progress = if max_scroll > 0.0 {
            (-self.offset_x / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let track_width = viewport - self.config.scrollbar.edge_padding * 2.0;
        let max_thumb_travel = track_width - thumb_width;
        let thumb_x = self.config.scrollbar.edge_padding + (scroll_progress * max_thumb_travel);

        (thumb_width, thumb_x)
    }

    /// Get vertical thumb travel range and scroll range
    fn thumb_travel_y(&self) -> (f32, f32) {
        let viewport = self.viewport_height;
        let content = self.content_height.max(viewport);
        let (thumb_height, _) = self.thumb_dimensions_y();

        let track_height = viewport - self.config.scrollbar.edge_padding * 2.0;
        let thumb_travel = track_height - thumb_height;
        let scroll_range = (content - viewport).max(0.0);

        (thumb_travel, scroll_range)
    }

    /// Get horizontal thumb travel range and scroll range
    fn thumb_travel_x(&self) -> (f32, f32) {
        let viewport = self.viewport_width;
        let content = self.content_width.max(viewport);
        let (thumb_width, _) = self.thumb_dimensions_x();

        let track_width = viewport - self.config.scrollbar.edge_padding * 2.0;
        let thumb_travel = track_width - thumb_width;
        let scroll_range = (content - viewport).max(0.0);

        (thumb_travel, scroll_range)
    }

    /// Check if content is scrollable vertically
    pub fn can_scroll_y(&self) -> bool {
        self.content_height > self.viewport_height
    }

    /// Check if content is scrollable horizontally
    pub fn can_scroll_x(&self) -> bool {
        self.content_width > self.viewport_width
    }

    /// Hit test a point against the scrollbar
    ///
    /// Takes coordinates relative to the scroll container (local space).
    /// Returns what part of the scrollbar (if any) the point is over.
    pub fn hit_test_scrollbar(&self, local_x: f32, local_y: f32) -> ScrollbarHitResult {
        let config = &self.config.scrollbar;
        let scrollbar_width = config.width();
        let edge_padding = config.edge_padding;

        // Check vertical scrollbar (right edge)
        if self.can_scroll_y()
            && matches!(
                self.config.direction,
                ScrollDirection::Vertical | ScrollDirection::Both
            )
        {
            let track_x = self.viewport_width - scrollbar_width - edge_padding;
            let track_y = edge_padding;
            let track_height = self.viewport_height - edge_padding * 2.0;

            // Check if in vertical track area
            if local_x >= track_x
                && local_x <= track_x + scrollbar_width
                && local_y >= track_y
                && local_y <= track_y + track_height
            {
                // Check if over thumb
                let (thumb_height, thumb_y) = self.thumb_dimensions_y();
                if local_y >= thumb_y && local_y <= thumb_y + thumb_height {
                    return ScrollbarHitResult::VerticalThumb;
                }
                return ScrollbarHitResult::VerticalTrack;
            }
        }

        // Check horizontal scrollbar (bottom edge)
        if self.can_scroll_x()
            && matches!(
                self.config.direction,
                ScrollDirection::Horizontal | ScrollDirection::Both
            )
        {
            let track_x = edge_padding;
            let track_y = self.viewport_height - scrollbar_width - edge_padding;
            let track_width = self.viewport_width - edge_padding * 2.0;

            // Check if in horizontal track area
            if local_x >= track_x
                && local_x <= track_x + track_width
                && local_y >= track_y
                && local_y <= track_y + scrollbar_width
            {
                // Check if over thumb
                let (thumb_width, thumb_x) = self.thumb_dimensions_x();
                if local_x >= thumb_x && local_x <= thumb_x + thumb_width {
                    return ScrollbarHitResult::HorizontalThumb;
                }
                return ScrollbarHitResult::HorizontalTrack;
            }
        }

        ScrollbarHitResult::None
    }

    /// Handle pointer down on scrollbar
    ///
    /// Returns true if the event was handled (pointer was over scrollbar).
    pub fn on_scrollbar_pointer_down(&mut self, local_x: f32, local_y: f32) -> bool {
        let hit = self.hit_test_scrollbar(local_x, local_y);
        match hit {
            ScrollbarHitResult::VerticalThumb | ScrollbarHitResult::HorizontalThumb => {
                self.on_scrollbar_drag_start(local_x, local_y);
                true
            }
            ScrollbarHitResult::VerticalTrack => {
                // Click on track - jump to that position
                let (thumb_height, _) = self.thumb_dimensions_y();
                let track_height = self.viewport_height - self.config.scrollbar.edge_padding * 2.0;
                let click_ratio =
                    (local_y - self.config.scrollbar.edge_padding - thumb_height / 2.0)
                        / (track_height - thumb_height);
                let click_ratio = click_ratio.clamp(0.0, 1.0);
                let max_scroll = (self.content_height - self.viewport_height).max(0.0);
                self.offset_y = -click_ratio * max_scroll;
                self.on_scroll_activity();
                true
            }
            ScrollbarHitResult::HorizontalTrack => {
                // Click on track - jump to that position
                let (thumb_width, _) = self.thumb_dimensions_x();
                let track_width = self.viewport_width - self.config.scrollbar.edge_padding * 2.0;
                let click_ratio =
                    (local_x - self.config.scrollbar.edge_padding - thumb_width / 2.0)
                        / (track_width - thumb_width);
                let click_ratio = click_ratio.clamp(0.0, 1.0);
                let max_scroll = (self.content_width - self.viewport_width).max(0.0);
                self.offset_x = -click_ratio * max_scroll;
                self.on_scroll_activity();
                true
            }
            ScrollbarHitResult::None => false,
        }
    }

    /// Handle pointer move during scrollbar drag
    ///
    /// Returns Some((new_offset_x, new_offset_y)) if dragging, None otherwise.
    pub fn on_scrollbar_pointer_move(&mut self, local_x: f32, local_y: f32) -> Option<(f32, f32)> {
        if self.scrollbar_state == ScrollbarState::Dragging {
            let (new_x, new_y) = self.on_scrollbar_drag(local_x, local_y);
            self.offset_x = new_x;
            self.offset_y = new_y;
            Some((new_x, new_y))
        } else {
            // Update hover state
            let hit = self.hit_test_scrollbar(local_x, local_y);
            match hit {
                ScrollbarHitResult::VerticalThumb | ScrollbarHitResult::HorizontalThumb => {
                    self.on_scrollbar_thumb_hover();
                }
                ScrollbarHitResult::VerticalTrack | ScrollbarHitResult::HorizontalTrack => {
                    self.on_scrollbar_track_hover();
                }
                ScrollbarHitResult::None => {
                    if self.scrollbar_state == ScrollbarState::ThumbHovered
                        || self.scrollbar_state == ScrollbarState::TrackHovered
                    {
                        self.on_scrollbar_hover_leave();
                    }
                }
            }
            None
        }
    }

    /// Handle pointer up during scrollbar drag
    pub fn on_scrollbar_pointer_up(&mut self) {
        if self.scrollbar_state == ScrollbarState::Dragging {
            self.on_scrollbar_drag_end();
        }
    }

    /// Get current scrollbar render info
    pub fn scrollbar_render_info(&self) -> ScrollbarRenderInfo {
        let (thumb_height, thumb_y) = self.thumb_dimensions_y();
        let (thumb_width, thumb_x) = self.thumb_dimensions_x();

        ScrollbarRenderInfo {
            state: self.scrollbar_state,
            opacity: self.scrollbar_opacity,
            config: self.config.scrollbar,
            // Vertical scrollbar
            show_vertical: self.can_scroll_y()
                && matches!(
                    self.config.direction,
                    ScrollDirection::Vertical | ScrollDirection::Both
                ),
            vertical_thumb_height: thumb_height,
            vertical_thumb_y: thumb_y,
            // Horizontal scrollbar
            show_horizontal: self.can_scroll_x()
                && matches!(
                    self.config.direction,
                    ScrollDirection::Horizontal | ScrollDirection::Both
                ),
            horizontal_thumb_width: thumb_width,
            horizontal_thumb_x: thumb_x,
        }
    }
}

// ============================================================================
// Shared Physics Handle
// ============================================================================

/// Shared handle to scroll physics for external access
pub type SharedScrollPhysics = Arc<Mutex<ScrollPhysics>>;

// ============================================================================
// Scrollbar Render Info (for renderer)
// ============================================================================

/// Information about scrollbar state for rendering
#[derive(Debug, Clone, Copy)]
pub struct ScrollbarRenderInfo {
    /// Current scrollbar interaction state
    pub state: ScrollbarState,
    /// Current opacity (0.0 to 1.0)
    pub opacity: f32,
    /// Scrollbar configuration
    pub config: ScrollbarConfig,
    /// Whether to show vertical scrollbar
    pub show_vertical: bool,
    /// Vertical thumb height in pixels
    pub vertical_thumb_height: f32,
    /// Vertical thumb Y position in pixels
    pub vertical_thumb_y: f32,
    /// Whether to show horizontal scrollbar
    pub show_horizontal: bool,
    /// Horizontal thumb width in pixels
    pub horizontal_thumb_width: f32,
    /// Horizontal thumb X position in pixels
    pub horizontal_thumb_x: f32,
}

impl Default for ScrollbarRenderInfo {
    fn default() -> Self {
        Self {
            state: ScrollbarState::Idle,
            opacity: 0.0,
            config: ScrollbarConfig::default(),
            show_vertical: false,
            vertical_thumb_height: 30.0,
            vertical_thumb_y: 0.0,
            show_horizontal: false,
            horizontal_thumb_width: 30.0,
            horizontal_thumb_x: 0.0,
        }
    }
}

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

/// A scrollable container element with optional bounce physics
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
    /// Optional scroll reference for programmatic control
    scroll_ref: Option<ScrollRef>,
    /// When true, the renderer skips painting any descendant whose
    /// post-scroll bounds don't intersect the visible viewport (plus a
    /// small overscan buffer). Saves GPU primitive memory and draw calls
    /// for long scrolls with many off-screen children. Layout is still
    /// computed for all children — this only culls paint.
    viewport_cull: bool,
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
            // Set items_start, justify_start, and content_start to ensure child starts
            // at top-left edge (not centered/stretched) on all alignment axes.
            inner: Div::new()
                .overflow_scroll_style_only()
                .items_start()
                .justify_start()
                .content_start(),
            content: None,
            physics,
            handlers,
            scroll_ref: None,
            viewport_cull: false,
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: ScrollConfig) -> Self {
        let physics = Arc::new(Mutex::new(ScrollPhysics::new(config)));
        let handlers = Self::create_internal_handlers(Arc::clone(&physics));

        Self {
            inner: Div::new()
                .overflow_scroll_style_only()
                .items_start()
                .justify_start()
                .content_start(),
            content: None,
            physics,
            handlers,
            scroll_ref: None,
            viewport_cull: false,
        }
    }

    /// Create with external shared physics (for state persistence)
    pub fn with_physics(physics: SharedScrollPhysics) -> Self {
        let handlers = Self::create_internal_handlers(Arc::clone(&physics));

        Self {
            inner: Div::new()
                .overflow_scroll_style_only()
                .items_start()
                .justify_start()
                .content_start(),
            content: None,
            physics,
            handlers,
            scroll_ref: None,
            viewport_cull: false,
        }
    }

    /// Create internal event handlers that update physics state
    ///
    /// This is public so that other containers (e.g., Div with `overflow: scroll`)
    /// can reuse the same scroll handler infrastructure.
    pub fn create_internal_handlers(physics: SharedScrollPhysics) -> EventHandlers {
        let mut handlers = EventHandlers::new();

        // Internal handler that applies scroll delta to physics and shows scrollbar
        handlers.on_scroll({
            let physics = Arc::clone(&physics);
            move |ctx| {
                let mut p = physics.lock().unwrap();
                // Use touch scroll with velocity tracking if time is provided (mobile)
                if let Some(time) = ctx.scroll_time {
                    p.apply_touch_scroll_delta(ctx.scroll_delta_x, ctx.scroll_delta_y, time);
                } else {
                    // Desktop/trackpad - system provides momentum
                    p.apply_scroll_delta(ctx.scroll_delta_x, ctx.scroll_delta_y);
                }
                // Show scrollbar when scrolling
                p.on_scroll_activity();
            }
        });

        // Show scrollbar on hover (for Hover visibility mode)
        handlers.on_hover_enter({
            let physics = Arc::clone(&physics);
            move |_ctx| {
                physics.lock().unwrap().on_area_hover_enter();
            }
        });

        // Hide scrollbar on hover leave
        handlers.on_hover_leave({
            let physics = Arc::clone(&physics);
            move |_ctx| {
                physics.lock().unwrap().on_area_hover_leave();
            }
        });

        // Handle pointer down on scrollbar (start drag or track click)
        handlers.on_mouse_down({
            let physics = Arc::clone(&physics);
            move |ctx| {
                let mut p = physics.lock().unwrap();
                // Check if click is on scrollbar using local coordinates
                p.on_scrollbar_pointer_down(ctx.local_x, ctx.local_y);
            }
        });

        // Handle drag events for scrollbar dragging
        handlers.on_drag({
            let physics = Arc::clone(&physics);
            move |ctx| {
                let mut p = physics.lock().unwrap();
                if p.scrollbar_state == ScrollbarState::Dragging {
                    p.on_scrollbar_pointer_move(ctx.local_x, ctx.local_y);
                }
            }
        });

        // Handle drag end for scrollbar
        handlers.on_drag_end({
            let physics = Arc::clone(&physics);
            move |_ctx| {
                let mut p = physics.lock().unwrap();
                p.on_scrollbar_pointer_up();
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

    /// Enable or disable bounce physics (default: disabled)
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
    ///
    /// This also updates the Taffy overflow settings:
    /// - Vertical: overflow_y = Scroll, overflow_x = Clip (width constrained)
    /// - Horizontal: overflow_x = Scroll, overflow_y = Clip (height constrained)
    /// - Both: overflow = Scroll on both axes
    pub fn direction(mut self, direction: ScrollDirection) -> Self {
        self.physics.lock().unwrap().config.direction = direction;

        // Update Taffy overflow based on direction to constrain the non-scrolling axis
        use taffy::Overflow;
        match direction {
            ScrollDirection::Vertical => {
                // Width constrained, height scrollable
                self.inner = std::mem::take(&mut self.inner)
                    .overflow_x(Overflow::Clip)
                    .overflow_y(Overflow::Scroll);
            }
            ScrollDirection::Horizontal => {
                // Height constrained, width scrollable
                self.inner = std::mem::take(&mut self.inner)
                    .overflow_x(Overflow::Scroll)
                    .overflow_y(Overflow::Clip);
            }
            ScrollDirection::Both => {
                // Both axes scrollable
                self.inner = std::mem::take(&mut self.inner).overflow_scroll();
            }
        }
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
    // Scrollbar Configuration
    // =========================================================================

    /// Set scrollbar visibility mode
    pub fn scrollbar_visibility(self, visibility: ScrollbarVisibility) -> Self {
        let mut physics = self.physics.lock().unwrap();
        physics.config.scrollbar.visibility = visibility;
        // Update visibility immediately so initial state is correct
        physics.update_scrollbar_visibility();
        drop(physics);
        self
    }

    /// Always show scrollbar
    pub fn scrollbar_always(self) -> Self {
        self.scrollbar_visibility(ScrollbarVisibility::Always)
    }

    /// Show scrollbar on hover only
    pub fn scrollbar_on_hover(self) -> Self {
        self.scrollbar_visibility(ScrollbarVisibility::Hover)
    }

    /// Auto-show scrollbar when scrolling (default macOS style)
    pub fn scrollbar_auto(self) -> Self {
        self.scrollbar_visibility(ScrollbarVisibility::Auto)
    }

    /// Hide scrollbar completely
    pub fn scrollbar_hidden(self) -> Self {
        self.scrollbar_visibility(ScrollbarVisibility::Never)
    }

    /// Set scrollbar size preset
    pub fn scrollbar_size(self, size: ScrollbarSize) -> Self {
        self.physics.lock().unwrap().config.scrollbar.size = size;
        self
    }

    /// Set thin scrollbar (4px)
    pub fn scrollbar_thin(self) -> Self {
        self.scrollbar_size(ScrollbarSize::Thin)
    }

    /// Set wide scrollbar (10px)
    pub fn scrollbar_wide(self) -> Self {
        self.scrollbar_size(ScrollbarSize::Wide)
    }

    /// Set custom scrollbar width
    pub fn scrollbar_width(self, width: f32) -> Self {
        self.physics.lock().unwrap().config.scrollbar.custom_width = Some(width);
        self
    }

    /// Set scrollbar thumb color
    pub fn scrollbar_thumb_color(self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.physics.lock().unwrap().config.scrollbar.thumb_color = [r, g, b, a];
        self
    }

    /// Set scrollbar track color
    pub fn scrollbar_track_color(self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.physics.lock().unwrap().config.scrollbar.track_color = [r, g, b, a];
        self
    }

    /// Set scrollbar auto-dismiss delay in seconds
    pub fn scrollbar_dismiss_delay(self, seconds: f32) -> Self {
        self.physics
            .lock()
            .unwrap()
            .config
            .scrollbar
            .auto_dismiss_delay = seconds;
        self
    }

    /// Get current scrollbar render info
    pub fn scrollbar_info(&self) -> ScrollbarRenderInfo {
        self.physics.lock().unwrap().scrollbar_render_info()
    }

    // =========================================================================
    // Element ID and ScrollRef binding
    // =========================================================================

    /// Set element ID for this scroll container
    ///
    /// This allows the element to be queried by ID via `ctx.query("id")`.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.inner = std::mem::take(&mut self.inner).id(id);
        self
    }

    /// Bind a ScrollRef for programmatic scroll control
    ///
    /// The ScrollRef can be used to:
    /// - Scroll to elements by ID: `scroll_ref.scroll_to("item-42")`
    /// - Scroll to top/bottom: `scroll_ref.scroll_to_top()`, `scroll_ref.scroll_to_bottom()`
    /// - Scroll by offset: `scroll_ref.scroll_by(0.0, 100.0)`
    /// - Query scroll state: `scroll_ref.offset()`, `scroll_ref.is_at_bottom()`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let scroll_ref = ScrollRef::new();
    ///
    /// scroll()
    ///     .bind(&scroll_ref)
    ///     .child(items.iter().map(|i| div().id(format!("item-{}", i.id))))
    ///
    /// // Later:
    /// scroll_ref.scroll_to("item-42");
    /// scroll_ref.scroll_to_bottom();
    /// ```
    pub fn bind(mut self, scroll_ref: &ScrollRef) -> Self {
        self.scroll_ref = Some(scroll_ref.clone());
        self
    }

    /// Get the bound ScrollRef, if any
    pub fn scroll_ref(&self) -> Option<&ScrollRef> {
        self.scroll_ref.as_ref()
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

    pub fn border(mut self, width: f32, color: blinc_core::Color) -> Self {
        self.inner = std::mem::take(&mut self.inner).border(width, color);
        self
    }

    pub fn border_color(mut self, color: blinc_core::Color) -> Self {
        self.inner = std::mem::take(&mut self.inner).border_color(color);
        self
    }

    pub fn border_width(mut self, width: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).border_width(width);
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

    pub fn overflow_fade(mut self, distance: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).overflow_fade(distance);
        self
    }

    pub fn overflow_fade_x(mut self, distance: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).overflow_fade_x(distance);
        self
    }

    pub fn overflow_fade_y(mut self, distance: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).overflow_fade_y(distance);
        self
    }

    pub fn overflow_fade_edges(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).overflow_fade_edges(top, right, bottom, left);
        self
    }

    /// Add scrollable child content (alias for content())
    pub fn child(self, child: impl ElementBuilder + 'static) -> Self {
        self.content(child)
    }

    /// Cull descendants whose post-scroll bounds fall outside the
    /// visible viewport (with a small overscan buffer for smooth scroll
    /// preroll). Skips paint only — layout is still computed for every
    /// child. Use for scroll containers with many children where most
    /// are off-screen at any time (long lists, dashboards).
    pub fn viewport_cull(mut self, on: bool) -> Self {
        self.viewport_cull = on;
        self
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
        ElementTypeId::Div
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("scroll")
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

    fn viewport_cull(&self) -> bool {
        self.viewport_cull
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }

    fn bound_scroll_ref(&self) -> Option<&ScrollRef> {
        self.scroll_ref.as_ref()
    }
}

// ============================================================================
// Convenience Constructor
// ============================================================================

/// Per-call-site registry of `SharedScrollPhysics` keyed by
/// [`InstanceKey`]. Each `scroll()` / `scroll_no_bounce()` call site
/// gets its own slot, identified by the source location of the call
/// (file/line/column + per-frame call index — see [`InstanceKey`] for
/// the exact key format).
///
/// This is what makes `scroll()` survive tree rebuilds without the
/// caller having to reach for `ctx.use_state_keyed("scroll_physics",
/// …) + Scroll::with_physics(…)`. On each rebuild, the same call
/// site looks up the same key and gets back the same physics —
/// scroll position, momentum, and bounce state are all preserved.
///
/// Resize is the canonical case: when the window resizes, the runner
/// re-runs the user's UI builder so width/height-dependent layout
/// (e.g. `scroll().w_full().h(ctx.height - 96.0)`) reflects the new
/// dimensions, but the underlying physics has to survive that
/// rebuild or the user sees their scroll position snap back to 0
/// every time they grab the window edge.
///
/// **Threading**: `thread_local!` is correct on both desktop and
/// wasm32. Desktop's UI builder runs on the main thread; wasm32's
/// runs on the browser main thread (the only thread that exists).
/// We never share scroll physics between threads.
///
/// **Cleanup**: entries here are *not* garbage-collected when the
/// associated scroll widget disappears from the tree. The leak is
/// bounded by the number of distinct scroll-call sites in the user's
/// source, which is small in practice. If a future workload turns
/// up many short-lived dynamic scroll containers, gate cleanup on
/// the `reset_call_counters()` boundary.
thread_local! {
    static SCROLL_PHYSICS_REGISTRY: std::cell::RefCell<
        std::collections::HashMap<String, SharedScrollPhysics>
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Look up (or insert) the `SharedScrollPhysics` for a given
/// `InstanceKey`, seeding fresh entries with the caller-provided
/// [`ScrollConfig`]. The first call at a source location allocates
/// fresh physics; subsequent rebuilds return the same shared object.
fn physics_for_key_with_config(
    key: &crate::key::InstanceKey,
    config: ScrollConfig,
) -> SharedScrollPhysics {
    let id = key.get().to_string();
    SCROLL_PHYSICS_REGISTRY.with(|reg| {
        let mut reg = reg.borrow_mut();
        reg.entry(id)
            .or_insert_with(|| Arc::new(Mutex::new(ScrollPhysics::new(config))))
            .clone()
    })
}

/// Create a new scroll container.
///
/// Bounce physics is **disabled by default** — momentum scrolling
/// still works, but the rubber-band spring at edges is off. Native
/// HTML scrolling has no rubber-band either (except for iOS / macOS
/// Safari at the *page* level, owned by the OS), and most desktop
/// apps don't either; disabling matches expectations and avoids the
/// idle-CPU cost of a spring that occasionally fails to settle (see
/// `gotcha_scroll_state_latched` for the latching variant). Opt in
/// via [`scroll_bouncy`] or by supplying a custom `ScrollConfig` via
/// [`Scroll::with_physics`].
///
/// The scroll container inherits ALL Div methods, so you have full
/// layout control.
///
/// **Physics persists across rebuilds.** Each `scroll()` call site
/// gets its own [`crate::InstanceKey`]-derived slot in a thread-local
/// registry, so window resizes, theme changes, and other rebuild
/// triggers no longer reset the scroll position. If you need
/// explicit control of the physics object (e.g. to share one
/// physics across multiple Scroll widgets, or to read the current
/// offset from outside the scroll), use [`Scroll::with_physics`]
/// directly with your own `SharedScrollPhysics`.
///
/// # Example
///
/// ```rust,ignore
/// use blinc_layout::prelude::*;
///
/// let scrollable = scroll()
///     .h(400.0)
///     .rounded(16.0)
///     .shadow_sm()
///     .child(div().flex_col().gap(8.0));
/// ```
#[track_caller]
pub fn scroll() -> Scroll {
    let key = crate::key::InstanceKey::new("scroll");
    Scroll::with_physics(physics_for_key_with_config(&key, ScrollConfig::no_bounce()))
}

/// Create a scroll container with iOS-style bounce physics enabled.
///
/// Same auto-persistence semantics as [`scroll`] — each call site
/// gets its own slot in the per-call-site physics registry. Bounce
/// spring is critically damped (110, derived from the default
/// stiffness 3000); use [`Scroll::with_physics`] with
/// `ScrollConfig::stiff_bounce()` / `gentle_bounce()` if you need a
/// different feel.
///
/// On wasm32, bounce remains effectively off: the browser doesn't
/// emit `ScrollPhase::Ended`, so the spring has no reliable trigger
/// to fire from. The constructor accepts the call but the spring
/// will not animate in browser builds.
#[track_caller]
pub fn scroll_bouncy() -> Scroll {
    let key = crate::key::InstanceKey::new("scroll_bouncy");
    #[cfg(not(target_arch = "wasm32"))]
    let physics = physics_for_key_with_config(&key, ScrollConfig::bouncy());
    #[cfg(target_arch = "wasm32")]
    let physics = physics_for_key_with_config(&key, ScrollConfig::no_bounce());
    Scroll::with_physics(physics)
}

/// Create a scroll container with bounce disabled.
///
/// As of the bounce-disabled-by-default change this is now
/// functionally identical to [`scroll`]; retained for back-compat
/// and as an explicit-intent signal at call sites.
#[track_caller]
pub fn scroll_no_bounce() -> Scroll {
    let key = crate::key::InstanceKey::new("scroll_no_bounce");
    Scroll::with_physics(physics_for_key_with_config(&key, ScrollConfig::no_bounce()))
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
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
    fn test_scroll_config_default_has_no_bounce() {
        assert!(!ScrollConfig::default().bounce_enabled);
        assert!(ScrollConfig::bouncy().bounce_enabled);
        assert!(ScrollConfig::stiff_bounce().bounce_enabled);
        assert!(ScrollConfig::gentle_bounce().bounce_enabled);
    }

    #[test]
    fn test_scroll_physics_overscroll() {
        let mut physics = ScrollPhysics::new(ScrollConfig::bouncy());
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        // Scroll past top (vertical)
        physics.apply_scroll_delta(0.0, 50.0);
        assert!(physics.is_overscrolling_y());
        assert!(physics.overscroll_amount_y() > 0.0);
    }

    #[test]
    fn test_scroll_physics_bounce() {
        // Create scheduler for bounce animation
        let scheduler = Arc::new(Mutex::new(AnimationScheduler::new()));

        let mut physics = ScrollPhysics::with_scheduler(ScrollConfig::bouncy(), &scheduler);
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        // Overscroll at top
        physics.offset_y = 50.0;
        physics.state = ScrollState::Scrolling;

        // End scroll gesture - should transition to Bouncing
        physics.on_scroll_end();

        // Should be bouncing back (spring created for animation)
        assert_eq!(physics.state, ScrollState::Bouncing);
        assert!(physics.spring_y.is_some());

        // Note: In real usage, the scheduler runs on a background thread and steps
        // springs over real time. In tests, we verify the state machine behavior:
        // - Bouncing state was entered
        // - Spring was created
        // - SETTLED event transitions to Idle

        // Simulate spring settling by manually triggering SETTLED event
        use crate::stateful::{StateTransitions, scroll_events};
        physics.offset_y = 0.0; // Spring would animate to target (0.0)
        if let Some(new_state) = physics.state.on_event(scroll_events::SETTLED) {
            physics.state = new_state;
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
    fn test_scrolling_tick_is_input_paced_not_frame_animating() {
        let mut physics = ScrollPhysics::default();
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        physics.apply_scroll_delta(0.0, -50.0);

        assert_eq!(physics.state, ScrollState::Scrolling);
        assert!(!physics.tick(1.0 / 60.0));
        assert!(!physics.is_animating());
    }

    #[test]
    fn test_bouncy_overscroll_tick_stays_alive_for_rebound() {
        let mut physics = ScrollPhysics::new(ScrollConfig::bouncy());
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        physics.apply_scroll_delta(0.0, 50.0);

        assert_eq!(physics.state, ScrollState::Scrolling);
        assert!(physics.is_overscrolling());
        assert!(physics.tick(1.0 / 60.0));
        assert!(!physics.is_animating());
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

        // Tick returns false (no internal animation) - momentum comes from system scroll events
        let still_animating = physics.tick(1.0 / 60.0);
        assert!(!still_animating);

        // // State remains Decelerating until SETTLED event is sent externally
        // // (on macOS, this happens when system momentum scroll events stop)
        // assert_eq!(physics.state, ScrollState::Decelerating);

        // Manually trigger SETTLED to transition to Idle
        use crate::stateful::{StateTransitions, scroll_events};
        if let Some(new_state) = physics.state.on_event(scroll_events::SETTLED) {
            physics.state = new_state;
        }
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

    #[test]
    fn test_scroll_child_starts_at_origin() {
        use crate::div::div;
        use taffy::{AvailableSpace, Size};

        // Create a scroll container with a smaller child
        let s = scroll().w(400.0).h(300.0).child(div().w(200.0).h(100.0));

        let mut tree = LayoutTree::new();
        let root_id = s.build(&mut tree);

        // Compute layout
        tree.compute_layout(
            root_id,
            Size {
                width: AvailableSpace::Definite(400.0),
                height: AvailableSpace::Definite(300.0),
            },
        );

        // Get the scroll container's child
        let children = tree.children(root_id);
        assert!(!children.is_empty(), "Scroll should have a child");

        let child_id = children[0];
        let child_layout = tree.get_layout(child_id).expect("Child should have layout");

        // Child should start at (0, 0) relative to scroll container
        assert_eq!(
            child_layout.location.x, 0.0,
            "Child x should be at origin, got {}",
            child_layout.location.x
        );
        assert_eq!(
            child_layout.location.y, 0.0,
            "Child y should be at origin, got {}",
            child_layout.location.y
        );
    }

    #[test]
    fn test_scroll_with_items_center_still_starts_at_origin() {
        use crate::div::div;
        use taffy::{AvailableSpace, Size};

        // Create a scroll container with items_center - this should NOT offset content
        // since scroll content should always start at the edge
        let s = scroll()
            .w(400.0)
            .h(300.0)
            .items_center() // User applies items_center
            .child(div().w(200.0).h(100.0));

        let mut tree = LayoutTree::new();
        let root_id = s.build(&mut tree);

        tree.compute_layout(
            root_id,
            Size {
                width: AvailableSpace::Definite(400.0),
                height: AvailableSpace::Definite(300.0),
            },
        );

        let children = tree.children(root_id);
        let child_id = children[0];
        let child_layout = tree.get_layout(child_id).expect("Child should have layout");

        // For horizontal scroll (default is vertical), items_center centers on cross axis (x)
        // For vertical scroll, items_center centers on cross axis (x)
        // This test documents current behavior
        println!(
            "With items_center: child location = ({}, {})",
            child_layout.location.x, child_layout.location.y
        );

        // Note: When items_center is applied, the child WILL be centered horizontally
        // This is expected behavior based on flexbox rules
        // The issue the user reports might be about main axis (justify), not cross axis
    }

    #[test]
    fn test_scroll_with_justify_center_offsets_content() {
        use crate::div::div;
        use taffy::{AvailableSpace, Size};

        // If user applies justify_center, content WILL be offset on main axis
        // Default flex direction is Row, so main axis is X
        let s = scroll()
            .w(400.0)
            .h(300.0)
            .justify_center() // User explicitly applies justify_center
            .child(div().w(200.0).h(100.0));

        let mut tree = LayoutTree::new();
        let root_id = s.build(&mut tree);

        tree.compute_layout(
            root_id,
            Size {
                width: AvailableSpace::Definite(400.0),
                height: AvailableSpace::Definite(300.0),
            },
        );

        let children = tree.children(root_id);
        let child_id = children[0];
        let child_layout = tree.get_layout(child_id).expect("Child should have layout");

        // Default flex direction is Row, so justify_center centers on X axis
        // Child width 200, viewport 400 -> centered at x=100
        assert_eq!(
            child_layout.location.x, 100.0,
            "justify_center should center child on x axis (main axis for flex-row)"
        );
        assert_eq!(
            child_layout.location.y, 0.0,
            "y should be 0 (cross axis not affected)"
        );
    }

    #[test]
    fn test_horizontal_scroll_content_position() {
        use crate::div::div;
        use taffy::{AvailableSpace, Size};

        // Simulate carousel structure: horizontal scroll with flex_row child
        let s = scroll()
            .direction(ScrollDirection::Horizontal)
            .w(400.0)
            .h(300.0)
            .items_start() // Cross axis (Y) alignment
            .child(div().flex_row().gap(20.0).children(vec![
                div().w(280.0).h(280.0),
                div().w(280.0).h(280.0),
                div().w(280.0).h(280.0),
            ]));

        let mut tree = LayoutTree::new();
        let root_id = s.build(&mut tree);

        tree.compute_layout(
            root_id,
            Size {
                width: AvailableSpace::Definite(400.0),
                height: AvailableSpace::Definite(300.0),
            },
        );

        // Get scroll's direct child (the flex_row container)
        let children = tree.children(root_id);
        assert!(!children.is_empty(), "Scroll should have content");
        let content_id = children[0];
        let content_layout = tree
            .get_layout(content_id)
            .expect("Content should have layout");

        // Print for debugging
        println!(
            "Content container: location=({}, {}), size=({}, {})",
            content_layout.location.x,
            content_layout.location.y,
            content_layout.size.width,
            content_layout.size.height
        );

        // Content should start at origin (0, 0)
        assert_eq!(
            content_layout.location.x, 0.0,
            "Horizontal scroll content should start at x=0, got {}",
            content_layout.location.x
        );
        assert_eq!(
            content_layout.location.y, 0.0,
            "Content should start at y=0, got {}",
            content_layout.location.y
        );

        // Get the first card
        let content_children = tree.children(content_id);
        if !content_children.is_empty() {
            let first_card = content_children[0];
            let first_card_layout = tree.get_layout(first_card).expect("First card layout");
            println!(
                "First card: location=({}, {})",
                first_card_layout.location.x, first_card_layout.location.y
            );

            // First card should be at origin within the content container
            assert_eq!(
                first_card_layout.location.x, 0.0,
                "First card should be at x=0, got {}",
                first_card_layout.location.x
            );
        }
    }

    // =========================================================================
    // Scrollbar State Tests
    // =========================================================================

    #[test]
    fn test_scrollbar_state_visibility() {
        assert!(!ScrollbarState::Idle.is_visible());
        assert!(ScrollbarState::TrackHovered.is_visible());
        assert!(ScrollbarState::ThumbHovered.is_visible());
        assert!(ScrollbarState::Dragging.is_visible());
        assert!(ScrollbarState::Scrolling.is_visible());
        assert!(ScrollbarState::FadingOut.is_visible());
    }

    #[test]
    fn test_scrollbar_state_interacting() {
        assert!(!ScrollbarState::Idle.is_interacting());
        assert!(!ScrollbarState::TrackHovered.is_interacting());
        assert!(ScrollbarState::ThumbHovered.is_interacting());
        assert!(ScrollbarState::Dragging.is_interacting());
        assert!(!ScrollbarState::Scrolling.is_interacting());
        assert!(!ScrollbarState::FadingOut.is_interacting());
    }

    #[test]
    fn test_scrollbar_state_opacity() {
        assert_eq!(ScrollbarState::Idle.opacity(), 0.0);
        assert!(ScrollbarState::Dragging.opacity() > ScrollbarState::ThumbHovered.opacity());
        assert!(ScrollbarState::ThumbHovered.opacity() > ScrollbarState::Scrolling.opacity());
        assert!(ScrollbarState::FadingOut.opacity() > 0.0);
    }

    #[test]
    fn test_scrollbar_size_presets() {
        assert_eq!(ScrollbarSize::Thin.width(), 4.0);
        assert_eq!(ScrollbarSize::Normal.width(), 6.0);
        assert_eq!(ScrollbarSize::Wide.width(), 10.0);
    }

    #[test]
    fn test_scrollbar_config_default() {
        let config = ScrollbarConfig::default();
        assert_eq!(config.visibility, ScrollbarVisibility::Auto);
        assert_eq!(config.size, ScrollbarSize::Normal);
        assert!(config.custom_width.is_none());
        assert!(config.auto_dismiss_delay > 0.0);
        assert!(config.min_thumb_length > 0.0);
    }

    #[test]
    fn test_scrollbar_config_presets() {
        let always = ScrollbarConfig::always_visible();
        assert_eq!(always.visibility, ScrollbarVisibility::Always);

        let hover = ScrollbarConfig::show_on_hover();
        assert_eq!(hover.visibility, ScrollbarVisibility::Hover);

        let hidden = ScrollbarConfig::hidden();
        assert_eq!(hidden.visibility, ScrollbarVisibility::Never);
    }

    #[test]
    fn test_scrollbar_config_width() {
        let mut config = ScrollbarConfig::default();
        assert_eq!(config.width(), 6.0); // Normal size

        config.size = ScrollbarSize::Thin;
        assert_eq!(config.width(), 4.0);

        config.custom_width = Some(12.0);
        assert_eq!(config.width(), 12.0); // Custom takes precedence
    }

    #[test]
    fn test_scrollbar_thumb_dimensions() {
        let mut physics = ScrollPhysics::default();
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;
        physics.viewport_width = 300.0;
        physics.content_width = 600.0;

        // Test vertical thumb
        let (thumb_height, thumb_y) = physics.thumb_dimensions_y();
        assert!(thumb_height >= physics.config.scrollbar.min_thumb_length);
        assert!(thumb_height <= physics.viewport_height);
        assert!(thumb_y >= 0.0);

        // Test horizontal thumb
        let (thumb_width, thumb_x) = physics.thumb_dimensions_x();
        assert!(thumb_width >= physics.config.scrollbar.min_thumb_length);
        assert!(thumb_width <= physics.viewport_width);
        assert!(thumb_x >= 0.0);
    }

    #[test]
    fn test_scrollbar_thumb_position_updates() {
        let mut physics = ScrollPhysics::default();
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        // At top
        physics.offset_y = 0.0;
        let (_, thumb_y_at_top) = physics.thumb_dimensions_y();

        // Scroll to middle
        physics.offset_y = -300.0; // Halfway through 600px scrollable
        let (_, thumb_y_at_middle) = physics.thumb_dimensions_y();

        // Scroll to bottom
        physics.offset_y = -600.0;
        let (_, thumb_y_at_bottom) = physics.thumb_dimensions_y();

        // Thumb should move down as we scroll
        assert!(thumb_y_at_middle > thumb_y_at_top);
        assert!(thumb_y_at_bottom > thumb_y_at_middle);
    }

    #[test]
    fn test_scrollbar_state_transitions() {
        let mut physics = ScrollPhysics::default();
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;

        // Initial state
        assert_eq!(physics.scrollbar_state, ScrollbarState::Idle);
        assert_eq!(physics.scrollbar_opacity, 0.0);

        // Area hover enter
        physics.on_area_hover_enter();
        assert!(physics.area_hovered);

        // Thumb hover
        physics.on_scrollbar_thumb_hover();
        assert_eq!(physics.scrollbar_state, ScrollbarState::ThumbHovered);

        // Start drag
        physics.on_scrollbar_drag_start(100.0, 100.0);
        assert_eq!(physics.scrollbar_state, ScrollbarState::Dragging);
        assert_eq!(physics.thumb_drag_start_y, 100.0);

        // End drag
        physics.on_scrollbar_drag_end();
        assert_ne!(physics.scrollbar_state, ScrollbarState::Dragging);

        // Area hover leave
        physics.on_area_hover_leave();
        assert!(!physics.area_hovered);
    }

    #[test]
    fn test_scrollbar_can_scroll() {
        let mut physics = ScrollPhysics::default();

        // No content - can't scroll
        physics.viewport_height = 400.0;
        physics.content_height = 300.0;
        assert!(!physics.can_scroll_y());

        // More content than viewport - can scroll
        physics.content_height = 1000.0;
        assert!(physics.can_scroll_y());

        // Same for horizontal
        physics.viewport_width = 300.0;
        physics.content_width = 200.0;
        assert!(!physics.can_scroll_x());

        physics.content_width = 600.0;
        assert!(physics.can_scroll_x());
    }

    #[test]
    fn test_scrollbar_render_info() {
        let mut physics = ScrollPhysics::default();
        physics.viewport_height = 400.0;
        physics.content_height = 1000.0;
        physics.viewport_width = 300.0;
        physics.content_width = 300.0; // No horizontal scroll

        let info = physics.scrollbar_render_info();

        assert_eq!(info.state, ScrollbarState::Idle);
        assert!(info.show_vertical); // Content > viewport
        assert!(!info.show_horizontal); // Content == viewport
        assert!(info.vertical_thumb_height > 0.0);
    }

    #[test]
    fn test_scroll_builder_scrollbar_config() {
        let s = scroll()
            .h(400.0)
            .scrollbar_always()
            .scrollbar_thin()
            .scrollbar_dismiss_delay(2.0);

        let physics = s.physics.lock().unwrap();
        assert_eq!(
            physics.config.scrollbar.visibility,
            ScrollbarVisibility::Always
        );
        assert_eq!(physics.config.scrollbar.size, ScrollbarSize::Thin);
        assert_eq!(physics.config.scrollbar.auto_dismiss_delay, 2.0);
    }
}
