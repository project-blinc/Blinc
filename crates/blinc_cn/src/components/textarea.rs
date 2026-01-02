//! Textarea component - styled multi-line text input following shadcn/ui patterns
//!
//! A themed wrapper around `blinc_layout::TextArea` with consistent styling,
//! size variants, and optional label/error message support.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Basic textarea
//! let bio = text_area_state();
//! cn::textarea(&bio)
//!     .placeholder("Enter your bio")
//!
//! // With rows/cols sizing (like HTML textarea)
//! cn::textarea(&content)
//!     .rows(5)
//!     .cols(40)
//!
//! // With label
//! cn::textarea(&description)
//!     .label("Description")
//!     .placeholder("Enter description...")
//!
//! // With error message
//! cn::textarea(&message)
//!     .label("Message")
//!     .error("Message is required")
//!
//! // Disabled state
//! cn::textarea(&notes)
//!     .disabled(true)
//! ```

use super::label::{label, LabelSize};
use blinc_layout::prelude::*;
use blinc_layout::widgets::text_area::{text_area, SharedTextAreaState, TextArea as LayoutTextArea};
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState, TypographyTokens};

/// Textarea size variants (affects default dimensions)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextareaSize {
    /// Small textarea (3 rows, smaller font)
    Small,
    /// Medium/default textarea (4 rows)
    #[default]
    Medium,
    /// Large textarea (6 rows, larger font)
    Large,
}

impl TextareaSize {
    fn default_rows(&self) -> usize {
        match self {
            TextareaSize::Small => 3,
            TextareaSize::Medium => 4,
            TextareaSize::Large => 6,
        }
    }

    fn font_size(&self, typography: &TypographyTokens) -> f32 {
        match self {
            TextareaSize::Small => typography.text_xs,  // 12px
            TextareaSize::Medium => typography.text_sm, // 14px
            TextareaSize::Large => typography.text_base, // 16px
        }
    }
}

/// Styled Textarea component
pub struct Textarea {
    state: SharedTextAreaState,
    size: TextareaSize,
    rows: Option<usize>,
    cols: Option<usize>,
    label: Option<String>,
    description: Option<String>,
    error: Option<String>,
    disabled: bool,
    required: bool,
    placeholder: Option<String>,
    max_length: Option<usize>,
    wrap: bool,
    width: Option<f32>,
    height: Option<f32>,
    full_width: bool,
    border_width: Option<f32>,
    corner_radius: Option<f32>,
}

impl Textarea {
    /// Create a new textarea with the given state
    pub fn new(state: &SharedTextAreaState) -> Self {
        Self {
            state: state.clone(),
            size: TextareaSize::default(),
            rows: None,
            cols: None,
            label: None,
            description: None,
            error: None,
            disabled: false,
            required: false,
            placeholder: None,
            max_length: None,
            wrap: true,
            width: None,
            height: None,
            full_width: true,
            border_width: None,
            corner_radius: None,
        }
    }

    /// Set the textarea size preset
    pub fn size(mut self, size: TextareaSize) -> Self {
        self.size = size;
        self
    }

    /// Set number of visible rows (like HTML textarea)
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = Some(rows);
        self
    }

    /// Set number of visible columns (like HTML textarea)
    pub fn cols(mut self, cols: usize) -> Self {
        self.cols = Some(cols);
        self
    }

    /// Set a label above the textarea
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set a description/helper text below the textarea
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set an error message (shows in red, replaces description)
    pub fn error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Set placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set maximum character length
    pub fn max_length(mut self, max: usize) -> Self {
        self.max_length = Some(max);
        self
    }

    /// Enable or disable text wrapping (default: true)
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Disable text wrapping
    pub fn no_wrap(mut self) -> Self {
        self.wrap = false;
        self
    }

    /// Disable the textarea
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Mark the textarea as required (shows asterisk on label)
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set a fixed width
    pub fn w(mut self, width: f32) -> Self {
        self.width = Some(width);
        self.full_width = false;
        self
    }

    /// Set a fixed height
    pub fn h(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Make the textarea fill its parent width (default)
    pub fn w_full(mut self) -> Self {
        self.full_width = true;
        self.width = None;
        self
    }

    /// Set the border width
    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = Some(width);
        self
    }

    /// Set the corner radius
    pub fn rounded(mut self, radius: f32) -> Self {
        self.corner_radius = Some(radius);
        self
    }

    /// Build the textarea element
    fn build_textarea(&self) -> LayoutTextArea {
        let theme = ThemeState::get();
        let typography = theme.typography();

        let radius = self.corner_radius.unwrap_or_else(|| theme.radius(RadiusToken::Md));

        // Create base textarea
        let mut ta = text_area(&self.state)
            .font_size(self.size.font_size(&typography))
            .rounded(radius)
            .disabled(self.disabled)
            .wrap(self.wrap);

        // Apply border width if specified
        if let Some(border) = self.border_width {
            ta = ta.border_width(border);
        }

        // Apply rows/cols or use size preset
        if let Some(rows) = self.rows {
            ta = ta.rows(rows);
        } else {
            ta = ta.rows(self.size.default_rows());
        }

        if let Some(cols) = self.cols {
            ta = ta.cols(cols);
        }

        // Apply explicit dimensions if provided
        if let Some(w) = self.width {
            ta = ta.w(w);
        } else if self.full_width {
            ta = ta.w_full();
        }

        if let Some(h) = self.height {
            ta = ta.h(h);
        }

        // Apply placeholder
        if let Some(ref placeholder) = self.placeholder {
            ta = ta.placeholder(placeholder.clone());
        }

        // Apply max length
        if let Some(max) = self.max_length {
            ta = ta.max_length(max);
        }

        ta
    }
}

