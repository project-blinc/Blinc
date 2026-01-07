//! Slider component for range value selection
//!
//! A themed slider/range input with click-to-set and drag-to-adjust.
//! Uses context-driven state for proper persistence across UI rebuilds.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     // Create slider state from context (0.0 to 1.0 by default)
//!     let volume = ctx.use_state_for("volume", 0.5);
//!
//!     cn::slider(ctx, &volume)
//!         .label("Volume")
//!         .on_change(|value| println!("Volume: {}", value))
//! }
//!
//! // Custom range
//! let brightness = ctx.use_state_for("brightness", 50.0);
//! cn::slider(ctx, &brightness)
//!     .min(0.0)
//!     .max(100.0)
//!     .step(1.0)
//!
//! // Different sizes
//! cn::slider(ctx, &value)
//!     .size(SliderSize::Large)
//!
//! // Custom colors
//! cn::slider(ctx, &value)
//!     .track_color(Color::GRAY)
//!     .fill_color(Color::BLUE)
//!     .thumb_color(Color::WHITE)
//!
//! // Disabled state
//! cn::slider(ctx, &value)
//!     .disabled(true)
//! ```

use blinc_animation::{AnimationContext, SpringConfig};
use blinc_core::events::event_types;
use blinc_core::{BlincContext, Color, State};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::motion::motion;
use blinc_layout::prelude::*;
use blinc_layout::stateful::StateTransitions;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_macros::BlincComponent;
use blinc_theme::{ColorToken, RadiusToken, ThemeState};
use std::sync::Arc;

use super::label::{label, LabelSize};
use blinc_layout::InstanceKey;

/// Slider thumb interaction states
///
/// Unlike `ButtonState`, this FSM handles DRAG and DRAG_END events
/// to properly track dragging state even when mouse leaves the thumb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SliderThumbState {
    #[default]
    Idle,
    Hovered,
    Pressed,
    Dragging,
}

impl StateTransitions for SliderThumbState {
    fn on_event(&self, event: u32) -> Option<Self> {
        match (self, event) {
            // Idle transitions
            (SliderThumbState::Idle, event_types::POINTER_ENTER) => Some(SliderThumbState::Hovered),

            // Hovered transitions
            (SliderThumbState::Hovered, event_types::POINTER_LEAVE) => Some(SliderThumbState::Idle),
            (SliderThumbState::Hovered, event_types::POINTER_DOWN) => {
                Some(SliderThumbState::Pressed)
            }

            // Pressed transitions
            (SliderThumbState::Pressed, event_types::POINTER_UP) => Some(SliderThumbState::Hovered),
            (SliderThumbState::Pressed, event_types::POINTER_LEAVE) => Some(SliderThumbState::Idle),
            // When dragging starts, transition to Dragging
            (SliderThumbState::Pressed, event_types::DRAG) => Some(SliderThumbState::Dragging),

            // Dragging transitions - stays in Dragging until DRAG_END
            (SliderThumbState::Dragging, event_types::DRAG) => None, // Stay in Dragging
            (SliderThumbState::Dragging, event_types::DRAG_END) => Some(SliderThumbState::Idle),
            // Ignore POINTER_LEAVE/ENTER while dragging - we don't want visual changes
            (SliderThumbState::Dragging, event_types::POINTER_LEAVE) => None,
            (SliderThumbState::Dragging, event_types::POINTER_ENTER) => None,
            // POINTER_UP also ends dragging (fallback if DRAG_END not fired)
            (SliderThumbState::Dragging, event_types::POINTER_UP) => Some(SliderThumbState::Idle),

            _ => None,
        }
    }
}

/// BlincComponent for slider state and animations
/// Generates type-safe hooks that persist across UI rebuilds:
/// - SliderState::use_thumb_offset(ctx, initial, config) -> SharedAnimatedValue
/// - SliderState::use_drag_start_x(ctx, 0.0) -> State<f32>
#[derive(BlincComponent)]
struct SliderState {
    /// Animated X offset for thumb position
    #[animation]
    thumb_offset: f32,
    /// Mouse X position at drag start (screen coordinates)
    drag_start_x: f32,
    /// Thumb offset at drag start
    drag_start_offset: f32,
    /// Whether a drag is currently in progress (to suppress click-to-jump)
    is_dragging: bool,
}

/// Slider size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SliderSize {
    /// Small slider (track: 4px, thumb: 14px)
    Small,
    /// Medium slider (track: 6px, thumb: 18px)
    #[default]
    Medium,
    /// Large slider (track: 8px, thumb: 22px)
    Large,
}

