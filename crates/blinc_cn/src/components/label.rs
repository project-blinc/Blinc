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
    text: String,
    size: LabelSize,
    required: bool,
    disabled: bool,
}

impl Label {
    /// Create a new label with the given text
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            size: LabelSize::default(),
            required: false,
            disabled: false,
        }
    }

    /// Set the label size
    pub fn size(mut self, size: LabelSize) -> Self {
        self.size = size;
        self
    }

    /// Mark the label as required (shows asterisk)
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Disable the label (dimmed appearance)
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl ElementBuilder for Label {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        let theme = ThemeState::get();
        let typography = theme.typography();

        let text_color = if self.disabled {
            theme.color(ColorToken::TextTertiary)
        } else {
            theme.color(ColorToken::TextPrimary)
        };

        let font_size = self.size.font_size(&typography);

        if self.required {
            // Build a row with text + asterisk
            let required_color = theme.color(ColorToken::Error);

            div()
                .flex_row()
                .gap(2.0)
                .child(
                    text(&self.text)
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
                .build(tree)
        } else {
            text(&self.text)
                .size(font_size)
                .color(text_color)
                .medium()
                .build(tree)
        }
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        blinc_layout::element::RenderProps::default()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        blinc_layout::div::ElementTypeId::Text
    }
}

/// Create a styled label component
pub fn label(text: impl Into<String>) -> Label {
    Label::new(text)
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
        let label = Label::new("Username").required().size(LabelSize::Large);

        assert!(label.required);
        assert_eq!(label.size, LabelSize::Large);
    }
}