impl ElementBuilder for Textarea {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        let theme = ThemeState::get();

        // If no label, description, or error, just return the textarea directly
        if self.label.is_none() && self.description.is_none() && self.error.is_none() {
            return self.build_textarea().build(tree);
        }

        // Build a container with label, textarea, and description/error
        let spacing = theme.spacing_value(SpacingToken::Space2);
        let mut container = div().flex_col().gap_px(spacing);

        // Apply width to container
        if self.full_width {
            container = container.w_full();
        } else if let Some(w) = self.width {
            container = container.w(w);
        }

        let typography = theme.typography();

        // Label
        if let Some(ref label_text) = self.label {
            let mut lbl = label(label_text).size(LabelSize::Medium);
            if self.required {
                lbl = lbl.required();
            }
            if self.disabled {
                lbl = lbl.disabled(true);
            }
            container = container.child(lbl);
        }

        // Textarea
        container = container.child(self.build_textarea());

        // Error or description
        if let Some(ref error_text) = self.error {
            let error_color = theme.color(ColorToken::Error);
            container = container.child(text(error_text).size(typography.text_xs).color(error_color));
        } else if let Some(ref desc_text) = self.description {
            let desc_color = theme.color(ColorToken::TextTertiary);
            container = container.child(text(desc_text).size(typography.text_xs).color(desc_color));
        }

        container.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        // If we have label/description, we're a container
        if self.label.is_some() || self.description.is_some() || self.error.is_some() {
            blinc_layout::element::RenderProps::default()
        } else {
            self.build_textarea().render_props()
        }
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        blinc_layout::div::ElementTypeId::Div
    }
}

/// Create a styled textarea component
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// let bio = text_area_state();
/// cn::textarea(&bio)
///     .label("Bio")
///     .placeholder("Tell us about yourself...")
///     .rows(5)
/// ```
pub fn textarea(state: &SharedTextAreaState) -> Textarea {
    Textarea::new(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blinc_theme::TypographyTokens;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_textarea_size_values() {
        let typography = TypographyTokens::default();

        // Default rows for each size
        assert_eq!(TextareaSize::Small.default_rows(), 3);
        assert_eq!(TextareaSize::Medium.default_rows(), 4);
        assert_eq!(TextareaSize::Large.default_rows(), 6);

        // Font sizes
        assert_eq!(TextareaSize::Small.font_size(&typography), typography.text_xs);
        assert_eq!(TextareaSize::Medium.font_size(&typography), typography.text_sm);
        assert_eq!(TextareaSize::Large.font_size(&typography), typography.text_base);
    }

    #[test]
    fn test_textarea_builder() {
        init_theme();
        let state = blinc_layout::widgets::text_area::text_area_state();
        let ta = Textarea::new(&state)
            .label("Description")
            .placeholder("Enter description...")
            .rows(5)
            .size(TextareaSize::Large);

        assert_eq!(ta.size, TextareaSize::Large);
        assert_eq!(ta.label, Some("Description".to_string()));
        assert_eq!(ta.rows, Some(5));
    }

    #[test]
    fn test_textarea_wrap_settings() {
        init_theme();
        let state = blinc_layout::widgets::text_area::text_area_state();

        let ta = Textarea::new(&state);
        assert!(ta.wrap); // Default is wrap enabled

        let ta_no_wrap = Textarea::new(&state).no_wrap();
        assert!(!ta_no_wrap.wrap);
    }
}
