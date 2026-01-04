//! Motion container for animations
//!
//! A container that applies animations to its children. Supports:
//! - Enter/exit animations (fade_in, scale_in, slide_in, etc.)
//! - Staggered animations for lists
//! - **Continuous animations** driven by `AnimatedValue` or `AnimatedTimeline`
//!
//! # Example - Enter/Exit
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! motion()
//!     .fade_in(300)
//!     .fade_out(200)
//!     .child(my_content)
//! ```
//!
//! # Example - Continuous Animation with AnimatedValue
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//! use blinc_animation::{AnimatedValue, SpringConfig};
//!
//! // Create animated value for Y translation
//! let offset_y = Rc::new(RefCell::new(
//!     AnimatedValue::new(ctx.animation_handle(), 0.0, SpringConfig::wobbly())
//! ));
//!
//! motion()
//!     .translate_y(offset_y.clone())  // Bind to AnimatedValue
//!     .child(my_content)
//!
//! // Later, in drag handler:
//! offset_y.borrow_mut().set_target(100.0);  // Animates smoothly
//! ```

use crate::div::{ElementBuilder, ElementTypeId};
use crate::element::{MotionAnimation, MotionKeyframe, RenderProps};
use crate::key::InstanceKey;
use crate::tree::{LayoutNodeId, LayoutTree};
use blinc_animation::{AnimatedValue, AnimationPreset, MultiKeyframeAnimation};
use blinc_core::Transform;
use taffy::{Display, FlexDirection, Style};

/// Animation configuration for element lifecycle
#[derive(Clone)]
pub struct ElementAnimation {
    /// The animation to play
    pub animation: MultiKeyframeAnimation,
}

impl ElementAnimation {
    /// Create a new element animation
    pub fn new(animation: MultiKeyframeAnimation) -> Self {
        Self { animation }
    }

    /// Set delay before animation starts
    pub fn with_delay(mut self, delay_ms: u32) -> Self {
        self.animation = self.animation.delay(delay_ms);
        self
    }
}

impl From<MultiKeyframeAnimation> for ElementAnimation {
    fn from(animation: MultiKeyframeAnimation) -> Self {
        Self::new(animation)
    }
}

/// Direction for stagger animations
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StaggerDirection {
    /// Animate first to last
    #[default]
    Forward,
    /// Animate last to first
    Reverse,
    /// Animate from center outward
    FromCenter,
}

/// Direction for slide animations
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideDirection {
    Left,
    Right,
    Top,
    Bottom,
}

/// Configuration for stagger animations
#[derive(Clone)]
pub struct StaggerConfig {
    /// Delay between each child's animation start (ms)
    pub delay_ms: u32,
    /// Animation to apply to each child
    pub animation: ElementAnimation,
    /// Direction of stagger
    pub direction: StaggerDirection,
    /// Optional: limit stagger to first N items
    pub limit: Option<usize>,
}

impl StaggerConfig {
    /// Create a new stagger config with delay between items
    pub fn new(delay_ms: u32, animation: impl Into<ElementAnimation>) -> Self {
        Self {
            delay_ms,
            animation: animation.into(),
            direction: StaggerDirection::Forward,
            limit: None,
        }
    }

    /// Stagger from last to first
    pub fn reverse(mut self) -> Self {
        self.direction = StaggerDirection::Reverse;
        self
    }

    /// Stagger from center outward
    pub fn from_center(mut self) -> Self {
        self.direction = StaggerDirection::FromCenter;
        self
    }

    /// Limit stagger to first N items
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    /// Calculate delay for a specific child index
    pub fn delay_for_index(&self, index: usize, total: usize) -> u32 {
        let effective_index = match self.direction {
            StaggerDirection::Forward => index,
            StaggerDirection::Reverse => total.saturating_sub(1).saturating_sub(index),
            StaggerDirection::FromCenter => {
                let center = total / 2;
                if index <= center {
                    center - index
                } else {
                    index - center
                }
            }
        };

        // Apply limit if set
        let capped_index = if let Some(limit) = self.limit {
            effective_index.min(limit)
        } else {
            effective_index
        };

        self.delay_ms * capped_index as u32
    }
}

/// Shared animated value type for motion bindings (thread-safe)
pub type SharedAnimatedValue = std::sync::Arc<std::sync::Mutex<AnimatedValue>>;

