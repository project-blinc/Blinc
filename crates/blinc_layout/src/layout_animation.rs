//! Layout animation system for animating bounds changes
//!
//! Provides FLIP-style animations for layout changes (position, size).
//! Unlike motion containers which use visual transforms, this system
//! animates the **rendered bounds** while letting layout settle instantly.
//!
//! # How It Works
//!
//! 1. Layout computes final positions instantly (taffy runs)
//! 2. System detects bounds changes from previous frame
//! 3. Animated values interpolate from old bounds to new bounds
//! 4. Renderer uses animated bounds during transition
//! 5. Content clips to animated bounds during animation
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! // Animate height changes (good for accordions)
//! div()
//!     .animate_layout(LayoutAnimation::height())
//!     .overflow_clip()
//!     .child(expandable_content)
//!
//! // Animate all bounds with custom spring
//! div()
//!     .animate_layout(
//!         LayoutAnimation::all()
//!             .with_spring(SpringConfig::wobbly())
//!     )
//!     .child(content)
//! ```

use blinc_animation::{AnimatedValue, SchedulerHandle, SpringConfig};

use crate::element::ElementBounds;

// ============================================================================
// Configuration (DEPRECATED)
// ============================================================================

/// Configuration for which layout properties to animate
///
/// # Deprecated
///
/// Use [`VisualAnimationConfig`](crate::visual_animation::VisualAnimationConfig) instead.
/// The new system uses FLIP-style animations that never modify the layout tree.
#[deprecated(
    since = "0.3.0",
    note = "Use VisualAnimationConfig from visual_animation module. The old system modifies taffy which causes issues."
)]
#[derive(Clone, Debug)]
pub struct LayoutAnimationConfig {
    /// Stable key for tracking this animation across rebuilds
    ///
    /// When set, the animation state is tracked by this key instead of node ID.
    /// This allows animations to persist when Stateful components rebuild,
    /// creating new nodes but representing the same logical element.
    pub stable_key: Option<String>,
    /// Animate height changes
    pub height: bool,
    /// Animate width changes
    pub width: bool,
    /// Animate x position changes
    pub x: bool,
    /// Animate y position changes
    pub y: bool,
    /// Spring configuration for the animation
    pub spring: SpringConfig,
    /// Minimum change threshold in pixels (ignore tiny changes)
    pub threshold: f32,
}

impl Default for LayoutAnimationConfig {
    fn default() -> Self {
        Self::height()
    }
}

impl LayoutAnimationConfig {
    /// Animate only height changes (most common for accordions/collapsibles)
    pub fn height() -> Self {
        Self {
            stable_key: None,
            height: true,
            width: false,
            x: false,
            y: false,
            spring: SpringConfig::snappy(),
            threshold: 1.0,
        }
    }

    /// Animate only width changes
    pub fn width() -> Self {
        Self {
            stable_key: None,
            height: false,
            width: true,
            x: false,
            y: false,
            spring: SpringConfig::snappy(),
            threshold: 1.0,
        }
    }

    /// Animate both width and height changes
    pub fn size() -> Self {
        Self {
            stable_key: None,
            height: true,
            width: true,
            x: false,
            y: false,
            spring: SpringConfig::snappy(),
            threshold: 1.0,
        }
    }

    /// Animate only position changes (x, y)
    pub fn position() -> Self {
        Self {
            stable_key: None,
            height: false,
            width: false,
            x: true,
            y: true,
            spring: SpringConfig::snappy(),
            threshold: 1.0,
        }
    }

    /// Animate all bounds (position and size)
    pub fn all() -> Self {
        Self {
            stable_key: None,
            height: true,
            width: true,
            x: true,
            y: true,
            spring: SpringConfig::snappy(),
            threshold: 1.0,
        }
    }

    /// Set a stable key for tracking this animation across rebuilds
    ///
    /// This is essential when using layout animation inside Stateful components,
    /// since node IDs change on rebuild. The stable key allows the animation
    /// system to recognize "this is the same logical element" and animate
    /// from the previous bounds to the new bounds.
    ///
    /// # Example
    ///
    /// ```ignore
    /// div()
    ///     .animate_layout(
    ///         LayoutAnimationConfig::height()
    ///             .with_key("accordion-item-1")
    ///             .snappy()
    ///     )
    /// ```
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.stable_key = Some(key.into());
        self
    }

    /// Set the spring configuration
    pub fn with_spring(mut self, spring: SpringConfig) -> Self {
        self.spring = spring;
        self
    }

    /// Set the minimum change threshold
    ///
    /// Changes smaller than this value (in pixels) will not trigger animation.
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
}

