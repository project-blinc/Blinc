//! Switch component for boolean toggle
//!
//! A themed toggle switch with smooth animated thumb movement.
//! Uses State<bool> from context for reactive state management.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     // Create switch state from context
//!     let enabled = ctx.use_state_for("notifications", false);
//!
//!     // Smooth animation enabled by default when scheduler is provided
//!     cn::switch(&enabled, ctx.animation_handle())
//!         .label("Enable notifications")
//!         .on_change(|is_on| println!("Switch: {}", is_on))
//! }
//!
//! // Different sizes
//! let dark_mode = ctx.use_state_for("dark_mode", true);
//! cn::switch(&dark_mode, ctx.animation_handle())
//!     .size(SwitchSize::Small)
//!
//! // Custom colors
//! cn::switch(&enabled, ctx.animation_handle())
//!     .on_color(Color::GREEN)
//!     .off_color(Color::GRAY)
//!
//! // Disabled state
//! cn::switch(&enabled, ctx.animation_handle())
//!     .disabled(true)
//!
//! // Custom spring config for different feel
//! cn::switch(&enabled, ctx.animation_handle())
//!     .spring(SpringConfig::wobbly())
//! ```

use blinc_animation::{AnimatedValue, SchedulerHandle, SpringConfig};
use blinc_core::{Color, State, Transform};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::motion::SharedAnimatedValue;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, ThemeState};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

/// Switch size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SwitchSize {
    /// Small switch (32x18)
    Small,
    /// Medium switch (44x24)
    #[default]
    Medium,
    /// Large switch (52x28)
    Large,
}

impl SwitchSize {
    /// Get the track width for this size
    fn track_width(&self) -> f32 {
        match self {
            SwitchSize::Small => 32.0,
            SwitchSize::Medium => 44.0,
            SwitchSize::Large => 52.0,
        }
    }

    /// Get the track height for this size
    fn track_height(&self) -> f32 {
        match self {
            SwitchSize::Small => 18.0,
            SwitchSize::Medium => 24.0,
            SwitchSize::Large => 28.0,
        }
    }

    /// Get the thumb diameter for this size
    fn thumb_size(&self) -> f32 {
        match self {
            SwitchSize::Small => 14.0,
            SwitchSize::Medium => 20.0,
            SwitchSize::Large => 24.0,
        }
    }
}

/// Switch component
///
/// A toggle switch with smooth spring animation.
/// Uses State<bool> from context for reactive state management.
pub struct Switch {
    inner: Stateful<ButtonState>,
    on_state: State<bool>,
    size: SwitchSize,
    label: Option<String>,
    disabled: bool,
    // Colors
    on_color: Option<Color>,
    off_color: Option<Color>,
    thumb_color: Option<Color>,
    // Animation - created internally with scheduler handle
    thumb_anim: SharedAnimatedValue,
    spring_config: SpringConfig,
    // Callback
    on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl Switch {
    /// Create a new switch with state and animation scheduler from context
    ///
    /// The switch uses spring animation for smooth thumb movement.
    ///
    /// # Example
    /// ```ignore
    /// let enabled = ctx.use_state_for("my_switch", false);
    /// cn::switch(&enabled, ctx.animation_handle())
    /// ```
    pub fn new(on_state: &State<bool>, scheduler: SchedulerHandle) -> Self {
        let inner = Stateful::new(ButtonState::Idle);

        // Calculate initial thumb position based on current state
        let size = SwitchSize::default();
        let padding = 2.0;
        let thumb_travel = size.track_width() - size.thumb_size() - (padding * 2.0);
        let initial_x = if on_state.get() { thumb_travel } else { 0.0 };

        // Create internal animated value with snappy spring config
        let spring_config = SpringConfig::snappy();
        let thumb_anim: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
            scheduler,
            initial_x,
            spring_config,
        )));

        Self {
            inner,
            on_state: on_state.clone(),
            size,
            label: None,
            disabled: false,
            on_color: None,
            off_color: None,
            thumb_color: None,
            thumb_anim,
            spring_config,
            on_change: None,
        }
    }

    /// Set the switch size
    pub fn size(mut self, size: SwitchSize) -> Self {
        self.size = size;
        self
    }

    /// Add a label to the switch
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set the track color when on
    pub fn on_color(mut self, color: impl Into<Color>) -> Self {
        self.on_color = Some(color.into());
        self
    }

    /// Set the track color when off
    pub fn off_color(mut self, color: impl Into<Color>) -> Self {
        self.off_color = Some(color.into());
        self
    }

    /// Set the thumb color
    pub fn thumb_color(mut self, color: impl Into<Color>) -> Self {
        self.thumb_color = Some(color.into());
        self
    }

    /// Set custom spring configuration for thumb animation
    ///
    /// By default, the switch uses `SpringConfig::snappy()`.
    /// Use this to customize the animation feel.
    ///
    /// # Example
    /// ```ignore
    /// cn::switch(&enabled, ctx.animation_handle())
    ///     .spring(SpringConfig::wobbly())
    /// ```
    pub fn spring(mut self, config: SpringConfig) -> Self {
        self.spring_config = config;
        self
    }

    /// Set the change callback
    ///
    /// Called when the switch is toggled, with the new state.
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        self.on_change = Some(Arc::new(callback));
        self
    }

    /// Build the switch element
    fn build_switch(&self) -> Stateful<ButtonState> {
        let theme = ThemeState::get();
        let track_width = self.size.track_width();
        let track_height = self.size.track_height();
        let thumb_size = self.size.thumb_size();
        let padding = 2.0; // Padding from track edge
        let thumb_travel = track_width - thumb_size - (padding * 2.0);
        let radius = track_height / 2.0; // Fully rounded track

        // Get colors
        let on_bg = self
            .on_color
            .unwrap_or_else(|| theme.color(ColorToken::Primary));
        let off_bg = self
            .off_color
            .unwrap_or_else(|| theme.color(ColorToken::Border));
        let thumb_color = self
            .thumb_color
            .unwrap_or_else(|| theme.color(ColorToken::TextInverse));

        let disabled = self.disabled;
        let on_change = self.on_change.clone();
        let on_state = self.on_state.clone();
        let on_state_for_click = self.on_state.clone();
        let thumb_anim = self.thumb_anim.clone();
        let thumb_anim_for_click = self.thumb_anim.clone();

        let mut switch = Stateful::new(ButtonState::Idle)
            .w(track_width)
            .h(track_height)
            .rounded(radius)
            .cursor_pointer()
            .items_center()
            .px(padding)
            // Subscribe to the on_state signal for reactive updates
            .deps(&[on_state.signal_id()]);

        if disabled {
            switch = switch.opacity(0.5);
        }

        // State callback for visual changes
        switch = switch.on_state(move |state: &ButtonState, container: &mut Div| {
            let is_on = on_state.get();
            let is_hovered = matches!(state, ButtonState::Hovered | ButtonState::Pressed);
            let is_pressed = matches!(state, ButtonState::Pressed);

            // Track background color
            let track_bg = if is_on { on_bg } else { off_bg };

            // Hover effect: slightly brighter/darker
            let track_bg = if is_hovered && !disabled {
                if is_on {
                    Color::rgba(
                        (track_bg.r + 0.05).min(1.0),
                        (track_bg.g + 0.05).min(1.0),
                        (track_bg.b + 0.05).min(1.0),
                        track_bg.a,
                    )
                } else {
                    Color::rgba(
                        (track_bg.r - 0.05).max(0.0),
                        (track_bg.g - 0.05).max(0.0),
                        (track_bg.b - 0.05).max(0.0),
                        track_bg.a,
                    )
                }
            } else {
                track_bg
            };

            // Scale effect on press
            let thumb_scale = if is_pressed && !disabled { 0.9 } else { 1.0 };

            // Build visual update with animated thumb using motion container
            let thumb_element = div()
                .w(thumb_size)
                .h(thumb_size)
                .rounded(thumb_size / 2.0)
                .bg(thumb_color)
                .transform(Transform::scale(thumb_scale, thumb_scale));

            // Always use motion() with translate_x for smooth spring animation
            let visual = div()
                .bg(track_bg)
                .child(motion().translate_x(thumb_anim.clone()).child(thumb_element));

            container.merge(visual);
        });

        // Add click handler to toggle the state
        switch = switch.on_click(move |_| {
            if disabled {
                return;
            }

            let current = on_state_for_click.get();
            let new_value = !current;
            on_state_for_click.set(new_value);

            // Update animated value target for smooth thumb movement
            let target = if new_value { thumb_travel } else { 0.0 };
            thumb_anim_for_click.lock().unwrap().set_target(target);

            if let Some(ref callback) = on_change {
                callback(new_value);
            }
        });

        switch
    }
}