/// Timeline rotation binding for continuous spinning animations
///
/// Used for spinners and other continuously rotating elements that use
/// timeline-based animation instead of spring physics.
#[derive(Clone)]
pub struct TimelineRotation {
    /// The timeline containing the rotation animation
    pub timeline: blinc_animation::SharedAnimatedTimeline,
    /// The entry ID for the rotation value in the timeline
    pub entry_id: blinc_animation::TimelineEntryId,
}

/// Motion bindings for continuous animation driven by AnimatedValue
///
/// This struct holds references to animated values that are sampled every frame
/// during rendering, enabling smooth continuous animations.
#[derive(Clone, Default)]
pub struct MotionBindings {
    /// Animated X translation
    pub translate_x: Option<SharedAnimatedValue>,
    /// Animated Y translation
    pub translate_y: Option<SharedAnimatedValue>,
    /// Animated uniform scale
    pub scale: Option<SharedAnimatedValue>,
    /// Animated X scale
    pub scale_x: Option<SharedAnimatedValue>,
    /// Animated Y scale
    pub scale_y: Option<SharedAnimatedValue>,
    /// Animated rotation (degrees) - spring-based
    pub rotation: Option<SharedAnimatedValue>,
    /// Animated rotation (degrees) - timeline-based for continuous spin
    pub rotation_timeline: Option<TimelineRotation>,
    /// Animated opacity
    pub opacity: Option<SharedAnimatedValue>,
}

impl MotionBindings {
    /// Check if any bindings are set
    pub fn is_empty(&self) -> bool {
        self.translate_x.is_none()
            && self.translate_y.is_none()
            && self.scale.is_none()
            && self.scale_x.is_none()
            && self.scale_y.is_none()
            && self.rotation.is_none()
            && self.rotation_timeline.is_none()
            && self.opacity.is_none()
    }

    /// Get the current translation from animated values
    ///
    /// Returns a translation transform for the tx/ty bindings.
    /// Scale and rotation should be queried separately for proper centered application.
    pub fn get_transform(&self) -> Option<Transform> {
        let tx = self
            .translate_x
            .as_ref()
            .map(|v| v.lock().unwrap().get())
            .unwrap_or(0.0);
        let ty = self
            .translate_y
            .as_ref()
            .map(|v| v.lock().unwrap().get())
            .unwrap_or(0.0);

        if tx.abs() > 0.001 || ty.abs() > 0.001 {
            Some(Transform::translate(tx, ty))
        } else {
            None
        }
    }

    /// Get the current scale values from animated bindings
    ///
    /// Returns (scale_x, scale_y) if any scale is bound.
    /// The renderer should apply this centered around the element.
    pub fn get_scale(&self) -> Option<(f32, f32)> {
        let scale = self.scale.as_ref().map(|v| v.lock().unwrap().get());
        let scale_x = self.scale_x.as_ref().map(|v| v.lock().unwrap().get());
        let scale_y = self.scale_y.as_ref().map(|v| v.lock().unwrap().get());

        if let Some(s) = scale {
            Some((s, s))
        } else if scale_x.is_some() || scale_y.is_some() {
            Some((scale_x.unwrap_or(1.0), scale_y.unwrap_or(1.0)))
        } else {
            None
        }
    }

    /// Get the current rotation from animated values (in degrees)
    ///
    /// The renderer should apply this centered around the element.
    /// Checks timeline-based rotation first, then spring-based.
    pub fn get_rotation(&self) -> Option<f32> {
        // Timeline rotation takes precedence (for continuous spinning)
        if let Some(ref tl_rot) = self.rotation_timeline {
            if let Ok(timeline) = tl_rot.timeline.lock() {
                return timeline.get(tl_rot.entry_id);
            }
        }
        // Fall back to spring-based rotation
        self.rotation.as_ref().map(|v| v.lock().unwrap().get())
    }

    /// Get the current opacity from animated value
    pub fn get_opacity(&self) -> Option<f32> {
        self.opacity.as_ref().map(|v| v.lock().unwrap().get())
    }
}