impl SliderSize {
    /// Get the track height for this size
    fn track_height(&self) -> f32 {
        match self {
            SliderSize::Small => 4.0,
            SliderSize::Medium => 6.0,
            SliderSize::Large => 8.0,
        }
    }

    /// Get the thumb diameter for this size
    fn thumb_size(&self) -> f32 {
        match self {
            SliderSize::Small => 14.0,
            SliderSize::Medium => 18.0,
            SliderSize::Large => 22.0,
        }
    }
}

/// Slider component
///
/// A range slider with click-to-set and drag-to-adjust value.
/// Uses context-driven state for proper persistence across UI rebuilds.
pub struct Slider {
    /// The fully-built inner element (Div containing slider and optional label)
    inner: Div,
}

impl Slider {
    /// Create a new slider with context and state
    ///
    /// # Example
    /// ```ignore
    /// let volume = ctx.use_state_for("volume", 0.5);
    /// cn::slider(&volume).build_final(ctx)
    /// ```
    #[track_caller]
    pub fn new<C: BlincContext + AnimationContext>(ctx: &C, value_state: &State<f32>) -> Self {
        Self::with_config(
            ctx,
            InstanceKey::new("slider"),
            SliderConfig::new(value_state.clone()),
        )
    }

    /// Create from a full configuration
    fn with_config<C: BlincContext + AnimationContext>(
        ctx: &C,
        key: InstanceKey,
        config: SliderConfig,
    ) -> Self {
        let theme = ThemeState::get();
        let track_height = config.size.track_height();
        let thumb_size = config.size.thumb_size();
        let radius = theme.radius(RadiusToken::Full);

        // Get colors
        let track_bg = config
            .track_color
            .unwrap_or_else(|| theme.color(ColorToken::SurfaceElevated));
        let thumb_bg = config
            .thumb_color
            .unwrap_or_else(|| theme.color(ColorToken::TextInverse));
        // Fill color for the filled portion of the track
        let fill_bg = config
            .fill_color
            .unwrap_or_else(|| theme.color(ColorToken::Primary));
        // Note: border_hover could be used for hover effects (not yet implemented due to motion transform limitation)
        let _border_hover = theme.color(ColorToken::BorderHover);

        let disabled = config.disabled;
        let min = config.min;
        let max = config.max;
        let step = config.step;
        let width = config.width;

        // Track width - use config width or default
        let track_width = config.width.unwrap_or(300.0);

        // Calculate initial thumb offset based on current value
        let initial_value = config.value_state.get();
        let initial_norm = ((initial_value - min) / (max - min)).clamp(0.0, 1.0);
        let initial_offset = initial_norm * (track_width - thumb_size);

        // Get PERSISTED state from context using BlincComponent macro
        // These survive across UI rebuilds!
        // Use the instance_key from InstanceKey so each slider has its own state
        let instance_key = key.get();
        let thumb_offset = SliderState::use_thumb_offset_for(
            ctx,
            instance_key,
            initial_offset,
            SpringConfig::snappy(),
        );
        let drag_start_x = SliderState::use_drag_start_x_for(ctx, instance_key, 0.0);
        let drag_start_offset = SliderState::use_drag_start_offset_for(ctx, instance_key, 0.0);
        let is_dragging = SliderState::use_is_dragging_for(ctx, instance_key, false);

        // Clones for closures
        let thumb_offset_for_click = thumb_offset.clone();

        // Round to step helper
        let round_to_step = move |value: f32| -> f32 {
            if let Some(s) = step {
                if s > 0.0 {
                    let steps = ((value - min) / s).round();
                    (min + steps * s).clamp(min, max)
                } else {
                    value.clamp(min, max)
                }
            } else {
                value.clamp(min, max)
            }
        };
        let round_to_step_click = round_to_step;
        let round_to_step_drag = round_to_step;

        // Clones for event handlers
        let value_state_for_click = config.value_state.clone();
        let value_state_for_drag = config.value_state.clone();
        // let value_state_for_fill = config.value_state.clone();
        let on_change_for_click = config.on_change.clone();
        let on_change_for_drag = config.on_change.clone();

        // Clone for container drag handling and fill
        let thumb_offset_for_fill = thumb_offset.clone();
        let thumb_offset_for_drag = thumb_offset.clone();
        let thumb_offset_for_down = thumb_offset.clone();
        let drag_start_x_for_down = drag_start_x.clone();
        let drag_start_offset_for_down = drag_start_offset.clone();
        let drag_start_x_for_drag = drag_start_x.clone();
        let drag_start_offset_for_drag = drag_start_offset.clone();
        let is_dragging_for_click = is_dragging.clone();
        let is_dragging_for_drag = is_dragging.clone();
        let is_dragging_for_drag_end = is_dragging.clone();
        let is_dragging_for_thumb = is_dragging.clone();
        let is_dragging_for_leave = is_dragging.clone();

        // Get visual feedback colors
        let thumb_border_dragging = theme.color(ColorToken::Primary);

        // Thumb element - uses Stateful with deps on is_dragging to show visual feedback
        // Since motion.translate_x() uses visual transform, hit testing misses the thumb,
        // but we can still react to the is_dragging state signal for visual changes.
        let thumb = Stateful::<()>::new(())
            .deps(&[is_dragging.signal_id()])
            .on_state(move |_state: &(), container: &mut Div| {
                let dragging = is_dragging_for_thumb.get();
                let mut thumb_div = div()
                    .w(thumb_size)
                    .h(thumb_size)
                    .rounded(thumb_size / 2.0)
                    .border(2.0, theme.color(ColorToken::Border))
                    .bg(thumb_bg)
                    .shadow_sm();

                if dragging {
                    // Visual feedback when dragging: add border
                    thumb_div = thumb_div.border(2.0, thumb_border_dragging).shadow_md();
                }

                container.merge(thumb_div);
            });

        // Filled portion of track
        //
        // The fill bar is positioned so its right edge aligns with the thumb center.
        // Both fill and thumb share the same animated offset value, so they move together.
        //
        // Layout:
        // - A full-width fill bar starts at negative left position
        // - Motion translates it by thumb_offset (same as thumb)
        // - Result: fill right edge aligns with thumb center

        // The fill bar - full track width
        let fill_bar = div()
            .w(track_width)
            .h(track_height)
            .rounded(radius)
            .bg(fill_bg);

        // Position fill so its right edge is at thumb center when thumb_offset=0
        // At offset=0, fill right edge should be at thumb_size/2
        // So fill left edge should be at: thumb_size/2 - track_width
        let fill_left = thumb_size / 2.0 - track_width;
        let fill_positioned = div().absolute().left(fill_left).top(0.0).child(fill_bar);

        // Motion translates by thumb_offset - same value as thumb uses
        let animated_fill = motion()
            .translate_x(thumb_offset_for_fill.clone())
            .child(fill_positioned);

        // Container for animated fill with clipping
        let track_fill = div()
            .absolute()
            .left(0.0)
            .top((thumb_size - track_height) / 2.0)
            .w(track_width)
            .h(track_height)
            .overflow_clip()
            .rounded(radius)
            .relative() // Positioning context for absolute child
            .child(animated_fill);

        // Track visual element (the thin bar) - owns click-to-jump behavior
        // Track is absolutely positioned and centered vertically
        let track_visual = div()
            .absolute()
            .left(0.0)
            .right(0.0)
            .top((thumb_size - track_height) / 2.0) // Center vertically
            .h(track_height)
            .rounded(radius)
            .bg(track_bg)
            .cursor_pointer()
            // Track owns the click-to-jump behavior
            // Skip if a drag just occurred (is_dragging is cleared on DRAG_END)
            .on_click(move |event| {
                if disabled {
                    return;
                }

                // Skip click-to-jump if we just finished dragging
                // (the click event fires after drag end)
                if is_dragging_for_click.get() {
                    is_dragging_for_click.set(false);
                    return;
                }

                let track_w = event.bounds_width;

                if track_w > 0.0 {
                    // Calculate normalized position from click
                    let x = event.local_x;
                    let norm = (x / track_w).clamp(0.0, 1.0);
                    let raw = min + norm * (max - min);
                    let new_val = round_to_step_click(raw);
                    value_state_for_click.set(new_val);

                    // Animate thumb to clicked position with spring
                    let x_offset = norm * (track_w - thumb_size);
                    thumb_offset_for_click.lock().unwrap().set_target(x_offset);

                    if let Some(ref cb) = on_change_for_click {
                        cb(new_val);
                    }
                }
            });

        // Thumb wrapper - absolutely positioned at left=0, top=0
        // Motion.translate_x moves it visually from this base position
        let thumb_wrapper = div()
            .absolute()
            .left(0.0)
            .top(0.0)
            .child(motion().translate_x(thumb_offset).child(thumb));

        // Build the slider using div() with relative positioning
        //
        // IMPORTANT: The container handles ALL drag events because:
        // - motion().translate_x() uses visual transform (GPU-level), not layout transform
        // - Hit testing uses layout bounds, so clicks at the thumb's visual position miss it
        // - The container spans the full track width and always receives events correctly
        let mut slider_container = div()
            .relative() // Positioning context for absolute children
            .h(thumb_size)
            .overflow_visible() // Allow thumb to overflow if needed
            .cursor(CursorStyle::Grab)
            // Track background layer (absolutely positioned, centered)
            .child(track_visual)
            // Track fill layer (shows progress, on top of background)
            .child(track_fill)
            // Thumb with motion translation for visual positioning (absolutely positioned)
            .child(thumb_wrapper)
            // Container handles POINTER_DOWN to capture drag start position
            .on_mouse_down(move |event| {
                if disabled {
                    return;
                }
                // Store mouse X position and current thumb offset at drag start
                drag_start_x_for_down.set(event.mouse_x);
                let current = thumb_offset_for_down.lock().unwrap().get();
                drag_start_offset_for_down.set(current);
            })
            // Container handles DRAG to update thumb position
            // Uses mouse_x delta from drag start to calculate new offset
            .on_drag(move |event| {
                if disabled {
                    return;
                }
                // Mark that we're dragging (to suppress click-to-jump on release)
                is_dragging_for_drag.set(true);

                // Calculate delta from drag start using absolute mouse coordinates
                let start_x = drag_start_x_for_drag.get();
                let delta_x = event.mouse_x - start_x;
                let start_offset = drag_start_offset_for_drag.get();
                let max_offset = track_width - thumb_size;
                let new_offset = (start_offset + delta_x).clamp(0.0, max_offset);

                // Update thumb position immediately (no spring animation during drag)
                thumb_offset_for_drag
                    .lock()
                    .unwrap()
                    .set_immediate(new_offset);

                // Calculate and update value
                let norm = new_offset / max_offset;
                let raw = min + norm * (max - min);
                let new_val = round_to_step_drag(raw);
                value_state_for_drag.set(new_val);

                if let Some(ref cb) = on_change_for_drag {
                    cb(new_val);
                }
            })
            // DRAG_END - keep is_dragging true so click handler can clear it
            // (click fires after drag_end, so we need the flag to persist briefly)
            .on_drag_end(move |_event| {
                // is_dragging stays true - click handler will clear it
                // This prevents click-to-jump from firing after a drag
                let _ = is_dragging_for_drag_end.get(); // keep closure alive
            })
            // Mouse leave - clear is_dragging to reset visual state
            .on_hover_leave(move |_event| {
                is_dragging_for_leave.set(false);
            });

        // Apply width
        if let Some(w) = width {
            slider_container = slider_container.w(w);
        } else {
            slider_container = slider_container.w_full();
        }

        if disabled {
            slider_container = slider_container.opacity(0.5);
        }

        // If there's a label or show_value, wrap in a container
        let inner = if config.label.is_some() || config.show_value {
            let spacing = theme.spacing_value(blinc_theme::SpacingToken::Space2);
            let mut outer = div().h_fit().flex_col().gap_px(spacing);

            // Apply width to container
            if let Some(w) = width {
                outer = outer.w(w);
            } else {
                outer = outer.w_full();
            }

            // Header row with label and optional value
            if config.label.is_some() || config.show_value {
                let mut header = div().flex_row().justify_between().items_center();

                if let Some(ref label_text) = config.label {
                    let mut lbl = label(label_text).size(LabelSize::Medium);
                    if disabled {
                        lbl = lbl.disabled(true);
                    }
                    header = header.child(lbl);
                }

                if config.show_value {
                    let value_color = if disabled {
                        theme.color(ColorToken::TextTertiary)
                    } else {
                        theme.color(ColorToken::TextSecondary)
                    };
                    let value_state_for_display = config.value_state.clone();
                    let step_for_display = config.step;

                    // Use Stateful with deps to make value text reactive
                    let value_display = Stateful::<()>::new(())
                        .deps(&[config.value_state.signal_id()])
                        .on_state(move |_state: &(), container: &mut Div| {
                            let current_value = value_state_for_display.get();
                            let value_text =
                                if step_for_display.is_some() && step_for_display.unwrap() >= 1.0 {
                                    format!("{:.0}", current_value)
                                } else {
                                    format!("{:.2}", current_value)
                                };
                            container.merge(
                                div().child(text(&value_text).size(14.0).color(value_color)),
                            );
                        });
                    header = header.child(value_display);
                }

                outer = outer.child(header);
            }

            outer = outer.child(slider_container);
            outer
        } else {
            // Wrap container in a div for consistent return type
            div().h_fit().child(slider_container)
        };

        Self { inner }
    }
}

