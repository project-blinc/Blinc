//! Checkbox widget with FSM-driven interactions
//!
//! The Checkbox widget provides:
//! - Toggle states: checked/unchecked
//! - Visual states: idle, hovered, pressed (for each toggle state)
//! - FSM-driven state transitions with toggle on click
//! - Spring physics animations for checkmark appearance
//! - Customizable appearance

use blinc_animation::spring::{Spring, SpringConfig};
use blinc_core::events::{event_types, Event};
use blinc_core::fsm::StateMachine;
use blinc_core::Color;
use blinc_layout::prelude::*;

use crate::context::WidgetContext;
use crate::widget::WidgetId;

/// Checkbox states (combines interaction state with checked state)
pub mod states {
    /// Unchecked + idle
    pub const UNCHECKED_IDLE: u32 = 0;
    /// Unchecked + hovered
    pub const UNCHECKED_HOVERED: u32 = 1;
    /// Unchecked + pressed
    pub const UNCHECKED_PRESSED: u32 = 2;
    /// Checked + idle
    pub const CHECKED_IDLE: u32 = 10;
    /// Checked + hovered
    pub const CHECKED_HOVERED: u32 = 11;
    /// Checked + pressed
    pub const CHECKED_PRESSED: u32 = 12;
}

/// Checkbox configuration
#[derive(Clone)]
pub struct CheckboxConfig {
    /// Optional label text
    pub label: Option<String>,
    /// Size of the checkbox box
    pub size: f32,
    /// Unchecked background color
    pub unchecked_bg: Color,
    /// Checked background color
    pub checked_bg: Color,
    /// Checkmark color
    pub check_color: Color,
    /// Corner radius
    pub corner_radius: f32,
    /// Label font size
    pub label_size: f32,
    /// Label color
    pub label_color: Color,
    /// Gap between checkbox and label
    pub gap: f32,
    /// Whether initially checked
    pub initial_checked: bool,
}

impl Default for CheckboxConfig {
    fn default() -> Self {
        Self {
            label: None,
            size: 20.0,
            unchecked_bg: Color::rgba(0.15, 0.15, 0.2, 1.0),
            checked_bg: Color::rgba(0.2, 0.5, 0.9, 1.0),
            check_color: Color::WHITE,
            corner_radius: 4.0,
            label_size: 14.0,
            label_color: Color::WHITE,
            gap: 8.0,
            initial_checked: false,
        }
    }
}

impl CheckboxConfig {
    /// Create a new checkbox config
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a label
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the checkbox size
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Set the checked background color
    pub fn checked_bg(mut self, color: Color) -> Self {
        self.checked_bg = color;
        self
    }

    /// Set whether initially checked
    pub fn checked(mut self, checked: bool) -> Self {
        self.initial_checked = checked;
        self
    }

    /// Set corner radius
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = radius;
        self
    }
}

/// Checkbox widget state
pub struct CheckboxState {
    /// Current scale for the checkmark (0 = hidden, 1 = visible)
    pub check_scale: f32,
    /// Spring for checkmark animation
    check_spring: Spring,
    /// Whether the checkbox value just changed (cleared after reading)
    changed: bool,
}

impl Clone for CheckboxState {
    fn clone(&self) -> Self {
        Self {
            check_scale: self.check_scale,
            check_spring: Spring::new(SpringConfig::snappy(), self.check_scale),
            changed: self.changed,
        }
    }
}

impl CheckboxState {
    /// Create new checkbox state
    pub fn new(initial_checked: bool) -> Self {
        let initial_scale = if initial_checked { 1.0 } else { 0.0 };
        Self {
            check_scale: initial_scale,
            check_spring: Spring::new(SpringConfig::snappy(), initial_scale),
            changed: false,
        }
    }

    /// Update animations (call each frame)
    pub fn update(&mut self, dt: f32) {
        self.check_spring.step(dt);
        self.check_scale = self.check_spring.value();
    }

    /// Set the checkmark visibility target
    pub fn set_check_target(&mut self, visible: bool) {
        self.check_spring
            .set_target(if visible { 1.0 } else { 0.0 });
    }

    /// Check if the value just changed and clear the flag
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

/// Checkbox widget
pub struct Checkbox {
    /// Widget ID
    id: WidgetId,
    /// Configuration
    config: CheckboxConfig,
    /// Change callback
    on_change: Option<Box<dyn FnMut(bool) + Send>>,
}

impl Checkbox {
    /// Create a new checkbox
    pub fn new(ctx: &mut WidgetContext) -> Self {
        Self::with_config(ctx, CheckboxConfig::default())
    }

    /// Create a checkbox with custom config
    pub fn with_config(ctx: &mut WidgetContext, config: CheckboxConfig) -> Self {
        let fsm = Self::create_fsm(config.initial_checked);
        let id = ctx.register_widget_with_fsm(fsm);

        // Initialize checkbox state
        let state = CheckboxState::new(config.initial_checked);
        ctx.set_widget_state(id, state);

        Self {
            id,
            config,
            on_change: None,
        }
    }