/// Motion container for animations
///
/// Wraps child elements and applies animations. Supports:
/// - Entry/exit animations (one-time on mount/unmount)
/// - Continuous animations driven by `AnimatedValue` bindings
///
/// The container itself is transparent but can have layout properties
/// to control how children are arranged (flex direction, gap, etc.).
///
/// # Stable Keys
///
/// Motion containers automatically generate a stable key based on their call site
/// (file, line, column). This allows animations to persist across tree rebuilds,
/// which is essential for overlays and other dynamically rebuilt content.
///
/// For additional uniqueness (e.g., in loops), use `.id()` to append a suffix:
///
/// ```ignore
/// for i in 0..items.len() {
///     motion()
///         .id(i)  // Appends index to auto-generated key
///         .fade_in(300)
///         .child(item_content)
/// }
/// ```
pub struct Motion {
    /// Children to animate (single or multiple)
    children: Vec<Box<dyn ElementBuilder>>,
    /// Entry animation
    enter: Option<ElementAnimation>,
    /// Exit animation
    exit: Option<ElementAnimation>,
    /// Stagger configuration for multiple children
    stagger_config: Option<StaggerConfig>,
    /// Layout style for the container
    style: Style,

    /// Stable key for motion tracking across tree rebuilds
    /// Auto-generated with UUID for uniqueness even in loops/closures
    key: InstanceKey,

    /// Whether to use stable keying for this motion
    /// When true (default), animation state persists across tree rebuilds using stable_key
    /// When false, each new node gets a fresh animation (useful for tabs, lists)
    use_stable_key: bool,

    /// Whether to replay the animation even if the motion already exists
    /// When true, the animation restarts from the beginning
    /// Useful for tab transitions where content changes but key stays stable
    replay: bool,

    // =========================================================================
    // Continuous animation bindings (AnimatedValue driven)
    // =========================================================================
    /// Animated X translation
    translate_x: Option<SharedAnimatedValue>,
    /// Animated Y translation
    translate_y: Option<SharedAnimatedValue>,
    /// Animated uniform scale
    scale: Option<SharedAnimatedValue>,
    /// Animated X scale
    scale_x: Option<SharedAnimatedValue>,
    /// Animated Y scale
    scale_y: Option<SharedAnimatedValue>,
    /// Animated rotation (degrees) - spring-based
    rotation: Option<SharedAnimatedValue>,
    /// Animated rotation (degrees) - timeline-based for continuous spin
    rotation_timeline: Option<TimelineRotation>,
    /// Animated opacity
    opacity: Option<SharedAnimatedValue>,
}

/// Convert a MotionKeyframe to KeyframeProperties for animation system integration
fn motion_keyframe_to_properties(kf: &MotionKeyframe) -> blinc_animation::KeyframeProperties {
    let mut props = blinc_animation::KeyframeProperties::default();

    if let Some(opacity) = kf.opacity {
        props = props.with_opacity(opacity);
    }
    if let Some(scale_x) = kf.scale_x {
        props.scale_x = Some(scale_x);
    }
    if let Some(scale_y) = kf.scale_y {
        props.scale_y = Some(scale_y);
    }
    if let Some(tx) = kf.translate_x {
        props.translate_x = Some(tx);
    }
    if let Some(ty) = kf.translate_y {
        props.translate_y = Some(ty);
    }
    if let Some(rotate) = kf.rotate {
        props.rotate = Some(rotate);
    }

    props
}

/// Create a motion container
///
/// The motion container automatically generates a stable unique key using UUID,
/// ensuring uniqueness even when created in loops or closures.
///
/// For additional uniqueness in loops, use `.id()`:
///
/// ```ignore
/// for i in 0..items.len() {
///     motion()
///         .id(i)  // Appends index to auto-generated key
///         .fade_in(300)
///         .child(item_content)
/// }
/// ```
#[track_caller]
pub fn motion() -> Motion {
    Motion {
        children: Vec::new(),
        enter: None,
        exit: None,
        stagger_config: None,
        style: Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            // Default to filling parent container (acts as transparent wrapper)
            size: taffy::Size {
                width: taffy::Dimension::Percent(1.0),
                height: taffy::Dimension::Auto,
            },
            flex_grow: 1.0,
            ..Style::default()
        },
        key: InstanceKey::new("motion"),
        use_stable_key: true, // Default to stable keying for overlays
        replay: false,
        translate_x: None,
        translate_y: None,
        scale: None,
        scale_x: None,
        scale_y: None,
        rotation: None,
        rotation_timeline: None,
        opacity: None,
    }
}

