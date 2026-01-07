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
use blinc_layout::widgets::text_area::{
    text_area, SharedTextAreaState, TextArea as LayoutTextArea,
};
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
            TextareaSize::Small => typography.text_xs,   // 12px
            TextareaSize::Medium => typography.text_sm,  // 14px
            TextareaSize::Large => typography.text_base, // 16px
        }
    }
}

/// Configuration for building a Textarea
#[derive(Clone)]
struct TextareaConfig {
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

impl TextareaConfig {
    fn new(state: SharedTextAreaState) -> Self {
        Self {
            state,
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
}

/// Styled Textarea component (final built element)
pub struct Textarea {
    /// Inner element containing the complete structure
    inner: Div,
}

impl Textarea {
    /// Build from config
    fn from_config(config: TextareaConfig) -> Self {
        let theme = ThemeState::get();
        let typography = theme.typography();

        // Build the layout TextArea
        let radius = config
            .corner_radius
            .unwrap_or_else(|| theme.radius(RadiusToken::Md));

        let mut ta = text_area(&config.state)
            .font_size(config.size.font_size(&typography))
            .rounded(radius)
            .disabled(config.disabled)
            .wrap(config.wrap);

        // Apply border width if specified
        if let Some(border) = config.border_width {
            ta = ta.border_width(border);
        }

        // Apply rows/cols or use size preset
        if let Some(rows) = config.rows {
            ta = ta.rows(rows);
        } else {
            ta = ta.rows(config.size.default_rows());
        }

        if let Some(cols) = config.cols {
            ta = ta.cols(cols);
        }

        // Apply explicit dimensions if provided
        if let Some(w) = config.width {
            ta = ta.w(w);
        } else if config.full_width {
            ta = ta.w_full();
        }

        if let Some(h) = config.height {
            ta = ta.h(h);
        }

        // Apply placeholder
        if let Some(ref placeholder) = config.placeholder {
            ta = ta.placeholder(placeholder.clone());
        }

        // Apply max length
        if let Some(max) = config.max_length {
            ta = ta.max_length(max);
        }

        // If no label, description, or error, wrap textarea in a div
        let inner =
            if config.label.is_none() && config.description.is_none() && config.error.is_none() {
                div().child(ta)
            } else {
                // Build a container with label, textarea, and description/error
                let spacing = theme.spacing_value(SpacingToken::Space2);
                let mut container = div().flex_col().gap_px(spacing).h_fit();

                // Apply width to container
                if config.full_width {
                    container = container.w_full();
                } else if let Some(w) = config.width {
                    container = container.w(w);
                }

                // Label
                if let Some(ref label_text) = config.label {
                    let mut lbl = label(label_text).size(LabelSize::Medium);
                    if config.required {
                        lbl = lbl.required();
                    }
                    if config.disabled {
                        lbl = lbl.disabled(true);
                    }
                    container = container.child(lbl);
                }

                // Textarea
                container = container.child(ta);

                // Error or description
                if let Some(ref error_text) = config.error {
                    let error_color = theme.color(ColorToken::Error);
                    container = container
                        .child(text(error_text).size(typography.text_xs).color(error_color));
                } else if let Some(ref desc_text) = config.description {
                    let desc_color = theme.color(ColorToken::TextTertiary);
                    container =
                        container.child(text(desc_text).size(typography.text_xs).color(desc_color));
                }

                container
            };

        Self { inner }
    }
}

impl ElementBuilder for Textarea {
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
}

/// Builder for creating Textarea components with fluent API
pub struct TextareaBuilder {
    config: TextareaConfig,
    /// Cached built Textarea - built lazily on first access
    built: std::cell::OnceCell<Textarea>,
}

impl TextareaBuilder {
    /// Create a new textarea builder with the given state
    pub fn new(state: &SharedTextAreaState) -> Self {
        Self {
            config: TextareaConfig::new(state.clone()),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Textarea
    fn get_or_build(&self) -> &Textarea {
        self.built
            .get_or_init(|| Textarea::from_config(self.config.clone()))
    }

    /// Set the textarea size preset
    pub fn size(mut self, size: TextareaSize) -> Self {
        self.config.size = size;
        self
    }

    /// Set number of visible rows (like HTML textarea)
    pub fn rows(mut self, rows: usize) -> Self {
        self.config.rows = Some(rows);
        self
    }

    /// Set number of visible columns (like HTML textarea)
    pub fn cols(mut self, cols: usize) -> Self {
        self.config.cols = Some(cols);
        self
    }

    /// Set a label above the textarea
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Set a description/helper text below the textarea
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.config.description = Some(description.into());
        self
    }

    /// Set an error message (shows in red, replaces description)
    pub fn error(mut self, error: impl Into<String>) -> Self {
        self.config.error = Some(error.into());
        self
    }

    /// Set placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.config.placeholder = Some(placeholder.into());
        self
    }

    /// Set maximum character length
    pub fn max_length(mut self, max: usize) -> Self {
        self.config.max_length = Some(max);
        self
    }

    /// Enable or disable text wrapping (default: true)
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.config.wrap = wrap;
        self
    }

    /// Disable text wrapping
    pub fn no_wrap(mut self) -> Self {
        self.config.wrap = false;
        self
    }

    /// Disable the textarea
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Mark the textarea as required (shows asterisk on label)
    pub fn required(mut self) -> Self {
        self.config.required = true;
        self
    }

    /// Set a fixed width
    pub fn w(mut self, width: f32) -> Self {
        self.config.width = Some(width);
        self.config.full_width = false;
        self
    }

    /// Set a fixed height
    pub fn h(mut self, height: f32) -> Self {
        self.config.height = Some(height);
        self
    }

    /// Make the textarea fill its parent width (default)
    pub fn w_full(mut self) -> Self {
        self.config.full_width = true;
        self.config.width = None;
        self
    }

    /// Set the border width
    pub fn border_width(mut self, width: f32) -> Self {
        self.config.border_width = Some(width);
        self
    }

    /// Set the corner radius
    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.corner_radius = Some(radius);
        self
    }

    /// Build the final Textarea component
    pub fn build_component(self) -> Textarea {
        Textarea::from_config(self.config)
    }
}

impl ElementBuilder for TextareaBuilder {
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

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
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
pub fn textarea(state: &SharedTextAreaState) -> TextareaBuilder {
    TextareaBuilder::new(state)
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
        assert_eq!(
            TextareaSize::Small.font_size(&typography),
            typography.text_xs
        );
        assert_eq!(
            TextareaSize::Medium.font_size(&typography),
            typography.text_sm
        );
        assert_eq!(
            TextareaSize::Large.font_size(&typography),
            typography.text_base
        );
    }

    #[test]
    fn test_textarea_builder() {
        init_theme();
        let state = blinc_layout::widgets::text_area::text_area_state();
        let ta = TextareaBuilder::new(&state)
            .label("Description")
            .placeholder("Enter description...")
            .rows(5)
            .size(TextareaSize::Large);

        assert_eq!(ta.config.size, TextareaSize::Large);
        assert_eq!(ta.config.label, Some("Description".to_string()));
        assert_eq!(ta.config.rows, Some(5));
    }

    #[test]
    fn test_textarea_wrap_settings() {
        init_theme();
        let state = blinc_layout::widgets::text_area::text_area_state();

        let ta = TextareaBuilder::new(&state);
        assert!(ta.config.wrap); // Default is wrap enabled

        let ta_no_wrap = TextareaBuilder::new(&state).no_wrap();
        assert!(!ta_no_wrap.config.wrap);
    }
}
