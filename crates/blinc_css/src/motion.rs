//! Enter and exit motion for elements.
//!
//! A `MotionAnimation` holds the keyframe an element animates *from* on
//! enter and *to* on exit. Both endpoints are optional, and every field
//! inside a keyframe is too: a missing value means the property is left
//! alone rather than driven to a default.

/// Motion animation configuration for enter/exit animations
#[derive(Clone, Debug, Default)]
pub struct MotionAnimation {
    /// Enter animation properties (opacity, scale, translate at t=0)
    pub enter_from: Option<MotionKeyframe>,
    /// Enter animation duration in ms
    pub enter_duration_ms: u32,
    /// Enter animation delay in ms (for stagger)
    pub enter_delay_ms: u32,
    /// Exit animation properties (opacity, scale, translate at t=1)
    pub exit_to: Option<MotionKeyframe>,
    /// Exit animation duration in ms
    pub exit_duration_ms: u32,
}

/// A single keyframe of motion animation values
#[derive(Clone, Debug, Default)]
pub struct MotionKeyframe {
    /// Opacity (0.0 - 1.0)
    pub opacity: Option<f32>,
    /// Scale X
    pub scale_x: Option<f32>,
    /// Scale Y
    pub scale_y: Option<f32>,
    /// Translate X in pixels
    pub translate_x: Option<f32>,
    /// Translate Y in pixels
    pub translate_y: Option<f32>,
    /// Rotation in degrees
    pub rotate: Option<f32>,
}

impl MotionKeyframe {
    /// Create a new empty keyframe
    pub fn new() -> Self {
        Self::default()
    }

    /// Set opacity
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// Set uniform scale
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale_x = Some(scale);
        self.scale_y = Some(scale);
        self
    }

    /// Set translation
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.translate_x = Some(x);
        self.translate_y = Some(y);
        self
    }

    /// Set rotation in degrees
    pub fn rotate(mut self, degrees: f32) -> Self {
        self.rotate = Some(degrees);
        self
    }

    /// Create from blinc_animation KeyframeProperties
    pub fn from_keyframe_properties(props: &blinc_animation::KeyframeProperties) -> Self {
        Self {
            opacity: props.opacity,
            scale_x: props.scale_x,
            scale_y: props.scale_y,
            translate_x: props.translate_x,
            translate_y: props.translate_y,
            rotate: props.rotate,
        }
    }

    /// Interpolate between two keyframes
    ///
    /// When a property is Some in one keyframe but None in the other,
    /// we use the "identity" value (1.0 for opacity/scale, 0.0 for translate/rotate).
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            // Opacity: identity is 1.0
            opacity: lerp_opt_with_default(self.opacity, other.opacity, t, 1.0),
            // Scale: identity is 1.0
            scale_x: lerp_opt_with_default(self.scale_x, other.scale_x, t, 1.0),
            scale_y: lerp_opt_with_default(self.scale_y, other.scale_y, t, 1.0),
            // Translation: identity is 0.0
            translate_x: lerp_opt_with_default(self.translate_x, other.translate_x, t, 0.0),
            translate_y: lerp_opt_with_default(self.translate_y, other.translate_y, t, 0.0),
            // Rotation: identity is 0.0
            rotate: lerp_opt_with_default(self.rotate, other.rotate, t, 0.0),
        }
    }

    /// Get the resolved opacity (defaults to 1.0 if not set)
    pub fn resolved_opacity(&self) -> f32 {
        self.opacity.unwrap_or(1.0)
    }

    /// Get the resolved scale (defaults to 1.0 if not set)
    pub fn resolved_scale(&self) -> (f32, f32) {
        (self.scale_x.unwrap_or(1.0), self.scale_y.unwrap_or(1.0))
    }

    /// Get the resolved translation (defaults to 0.0 if not set)
    pub fn resolved_translate(&self) -> (f32, f32) {
        (
            self.translate_x.unwrap_or(0.0),
            self.translate_y.unwrap_or(0.0),
        )
    }

    /// Get the resolved rotation (defaults to 0.0 if not set)
    pub fn resolved_rotate(&self) -> f32 {
        self.rotate.unwrap_or(0.0)
    }
}

/// Helper to interpolate optional values with a default for missing values
fn lerp_opt_with_default(a: Option<f32>, b: Option<f32>, t: f32, default: f32) -> Option<f32> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a + (b - a) * t),
        (Some(a), None) => Some(a + (default - a) * t), // Lerp toward default
        (None, Some(b)) => Some(default + (b - default) * t), // Lerp from default
        (None, None) => None,
    }
}

impl MotionAnimation {
    /// Create a motion animation from a MultiKeyframeAnimation (enter animation)
    pub fn from_enter_animation(
        anim: &blinc_animation::MultiKeyframeAnimation,
        delay_ms: u32,
    ) -> Self {
        // Get the first keyframe's properties as the "from" state
        let enter_from = anim
            .first_keyframe()
            .map(|kf| MotionKeyframe::from_keyframe_properties(&kf.properties));

        Self {
            enter_from,
            enter_duration_ms: anim.duration_ms(),
            enter_delay_ms: delay_ms,
            exit_to: None,
            exit_duration_ms: 0,
        }
    }

    /// Add exit animation
    pub fn with_exit_animation(mut self, anim: &blinc_animation::MultiKeyframeAnimation) -> Self {
        // Get the last keyframe's properties as the "to" state
        self.exit_to = anim
            .last_keyframe()
            .map(|kf| MotionKeyframe::from_keyframe_properties(&kf.properties));
        self.exit_duration_ms = anim.duration_ms();
        self
    }
}