// ============================================================================
// Animation State
// ============================================================================

/// Active animation state for a layout-animating element
pub struct LayoutAnimationState {
    /// Start bounds (where animation began - the "old" layout)
    pub start_bounds: ElementBounds,
    /// Target bounds (from taffy layout - the "new" layout)
    pub end_bounds: ElementBounds,
    /// Animated height value (if animating height)
    pub height_anim: Option<AnimatedValue>,
    /// Animated width value (if animating width)
    pub width_anim: Option<AnimatedValue>,
    /// Animated x position value (if animating x)
    pub x_anim: Option<AnimatedValue>,
    /// Animated y position value (if animating y)
    pub y_anim: Option<AnimatedValue>,
}

impl LayoutAnimationState {
    /// Create a new animation state from bounds change
    ///
    /// Returns `None` if no properties need animation (no significant change).
    pub fn from_bounds_change(
        old_bounds: ElementBounds,
        new_bounds: ElementBounds,
        config: &LayoutAnimationConfig,
        scheduler: SchedulerHandle,
    ) -> Option<Self> {
        let height_changed =
            config.height && (new_bounds.height - old_bounds.height).abs() > config.threshold;
        let width_changed =
            config.width && (new_bounds.width - old_bounds.width).abs() > config.threshold;
        let x_changed = config.x && (new_bounds.x - old_bounds.x).abs() > config.threshold;
        let y_changed = config.y && (new_bounds.y - old_bounds.y).abs() > config.threshold;

        // No significant changes
        if !height_changed && !width_changed && !x_changed && !y_changed {
            return None;
        }

        let height_anim = if height_changed {
            let mut anim = AnimatedValue::new(scheduler.clone(), old_bounds.height, config.spring);
            anim.set_target(new_bounds.height);
            Some(anim)
        } else {
            None
        };

        let width_anim = if width_changed {
            let mut anim = AnimatedValue::new(scheduler.clone(), old_bounds.width, config.spring);
            anim.set_target(new_bounds.width);
            Some(anim)
        } else {
            None
        };

        let x_anim = if x_changed {
            let mut anim = AnimatedValue::new(scheduler.clone(), old_bounds.x, config.spring);
            anim.set_target(new_bounds.x);
            Some(anim)
        } else {
            None
        };

        let y_anim = if y_changed {
            let mut anim = AnimatedValue::new(scheduler.clone(), old_bounds.y, config.spring);
            anim.set_target(new_bounds.y);
            Some(anim)
        } else {
            None
        };

        Some(Self {
            start_bounds: old_bounds,
            end_bounds: new_bounds,
            height_anim,
            width_anim,
            x_anim,
            y_anim,
        })
    }

    /// Update animation targets when bounds change again mid-animation
    pub fn update_target(&mut self, new_bounds: ElementBounds, config: &LayoutAnimationConfig) {
        self.end_bounds = new_bounds;

        if let Some(ref mut anim) = self.height_anim {
            if config.height {
                anim.set_target(new_bounds.height);
            }
        }

        if let Some(ref mut anim) = self.width_anim {
            if config.width {
                anim.set_target(new_bounds.width);
            }
        }

        if let Some(ref mut anim) = self.x_anim {
            if config.x {
                anim.set_target(new_bounds.x);
            }
        }

        if let Some(ref mut anim) = self.y_anim {
            if config.y {
                anim.set_target(new_bounds.y);
            }
        }
    }

    /// Get current interpolated bounds for rendering
    pub fn current_bounds(&self) -> ElementBounds {
        ElementBounds {
            x: self
                .x_anim
                .as_ref()
                .map(|a| a.get())
                .unwrap_or(self.end_bounds.x),
            y: self
                .y_anim
                .as_ref()
                .map(|a| a.get())
                .unwrap_or(self.end_bounds.y),
            width: self
                .width_anim
                .as_ref()
                .map(|a| a.get())
                .unwrap_or(self.end_bounds.width),
            height: self
                .height_anim
                .as_ref()
                .map(|a| a.get())
                .unwrap_or(self.end_bounds.height),
        }
    }