impl ElementBuilder for Slider {
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
}

/// Internal configuration for building a Slider
#[derive(Clone)]
struct SliderConfig {
    value_state: State<f32>,
    min: f32,
    max: f32,
    step: Option<f32>,
    size: SliderSize,
    label: Option<String>,
    show_value: bool,
    disabled: bool,
    width: Option<f32>,
    track_color: Option<Color>,
    fill_color: Option<Color>,
    thumb_color: Option<Color>,
    on_change: Option<Arc<dyn Fn(f32) + Send + Sync>>,
}

impl SliderConfig {
    fn new(value_state: State<f32>) -> Self {
        Self {
            value_state,
            min: 0.0,
            max: 1.0,
            step: None,
            size: SliderSize::default(),
            label: None,
            show_value: false,
            disabled: false,
            width: None,
            track_color: None,
            fill_color: None,
            thumb_color: None,
            on_change: None,
        }
    }
}

/// Builder for creating Slider components with fluent API
///
/// Unlike other builders, this one builds the slider immediately when `build_final()` is called,
/// because the context reference cannot be stored due to lifetime constraints.
pub struct SliderBuilder {
    key: InstanceKey,
    config: SliderConfig,
}

impl SliderBuilder {
    /// Create a new slider builder with value state
    ///
    /// Uses `#[track_caller]` to generate a unique instance key based on the call site.
    #[track_caller]
    pub fn new(value_state: &State<f32>) -> Self {
        Self {
            key: InstanceKey::new("slider"),
            config: SliderConfig::new(value_state.clone()),
        }
    }

