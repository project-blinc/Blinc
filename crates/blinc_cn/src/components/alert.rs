//! Alert component for feedback messages
//!
//! Displays important messages with appropriate styling based on severity.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Info alert (default)
//! cn::alert("This is an informational message")
//!
//! // Success alert
//! cn::alert("Operation completed successfully")
//!     .variant(AlertVariant::Success)
//!
//! // Warning alert
//! cn::alert("Please review before proceeding")
//!     .variant(AlertVariant::Warning)
//!
//! // Error alert
//! cn::alert("An error occurred")
//!     .variant(AlertVariant::Destructive)
//!
//! // Alert with title and description
//! cn::alert_box()
//!     .variant(AlertVariant::Warning)
//!     .title("Heads up!")
//!     .description("This action cannot be undone.")
//! ```

use std::ops::{Deref, DerefMut};

use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::prelude::*;
use blinc_theme::ColorToken;
use blinc_theme::ThemeState;

/// Alert severity variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertVariant {
    /// Default/info alert
    #[default]
    Default,
    /// Success alert - green
    Success,
    /// Warning alert - yellow/orange
    Warning,
    /// Destructive/error alert - red
    Destructive,
}

// All variant visuals (background, border, text color) defined in CSS:
// .cn-alert--info, .cn-alert--success, .cn-alert--warning, .cn-alert--error

/// Simple alert with a single message
pub struct Alert {
    inner: Div,
    message: blinc_layout::binding::Reactive<String>,
    variant: AlertVariant,
}

impl Alert {
    /// Create a new alert with a message.
    ///
    /// Bind a signal here and the message follows it; text content has
    /// no property writer, so a bound message rebuilds through `deps()`.
    pub fn new(message: impl blinc_layout::binding::IntoReactive<String>) -> Self {
        Self::with_variant(message, AlertVariant::default())
    }

    fn with_variant(
        message: impl blinc_layout::binding::IntoReactive<String>,
        variant: AlertVariant,
    ) -> Self {
        let message = message.into_reactive();
        let inner = Self::build_div(&message, variant);
        Self {
            inner,
            message,
            variant,
        }
    }

    fn build_div(message: &blinc_layout::binding::Reactive<String>, variant: AlertVariant) -> Div {
        let variant_class = match variant {
            AlertVariant::Default => "cn-alert--info",
            AlertVariant::Success => "cn-alert--success",
            AlertVariant::Warning => "cn-alert--warning",
            AlertVariant::Destructive => "cn-alert--error",
        };

        // Visual props come from CSS: .cn-alert + .cn-alert--{variant}.
        // The message colour is ALSO set here, because a bound message
        // is rebuilt inside a stateful and cannot inherit the class's
        // `color` -- see `reactive_text`.
        let fg = ThemeState::get().color(match variant {
            AlertVariant::Default => ColorToken::Info,
            AlertVariant::Success => ColorToken::Success,
            AlertVariant::Warning => ColorToken::Warning,
            AlertVariant::Destructive => ColorToken::Error,
        });
        div().class("cn-alert").class(variant_class).child_box(
            crate::reactive_props::reactive_text(message, move |t| t.size(14.0).color(fg)),
        )
    }

    /// Set the alert variant
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self.inner = Self::build_div(&self.message, variant);
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl Deref for Alert {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Alert {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for Alert {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.inner)
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementBuilder::element_type_id(&self.inner)
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Create a simple alert with a message
pub fn alert(message: impl blinc_layout::binding::IntoReactive<String>) -> Alert {
    Alert::new(message)
}

// ============================================================================
// AlertBox - structured alert with title and description
// ============================================================================

/// Alert box with title and description
pub struct AlertBox {
    inner: Div,
    variant: AlertVariant,
}

impl AlertBox {
    /// Create a new empty alert box
    pub fn new() -> Self {
        Self {
            inner: Self::build_container(AlertVariant::default()),
            variant: AlertVariant::default(),
        }
    }

    fn build_container(variant: AlertVariant) -> Div {
        let variant_class = match variant {
            AlertVariant::Default => "cn-alert--info",
            AlertVariant::Success => "cn-alert--success",
            AlertVariant::Warning => "cn-alert--warning",
            AlertVariant::Destructive => "cn-alert--error",
        };

        // All visual props from CSS: .cn-alert-box + .cn-alert--{variant}
        div().class("cn-alert-box").class(variant_class).flex_col()
    }

    /// Set the alert variant
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self.inner = Self::build_container(variant);
        self
    }

    /// Set the alert title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.inner = self.inner.child(text(title).size(14.0).semibold());
        self
    }

    /// Set the alert description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.inner = self.inner.child(text(desc).size(14.0));
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.inner = self.inner.id(id);
        self
    }
}

impl Default for AlertBox {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for AlertBox {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for AlertBox {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for AlertBox {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.inner)
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementBuilder::element_type_id(&self.inner)
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Create an alert box with title and description support
pub fn alert_box() -> AlertBox {
    AlertBox::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_alert_default() {
        init_theme();
        let _ = alert("Test message");
    }

    #[test]
    fn test_alert_variants() {
        init_theme();
        let _ = alert("Info").variant(AlertVariant::Default);
        let _ = alert("Success").variant(AlertVariant::Success);
        let _ = alert("Warning").variant(AlertVariant::Warning);
        let _ = alert("Error").variant(AlertVariant::Destructive);
    }

    #[test]
    fn test_alert_box() {
        init_theme();
        let _ = alert_box()
            .variant(AlertVariant::Warning)
            .title("Warning")
            .description("This is a warning message");
    }
}