    /// Check if any animations are still running
    pub fn is_animating(&self) -> bool {
        self.height_anim
            .as_ref()
            .map(|a| a.is_animating())
            .unwrap_or(false)
            || self
                .width_anim
                .as_ref()
                .map(|a| a.is_animating())
                .unwrap_or(false)
            || self
                .x_anim
                .as_ref()
                .map(|a| a.is_animating())
                .unwrap_or(false)
            || self
                .y_anim
                .as_ref()
                .map(|a| a.is_animating())
                .unwrap_or(false)
    }

    /// Get the current animated height (or final if not animating)
    pub fn current_height(&self) -> f32 {
        self.height_anim
            .as_ref()
            .map(|a| a.get())
            .unwrap_or(self.end_bounds.height)
    }

    /// Get the current animated width (or final if not animating)
    pub fn current_width(&self) -> f32 {
        self.width_anim
            .as_ref()
            .map(|a| a.get())
            .unwrap_or(self.end_bounds.width)
    }

    /// Check if width is collapsing (animating from larger to smaller)
    ///
    /// During collapse, current animated width > target width.
    /// This is important because children should be laid out at the larger
    /// (animated) size during collapse, then clipped.
    pub fn is_width_collapsing(&self) -> bool {
        self.width_anim
            .as_ref()
            .map(|a| a.get() > self.end_bounds.width)
            .unwrap_or(false)
    }

    /// Check if height is collapsing (animating from larger to smaller)
    pub fn is_height_collapsing(&self) -> bool {
        self.height_anim
            .as_ref()
            .map(|a| a.get() > self.end_bounds.height)
            .unwrap_or(false)
    }

    /// Check if any dimension is collapsing
    pub fn is_collapsing(&self) -> bool {
        self.is_width_collapsing() || self.is_height_collapsing()
    }

    /// Get bounds that should be used for laying out children during animation
    ///
    /// During collapse: returns current animated bounds (larger than target)
    /// During expand: returns target bounds (children at final size, revealed by clip)
    ///
    /// This ensures children are laid out at the larger size during collapse,
    /// so there's content to clip as the animation progresses.
    pub fn layout_constraint_bounds(&self) -> ElementBounds {
        let current = self.current_bounds();

        ElementBounds {
            x: current.x,
            y: current.y,
            // Use the larger of current animated or target for layout
            width: current.width.max(self.end_bounds.width),
            height: current.height.max(self.end_bounds.height),
        }
    }

    /// Snap all springs to their target values, immediately completing the animation
    ///
    /// This is useful for immediately finishing an animation without waiting for
    /// the springs to settle naturally.
    pub fn snap_to_target(&mut self) {
        if let Some(ref mut anim) = self.height_anim {
            anim.snap_to_target();
        }
        if let Some(ref mut anim) = self.width_anim {
            anim.snap_to_target();
        }
        if let Some(ref mut anim) = self.x_anim {
            anim.snap_to_target();
        }
        if let Some(ref mut anim) = self.y_anim {
            anim.snap_to_target();
        }
    }
}

// ============================================================================
// Convenience type alias
// ============================================================================

/// Type alias for layout animation configuration
pub type LayoutAnimation = LayoutAnimationConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builders() {
        let config = LayoutAnimation::height();
        assert!(config.height);
        assert!(!config.width);
        assert!(!config.x);
        assert!(!config.y);

        let config = LayoutAnimation::all();
        assert!(config.height);
        assert!(config.width);
        assert!(config.x);
        assert!(config.y);

        let config = LayoutAnimation::size().with_threshold(2.0);
        assert!(config.height);
        assert!(config.width);
        assert!(!config.x);
        assert!(!config.y);
        assert_eq!(config.threshold, 2.0);
    }

    #[test]
    fn test_spring_presets() {
        let gentle = LayoutAnimation::height().gentle();
        let wobbly = LayoutAnimation::height().wobbly();
        let stiff = LayoutAnimation::height().stiff();
        let snappy = LayoutAnimation::height().snappy();

        // Just verify they don't panic and produce different configs
        assert!(gentle.spring.stiffness < stiff.spring.stiffness);
        assert!(wobbly.spring.damping < stiff.spring.damping);
        assert!(snappy.spring.stiffness > gentle.spring.stiffness);
    }
}