    /// Create the checkbox FSM
    fn create_fsm(initial_checked: bool) -> StateMachine {
        let initial_state = if initial_checked {
            states::CHECKED_IDLE
        } else {
            states::UNCHECKED_IDLE
        };

        StateMachine::builder(initial_state)
            // Unchecked hover transitions
            .on(
                states::UNCHECKED_IDLE,
                event_types::POINTER_ENTER,
                states::UNCHECKED_HOVERED,
            )
            .on(
                states::UNCHECKED_HOVERED,
                event_types::POINTER_LEAVE,
                states::UNCHECKED_IDLE,
            )
            .on(
                states::UNCHECKED_HOVERED,
                event_types::POINTER_DOWN,
                states::UNCHECKED_PRESSED,
            )
            // Toggle on release: unchecked -> checked
            .on(
                states::UNCHECKED_PRESSED,
                event_types::POINTER_UP,
                states::CHECKED_HOVERED,
            )
            .on(
                states::UNCHECKED_PRESSED,
                event_types::POINTER_LEAVE,
                states::UNCHECKED_IDLE,
            )
            // Checked hover transitions
            .on(
                states::CHECKED_IDLE,
                event_types::POINTER_ENTER,
                states::CHECKED_HOVERED,
            )
            .on(
                states::CHECKED_HOVERED,
                event_types::POINTER_LEAVE,
                states::CHECKED_IDLE,
            )
            .on(
                states::CHECKED_HOVERED,
                event_types::POINTER_DOWN,
                states::CHECKED_PRESSED,
            )
            // Toggle on release: checked -> unchecked
            .on(
                states::CHECKED_PRESSED,
                event_types::POINTER_UP,
                states::UNCHECKED_HOVERED,
            )
            .on(
                states::CHECKED_PRESSED,
                event_types::POINTER_LEAVE,
                states::CHECKED_IDLE,
            )
            .build()
    }

    /// Get the widget ID
    pub fn id(&self) -> WidgetId {
        self.id
    }

    /// Check if the checkbox is currently checked
    pub fn is_checked(&self, ctx: &WidgetContext) -> bool {
        let state = ctx.get_fsm_state(self.id).unwrap_or(states::UNCHECKED_IDLE);
        state >= states::CHECKED_IDLE
    }

    /// Set the change callback
    pub fn on_change<F: FnMut(bool) + Send + 'static>(mut self, callback: F) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }

    /// Handle an event
    pub fn handle_event(&mut self, ctx: &mut WidgetContext, event: &Event) {
        let old_state = ctx.get_fsm_state(self.id).unwrap_or(states::UNCHECKED_IDLE);
        let was_checked = old_state >= states::CHECKED_IDLE;

        // Dispatch to FSM
        ctx.dispatch_event(self.id, event);

        let new_state = ctx.get_fsm_state(self.id).unwrap_or(states::UNCHECKED_IDLE);
        let is_checked = new_state >= states::CHECKED_IDLE;

        // Update visual state
        if let Some(state) = ctx.get_widget_state_mut::<CheckboxState>(self.id) {
            state.set_check_target(is_checked);

            // Detect value change
            if was_checked != is_checked {
                state.changed = true;
                if let Some(ref mut callback) = self.on_change {
                    callback(is_checked);
                }
            }
        }
    }

    /// Update animations (call each frame)
    pub fn update(&self, ctx: &mut WidgetContext, dt: f32) {
        if let Some(state) = ctx.get_widget_state_mut::<CheckboxState>(self.id) {
            let old_scale = state.check_scale;
            state.update(dt);

            // Mark dirty if scale changed significantly
            if (state.check_scale - old_scale).abs() > 0.001 {
                ctx.mark_dirty(self.id);
            }
        }
    }

    /// Check if value just changed (and clear the flag)
    pub fn was_changed(&self, ctx: &mut WidgetContext) -> bool {
        ctx.get_widget_state_mut::<CheckboxState>(self.id)
            .map(|s| s.take_changed())
            .unwrap_or(false)
    }

