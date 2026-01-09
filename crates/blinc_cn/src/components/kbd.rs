//! Kbd component - keyboard shortcut display
//!
//! A styled inline element for displaying keyboard shortcuts.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui() -> impl ElementBuilder {
//!     div().flex_row().gap(4.0).items_center().children([
//!         text("Press"),
//!         cn::kbd("⌘"),
//!         text("+"),
//!         cn::kbd("K"),
//!         text("to search"),
//!     ])
//! }
//! ```

use std::cell::OnceCell;

use blinc_layout::div::{FontFamily, GenericFont};
use blinc_layout::prelude::*;
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

/// Size variants for the Kbd component
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KbdSize {
    /// Small size (text: 10px, padding: 2px 4px)
    Small,
    /// Medium size (text: 12px, padding: 2px 6px) - default
    #[default]
    Medium,
    /// Large size (text: 14px, padding: 4px 8px)
    Large,
}

/// Configuration for the Kbd component
#[derive(Clone, Debug)]
struct KbdConfig {
    text: String,
    size: KbdSize,
}

/// Builder for the Kbd component
pub struct KbdBuilder {
    config: KbdConfig,
    built: OnceCell<Kbd>,
}

impl std::fmt::Debug for KbdBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KbdBuilder")
            .field("text", &self.config.text)
            .field("size", &self.config.size)
            .finish()
    }
}

impl KbdBuilder {
    /// Create a new Kbd builder with the given text
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            config: KbdConfig {
                text: text.into(),
                size: KbdSize::Medium,
            },
            built: OnceCell::new(),
        }
    }

    /// Set the size of the Kbd
    pub fn size(mut self, size: KbdSize) -> Self {
        self.config.size = size;
        self
    }

    /// Get or build the component
    fn get_or_build(&self) -> &Kbd {
        self.built.get_or_init(|| self.build_component())
    }

    /// Build the Kbd component
    fn build_component(&self) -> Kbd {
        let theme = ThemeState::get();

        // Get colors
        let bg = theme.color(ColorToken::Surface);
        let border_color = theme.color(ColorToken::Border);
        let text_color = theme.color(ColorToken::TextSecondary);

        // Size-based styling
        let (font_size, px, py, radius) = match self.config.size {
            KbdSize::Small => (10.0, 4.0, 2.0, theme.radius(RadiusToken::Sm)),
            KbdSize::Medium => (12.0, 6.0, 2.0, theme.radius(RadiusToken::Sm)),
            KbdSize::Large => (14.0, 8.0, 4.0, theme.radius(RadiusToken::Md)),
        };

        // Build the Kbd element
        let inner = div()
            .items_center()
            .justify_center()
            .w_fit()
            .bg(bg)
            .border(1.0, border_color)
            .border_bottom(3.0, border_color)
            .rounded(radius)
            .padding_x_px(px)
            .padding_y_px(py)
            // Subtle shadow for depth
            .shadow_sm()
            .child(
                text(&self.config.text)
                    .size(font_size)
                    .color(text_color)
                    .font_family(FontFamily::generic(GenericFont::Monospace))
                    .weight(FontWeight::SemiBold)
                    .medium()
                    .no_wrap(),
            );

        Kbd { inner }
    }
}

/// Built Kbd component
pub struct Kbd {
    inner: Div,
}

impl std::fmt::Debug for Kbd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kbd").finish()
    }
}

impl ElementBuilder for KbdBuilder {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.get_or_build().inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.get_or_build().inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().inner.children_builders()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.get_or_build().inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().inner.layout_style()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.get_or_build().inner)
    }
}

impl ElementBuilder for Kbd {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }
}

/// Create a Kbd component to display a keyboard shortcut
///
/// # Example
///
/// ```ignore
/// // Simple key
/// cn::kbd("K")
///
/// // Modifier key
/// cn::kbd("⌘")
///
/// // With size
/// cn::kbd("Enter").size(KbdSize::Large)
/// ```
pub fn kbd(text: impl Into<String>) -> KbdBuilder {
    KbdBuilder::new(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kbd_sizes() {
        // Just verify the enum values exist
        assert_eq!(KbdSize::default(), KbdSize::Medium);
    }
}
