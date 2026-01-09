//! Visual Animation System (FLIP-style)
//!
//! This module implements a FLIP-style animation system for layout changes:
//! - **F**irst: Record old visual bounds before layout change
//! - **L**ast: Let layout compute new (final) bounds
//! - **I**nvert: Calculate visual transform to show old position
//! - **P**lay: Animate transform from inverted to identity (zero offset)
//!
//! Key principle: **Taffy owns layout truth** - animations never modify the layout tree.
//! Instead, we track visual offsets that get animated back to zero.

use blinc_animation::{AnimatedValue, SchedulerHandle, SpringConfig};
use blinc_core::Rect;

use crate::element::ElementBounds;

// ============================================================================
// Animation Direction
// ============================================================================

/// Direction of the animation (affects clipping strategy)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationDirection {
    /// Growing from smaller to larger
    Expanding,
    /// Shrinking from larger to smaller
    Collapsing,
    /// Different directions for different properties
    Mixed,
}

// ============================================================================
// Animated Offset Components
// ============================================================================

/// Animated offset from layout position to visual position
/// Values animate from initial offset back to 0 (identity)
pub struct AnimatedOffset {
    /// Horizontal offset (dx from layout position)
    pub x: Option<AnimatedValue>,
    /// Vertical offset (dy from layout position)
    pub y: Option<AnimatedValue>,
}

impl AnimatedOffset {
    /// Create with no offset animation
    pub fn none() -> Self {
        Self { x: None, y: None }
    }

    /// Get current x offset (0 if not animating)
    pub fn get_x(&self) -> f32 {
        self.x.as_ref().map(|v| v.get()).unwrap_or(0.0)
    }

    /// Get current y offset (0 if not animating)
    pub fn get_y(&self) -> f32 {
        self.y.as_ref().map(|v| v.get()).unwrap_or(0.0)
    }

    /// Check if any offset is animating
    pub fn is_animating(&self) -> bool {
        self.x.as_ref().map(|v| v.is_animating()).unwrap_or(false)
            || self.y.as_ref().map(|v| v.is_animating()).unwrap_or(false)
    }
}

/// Animated size delta from layout size
/// Values animate from initial delta back to 0 (identity)
pub struct AnimatedSizeDelta {
    /// Width delta (dw from layout width)
    pub width: Option<AnimatedValue>,
    /// Height delta (dh from layout height)
    pub height: Option<AnimatedValue>,
}

impl AnimatedSizeDelta {
    /// Create with no size animation
    pub fn none() -> Self {
        Self {
            width: None,
            height: None,
        }
    }

    /// Get current width delta (0 if not animating)
    pub fn get_width(&self) -> f32 {
        self.width.as_ref().map(|v| v.get()).unwrap_or(0.0)
    }

    /// Get current height delta (0 if not animating)
    pub fn get_height(&self) -> f32 {
        self.height.as_ref().map(|v| v.get()).unwrap_or(0.0)
    }

    /// Check if any size is animating
    pub fn is_animating(&self) -> bool {
        self.width
            .as_ref()
            .map(|v| v.is_animating())
            .unwrap_or(false)
            || self
                .height
                .as_ref()
                .map(|v| v.is_animating())
                .unwrap_or(false)
    }
}

// ============================================================================
// Visual Animation State
// ============================================================================

/// Visual animation state - purely tracks visual offsets, never touches layout
///
/// This is the FLIP technique:
/// - from_bounds: The visual bounds we're animating FROM (snapshot at animation start)
/// - to_bounds: The layout bounds we're animating TO (updated each frame from taffy)
/// - offset/size_delta: Animated values that start at (from - to) and animate to 0
pub struct VisualAnimation {
    /// Stable key for tracking across rebuilds
    pub key: String,

    /// The layout bounds we're animating FROM (snapshot at animation start)
    pub from_bounds: ElementBounds,

    /// The layout bounds we're animating TO (updated each frame from taffy)
    pub to_bounds: ElementBounds,

    /// Animated offset values (visual-only, don't affect layout)
    /// These represent the DELTA from current layout to visual position
    pub offset: AnimatedOffset,

    /// Animated size delta (visual-only)
    pub size_delta: AnimatedSizeDelta,

    /// Whether this is expanding or collapsing (affects clipping strategy)
    pub direction: AnimationDirection,

    /// Spring configuration
    pub spring: SpringConfig,
}

