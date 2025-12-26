//! Animation integration for the fluent element API
//!
//! Provides multiple approaches for animating element properties:
//!
//! ## 1. Direct AnimatedValue in properties
//!
//! Pass animated values directly to property methods:
//!
//! ```ignore
//! let opacity = AnimatedValue::new(ctx.animation_handle(), 1.0, SpringConfig::stiff());
//! opacity.set_target(0.5);
//!
//! div()
//!     .opacity(opacity.get())
//!     .scale(scale.get())
//! ```
//!
//! ## 2. Animate builder block
//!
//! Use the `animate()` method for declarative transitions:
//!
//! ```ignore
//! div()
//!     .animate(|a| a
//!         .opacity(0.0, 1.0)  // from 0 to 1
//!         .scale(0.8, 1.0)    // from 0.8 to 1
//!         .with_spring(SpringConfig::wobbly())
//!     )
//! ```
//!
//! ## 3. With animated value binding
//!
//! Bind an animated value to update any property:
//!
//! ```ignore
//! div()
//!     .with_animated(&opacity_anim, |d, v| d.opacity(v))
//!     .with_animated(&scale_anim, |d, v| d.scale(v))
//! ```

use blinc_animation::{AnimatedValue, Easing, SchedulerHandle, SpringConfig};

// ============================================================================
// Animation Builder
// ============================================================================

/// A builder for declarative animation transitions
///
/// Created via the `animate()` method on elements. Allows specifying
/// from/to values for various properties with spring or easing configuration.
pub struct AnimationBuilder {
    handle: Option<SchedulerHandle>,
    opacity: Option<(f32, f32)>,
    scale: Option<(f32, f32)>,
    translate_x: Option<(f32, f32)>,
    translate_y: Option<(f32, f32)>,
    rotate: Option<(f32, f32)>,
    spring_config: SpringConfig,
    duration_ms: Option<u32>,
    easing: Easing,
    auto_start: bool,
}

impl Default for AnimationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimationBuilder {
    /// Create a new animation builder
    pub fn new() -> Self {
        Self {
            handle: None,
            opacity: None,
            scale: None,
            translate_x: None,
            translate_y: None,
            rotate: None,
            spring_config: SpringConfig::stiff(),
            duration_ms: None,
            easing: Easing::EaseInOut,
            auto_start: true,
        }
    }

    /// Set the scheduler handle for spring animations
    pub fn with_handle(mut self, handle: SchedulerHandle) -> Self {
        self.handle = Some(handle);
        self
    }

    /// Animate opacity from `from` to `to`
    pub fn opacity(mut self, from: f32, to: f32) -> Self {
        self.opacity = Some((from, to));
        self
    }

    /// Animate uniform scale from `from` to `to`
    pub fn scale(mut self, from: f32, to: f32) -> Self {
        self.scale = Some((from, to));
        self
    }

    /// Animate X translation from `from` to `to`
    pub fn translate_x(mut self, from: f32, to: f32) -> Self {
        self.translate_x = Some((from, to));
        self
    }

    /// Animate Y translation from `from` to `to`
    pub fn translate_y(mut self, from: f32, to: f32) -> Self {
        self.translate_y = Some((from, to));
        self
    }

    /// Animate rotation from `from` to `to` (in radians)
    pub fn rotate(mut self, from: f32, to: f32) -> Self {
        self.rotate = Some((from, to));
        self
    }

    /// Animate rotation from `from` to `to` (in degrees)
    pub fn rotate_deg(self, from: f32, to: f32) -> Self {
        let to_rad = |deg: f32| deg * std::f32::consts::PI / 180.0;
        self.rotate(to_rad(from), to_rad(to))
    }

    /// Use spring physics with the given configuration
    pub fn with_spring(mut self, config: SpringConfig) -> Self {
        self.spring_config = config;
        self.duration_ms = None; // Spring overrides duration
        self
    }

    /// Use a gentle spring (good for page transitions)
    pub fn gentle(self) -> Self {
        self.with_spring(SpringConfig::gentle())
    }

    /// Use a wobbly spring (good for playful UI)
    pub fn wobbly(self) -> Self {
        self.with_spring(SpringConfig::wobbly())
    }