    /// Build the checkbox's UI element
    pub fn build(&self, ctx: &WidgetContext) -> Div {
        let state = ctx
            .get_widget_state::<CheckboxState>(self.id)
            .cloned()
            .unwrap_or_else(|| CheckboxState::new(false));

        let fsm_state = ctx.get_fsm_state(self.id).unwrap_or(states::UNCHECKED_IDLE);
        let is_checked = fsm_state >= states::CHECKED_IDLE;
        let is_hovered = matches!(
            fsm_state,
            states::UNCHECKED_HOVERED
                | states::UNCHECKED_PRESSED
                | states::CHECKED_HOVERED
                | states::CHECKED_PRESSED
        );

        // Background color based on checked state and hover
        let bg_color = if is_checked {
            if is_hovered {
                // Lighten the checked color on hover
                Color::rgba(
                    (self.config.checked_bg.r + 0.1).min(1.0),
                    (self.config.checked_bg.g + 0.1).min(1.0),
                    (self.config.checked_bg.b + 0.1).min(1.0),
                    self.config.checked_bg.a,
                )
            } else {
                self.config.checked_bg
            }
        } else if is_hovered {
            // Lighten the unchecked color on hover
            Color::rgba(
                (self.config.unchecked_bg.r + 0.05).min(1.0),
                (self.config.unchecked_bg.g + 0.05).min(1.0),
                (self.config.unchecked_bg.b + 0.05).min(1.0),
                self.config.unchecked_bg.a,
            )
        } else {
            self.config.unchecked_bg
        };

        // Build the checkbox box
        let mut checkbox_box = div()
            .w(self.config.size)
            .h(self.config.size)
            .bg(bg_color.into())
            .rounded(self.config.corner_radius)
            .items_center()
            .justify_center();

        // Add checkmark with animated scale
        if state.check_scale > 0.01 {
            let scale = state.check_scale;
            let checkmark_text = text("✓")
                .size(self.config.size * 0.7 * scale)
                .color(self.config.check_color.into());
            checkbox_box = checkbox_box.child(checkmark_text);
        }

        // Wrap with label if present
        if let Some(ref label) = self.config.label {
            div()
                .flex_row()
                .gap(self.config.gap)
                .items_center()
                .child(checkbox_box)
                .child(
                    text(label)
                        .size(self.config.label_size)
                        .color(self.config.label_color.into()),
                )
        } else {
            checkbox_box
        }
    }
}

/// Create a checkbox
pub fn checkbox() -> CheckboxBuilder {
    CheckboxBuilder {
        config: CheckboxConfig::default(),
        on_change: None,
    }
}

/// Builder for creating checkboxes
pub struct CheckboxBuilder {
    config: CheckboxConfig,
    on_change: Option<Box<dyn FnMut(bool) + Send>>,
}

impl CheckboxBuilder {
    /// Add a label
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Set the checkbox size
    pub fn size(mut self, size: f32) -> Self {
        self.config.size = size;
        self
    }

    /// Set the checked background color
    pub fn checked_bg(mut self, color: impl Into<Color>) -> Self {
        self.config.checked_bg = color.into();
        self
    }

    /// Set whether initially checked
    pub fn checked(mut self, checked: bool) -> Self {
        self.config.initial_checked = checked;
        self
    }

    /// Set corner radius
    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.corner_radius = radius;
        self
    }

    /// Set the change callback
    pub fn on_change<F: FnMut(bool) + Send + 'static>(mut self, callback: F) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }

    /// Build the checkbox widget
    pub fn build(self, ctx: &mut WidgetContext) -> Checkbox {
        let mut checkbox = Checkbox::with_config(ctx, self.config);
        checkbox.on_change = self.on_change;
        checkbox
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_core::events::EventData;

    fn make_event(event_type: u32) -> Event {
        Event {
            event_type,
            target: 0,
            data: EventData::Pointer {
                x: 0.0,
                y: 0.0,
                button: 0,
                pressure: 1.0,
            },
            timestamp: 0,
            propagation_stopped: false,
        }
    }

    #[test]
    fn test_checkbox_creation() {
        let mut ctx = WidgetContext::new();
        let checkbox = Checkbox::new(&mut ctx);

        assert!(ctx.is_registered(checkbox.id()));
        assert!(!checkbox.is_checked(&ctx));
    }

    #[test]
    fn test_checkbox_toggle() {
        let mut ctx = WidgetContext::new();
        let mut checkbox = Checkbox::new(&mut ctx);

        // Initially unchecked
        assert!(!checkbox.is_checked(&ctx));

        // Hover
        let hover = make_event(event_types::POINTER_ENTER);
        checkbox.handle_event(&mut ctx, &hover);
        assert!(!checkbox.is_checked(&ctx));

        // Press
        let press = make_event(event_types::POINTER_DOWN);
        checkbox.handle_event(&mut ctx, &press);
        assert!(!checkbox.is_checked(&ctx));

        // Release - should toggle to checked
        let release = make_event(event_types::POINTER_UP);
        checkbox.handle_event(&mut ctx, &release);
        assert!(checkbox.is_checked(&ctx));
        assert!(checkbox.was_changed(&mut ctx));

        // Toggle again
        let press = make_event(event_types::POINTER_DOWN);
        checkbox.handle_event(&mut ctx, &press);
        let release = make_event(event_types::POINTER_UP);
        checkbox.handle_event(&mut ctx, &release);
        assert!(!checkbox.is_checked(&ctx));
    }

    #[test]
    fn test_checkbox_initially_checked() {
        let mut ctx = WidgetContext::new();
        let config = CheckboxConfig::new().checked(true);
        let checkbox = Checkbox::with_config(&mut ctx, config);

        assert!(checkbox.is_checked(&ctx));
    }
}
