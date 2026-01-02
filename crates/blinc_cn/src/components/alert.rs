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

use blinc_core::Color;
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::prelude::*;
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

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

impl AlertVariant {
    fn background(&self, theme: &ThemeState) -> Color {
        match self {
            AlertVariant::Default => theme.color(ColorToken::Surface),
            AlertVariant::Success => theme.color(ColorToken::Success).with_alpha(0.1),
            AlertVariant::Warning => theme.color(ColorToken::Warning).with_alpha(0.1),
            AlertVariant::Destructive => theme.color(ColorToken::Error).with_alpha(0.1),
        }
    }

    fn border(&self, theme: &ThemeState) -> Color {
        match self {
            AlertVariant::Default => theme.color(ColorToken::Border),
            AlertVariant::Success => theme.color(ColorToken::Success),
            AlertVariant::Warning => theme.color(ColorToken::Warning),
            AlertVariant::Destructive => theme.color(ColorToken::Error),
        }
    }

    fn text_color(&self, theme: &ThemeState) -> Color {
        match self {
            AlertVariant::Default => theme.color(ColorToken::TextPrimary),
            AlertVariant::Success => theme.color(ColorToken::Success),
            AlertVariant::Warning => theme.color(ColorToken::Warning),
            AlertVariant::Destructive => theme.color(ColorToken::Error),
        }
    }
}

/// Simple alert with a single message
pub struct Alert {
    inner: Div,
}

impl Alert {
    /// Create a new alert with a message
    pub fn new(message: impl Into<String>) -> Self {
        Self::with_variant(message, AlertVariant::default())
    }

    fn with_variant(message: impl Into<String>, variant: AlertVariant) -> Self {
        let theme = ThemeState::get();
        let message = message.into();

        let bg = variant.background(&theme);
        let border_color = variant.border(&theme);
        let text_color = variant.text_color(&theme);
        let radius = theme.radius(RadiusToken::Md);
        let padding = theme.spacing_value(SpacingToken::Space4); // 16px

        let inner = div()
            .bg(bg)
            .border(1.0, border_color)
            .rounded(radius)
            .p_px(padding)
            .child(text(&message).size(14.0).color(text_color));

        Self { inner }
    }

    /// Set the alert variant
    pub fn variant(self, variant: AlertVariant) -> Self {
        let theme = ThemeState::get();
        let bg = variant.background(&theme);
        let border_color = variant.border(&theme);

        let inner = self.inner.bg(bg).border(1.0, border_color);

        Self { inner }
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
}

/// Create a simple alert with a message
pub fn alert(message: impl Into<String>) -> Alert {
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
        let theme = ThemeState::get();

        let bg = variant.background(&theme);
        let border_color = variant.border(&theme);
        let radius = theme.radius(RadiusToken::Md);
        let padding = theme.spacing_value(SpacingToken::Space4);
        let gap = theme.spacing_value(SpacingToken::Space1); // 4px

        div()
            .bg(bg)
            .border(1.0, border_color)
            .rounded(radius)
            .p_px(padding)
            .flex_col()
            .gap_px(gap)
    }

    /// Set the alert variant
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self.inner = Self::build_container(variant);
        self
    }

    /// Set the alert title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        let theme = ThemeState::get();
        let color = self.variant.text_color(&theme);

        self.inner = self.inner.child(
            text(title)
                .size(14.0)
                .semibold()
                .color(color),
        );
        self
    }

    /// Set the alert description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        let theme = ThemeState::get();
        let color = theme.color(ColorToken::TextSecondary);

        self.inner = self.inner.child(text(desc).size(14.0).color(color));
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