/// Create a motion container with a key derived from a parent key.
///
/// Use this inside `on_state` callbacks or other contexts where the motion
/// is recreated on each rebuild. By deriving from a stable parent key,
/// the motion's animation state persists across rebuilds.
///
/// # Example
///
/// ```ignore
/// // In a component with a stable key
/// let key = InstanceKey::new("tabs");
///
/// Stateful::with_shared_state(state)
///     .on_state(move |_, container| {
///         // Derive motion key from parent - stable across rebuilds
///         let m = motion_derived(&key.derive("content"))
///             .fade_in(200)
///             .child(content);
///         container.merge(div().child(m));
///     })
/// ```
pub fn motion_derived(parent_key: &str) -> Motion {
    Motion {
        children: Vec::new(),
        enter: None,
        exit: None,
        stagger_config: None,
        style: Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            size: taffy::Size {
                width: taffy::Dimension::Percent(1.0),
                height: taffy::Dimension::Auto,
            },
            flex_grow: 1.0,
            ..Style::default()
        },
        key: InstanceKey::explicit(format!("motion:{}", parent_key)),
        use_stable_key: true,
        replay: false,
        translate_x: None,
        translate_y: None,
        scale: None,
        scale_x: None,
        scale_y: None,
        rotation: None,
        rotation_timeline: None,
        opacity: None,
    }
}

