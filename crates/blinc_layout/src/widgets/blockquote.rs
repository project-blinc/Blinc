//! Blockquote widget for quoted content
//!
//! A container for quoted text with a distinctive left border and background,
//! equivalent to `<blockquote>` in HTML.
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! blockquote()
//!     .child(p("To be or not to be, that is the question."))
//!     .child(p("— William Shakespeare"))
//! ```

use blinc_core::Color;

use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::tree::{LayoutNodeId, LayoutTree};

/// Configuration for blockquote styling
#[derive(Clone, Debug)]
pub struct BlockquoteConfig {
    /// Left border colour over the `.blinc-blockquote` rule's, when set;
    /// drawn `border_width` wide.
    pub border_color: Option<Color>,
    /// Left border width in pixels, with `border_color`.
    pub border_width: f32,
    /// Background over the rule's, when set.
    pub bg_color: Option<Color>,
    /// Padding inside the blockquote
    pub padding: f32,
    /// Vertical margin
    pub margin_y: f32,
}

impl Default for BlockquoteConfig {
    fn default() -> Self {
        Self {
            border_color: None,
            border_width: 4.0,
            bg_color: None,
            padding: 4.0,
            margin_y: 2.0,
        }
    }
}

/// A blockquote container widget
///
/// Uses a simple container with background, left padding to simulate a left border,
/// and children added directly.
pub struct Blockquote {
    /// The content container with all styling and children
    inner: Div,
    css_element_id: Option<String>,
    css_classes: Vec<std::sync::Arc<str>>,
}

impl Blockquote {
    /// Create a new blockquote with default styling
    pub fn new() -> Self {
        Self::with_config(BlockquoteConfig::default())
    }

    /// Create a blockquote with custom configuration
    pub fn with_config(config: BlockquoteConfig) -> Self {
        let mut inner = div()
            .flex_col()
            .w_full()
            .my(config.margin_y)
            .p(config.padding);
        if let Some(bg) = config.bg_color {
            inner = inner.bg(bg);
        }
        if let Some(color) = config.border_color {
            inner = inner.border_left(config.border_width, color);
        }

        Self {
            inner,
            css_element_id: None,
            css_classes: vec![blinc_core::intern::intern("blinc-blockquote")],
        }
    }

    /// Add a child element to the blockquote content area
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.inner = self.inner.child(child);
        self
    }

    /// Add a boxed child element to the blockquote content area.
    pub fn child_box(mut self, child: Box<dyn ElementBuilder>) -> Self {
        self.inner = self.inner.child_box(child);
        self
    }

    /// Set the element ID for CSS selector targeting
    pub fn id(mut self, id: &str) -> Self {
        self.css_element_id = Some(id.to_string());
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: &str) -> Self {
        self.css_classes.push(blinc_core::intern::intern(name));
        self
    }
}

impl Default for Blockquote {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementBuilder for Blockquote {
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

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("blockquote")
    }

    fn element_id(&self) -> Option<&str> {
        self.css_element_id.as_deref()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        &self.css_classes
    }
}

/// Create a blockquote container
///
/// # Example
///
/// ```ignore
/// blockquote()
///     .child(p("A famous quote here"))
/// ```
pub fn blockquote() -> Blockquote {
    Blockquote::new()
}

/// Create a blockquote with custom configuration
pub fn blockquote_with_config(config: BlockquoteConfig) -> Blockquote {
    Blockquote::with_config(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_theme::ThemeState;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_blockquote_creates_container() {
        init_theme();
        let mut tree = LayoutTree::new();
        let bq = blockquote();
        bq.build(&mut tree);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_blockquote_with_child() {
        init_theme();
        let mut tree = LayoutTree::new();
        let bq = blockquote().child(div());
        bq.build(&mut tree);
        assert!(tree.len() > 1);
    }
}