impl VisualAnimation {
    /// Create a new visual animation from a bounds change
    ///
    /// The FLIP calculation:
    /// - offset = from_bounds - to_bounds (inverted position)
    /// - Animated values start at offset, target 0 (play back to layout)
    pub fn from_bounds_change(
        key: String,
        from_bounds: ElementBounds,
        to_bounds: ElementBounds,
        config: &VisualAnimationConfig,
        scheduler: SchedulerHandle,
    ) -> Option<Self> {
        // Calculate deltas (FLIP "Invert" step)
        let dx = from_bounds.x - to_bounds.x;
        let dy = from_bounds.y - to_bounds.y;
        let dw = from_bounds.width - to_bounds.width;
        let dh = from_bounds.height - to_bounds.height;

        // Check if any property has significant change
        let has_position_change =
            config.animate.position && (dx.abs() > config.threshold || dy.abs() > config.threshold);
        let has_size_change =
            config.animate.size && (dw.abs() > config.threshold || dh.abs() > config.threshold);

        if !has_position_change && !has_size_change {
            return None;
        }

        // Determine animation direction based on size change
        let direction = if dh > config.threshold || dw > config.threshold {
            AnimationDirection::Collapsing
        } else if dh < -config.threshold || dw < -config.threshold {
            AnimationDirection::Expanding
        } else {
            AnimationDirection::Mixed
        };

        // Create animated offsets - start at delta, animate to 0
        let offset = AnimatedOffset {
            x: if config.animate.position && dx.abs() > config.threshold {
                let mut anim = AnimatedValue::new(scheduler.clone(), dx, config.spring);
                anim.set_target(0.0);
                Some(anim)
            } else {
                None
            },
            y: if config.animate.position && dy.abs() > config.threshold {
                let mut anim = AnimatedValue::new(scheduler.clone(), dy, config.spring);
                anim.set_target(0.0);
                Some(anim)
            } else {
                None
            },
        };

        let size_delta = AnimatedSizeDelta {
            width: if config.animate.size && dw.abs() > config.threshold {
                let mut anim = AnimatedValue::new(scheduler.clone(), dw, config.spring);
                anim.set_target(0.0);
                Some(anim)
            } else {
                None
            },
            height: if config.animate.size && dh.abs() > config.threshold {
                let mut anim = AnimatedValue::new(scheduler.clone(), dh, config.spring);
                anim.set_target(0.0);
                Some(anim)
            } else {
                None
            },
        };

        Some(Self {
            key,
            from_bounds,
            to_bounds,
            offset,
            size_delta,
            direction,
            spring: config.spring,
        })
    }

    /// Update target bounds when layout changes mid-animation
    ///
    /// When layout changes while animating, we need to update what we're animating TO
    /// but keep animating smoothly from current visual position.
    pub fn update_target(&mut self, new_to_bounds: ElementBounds, scheduler: SchedulerHandle) {
        // Get current visual bounds (layout + current offset)
        let current_visual = self.current_visual_bounds();

        // Calculate new deltas from current visual to new layout
        let dx = current_visual.x - new_to_bounds.x;
        let dy = current_visual.y - new_to_bounds.y;
        let dw = current_visual.width - new_to_bounds.width;
        let dh = current_visual.height - new_to_bounds.height;

        // Update direction based on new change
        if dh > 1.0 || dw > 1.0 {
            self.direction = AnimationDirection::Collapsing;
        } else if dh < -1.0 || dw < -1.0 {
            self.direction = AnimationDirection::Expanding;
        }

        // Update to_bounds
        self.to_bounds = new_to_bounds;

        // Update or create animated values with new initial values, still targeting 0
        if let Some(ref mut anim) = self.offset.x {
            // Set current value to the new offset, keep targeting 0
            anim.set_immediate(dx);
            anim.set_target(0.0);
        }
        if let Some(ref mut anim) = self.offset.y {
            anim.set_immediate(dy);
            anim.set_target(0.0);
        }
        if let Some(ref mut anim) = self.size_delta.width {
            anim.set_immediate(dw);
            anim.set_target(0.0);
        }
        if let Some(ref mut anim) = self.size_delta.height {
            anim.set_immediate(dh);
            anim.set_target(0.0);
        }
    }

    /// Get current visual bounds (layout bounds + animated offset)
    pub fn current_visual_bounds(&self) -> ElementBounds {
        ElementBounds {
            x: self.to_bounds.x + self.offset.get_x(),
            y: self.to_bounds.y + self.offset.get_y(),
            width: self.to_bounds.width + self.size_delta.get_width(),
            height: self.to_bounds.height + self.size_delta.get_height(),
        }
    }

    /// Check if any animation is still running
    pub fn is_animating(&self) -> bool {
        self.offset.is_animating() || self.size_delta.is_animating()
    }

    /// Check if this is a collapsing animation
    pub fn is_collapsing(&self) -> bool {
        matches!(self.direction, AnimationDirection::Collapsing)
    }

    /// Check if this is an expanding animation
    pub fn is_expanding(&self) -> bool {
        matches!(self.direction, AnimationDirection::Expanding)
    }
}

// ============================================================================
// Animated Render Bounds (Pre-computed per frame)
// ============================================================================

/// Pre-computed render bounds for an element, accounting for:
/// - Own animation state
/// - Parent's animation state (inherited via parent_offset)
/// - Clip rect for content clipping
#[derive(Clone, Debug)]
pub struct AnimatedRenderBounds {
    /// Position in parent-relative coordinates (including animation offset)
    pub x: f32,
    pub y: f32,

    /// Visual size (may differ from layout during animation)
    pub width: f32,
    pub height: f32,

    /// Clip rect for content (in local coordinates)
    /// None = no clipping, Some = clip to rect
    pub clip_rect: Option<Rect>,
}