impl Motion {
    /// Set the child element to animate
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        // Store single child in children vec so it's returned by children_builders()
        self.children = vec![Box::new(child)];
        self
    }

    /// Add multiple children with stagger animation support
    pub fn children<I, E>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: ElementBuilder + 'static,
    {
        self.children = children
            .into_iter()
            .map(|c| Box::new(c) as Box<dyn ElementBuilder>)
            .collect();
        self
    }

    /// Set animation to play when element enters the tree
    pub fn enter_animation(mut self, animation: impl Into<ElementAnimation>) -> Self {
        self.enter = Some(animation.into());
        self
    }

    /// Set animation to play when element exits the tree
    pub fn exit_animation(mut self, animation: impl Into<ElementAnimation>) -> Self {
        self.exit = Some(animation.into());
        self
    }

    /// Enable stagger animations for multiple children
    pub fn stagger(mut self, config: StaggerConfig) -> Self {
        self.stagger_config = Some(config);
        self
    }

    /// Set both enter and exit animations from a MotionAnimation config
    ///
    /// This is useful when you have a pre-built `MotionAnimation` from CSS
    /// keyframes or other sources.
    pub fn animation(self, config: MotionAnimation) -> Self {
        use blinc_animation::{Easing, KeyframeProperties};

        let mut result = self;

        if let Some(ref enter_from) = config.enter_from {
            // Build enter animation: start from enter_from, animate to defaults (visible)
            let from_props = motion_keyframe_to_properties(enter_from);
            let to_props = KeyframeProperties::default()
                .with_opacity(1.0)
                .with_scale(1.0)
                .with_translate(0.0, 0.0);

            let enter = MultiKeyframeAnimation::new(config.enter_duration_ms)
                .keyframe(0.0, from_props, Easing::Linear)
                .keyframe(1.0, to_props, Easing::EaseOut);

            result = result.enter_animation(enter);
        }

        if let Some(ref exit_to) = config.exit_to {
            // Build exit animation: start from defaults (visible), animate to exit_to
            let from_props = KeyframeProperties::default()
                .with_opacity(1.0)
                .with_scale(1.0)
                .with_translate(0.0, 0.0);
            let to_props = motion_keyframe_to_properties(exit_to);

            let exit = MultiKeyframeAnimation::new(config.exit_duration_ms)
                .keyframe(0.0, from_props, Easing::Linear)
                .keyframe(1.0, to_props, Easing::EaseIn);

            result = result.exit_animation(exit);
        }

        result
    }

    // ========================================================================
    // Convenience methods for common animations
    // ========================================================================

    /// Fade in on enter
    pub fn fade_in(self, duration_ms: u32) -> Self {
        self.enter_animation(AnimationPreset::fade_in(duration_ms))
    }

    /// Fade out on exit
    pub fn fade_out(self, duration_ms: u32) -> Self {
        self.exit_animation(AnimationPreset::fade_out(duration_ms))
    }

    /// Scale in on enter
    pub fn scale_in(self, duration_ms: u32) -> Self {
        self.enter_animation(AnimationPreset::scale_in(duration_ms))
    }

    /// Scale out on exit
    pub fn scale_out(self, duration_ms: u32) -> Self {
        self.exit_animation(AnimationPreset::scale_out(duration_ms))
    }

    /// Bounce in on enter
    pub fn bounce_in(self, duration_ms: u32) -> Self {
        self.enter_animation(AnimationPreset::bounce_in(duration_ms))
    }

    /// Bounce out on exit
    pub fn bounce_out(self, duration_ms: u32) -> Self {
        self.exit_animation(AnimationPreset::bounce_out(duration_ms))
    }

    /// Slide in from direction
    pub fn slide_in(self, direction: SlideDirection, duration_ms: u32) -> Self {
        let distance = 50.0;
        let anim = match direction {
            SlideDirection::Left => AnimationPreset::slide_in_left(duration_ms, distance),
            SlideDirection::Right => AnimationPreset::slide_in_right(duration_ms, distance),
            SlideDirection::Top => AnimationPreset::slide_in_top(duration_ms, distance),
            SlideDirection::Bottom => AnimationPreset::slide_in_bottom(duration_ms, distance),
        };
        self.enter_animation(anim)
    }

    /// Slide out to direction
    pub fn slide_out(self, direction: SlideDirection, duration_ms: u32) -> Self {
        let distance = 50.0;
        let anim = match direction {
            SlideDirection::Left => AnimationPreset::slide_out_left(duration_ms, distance),
            SlideDirection::Right => AnimationPreset::slide_out_right(duration_ms, distance),
            SlideDirection::Top => AnimationPreset::slide_out_top(duration_ms, distance),
            SlideDirection::Bottom => AnimationPreset::slide_out_bottom(duration_ms, distance),
        };
        self.exit_animation(anim)
    }

    /// Pop in (scale with overshoot)
    pub fn pop_in(self, duration_ms: u32) -> Self {
        self.enter_animation(AnimationPreset::pop_in(duration_ms))
    }

    // ========================================================================
    // Stylesheet Integration
    // ========================================================================

    /// Apply animation from a CSS stylesheet's `@keyframes` definition
    ///
    /// This is an alternative to `to_motion_animation()` that works with the
    /// Motion builder API. It looks up the named keyframes and applies them
    /// as enter/exit animations.
    ///
    /// # Arguments
    ///
    /// * `stylesheet` - The parsed CSS stylesheet containing @keyframes
    /// * `animation_name` - The name of the @keyframes to use
    /// * `enter_duration_ms` - Duration for enter animation
    /// * `exit_duration_ms` - Duration for exit animation
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     @keyframes modal-enter {
    ///         from { opacity: 0; transform: scale(0.95); }
    ///         to { opacity: 1; transform: scale(1); }
    ///     }
    /// "#;
    /// let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
    ///
    /// motion()
    ///     .from_stylesheet(&stylesheet, "modal-enter", 300, 200)
    ///     .child(modal_content)
    /// ```
    pub fn from_stylesheet(
        self,
        stylesheet: &crate::css_parser::Stylesheet,
        animation_name: &str,
        enter_duration_ms: u32,
        exit_duration_ms: u32,
    ) -> Self {
        if let Some(keyframes) = stylesheet.get_keyframes(animation_name) {
            let motion_anim = keyframes.to_motion_animation(enter_duration_ms, exit_duration_ms);
            self.animation(motion_anim)
        } else {
            tracing::warn!(
                animation_name = animation_name,
                "Keyframes not found in stylesheet"
            );
            self
        }
    }

    /// Apply animation from @keyframes with custom easing
    ///
    /// Similar to `from_stylesheet` but uses the `MultiKeyframeAnimation`
    /// system for more complex multi-step animations with custom easing.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let css = r#"
    ///     @keyframes pulse {
    ///         0%, 100% { opacity: 1; transform: scale(1); }
    ///         50% { opacity: 0.8; transform: scale(1.05); }
    ///     }
    /// "#;
    /// let stylesheet = Stylesheet::parse_with_errors(css).stylesheet;
    ///
    /// motion()
    ///     .keyframes_from_stylesheet(&stylesheet, "pulse", 1000, Easing::EaseInOut)
    ///     .child(button_content)
    /// ```
    pub fn keyframes_from_stylesheet(
        self,
        stylesheet: &crate::css_parser::Stylesheet,
        animation_name: &str,
        duration_ms: u32,
        easing: blinc_animation::Easing,
    ) -> Self {
        if let Some(keyframes) = stylesheet.get_keyframes(animation_name) {
            let animation = keyframes.to_multi_keyframe_animation(duration_ms, easing);
            self.enter_animation(animation)
        } else {
            tracing::warn!(
                animation_name = animation_name,
                "Keyframes not found in stylesheet"
            );
            self
        }
    }

    // ========================================================================
    // Continuous Animation Bindings (AnimatedValue driven)
    // ========================================================================

    /// Bind X translation to an AnimatedValue
    ///
    /// The motion element's X position will track this animated value.
    pub fn translate_x(mut self, value: SharedAnimatedValue) -> Self {
        self.translate_x = Some(value);
        self
    }

    /// Bind Y translation to an AnimatedValue
    ///
    /// The motion element's Y position will track this animated value.
    /// Perfect for pull-to-refresh, swipe gestures, etc.
    pub fn translate_y(mut self, value: SharedAnimatedValue) -> Self {
        self.translate_y = Some(value);
        self
    }

    /// Bind uniform scale to an AnimatedValue
    ///
    /// Scales both X and Y uniformly.
    pub fn scale(mut self, value: SharedAnimatedValue) -> Self {
        self.scale = Some(value);
        self
    }

    /// Bind X scale to an AnimatedValue
    pub fn scale_x(mut self, value: SharedAnimatedValue) -> Self {
        self.scale_x = Some(value);
        self
    }

    /// Bind Y scale to an AnimatedValue
    pub fn scale_y(mut self, value: SharedAnimatedValue) -> Self {
        self.scale_y = Some(value);
        self
    }

    /// Bind rotation to an AnimatedValue (in degrees)
    pub fn rotate(mut self, value: SharedAnimatedValue) -> Self {
        self.rotation = Some(value);
        self
    }

    /// Bind rotation to a timeline for continuous spinning (in degrees)
    ///
    /// Use this for spinners and other continuously rotating elements.
    /// The timeline should be configured with infinite looping.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let timeline = ctx.use_animated_timeline();
    /// let entry_id = timeline.lock().unwrap().configure(|t| {
    ///     let id = t.add(0, 1000, 0.0, 360.0);
    ///     t.set_loop(-1);
    ///     t.start();
    ///     id
    /// });
    /// motion()
    ///     .rotate_timeline(timeline, entry_id)
    ///     .child(spinner_visual)
    /// ```
    pub fn rotate_timeline(
        mut self,
        timeline: blinc_animation::SharedAnimatedTimeline,
        entry_id: blinc_animation::TimelineEntryId,
    ) -> Self {
        self.rotation_timeline = Some(TimelineRotation { timeline, entry_id });
        self
    }

    /// Bind opacity to an AnimatedValue (0.0 to 1.0)
    pub fn opacity(mut self, value: SharedAnimatedValue) -> Self {
        self.opacity = Some(value);
        self
    }

    /// Check if any continuous animations are bound
    pub fn has_animated_bindings(&self) -> bool {
        self.translate_x.is_some()
            || self.translate_y.is_some()
            || self.scale.is_some()
            || self.scale_x.is_some()
            || self.scale_y.is_some()
            || self.rotation.is_some()
            || self.rotation_timeline.is_some()
            || self.opacity.is_some()
    }

    /// Get the motion bindings for this element
    ///
    /// These bindings are stored in the RenderTree and sampled every frame
    /// during rendering to apply continuous animations.
    pub fn get_motion_bindings(&self) -> Option<MotionBindings> {
        if !self.has_animated_bindings() {
            return None;
        }

        Some(MotionBindings {
            translate_x: self.translate_x.clone(),
            translate_y: self.translate_y.clone(),
            scale: self.scale.clone(),
            scale_x: self.scale_x.clone(),
            scale_y: self.scale_y.clone(),
            rotation: self.rotation.clone(),
            rotation_timeline: self.rotation_timeline.clone(),
            opacity: self.opacity.clone(),
        })
    }

    // ========================================================================
    // Layout methods - control how children are arranged
    // ========================================================================

    /// Set the gap between children (in pixels)
    pub fn gap(mut self, gap: f32) -> Self {
        self.style.gap = taffy::Size {
            width: taffy::LengthPercentage::Length(gap),
            height: taffy::LengthPercentage::Length(gap),
        };
        self
    }

    /// Set flex direction to row
    pub fn flex_row(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Row;
        self
    }

    /// Set flex direction to column
    pub fn flex_col(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Column;
        self
    }

    /// Align items to center (cross-axis)
    pub fn items_center(mut self) -> Self {
        self.style.align_items = Some(taffy::AlignItems::Center);
        self
    }

    /// Align items to start (cross-axis)
    pub fn items_start(mut self) -> Self {
        self.style.align_items = Some(taffy::AlignItems::FlexStart);
        self
    }

    /// Justify content to center (main-axis)
    pub fn justify_center(mut self) -> Self {
        self.style.justify_content = Some(taffy::JustifyContent::Center);
        self
    }

    /// Justify content with space between (main-axis)
    pub fn justify_between(mut self) -> Self {
        self.style.justify_content = Some(taffy::JustifyContent::SpaceBetween);
        self
    }

    /// Set width to 100% of parent
    pub fn w_full(mut self) -> Self {
        self.style.size.width = taffy::Dimension::Percent(1.0);
        self
    }

    /// Set height to 100% of parent
    pub fn h_full(mut self) -> Self {
        self.style.size.height = taffy::Dimension::Percent(1.0);
        self
    }

    /// Allow this element to grow to fill available space
    pub fn flex_grow(mut self) -> Self {
        self.style.flex_grow = 1.0;
        self
    }

    /// Get the enter animation if set
    pub fn get_enter_animation(&self) -> Option<&ElementAnimation> {
        self.enter.as_ref()
    }

    /// Get the exit animation if set
    pub fn get_exit_animation(&self) -> Option<&ElementAnimation> {
        self.exit.as_ref()
    }

    /// Get the stagger config if set
    pub fn get_stagger_config(&self) -> Option<&StaggerConfig> {
        self.stagger_config.as_ref()
    }

    // ========================================================================
    // ID for uniqueness in loops/lists
    // ========================================================================

    /// Append an ID suffix for additional uniqueness
    ///
    /// Motion containers automatically generate a unique key using UUID.
    /// Use `.id()` when you need additional uniqueness, such as in loops or lists.
    ///
    /// The provided ID is appended to the generated key.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for (i, item) in items.iter().enumerate() {
    ///     motion()
    ///         .id(i)  // Creates unique key for each iteration
    ///         .fade_in(300)
    ///         .child(item_content)
    /// }
    /// ```
    pub fn id(mut self, id: impl std::fmt::Display) -> Self {
        // Create a new key that includes the user-provided suffix
        let new_key = format!("{}:{}", self.key.get(), id);
        self.key = InstanceKey::explicit(new_key);
        self
    }

    /// Get the stable key for this motion container
    ///
    /// Returns the unique key (UUID-based) with any user-provided ID suffixes appended.
    pub fn get_stable_key(&self) -> &str {
        self.key.get()
    }

    /// Make this motion transient (animation replays on each rebuild)
    ///
    /// By default, motion containers persist their animation state across tree
    /// rebuilds using a stable key. This is essential for overlays that rebuild
    /// frequently but should maintain animation continuity.
    ///
    /// For content that changes frequently (like tab panels, list items),
    /// use `.transient()` so the enter animation replays each time the
    /// content appears.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Tab content that animates on every tab switch
    /// motion()
    ///     .transient()
    ///     .fade_in(150)
    ///     .child(tab_content)
    /// ```
    pub fn transient(mut self) -> Self {
        self.use_stable_key = false;
        self
    }

    /// Request the animation to replay from the beginning
    ///
    /// Use this with `motion_derived` when you want the animation to play
    /// each time the content changes, while still maintaining a stable key
    /// to prevent animation restarts on unrelated rebuilds.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Tab content that animates on every tab switch
    /// let motion_key = format!("{}:{}", base_key, active_tab);
    /// motion_derived(&motion_key)
    ///     .replay()  // Animation plays each time active_tab changes
    ///     .fade_in(150)
    ///     .child(tab_content)
    /// ```
    pub fn replay(mut self) -> Self {
        self.replay = true;
        self
    }

    /// Check if replay is requested
    pub fn should_replay(&self) -> bool {
        self.replay
    }

    /// Get the motion animation configuration for a child at given index
    ///
    /// Takes stagger into account to compute the correct delay for each child.
    pub fn motion_animation_for_child(&self, child_index: usize) -> Option<MotionAnimation> {
        let total_children = self.children.len();

        if total_children == 0 {
            return None;
        }

        // Calculate delay based on stagger config
        let delay_ms = if let Some(ref stagger) = self.stagger_config {
            stagger.delay_for_index(child_index, total_children)
        } else {
            0
        };

        // Get base animation (from stagger config or direct enter/exit)
        let enter_anim = if let Some(ref stagger) = self.stagger_config {
            Some(&stagger.animation.animation)
        } else {
            self.enter.as_ref().map(|e| &e.animation)
        };

        let exit_anim = self.exit.as_ref().map(|e| &e.animation);

        // Build MotionAnimation
        if let Some(enter) = enter_anim {
            let enter_from = enter
                .first_keyframe()
                .map(|kf| MotionKeyframe::from_keyframe_properties(&kf.properties));

            let mut motion = MotionAnimation {
                enter_from,
                enter_duration_ms: enter.duration_ms(),
                enter_delay_ms: delay_ms,
                exit_to: None,
                exit_duration_ms: 0,
            };

            if let Some(exit) = exit_anim {
                motion.exit_to = exit
                    .last_keyframe()
                    .map(|kf| MotionKeyframe::from_keyframe_properties(&kf.properties));
                motion.exit_duration_ms = exit.duration_ms();
            }

            Some(motion)
        } else if let Some(exit) = exit_anim {
            let exit_to = exit
                .last_keyframe()
                .map(|kf| MotionKeyframe::from_keyframe_properties(&kf.properties));

            Some(MotionAnimation {
                enter_from: None,
                enter_duration_ms: 0,
                enter_delay_ms: delay_ms,
                exit_to,
                exit_duration_ms: exit.duration_ms(),
            })
        } else {
            None
        }
    }

    /// Get the number of children
    pub fn child_count(&self) -> usize {
        self.children.len()
    }
}

