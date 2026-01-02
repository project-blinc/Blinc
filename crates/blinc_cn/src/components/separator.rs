//! Separator component for visual dividers
//!
//! A horizontal or vertical line to separate content.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Horizontal separator (default)
//! cn::separator()
//!
//! // Vertical separator
//! cn::separator().vertical()
//!
//! // With custom styling
//! cn::separator()
//!     .my(16.0)  // Vertical margin
//! ```

use std::ops::{Deref, DerefMut};

use blinc_core::Color;
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::prelude::*;
use blinc_theme::{ColorToken, ThemeState};

/// Separator orientation
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SeparatorOrientation {
    #[default]
    Horizontal,
    Vertical,
}

/// Separator component
pub struct Separator {
    inner: Div,
}

impl Separator {
    /// Create a new horizontal separator
    pub fn new() -> Self {
        Self::with_orientation(SeparatorOrientation::Horizontal)
    }

    fn with_orientation(orientation: SeparatorOrientation) -> Self {
        let theme = ThemeState::get();
        let color = theme.color(ColorToken::Border);

        let inner = match orientation {
            SeparatorOrientation::Horizontal => div().h(1.0).w_full().bg(color),
            SeparatorOrientation::Vertical => div().w(1.0).h_full().bg(color),
        };

        Self { inner }
    }

    /// Make separator vertical
    pub fn vertical(self) -> Self {
        Self::with_orientation(SeparatorOrientation::Vertical)
    }

    /// Make separator horizontal (default)
    pub fn horizontal(self) -> Self {
        Self::with_orientation(SeparatorOrientation::Horizontal)
    }

    // Forwarding methods for common Div operations
    // These consume self and return Self for chaining

    /// Set width
    pub fn w(mut self, width: f32) -> Self {
        self.inner = self.inner.w(width);
        self
    }

    /// Set height
    pub fn h(mut self, height: f32) -> Self {
        self.inner = self.inner.h(height);
        self
    }

    /// Set margin on all sides
    pub fn m(mut self, margin: f32) -> Self {
        self.inner = self.inner.m(margin);
        self
    }

    /// Set horizontal margin
    pub fn mx(mut self, margin: f32) -> Self {
        self.inner = self.inner.mx(margin);
        self
    }

    /// Set vertical margin
    pub fn my(mut self, margin: f32) -> Self {
        self.inner = self.inner.my(margin);
        self
    }

    /// Set margin top
    pub fn mt(mut self, margin: f32) -> Self {
        self.inner = self.inner.mt(margin);
        self
    }

    /// Set margin bottom
    pub fn mb(mut self, margin: f32) -> Self {
        self.inner = self.inner.mb(margin);
        self
    }

    /// Set margin left
    pub fn ml(mut self, margin: f32) -> Self {
        self.inner = self.inner.ml(margin);
        self
    }

    /// Set margin right
    pub fn mr(mut self, margin: f32) -> Self {
        self.inner = self.inner.mr(margin);
        self
    }

    /// Set background color
    pub fn bg(mut self, color: impl Into<Color>) -> Self {
        self.inner = self.inner.bg(color.into());
        self
    }

    /// Set opacity
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.inner = self.inner.opacity(opacity);
        self
    }
}

impl Default for Separator {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Separator {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Separator {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for Separator {
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

/// Create a separator
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// div().col()
///     .child(text("Section 1"))
///     .child(cn::separator().my(8.0))
///     .child(text("Section 2"))
/// ```
pub fn separator() -> Separator {
    Separator::new()
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
    fn test_separator_horizontal() {
        init_theme();
        let _ = separator();
    }

    #[test]
    fn test_separator_vertical() {
        init_theme();
        let _ = separator().vertical();
    }
}