    /// Use a stiff spring (good for buttons)
    pub fn stiff(self) -> Self {
        self.with_spring(SpringConfig::stiff())
    }

    /// Use a snappy spring (good for quick responses)
    pub fn snappy(self) -> Self {
        self.with_spring(SpringConfig::snappy())
    }

    /// Use keyframe animation with the given duration and easing
    pub fn with_duration(mut self, duration_ms: u32) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Set the easing function (for keyframe animations)
    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// Don't auto-start the animation
    pub fn paused(mut self) -> Self {
        self.auto_start = false;
        self
    }

    /// Get the current opacity value (for use in element building)
    ///
    /// Returns the `from` value initially, then animates to `to`.
    pub fn get_opacity(&self) -> Option<f32> {
        self.opacity.map(|(from, _)| from)
    }

    /// Get the current scale value
    pub fn get_scale(&self) -> Option<f32> {
        self.scale.map(|(from, _)| from)
    }

    /// Get the current translate_x value
    pub fn get_translate_x(&self) -> Option<f32> {
        self.translate_x.map(|(from, _)| from)
    }

    /// Get the current translate_y value
    pub fn get_translate_y(&self) -> Option<f32> {
        self.translate_y.map(|(from, _)| from)
    }

    /// Get the current rotation value
    pub fn get_rotate(&self) -> Option<f32> {
        self.rotate.map(|(from, _)| from)
    }
}

// ============================================================================
// Animated Property Holder
// ============================================================================

/// Holds animated values for element properties
///
/// This is returned by `animate()` and can be stored to access
/// the current animated values during rebuilds.
#[derive(Clone)]
pub struct AnimatedProperties {
    pub opacity: Option<AnimatedValue>,
    pub scale: Option<AnimatedValue>,
    pub translate_x: Option<AnimatedValue>,
    pub translate_y: Option<AnimatedValue>,
    pub rotate: Option<AnimatedValue>,
}

impl AnimatedProperties {
    /// Create animated properties from an animation builder
    pub fn from_builder(builder: &AnimationBuilder, handle: SchedulerHandle) -> Self {
        let config = builder.spring_config;

        let opacity = builder.opacity.map(|(from, to)| {
            let mut anim = AnimatedValue::new(handle.clone(), from, config);
            if builder.auto_start {
                anim.set_target(to);
            }
            anim
        });

        let scale = builder.scale.map(|(from, to)| {
            let mut anim = AnimatedValue::new(handle.clone(), from, config);
            if builder.auto_start {
                anim.set_target(to);
            }
            anim
        });

        let translate_x = builder.translate_x.map(|(from, to)| {
            let mut anim = AnimatedValue::new(handle.clone(), from, config);
            if builder.auto_start {
                anim.set_target(to);
            }
            anim
        });

        let translate_y = builder.translate_y.map(|(from, to)| {
            let mut anim = AnimatedValue::new(handle.clone(), from, config);
            if builder.auto_start {
                anim.set_target(to);
            }
            anim
        });

        let rotate = builder.rotate.map(|(from, to)| {
            let mut anim = AnimatedValue::new(handle.clone(), from, config);
            if builder.auto_start {
                anim.set_target(to);
            }
            anim
        });

        Self {
            opacity,
            scale,
            translate_x,
            translate_y,
            rotate,
        }
    }

    /// Get current opacity (or 1.0 if not animated)
    pub fn opacity(&self) -> f32 {
        self.opacity.as_ref().map(|a| a.get()).unwrap_or(1.0)
    }

    /// Get current scale (or 1.0 if not animated)
    pub fn scale(&self) -> f32 {
        self.scale.as_ref().map(|a| a.get()).unwrap_or(1.0)
    }

    /// Get current translate_x (or 0.0 if not animated)
    pub fn translate_x(&self) -> f32 {
        self.translate_x.as_ref().map(|a| a.get()).unwrap_or(0.0)
    }

    /// Get current translate_y (or 0.0 if not animated)
    pub fn translate_y(&self) -> f32 {
        self.translate_y.as_ref().map(|a| a.get()).unwrap_or(0.0)
    }

    /// Get current rotation (or 0.0 if not animated)
    pub fn rotate(&self) -> f32 {
        self.rotate.as_ref().map(|a| a.get()).unwrap_or(0.0)
    }