impl ElementBuilder for Motion {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        // Create a container node with the configured style
        let node = tree.create_node(self.style.clone());

        // Build and add all children (stagger delay is computed later via motion_animation_for_child)
        for child in &self.children {
            let child_node = child.build(tree);
            tree.add_child(node, child_node);
        }

        node
    }

    fn render_props(&self) -> RenderProps {
        // Motion with animated bindings uses motion_bindings() instead of static props.
        // Return default props - the actual transform/opacity will be sampled at render time.
        RenderProps::default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        // Return children vec - single child is now stored in children vec as well
        &self.children
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Motion
    }

    fn motion_animation_for_child(&self, child_index: usize) -> Option<MotionAnimation> {
        self.motion_animation_for_child(child_index)
    }

    fn motion_bindings(&self) -> Option<MotionBindings> {
        self.get_motion_bindings()
    }

    fn motion_stable_id(&self) -> Option<&str> {
        // Return stable key only if stable keying is enabled
        // When disabled, each node gets fresh animations (node-based tracking)
        if self.use_stable_key {
            Some(self.key.get())
        } else {
            None
        }
    }

    fn motion_should_replay(&self) -> bool {
        self.replay
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        Some(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stagger_delay_forward() {
        let config = StaggerConfig::new(50, AnimationPreset::fade_in(300));

        assert_eq!(config.delay_for_index(0, 5), 0);
        assert_eq!(config.delay_for_index(1, 5), 50);
        assert_eq!(config.delay_for_index(2, 5), 100);
        assert_eq!(config.delay_for_index(4, 5), 200);
    }

    #[test]
    fn test_stagger_delay_reverse() {
        let config = StaggerConfig::new(50, AnimationPreset::fade_in(300)).reverse();

        assert_eq!(config.delay_for_index(0, 5), 200);
        assert_eq!(config.delay_for_index(1, 5), 150);
        assert_eq!(config.delay_for_index(4, 5), 0);
    }

    #[test]
    fn test_stagger_delay_from_center() {
        let config = StaggerConfig::new(50, AnimationPreset::fade_in(300)).from_center();

        // For 5 items, center is index 2
        // Distances from center: [2, 1, 0, 1, 2]
        assert_eq!(config.delay_for_index(0, 5), 100); // 2 steps from center
        assert_eq!(config.delay_for_index(1, 5), 50); // 1 step from center
        assert_eq!(config.delay_for_index(2, 5), 0); // at center
        assert_eq!(config.delay_for_index(3, 5), 50); // 1 step from center
        assert_eq!(config.delay_for_index(4, 5), 100); // 2 steps from center
    }

    #[test]
    fn test_stagger_delay_with_limit() {
        let config = StaggerConfig::new(50, AnimationPreset::fade_in(300)).limit(3);

        assert_eq!(config.delay_for_index(0, 10), 0);
        assert_eq!(config.delay_for_index(3, 10), 150); // capped at limit
        assert_eq!(config.delay_for_index(5, 10), 150); // still capped
        assert_eq!(config.delay_for_index(9, 10), 150); // still capped
    }
}