impl AnimatedRenderBounds {
    /// Create identity bounds (no offset, no clipping)
    pub fn identity() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            clip_rect: None,
        }
    }

    /// Create from layout bounds with no animation
    pub fn from_layout(bounds: ElementBounds) -> Self {
        Self {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
            clip_rect: None,
        }
    }
}

// ============================================================================
// Configuration
// ============================================================================

/// Which properties to animate
#[derive(Clone, Debug, Default)]
pub struct AnimateProperties {
    /// Animate x, y position
    pub position: bool,
    /// Animate width, height
    pub size: bool,
}

/// Clipping behavior during animation
#[derive(Clone, Debug, Default)]
pub enum ClipBehavior {
    /// Clip content to animated bounds (default for collapse)
    #[default]
    ClipToAnimated,
    /// Clip content to layout bounds
    ClipToLayout,
    /// No additional clipping
    NoClip,
}

/// Configuration for visual animations on layout changes
#[derive(Clone, Debug)]
pub struct VisualAnimationConfig {
    /// Stable key for tracking across rebuilds
    pub key: Option<String>,

    /// Which properties to animate
    pub animate: AnimateProperties,

    /// Spring configuration
    pub spring: SpringConfig,

    /// Minimum change threshold (ignore tiny changes)
    pub threshold: f32,

    /// Clipping behavior during animation
    pub clip_behavior: ClipBehavior,
}

impl Default for VisualAnimationConfig {
    fn default() -> Self {
        Self::height()
    }
}

impl VisualAnimationConfig {
    /// Animate only height changes (most common for accordions)
    pub fn height() -> Self {
        Self {
            key: None,
            animate: AnimateProperties {
                position: false,
                size: true,
            },
            spring: SpringConfig::snappy(),
            threshold: 1.0,
            clip_behavior: ClipBehavior::ClipToAnimated,
        }
    }

    /// Animate only width changes (sidebars)
    pub fn width() -> Self {
        Self {
            key: None,
            animate: AnimateProperties {
                position: false,
                size: true,
            },
            spring: SpringConfig::snappy(),
            threshold: 1.0,
            clip_behavior: ClipBehavior::ClipToAnimated,
        }
    }

    /// Animate both width and height
    pub fn size() -> Self {
        Self {
            key: None,
            animate: AnimateProperties {
                position: false,
                size: true,
            },
            spring: SpringConfig::snappy(),
            threshold: 1.0,
            clip_behavior: ClipBehavior::ClipToAnimated,
        }
    }

    /// Animate position only (for reordering animations)
    pub fn position() -> Self {
        Self {
            key: None,
            animate: AnimateProperties {
                position: true,
                size: false,
            },
            spring: SpringConfig::snappy(),
            threshold: 1.0,
            clip_behavior: ClipBehavior::NoClip,
        }
    }

    /// Animate all bounds properties
    pub fn all() -> Self {
        Self {
            key: None,
            animate: AnimateProperties {
                position: true,
                size: true,
            },
            spring: SpringConfig::snappy(),
            threshold: 1.0,
            clip_behavior: ClipBehavior::ClipToAnimated,
        }
    }

    /// Set stable key for Stateful components
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Use spring configuration
    pub fn with_spring(mut self, config: SpringConfig) -> Self {
        self.spring = config;
        self
    }

    /// Set minimum change threshold
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    /// Use a gentle spring (slow, smooth)
    pub fn gentle(self) -> Self {
        self.with_spring(SpringConfig::gentle())
    }

    /// Use a wobbly spring (with overshoot)
    pub fn wobbly(self) -> Self {
        self.with_spring(SpringConfig::wobbly())
    }

    /// Use a stiff spring (quick, snappy)
    pub fn stiff(self) -> Self {
        self.with_spring(SpringConfig::stiff())
    }

    /// Use a snappy spring (very responsive)
    pub fn snappy(self) -> Self {
        self.with_spring(SpringConfig::snappy())
    }

    /// Clip to animated bounds during animation
    pub fn clip_to_animated(mut self) -> Self {
        self.clip_behavior = ClipBehavior::ClipToAnimated;
        self
    }

    /// Clip to layout bounds during animation
    pub fn clip_to_layout(mut self) -> Self {
        self.clip_behavior = ClipBehavior::ClipToLayout;
        self
    }

    /// No additional clipping during animation
    pub fn no_clip(mut self) -> Self {
        self.clip_behavior = ClipBehavior::NoClip;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builders() {
        let config = VisualAnimationConfig::height();
        assert!(!config.animate.position);
        assert!(config.animate.size);

        let config = VisualAnimationConfig::all();
        assert!(config.animate.position);
        assert!(config.animate.size);

        let config = VisualAnimationConfig::position();
        assert!(config.animate.position);
        assert!(!config.animate.size);
    }

    #[test]
    fn test_animation_direction() {
        assert_eq!(
            AnimationDirection::Collapsing,
            AnimationDirection::Collapsing
        );
        assert_ne!(
            AnimationDirection::Expanding,
            AnimationDirection::Collapsing
        );
    }
}