impl Default for Switch {
    fn default() -> Self {
        // Note: This default requires State<bool> which needs context
        // Prefer using switch(&state) constructor
        panic!("Switch requires State<bool> from context. Use switch(&state) instead.")
    }
}

impl Deref for Switch {
    type Target = Stateful<ButtonState>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Switch {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for Switch {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        let switch = self.build_switch();

        // If there's a label, wrap in a row
        if let Some(ref label_text) = self.label {
            let theme = ThemeState::get();
            let label_color = if self.disabled {
                theme.color(ColorToken::TextTertiary)
            } else {
                theme.color(ColorToken::TextPrimary)
            };

            div()
                .flex_row()
                .gap(8.0)
                .items_center()
                .cursor_pointer()
                .child(switch)
                .child(text(label_text).size(14.0).color(label_color))
                .build(tree)
        } else {
            switch.build(tree)
        }
    }

    fn render_props(&self) -> RenderProps {
        RenderProps::default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Div
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        None
    }
}

/// Create a switch with state from context
///
/// The switch uses reactive `State<bool>` for its on/off status.
/// State changes automatically trigger visual updates via signals.
/// Smooth spring animation is enabled by default.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
///     let dark_mode = ctx.use_state_for("dark_mode", false);
///
///     cn::switch(&dark_mode, ctx.animation_handle())
///         .label("Dark mode")
///         .on_change(|enabled| println!("Dark mode: {}", enabled))
/// }
/// ```
pub fn switch(state: &State<bool>, scheduler: SchedulerHandle) -> Switch {
    Switch::new(state, scheduler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_switch_sizes() {
        assert_eq!(SwitchSize::Small.track_width(), 32.0);
        assert_eq!(SwitchSize::Medium.track_width(), 44.0);
        assert_eq!(SwitchSize::Large.track_width(), 52.0);
    }

    #[test]
    fn test_switch_track_heights() {
        assert_eq!(SwitchSize::Small.track_height(), 18.0);
        assert_eq!(SwitchSize::Medium.track_height(), 24.0);
        assert_eq!(SwitchSize::Large.track_height(), 28.0);
    }

    #[test]
    fn test_switch_thumb_sizes() {
        assert_eq!(SwitchSize::Small.thumb_size(), 14.0);
        assert_eq!(SwitchSize::Medium.thumb_size(), 20.0);
        assert_eq!(SwitchSize::Large.thumb_size(), 24.0);
    }

    #[test]
    fn test_thumb_travel() {
        let size = SwitchSize::Medium;
        let padding = 2.0;
        let travel = size.track_width() - size.thumb_size() - (padding * 2.0);
        // Travel should be: 44 - 20 - 4 = 20
        assert_eq!(travel, 20.0);
    }
}
