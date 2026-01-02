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
    /// The fully-built inner element (Div containing switch and optional label)
    inner: Div,
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
        Self::with_config(SwitchConfig::new(on_state.clone(), scheduler))
    }

    /// Create from a full configuration
    fn with_config(config: SwitchConfig) -> Self {
        let theme = ThemeState::get();
        let track_width = config.size.track_width();
        let track_height = config.size.track_height();
        let thumb_size = config.size.thumb_size();
        let padding = 2.0; // Padding from track edge
        let thumb_travel = track_width - thumb_size - (padding * 2.0);
        let radius = track_height / 2.0; // Fully rounded track

        // Get colors
        let on_bg = config
            .on_color
            .unwrap_or_else(|| theme.color(ColorToken::Primary));
        let off_bg = config
            .off_color
            .unwrap_or_else(|| theme.color(ColorToken::Border));
        let thumb_color = config
            .thumb_color
            .unwrap_or_else(|| theme.color(ColorToken::TextInverse));

        let disabled = config.disabled;
        let on_change = config.on_change.clone();
        let on_state = config.on_state.clone();
        let on_state_for_click = config.on_state.clone();
        let thumb_anim = config.thumb_anim.clone();
        let thumb_anim_for_click = config.thumb_anim.clone();
        let color_anim = config.color_anim.clone();
        let color_anim_for_click = config.color_anim.clone();

        // Build background layers outside of on_state so motion bindings are properly registered
        // Off layer is always visible as the base
        let off_layer = div()
            .absolute()
            .inset(0.0)
            .rounded(radius)
            .bg(off_bg);

        // On layer with animated opacity
        // The motion container must be absolutely positioned and sized to cover the track
        // (motion's child being absolute would make motion collapse to 0x0)
        let on_layer = div()
            .absolute()
            .inset(0.0)
            .child(
                motion()
                    .opacity(color_anim)
                    .child(
                        div()
                            .w(track_width)
                            .h(track_height)
                            .rounded(radius)
                            .bg(on_bg)
                    )
            );

        // Thumb with animated position
        let thumb_element = div()
            .w(thumb_size)
            .h(thumb_size)
            .rounded(thumb_size / 2.0)
            .bg(thumb_color);

        let animated_thumb = motion()
            .translate_x(thumb_anim)
            .child(thumb_element);

        // Main switch container
        let mut switch = div()
            .w(track_width)
            .h(track_height)
            .rounded(radius)
            .cursor_pointer()
            .relative()
            .items_center()
            .padding_x(blinc_layout::units::px(padding))
            // Background layers
            .child(off_layer)
            .child(on_layer)
            // Animated thumb
            .child(animated_thumb);

        if disabled {
            switch = switch.opacity(0.5);
        }

        // Add click handler to toggle the state
        switch = switch.on_click(move |_| {
            if disabled {
                return;
            }

            let current = on_state_for_click.get();
            let new_value = !current;
            on_state_for_click.set(new_value);

            // Update animated value targets for smooth thumb movement and color fade
            let thumb_target = if new_value { thumb_travel } else { 0.0 };
            let color_target = if new_value { 1.0 } else { 0.0 };
            thumb_anim_for_click.lock().unwrap().set_target(thumb_target);
            color_anim_for_click.lock().unwrap().set_target(color_target);

            if let Some(ref callback) = on_change {
                callback(new_value);
            }
        });

        // If there's a label, wrap in a row
        let inner = if let Some(ref label_text) = config.label {
            let label_color = if disabled {
                theme.color(ColorToken::TextTertiary)
            } else {
                theme.color(ColorToken::TextPrimary)
            };

            div()
                .flex_row()
                .gap(theme.spacing().space_1)
                .items_center()
                .cursor_pointer()
                .child(switch)
                .child(text(label_text).size(14.0).color(label_color))
        } else {
            // Wrap single switch in a div for consistent behavior
            div().child(switch)
        };

        Self { inner }
    }
}

impl ElementBuilder for Switch {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }
}

/// Internal configuration for building a Switch
#[derive(Clone)]
struct SwitchConfig {
    on_state: State<bool>,
    size: SwitchSize,
    label: Option<String>,
    disabled: bool,
    on_color: Option<Color>,
    off_color: Option<Color>,
    thumb_color: Option<Color>,
    thumb_anim: SharedAnimatedValue,
    /// Animated value for color transition (0.0 = off, 1.0 = on)
    color_anim: SharedAnimatedValue,
    spring_config: SpringConfig,
    on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl SwitchConfig {
    fn new(on_state: State<bool>, scheduler: SchedulerHandle) -> Self {
        let size = SwitchSize::default();
        let padding = 2.0;
        let thumb_travel = size.track_width() - size.thumb_size() - (padding * 2.0);
        let is_on = on_state.get();
        let initial_x = if is_on { thumb_travel } else { 0.0 };
        let initial_color_t = if is_on { 1.0 } else { 0.0 };

        let spring_config = SpringConfig::snappy();
        let thumb_anim: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
            scheduler.clone(),
            initial_x,
            spring_config,
        )));
        let color_anim: SharedAnimatedValue = Arc::new(Mutex::new(AnimatedValue::new(
            scheduler,
            initial_color_t,
            spring_config,
        )));

        Self {
            on_state,
            size,
            label: None,
            disabled: false,
            on_color: None,
            off_color: None,
            thumb_color: None,
            thumb_anim,
            color_anim,
            spring_config,
            on_change: None,
        }
    }
}

/// Builder for creating Switch components with fluent API
pub struct SwitchBuilder {
    config: SwitchConfig,
    /// Cached built Switch - built lazily on first access
    built: std::cell::OnceCell<Switch>,
}

impl SwitchBuilder {
    /// Create a new switch builder with state and scheduler from context
    pub fn new(on_state: &State<bool>, scheduler: SchedulerHandle) -> Self {
        Self {
            config: SwitchConfig::new(on_state.clone(), scheduler),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Switch
    fn get_or_build(&self) -> &Switch {
        self.built.get_or_init(|| Switch::with_config(self.config.clone()))
    }

    /// Set the switch size
    pub fn size(mut self, size: SwitchSize) -> Self {
        self.config.size = size;
        self
    }

    /// Add a label to the switch
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Set the track color when on
    pub fn on_color(mut self, color: impl Into<Color>) -> Self {
        self.config.on_color = Some(color.into());
        self
    }

    /// Set the track color when off
    pub fn off_color(mut self, color: impl Into<Color>) -> Self {
        self.config.off_color = Some(color.into());
        self
    }

    /// Set the thumb color
    pub fn thumb_color(mut self, color: impl Into<Color>) -> Self {
        self.config.thumb_color = Some(color.into());
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
        self.config.spring_config = config;
        self
    }

    /// Set the change callback
    ///
    /// Called when the switch is toggled, with the new state.
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(callback));
        self
    }

    /// Build the final Switch component
    pub fn build_component(self) -> Switch {
        Switch::with_config(self.config)
    }
}

impl ElementBuilder for SwitchBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
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
pub fn switch(state: &State<bool>, scheduler: SchedulerHandle) -> SwitchBuilder {
    SwitchBuilder::new(state, scheduler)
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