    /// Create a slider builder with an explicit key
    pub fn with_key(key: impl Into<String>, value_state: &State<f32>) -> Self {
        Self {
            key: InstanceKey::explicit(key),
            config: SliderConfig::new(value_state.clone()),
        }
    }

    /// Set the minimum value (default: 0.0)
    pub fn min(mut self, min: f32) -> Self {
        self.config.min = min;
        self
    }

    /// Set the maximum value (default: 1.0)
    pub fn max(mut self, max: f32) -> Self {
        self.config.max = max;
        self
    }

    /// Set the step size for discrete values
    pub fn step(mut self, step: f32) -> Self {
        self.config.step = Some(step);
        self
    }

    /// Set the slider size
    pub fn size(mut self, size: SliderSize) -> Self {
        self.config.size = size;
        self
    }

    /// Add a label above the slider
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Show the current value next to the slider
    pub fn show_value(mut self) -> Self {
        self.config.show_value = true;
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Set a fixed width for the slider track
    pub fn w(mut self, width: f32) -> Self {
        self.config.width = Some(width);
        self
    }

    /// Set the unfilled track color
    pub fn track_color(mut self, color: impl Into<Color>) -> Self {
        self.config.track_color = Some(color.into());
        self
    }

    /// Set the filled portion color
    pub fn fill_color(mut self, color: impl Into<Color>) -> Self {
        self.config.fill_color = Some(color.into());
        self
    }

    /// Set the thumb color
    pub fn thumb_color(mut self, color: impl Into<Color>) -> Self {
        self.config.thumb_color = Some(color.into());
        self
    }

    /// Set the change callback
    ///
    /// Called when the slider value changes.
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(f32) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(callback));
        self
    }

    /// Build the final Slider component with the given context
    ///
    /// This must be called last to create the actual Slider element.
    pub fn build_final<C: BlincContext + AnimationContext>(self, ctx: &C) -> Slider {
        Slider::with_config(ctx, self.key, self.config)
    }
}

/// Create a slider with context and state
///
/// The slider uses context-driven state that persists across UI rebuilds.
/// Uses BlincComponent macro for type-safe state management.
///
/// **Important**: Call `.build_final(ctx)` at the end of the builder chain
/// to create the final Slider element.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
///     let volume = ctx.use_state_for("volume", 0.5);
///
///     cn::slider(&volume)
///         .min(0.0)
///         .max(1.0)
///         .label("Volume")
///         .show_value()
///         .on_change(|v| println!("Volume: {}", v))
///         .build_final(ctx)
/// }
/// ```
#[track_caller]
pub fn slider(state: &State<f32>) -> SliderBuilder {
    SliderBuilder::new(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slider_sizes() {
        assert_eq!(SliderSize::Small.track_height(), 4.0);
        assert_eq!(SliderSize::Medium.track_height(), 6.0);
        assert_eq!(SliderSize::Large.track_height(), 8.0);
    }

    #[test]
    fn test_slider_thumb_sizes() {
        assert_eq!(SliderSize::Small.thumb_size(), 14.0);
        assert_eq!(SliderSize::Medium.thumb_size(), 18.0);
        assert_eq!(SliderSize::Large.thumb_size(), 22.0);
    }
}
