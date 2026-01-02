//! Label component - styled text label for form elements
//!
//! A themed label component following shadcn/ui patterns.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Basic label
//! cn::label("Email")
//!
//! // Required field label
//! cn::label("Password").required()
//!
//! // Disabled label
//! cn::label("Username").disabled()
//! ```

use blinc_layout::prelude::*;
use blinc_theme::{ColorToken, ThemeState, TypographyTokens};

/// Label size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelSize {
    /// Small label (12px)
    Small,
    /// Medium/default label (14px)
    #[default]
    Medium,
    /// Large label (16px)
    Large,
}

impl LabelSize {
    fn font_size(&self, typography: &TypographyTokens) -> f32 {
        match self {
            LabelSize::Small => typography.text_xs,
            LabelSize::Medium => typography.text_sm,
            LabelSize::Large => typography.text_base,
        }
    }
}

/// Styled Label component
pub struct Label {
    /// The fully-built inner element
    inner: Div,
}

impl Label {
    /// Create a new label with the given text
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_config(LabelConfig::new(text.into()))
    }

    /// Create from a full configuration
    fn with_config(config: LabelConfig) -> Self {
        let theme = ThemeState::get();
        let typography = theme.typography();

        let text_color = if config.disabled {
            theme.color(ColorToken::TextTertiary)
        } else {
            theme.color(ColorToken::TextPrimary)
        };

        let font_size = config.size.font_size(&typography);

        let inner = if config.required {
            // Build a row with text + asterisk
            let required_color = theme.color(ColorToken::Error);

            div()
                .flex_row()
                .gap(2.0)
                .child(
                    text(&config.text)
                        .size(font_size)
                        .color(text_color)
                        .medium(),
                )
                .child(
                    text("*")
                        .size(font_size)
                        .color(required_color)
                        .medium(),
                )
        } else {
            div().child(
                text(&config.text)
                    .size(font_size)
                    .color(text_color)
                    .medium(),
            )
        };

        Self { inner }
    }
}

/// Internal configuration for building a Label
#[derive(Clone)]
struct LabelConfig {
    text: String,
    size: LabelSize,
    required: bool,
    disabled: bool,
}

impl LabelConfig {
    fn new(text: String) -> Self {
        Self {
            text,
            size: LabelSize::default(),
            required: false,
            disabled: false,
        }
    }
}

/// Builder for creating Label components with fluent API
pub struct LabelBuilder {
    /// Internal configuration (pub for testing)
    pub(crate) config: LabelConfig,
    /// Cached built Label - built lazily on first access
    built: std::cell::OnceCell<Label>,
}

impl LabelBuilder {
    /// Create a new label builder with the given text
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            config: LabelConfig::new(text.into()),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Label
    fn get_or_build(&self) -> &Label {
        self.built
            .get_or_init(|| Label::with_config(self.config.clone()))
    }

    /// Set the label size
    pub fn size(mut self, size: LabelSize) -> Self {
        self.config.size = size;
        self
    }

    /// Mark the label as required (shows asterisk)
    pub fn required(mut self) -> Self {
        self.config.required = true;
        self
    }

    /// Disable the label (dimmed appearance)
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Build the final Label component
    pub fn build_component(self) -> Label {
        Label::with_config(self.config)
    }
}

impl ElementBuilder for Label {
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
}

impl ElementBuilder for LabelBuilder {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.get_or_build().element_type_id()
    }
}

/// Create a styled label component
pub fn label(text: impl Into<String>) -> LabelBuilder {
    LabelBuilder::new(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_theme::TypographyTokens;

    #[test]
    fn test_label_size_values() {
        let typography = TypographyTokens::default();
        assert_eq!(LabelSize::Small.font_size(&typography), typography.text_xs);
        assert_eq!(LabelSize::Medium.font_size(&typography), typography.text_sm);
        assert_eq!(LabelSize::Large.font_size(&typography), typography.text_base);
    }

    #[test]
    fn test_label_builder() {
        // Use the label() function which returns a LabelBuilder
        let lbl = label("Username").required().size(LabelSize::Large);

        // LabelBuilder has config field with the settings
        assert!(lbl.config.required);
        assert_eq!(lbl.config.size, LabelSize::Large);
    }
}
