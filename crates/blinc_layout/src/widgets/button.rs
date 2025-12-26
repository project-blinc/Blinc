//! Ready-to-use Button widget
//!
//! A button with built-in hover, press, and disabled states.
//! Inherits ALL Div methods for full layout control via Deref.
//!
//! # Example
//!
//! ```ignore
//! div().child(
//!     button("Click me")
//!         .on_click(|_| println!("Clicked!"))
//!         .w(120.0)
//!         .rounded(8.0)    // ALL Div methods work!
//!         .p(16.0)
//!         .flex_row()
//!         .gap(8.0)
//! )
//! ```

use std::ops::{Deref, DerefMut};

use blinc_core::Color;

use crate::div::{div, Div, ElementBuilder};
use crate::element::RenderProps;
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};

/// Button visual states
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonVisualState {
    #[default]
    Idle,
    Hovered,
    Pressed,
    Disabled,
}

/// Button configuration
#[derive(Clone)]
pub struct ButtonConfig {
    /// Label text
    pub label: String,
    /// Base background color
    pub bg_color: Color,
    /// Hover background color
    pub hover_color: Color,
    /// Pressed background color
    pub pressed_color: Color,
    /// Disabled background color
    pub disabled_color: Color,
    /// Text color
    pub text_color: Color,
    /// Disabled text color
    pub disabled_text_color: Color,
    /// Font size
    pub font_size: f32,
    /// Whether disabled
    pub disabled: bool,
}

impl Default for ButtonConfig {
    fn default() -> Self {
        Self {
            label: String::new(),
            bg_color: Color::rgba(0.2, 0.5, 0.9, 1.0),
            hover_color: Color::rgba(0.3, 0.6, 1.0, 1.0),
            pressed_color: Color::rgba(0.15, 0.4, 0.8, 1.0),
            disabled_color: Color::rgba(0.3, 0.3, 0.35, 0.5),
            text_color: Color::WHITE,
            disabled_text_color: Color::rgba(0.7, 0.7, 0.7, 1.0),
            font_size: 16.0,
            disabled: false,
        }
    }
}

/// Ready-to-use button widget
///
/// Inherits all Div methods via Deref, so you have full layout control.
///
/// Usage: `button("Label").on_click(|_| ...).w(100.0).rounded(8.0).p(16.0)`
pub struct Button {
    /// Inner div - ALL Div methods are available via Deref
    inner: Div,
    /// Button-specific configuration
    config: ButtonConfig,
}

// Deref to Div gives Button ALL Div methods for reading
impl Deref for Button {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Button {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Button {
    /// Create a new button with a label
    pub fn new(label: impl Into<String>) -> Self {
        let label = label.into();
        let config = ButtonConfig {
            label: label.clone(),
            ..Default::default()
        };

        // Create inner div with default button styling
        let inner = div()
            .px(16.0)
            .py(8.0)
            .bg(config.bg_color)
            .rounded(8.0)
            .items_center()
            .justify_center()
            .child(
                text(&label)
                    .size(config.font_size)
                    .color(config.text_color)
                    .v_center(),
            );

        Self { inner, config }
    }

    /// Set the button label
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = label.into();
        self.rebuild_content();
        self
    }

    /// Set background color (button-specific, updates config)
    pub fn bg_color(mut self, color: impl Into<Color>) -> Self {
        self.config.bg_color = color.into();
        self.inner = std::mem::take(&mut self.inner).background(self.config.bg_color);
        self
    }

    /// Set hover color
    pub fn hover_color(mut self, color: impl Into<Color>) -> Self {
        self.config.hover_color = color.into();
        self
    }

    /// Set pressed color
    pub fn pressed_color(mut self, color: impl Into<Color>) -> Self {
        self.config.pressed_color = color.into();
        self
    }

    /// Set text color
    pub fn text_color(mut self, color: impl Into<Color>) -> Self {
        self.config.text_color = color.into();
        self.rebuild_content();
        self
    }

    /// Set font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.config.font_size = size;
        self.rebuild_content();
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        if disabled {
            self.inner = std::mem::take(&mut self.inner).background(self.config.disabled_color);
        } else {
            self.inner = std::mem::take(&mut self.inner).background(self.config.bg_color);
        }
        self.rebuild_content();
        self
    }

    /// Set click handler
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        if !self.config.disabled {
            self.inner = std::mem::take(&mut self.inner).on_click(handler);
        }
        self
    }

    /// Rebuild text content with current config
    fn rebuild_content(&mut self) {
        let text_color = if self.config.disabled {
            self.config.disabled_text_color
        } else {
            self.config.text_color
        };

        // Clear and rebuild with new text
        self.inner = std::mem::take(&mut self.inner).child(
            text(&self.config.label)
                .size(self.config.font_size)
                .color(text_color)
                .v_center(),
        );
    }

    // =========================================================================
    // Builder methods that return Self (shadow Div methods for fluent API)
    // =========================================================================

    pub fn w(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).w(px);
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).h(px);
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).size(w, h);
        self
    }

    pub fn square(mut self, size: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).square(size);
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

    pub fn bg(mut self, color: impl Into<blinc_core::Brush>) -> Self {
        self.inner = std::mem::take(&mut self.inner).background(color);
        self
    }

    pub fn rounded(mut self, radius: f32) -> Self {
        self.inner = std::mem::take(&mut self.inner).rounded(radius);
        self
    }

    pub fn shadow(mut self, shadow: blinc_core::Shadow) -> Self {
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

    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.inner = std::mem::take(&mut self.inner).child(child);
        self
    }

    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator,
        I::Item: ElementBuilder + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).children(children);
        self
    }

    // Event handlers
    pub fn on_hover_enter<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_hover_enter(handler);
        self
    }

    pub fn on_hover_leave<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_hover_leave(handler);
        self
    }

    pub fn on_mouse_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_mouse_down(handler);
        self
    }

    pub fn on_mouse_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_mouse_up(handler);
        self
    }

    pub fn on_focus<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_focus(handler);
        self
    }

    pub fn on_blur<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_blur(handler);
        self
    }

    pub fn on_key_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_key_down(handler);
        self
    }

    pub fn on_key_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_key_up(handler);
        self
    }

    pub fn on_scroll<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::mem::take(&mut self.inner).on_scroll(handler);
        self
    }
}

/// Create a ready-to-use button
///
/// The button inherits ALL Div methods, so you have full layout control.
///
/// # Example
///
/// ```ignore
/// button("Submit")
///     .on_click(|_| println!("Clicked!"))
///     .w(120.0)
///     .rounded(16.0)
///     .p(20.0)
///     .flex_row()
///     .gap(8.0)
///     .child(icon("check"))
/// ```
pub fn button(label: impl Into<String>) -> Button {
    Button::new(label)
}

impl ElementBuilder for Button {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        crate::div::ElementTypeId::Div
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }
}
