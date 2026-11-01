//! Skeleton component for loading placeholders
//!
//! A placeholder that pulses while content loads. Its fill, radius and pulse
//! are `.cn-skeleton` rules (`@keyframes cn-skeleton-pulse`); restyle or slow
//! it with rules of your own:
//!
//! ```css
//! .my-skeleton { animation-duration: 3000ms; }
//! ```
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Simple skeleton line
//! cn::skeleton().h(20.0).w(200.0)
//!
//! // Avatar skeleton
//! cn::skeleton_circle(48.0)
//!
//! // Card skeleton
//! div().col().gap(8.0)
//!     .child(cn::skeleton().h(200.0).w_full())  // Image
//!     .child(cn::skeleton().h(24.0).w(150.0))   // Title
//!     .child(cn::skeleton().h(16.0).w_full())   // Description line 1
//!     .child(cn::skeleton().h(16.0).w(180.0))   // Description line 2
//! ```

use std::ops::{Deref, DerefMut};

use blinc_layout::div::{Div, ElementBuilder, ElementTypeId};
use blinc_layout::prelude::*;

/// Skeleton component for loading placeholders
pub struct Skeleton {
    inner: Div,
}

impl Skeleton {
    /// Create a new skeleton placeholder
    pub fn new() -> Self {
        Self {
            inner: div().class("cn-skeleton"),
        }
    }

    /// Create a circular skeleton (for avatars, icons)
    pub fn circle(size: f32) -> Self {
        Self {
            inner: div()
                .class("cn-skeleton")
                .class("cn-skeleton--circle")
                .w(size)
                .h(size),
        }
    }

    /// Set width
    pub fn w(mut self, width: impl blinc_layout::binding::IntoReactive<f32>) -> Self {
        self.inner = self.inner.w(width);
        self
    }

    /// Set height
    pub fn h(mut self, height: impl blinc_layout::binding::IntoReactive<f32>) -> Self {
        self.inner = self.inner.h(height);
        self
    }

    /// Set full width
    pub fn w_full(mut self) -> Self {
        self.inner = self.inner.w_full();
        self
    }

    /// Set border radius
    pub fn rounded(mut self, radius: impl blinc_layout::binding::IntoReactive<f32>) -> Self {
        self.inner = self.inner.rounded(radius);
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

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Skeleton {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Skeleton {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for Skeleton {
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

/// Create a skeleton placeholder
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// // Text line skeleton
/// cn::skeleton().h(16.0).w(200.0)
///
/// // Avatar skeleton
/// cn::skeleton_circle(40.0)
/// ```
pub fn skeleton() -> Skeleton {
    Skeleton::new()
}

/// Create a circular skeleton
///
/// # Example
///
/// ```ignore
/// cn::skeleton_circle(48.0)  // 48px avatar placeholder
/// ```
pub fn skeleton_circle(size: f32) -> Skeleton {
    Skeleton::circle(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_theme() {
        let _ = blinc_theme::ThemeState::try_get().unwrap_or_else(|| {
            blinc_theme::ThemeState::init_default();
            blinc_theme::ThemeState::get()
        });
    }

    #[test]
    fn test_skeleton_default() {
        init_theme();
        let _ = skeleton();
    }

    #[test]
    fn test_skeleton_sized() {
        init_theme();
        let _ = skeleton().h(20.0).w(200.0);
    }

    #[test]
    fn test_skeleton_circle() {
        init_theme();
        let _ = skeleton_circle(48.0);
    }
}