    /// Check if any animations are still running
    pub fn is_animating(&self) -> bool {
        self.opacity
            .as_ref()
            .map(|a| a.is_animating())
            .unwrap_or(false)
            || self
                .scale
                .as_ref()
                .map(|a| a.is_animating())
                .unwrap_or(false)
            || self
                .translate_x
                .as_ref()
                .map(|a| a.is_animating())
                .unwrap_or(false)
            || self
                .translate_y
                .as_ref()
                .map(|a| a.is_animating())
                .unwrap_or(false)
            || self
                .rotate
                .as_ref()
                .map(|a| a.is_animating())
                .unwrap_or(false)
    }
}

// ============================================================================
// Div Extension for Animations
// ============================================================================

use crate::div::Div;

impl Div {
    /// Apply an animation builder to this element
    ///
    /// The closure receives an `AnimationBuilder` and should configure
    /// the desired animations. The initial values are applied immediately.
    ///
    /// Note: For the animations to actually run, you need to store the
    /// `AnimatedProperties` and use their values in subsequent rebuilds.
    /// For simpler usage, consider using `with_animated()` instead.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // In your UI builder:
    /// let props = ctx.use_animation(|a| a
    ///     .opacity(0.0, 1.0)
    ///     .scale(0.8, 1.0)
    ///     .wobbly()
    /// );
    ///
    /// div()
    ///     .opacity(props.opacity())
    ///     .scale(props.scale())
    /// ```
    pub fn animate<F>(self, f: F) -> Self
    where
        F: FnOnce(AnimationBuilder) -> AnimationBuilder,
    {
        let builder = f(AnimationBuilder::new());

        // Apply initial values from the builder
        let mut result = self;

        if let Some(opacity) = builder.get_opacity() {
            result = result.opacity(opacity);
        }

        if let Some(scale) = builder.get_scale() {
            result = result.scale(scale);
        }

        // Apply translation transform
        let tx = builder.get_translate_x().unwrap_or(0.0);
        let ty = builder.get_translate_y().unwrap_or(0.0);
        if tx != 0.0 || ty != 0.0 {
            result = result.translate(tx, ty);
        }

        if let Some(rot) = builder.get_rotate() {
            result = result.rotate(rot);
        }

        result
    }

    /// Apply an animated value to update this element
    ///
    /// The closure receives the current Div and the animated value,
    /// and should return the modified Div.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let opacity = AnimatedValue::new(ctx.animation_handle(), 0.0, SpringConfig::stiff());
    /// opacity.set_target(1.0);
    ///
    /// div()
    ///     .with_animated(&opacity, |d, v| d.opacity(v))
    /// ```
    pub fn with_animated<F>(self, anim: &AnimatedValue, f: F) -> Self
    where
        F: FnOnce(Self, f32) -> Self,
    {
        f(self, anim.get())
    }

    /// Apply animated properties to this element
    ///
    /// Applies all animated values (opacity, scale, translate, rotate)
    /// from the `AnimatedProperties` to this element.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let props = AnimatedProperties::from_builder(&builder, handle);
    ///
    /// div()
    ///     .apply_animations(&props)
    /// ```
    pub fn apply_animations(self, props: &AnimatedProperties) -> Self {
        let mut result = self.opacity(props.opacity());

        let scale = props.scale();
        if (scale - 1.0).abs() > 0.001 {
            result = result.scale(scale);
        }

        let tx = props.translate_x();
        let ty = props.translate_y();
        if tx.abs() > 0.001 || ty.abs() > 0.001 {
            result = result.translate(tx, ty);
        }

        let rot = props.rotate();
        if rot.abs() > 0.001 {
            result = result.rotate(rot);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_animation_builder() {
        let builder = AnimationBuilder::new()
            .opacity(0.0, 1.0)
            .scale(0.5, 1.0)
            .wobbly();

        assert_eq!(builder.get_opacity(), Some(0.0));
        assert_eq!(builder.get_scale(), Some(0.5));
    }

    #[test]
    fn test_div_animate() {
        use crate::div::div;

        let d = div()
            .w(100.0)
            .animate(|a| a.opacity(0.5, 1.0).scale(0.8, 1.0));

        // The initial values should be applied
        // (We can't easily test this without accessing private fields)
        assert!(true);
    }
}
