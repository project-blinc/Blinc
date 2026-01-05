//! Input component - styled text input following shadcn/ui patterns
//!
//! A themed wrapper around `blinc_layout::TextInput` with consistent styling,
//! size variants, and optional label/error message support.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Basic input
//! let username = text_input_data();
//! cn::input(&username)
//!     .placeholder("Enter username")
//!
//! // With label and required field
//! cn::input(&email)
//!     .label("Email")
//!     .placeholder("you@example.com")
//!     .required()
//!
//! // Password input
//! cn::input(&password)
//!     .label("Password")
//!     .password()
//!
//! // With error (shows red border and error message)
//! cn::input(&field)
//!     .label("Username")
//!     .error("Username is required")
//!
//! // Custom border colors
//! cn::input(&custom)
//!     .focused_border_color(Color::BLUE)
//!     .error_border_color(Color::RED)
//!
//! // Custom background and text colors
//! cn::input(&dark)
//!     .idle_bg_color(Color::rgba(0.1, 0.1, 0.1, 1.0))
//!     .text_color(Color::WHITE)
//!     .placeholder_color(Color::rgba(0.5, 0.5, 0.5, 1.0))
//! ```

use std::sync::Arc;

use blinc_core::Color;
use blinc_layout::div::ElementTypeId;
use blinc_layout::prelude::*;
use blinc_layout::widgets::text_input::{
    InputType, OnChangeCallback, SharedTextInputData, TextInput,
};
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState, TypographyTokens};
use std::ops::{Deref, DerefMut};

use super::label::{label, LabelSize};

/// Border color configuration for different input states
#[derive(Clone, Debug)]
pub struct InputBorderColors {
    /// Border color when idle (not hovered or focused)
    pub idle: Option<Color>,
    /// Border color when hovered
    pub hover: Option<Color>,
    /// Border color when focused
    pub focused: Option<Color>,
    /// Border color when in error state
    pub error: Option<Color>,
}

impl Default for InputBorderColors {
    fn default() -> Self {
        Self {
            idle: None,
            hover: None,
            focused: None,
            error: None,
        }
    }
}

/// Background color configuration for different input states
#[derive(Clone, Debug, Default)]
pub struct InputBgColors {
    /// Background color when idle
    pub idle: Option<Color>,
    /// Background color when hovered
    pub hover: Option<Color>,
    /// Background color when focused
    pub focused: Option<Color>,
}

/// Input size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputSize {
    /// Small input (height: 32px, font: 13px)
    Small,
    /// Medium/default input (height: 40px, font: 14px)
    #[default]
    Medium,
    /// Large input (height: 48px, font: 16px)
    Large,
}

impl InputSize {
    fn height(&self, theme: &ThemeState) -> f32 {
        // Use spacing tokens for consistent sizing
        match self {
            InputSize::Small => theme.spacing_value(SpacingToken::Space8), // 32px
            InputSize::Medium => theme.spacing_value(SpacingToken::Space10), // 40px
            InputSize::Large => theme.spacing_value(SpacingToken::Space12), // 48px
        }
    }

    fn font_size(&self, typography: &TypographyTokens) -> f32 {
        match self {
            InputSize::Small => typography.text_xs,   // 12px
            InputSize::Medium => typography.text_sm,  // 14px
            InputSize::Large => typography.text_base, // 16px
        }
    }
}

/// Styled Input component
///
/// Wraps a Div that contains label, input field, and description/error.
pub struct Input {
    /// The fully-built inner element (Div container with children)
    inner: Div,
}

impl Input {
    /// Create a new input with the given data state
    pub fn new(data: &SharedTextInputData) -> Self {
        Self::with_config(InputConfig {
            data: data.clone(),
            ..Default::default()
        })
    }

    /// Create from a full configuration
    fn with_config(config: InputConfig) -> Self {
        let theme = ThemeState::get();
        let typography = theme.typography();

        // Sync error state to underlying data's is_valid field
        if config.error.is_some() {
            if let Ok(mut data) = config.data.lock() {
                data.is_valid = false;
            }
        }

        // Build the text input element
        let text_input = Self::build_text_input(&config, &theme, &typography);

        // If no label, description, or error, just wrap the input in a div
        if config.label.is_none() && config.description.is_none() && config.error.is_none() {
            let mut container = div().child(text_input);
            if config.full_width {
                container = container.w_full();
            } else if let Some(w) = config.width {
                container = container.w(w);
            }
            return Self { inner: container };
        }

        // Build a container with label, input, and description/error
        let spacing = theme.spacing_value(SpacingToken::Space2);
        let mut container = div().flex_col().gap_px(spacing);

        // Apply width to container
        if config.full_width {
            container = container.w_full();
        } else if let Some(w) = config.width {
            container = container.w(w);
        }

        // Label (reuses Label component)
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

        // Input
        container = container.child(text_input);

        // Error or description
        if let Some(ref error_text) = config.error {
            let error_color = theme.color(ColorToken::Error);
            container =
                container.child(text(error_text).size(typography.text_xs).color(error_color));
        } else if let Some(ref desc_text) = config.description {
            let desc_color = theme.color(ColorToken::TextTertiary);
            container = container.child(text(desc_text).size(typography.text_xs).color(desc_color));
        }

        Self { inner: container }
    }

    /// Build the text input element from config
    fn build_text_input(
        config: &InputConfig,
        theme: &ThemeState,
        typography: &TypographyTokens,
    ) -> TextInput {
        // Get default theme colors for fallbacks
        let default_border = theme.color(ColorToken::Border);
        let default_border_hover = theme.color(ColorToken::BorderHover);
        let default_border_focus = theme.color(ColorToken::BorderFocus);
        let default_border_error = theme.color(ColorToken::BorderError);
        let default_bg = theme.color(ColorToken::InputBg);
        let default_bg_hover = theme.color(ColorToken::InputBgHover);
        let default_bg_focus = theme.color(ColorToken::InputBgFocus);
        let default_text = theme.color(ColorToken::TextPrimary);
        let default_placeholder = theme.color(ColorToken::TextTertiary);
        let default_cursor = theme.color(ColorToken::Primary);
        let default_selection = theme.color(ColorToken::Selection);

        let radius = config
            .corner_radius
            .unwrap_or_else(|| theme.radius(RadiusToken::Md));

        let mut input = blinc_layout::widgets::text_input::text_input(&config.data)
            .h(config.size.height(theme))
            .text_size(config.size.font_size(typography))
            .rounded(radius)
            .input_type(config.input_type)
            .disabled(config.disabled)
            .masked(config.password);

        // Apply border colors (user overrides or theme defaults)
        input = input.idle_border_color(config.border_colors.idle.unwrap_or(default_border));
        input =
            input.hover_border_color(config.border_colors.hover.unwrap_or(default_border_hover));
        input = input
            .focused_border_color(config.border_colors.focused.unwrap_or(default_border_focus));
        input =
            input.error_border_color(config.border_colors.error.unwrap_or(default_border_error));

        // Apply background colors
        input = input.idle_bg_color(config.bg_colors.idle.unwrap_or(default_bg));
        input = input.hover_bg_color(config.bg_colors.hover.unwrap_or(default_bg_hover));
        input = input.focused_bg_color(config.bg_colors.focused.unwrap_or(default_bg_focus));

        // Apply text colors
        input = input.text_color(config.text_color.unwrap_or(default_text));
        input = input.placeholder_color(config.placeholder_color.unwrap_or(default_placeholder));
        input = input.cursor_color(config.cursor_color.unwrap_or(default_cursor));
        input = input.selection_color(config.selection_color.unwrap_or(default_selection));

        // Apply border width if specified
        if let Some(width) = config.border_width {
            input = input.border_width(width);
        }

        if let Some(ref placeholder) = config.placeholder {
            input = input.placeholder(placeholder.clone());
        }

        // Apply width
        if config.full_width {
            input = input.w_full();
        } else if let Some(w) = config.width {
            input = input.w(w);
        }

        // Apply on_change callback
        if let Some(ref callback) = config.on_change {
            input = input.on_change({
                let cb = Arc::clone(callback);
                move |value: &str| cb(value)
            });
        }

        input
    }
}

impl Deref for Input {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Input {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for Input {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }
}

/// Internal configuration for building an Input
#[derive(Clone)]
struct InputConfig {
    data: SharedTextInputData,
    size: InputSize,
    label: Option<String>,
    description: Option<String>,
    error: Option<String>,
    disabled: bool,
    required: bool,
    input_type: InputType,
    placeholder: Option<String>,
    password: bool,
    width: Option<f32>,
    full_width: bool,
    border_colors: InputBorderColors,
    bg_colors: InputBgColors,
    text_color: Option<Color>,
    placeholder_color: Option<Color>,
    cursor_color: Option<Color>,
    selection_color: Option<Color>,
    border_width: Option<f32>,
    corner_radius: Option<f32>,
    on_change: Option<OnChangeCallback>,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            data: blinc_layout::widgets::text_input::text_input_data(),
            size: InputSize::default(),
            label: None,
            description: None,
            error: None,
            disabled: false,
            required: false,
            input_type: InputType::Text,
            placeholder: None,
            password: false,
            width: None,
            full_width: true,
            border_colors: InputBorderColors::default(),
            bg_colors: InputBgColors::default(),
            text_color: None,
            placeholder_color: None,
            cursor_color: None,
            selection_color: None,
            border_width: None,
            corner_radius: None,
            on_change: None,
        }
    }
}

/// Builder for creating Input components with fluent API
///
/// The inner Input is built lazily when first needed and cached.
pub struct InputBuilder {
    config: InputConfig,
    /// Cached built Input - built lazily on first access
    built: std::cell::OnceCell<Input>,
}

impl InputBuilder {
    /// Create a new input builder with the given data state
    pub fn new(data: &SharedTextInputData) -> Self {
        Self {
            config: InputConfig {
                data: data.clone(),
                ..Default::default()
            },
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Input
    fn get_or_build(&self) -> &Input {
        self.built
            .get_or_init(|| Input::with_config(self.config.clone()))
    }

    /// Set the input size
    pub fn size(mut self, size: InputSize) -> Self {
        self.config.size = size;
        self
    }

    /// Set a label above the input
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Set a description/helper text below the input
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.config.description = Some(description.into());
        self
    }

    /// Set an error message (shows in red, replaces description)
    pub fn error(mut self, error: impl Into<String>) -> Self {
        self.config.error = Some(error.into());
        self
    }

    /// Set the input type for validation
    pub fn input_type(mut self, input_type: InputType) -> Self {
        self.config.input_type = input_type;
        self
    }

    /// Set placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.config.placeholder = Some(placeholder.into());
        self
    }

    /// Make this a password input (masked)
    pub fn password(mut self) -> Self {
        self.config.password = true;
        self.config.input_type = InputType::Password;
        self
    }

    /// Disable the input
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Mark the input as required (shows asterisk on label)
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

    /// Make the input fill its parent width (default)
    pub fn w_full(mut self) -> Self {
        self.config.full_width = true;
        self.config.width = None;
        self
    }

    // ========== Border Color Customization ==========

    /// Set the idle border color (when not hovered or focused)
    pub fn idle_border_color(mut self, color: impl Into<Color>) -> Self {
        self.config.border_colors.idle = Some(color.into());
        self
    }

    /// Set the hover border color
    pub fn hover_border_color(mut self, color: impl Into<Color>) -> Self {
        self.config.border_colors.hover = Some(color.into());
        self
    }

    /// Set the focused border color
    pub fn focused_border_color(mut self, color: impl Into<Color>) -> Self {
        self.config.border_colors.focused = Some(color.into());
        self
    }

    /// Set the error border color
    pub fn error_border_color(mut self, color: impl Into<Color>) -> Self {
        self.config.border_colors.error = Some(color.into());
        self
    }

    /// Set all border colors at once
    pub fn border_colors(
        mut self,
        idle: impl Into<Color>,
        hover: impl Into<Color>,
        focused: impl Into<Color>,
        error: impl Into<Color>,
    ) -> Self {
        self.config.border_colors = InputBorderColors {
            idle: Some(idle.into()),
            hover: Some(hover.into()),
            focused: Some(focused.into()),
            error: Some(error.into()),
        };
        self
    }

    // ========== Background Color Customization ==========

    /// Set the idle background color
    pub fn idle_bg_color(mut self, color: impl Into<Color>) -> Self {
        self.config.bg_colors.idle = Some(color.into());
        self
    }

    /// Set the hover background color
    pub fn hover_bg_color(mut self, color: impl Into<Color>) -> Self {
        self.config.bg_colors.hover = Some(color.into());
        self
    }

    /// Set the focused background color
    pub fn focused_bg_color(mut self, color: impl Into<Color>) -> Self {
        self.config.bg_colors.focused = Some(color.into());
        self
    }

    /// Set all background colors at once
    pub fn bg_colors(
        mut self,
        idle: impl Into<Color>,
        hover: impl Into<Color>,
        focused: impl Into<Color>,
    ) -> Self {
        self.config.bg_colors = InputBgColors {
            idle: Some(idle.into()),
            hover: Some(hover.into()),
            focused: Some(focused.into()),
        };
        self
    }

    // ========== Text Color Customization ==========

    /// Set the text color
    pub fn text_color(mut self, color: impl Into<Color>) -> Self {
        self.config.text_color = Some(color.into());
        self
    }

    /// Set the placeholder text color
    pub fn placeholder_color(mut self, color: impl Into<Color>) -> Self {
        self.config.placeholder_color = Some(color.into());
        self
    }

    /// Set the cursor color
    pub fn cursor_color(mut self, color: impl Into<Color>) -> Self {
        self.config.cursor_color = Some(color.into());
        self
    }

    /// Set the selection highlight color
    pub fn selection_color(mut self, color: impl Into<Color>) -> Self {
        self.config.selection_color = Some(color.into());
        self
    }

    // ========== Other Customization ==========

    /// Set the border width
    pub fn border_width(mut self, width: f32) -> Self {
        self.config.border_width = Some(width);
        self
    }

    /// Set the corner radius (overrides size-based radius)
    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.corner_radius = Some(radius);
        self
    }

    /// Set the callback to be invoked when the text value changes
    ///
    /// The callback receives the new text value as a string slice.
    /// This is called after insert or delete operations modify the text.
    ///
    /// # Example
    ///
    /// ```ignore
    /// cn::input(&data)
    ///     .on_change(|new_value| {
    ///         println!("Text changed to: {}", new_value);
    ///     })
    /// ```
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(callback));
        self
    }

    /// Build the final Input component
    pub fn build_component(self) -> Input {
        Input::with_config(self.config)
    }
}

impl ElementBuilder for InputBuilder {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().element_type_id()
    }
}

/// Create a styled input component
pub fn input(data: &SharedTextInputData) -> InputBuilder {
    InputBuilder::new(data)
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
    fn test_input_size_values() {
        init_theme();
        let theme = ThemeState::get();
        let typography = TypographyTokens::default();

        // Sizes use spacing tokens
        assert!(InputSize::Small.height(&theme) > 0.0);
        assert!(InputSize::Medium.height(&theme) > InputSize::Small.height(&theme));
        assert!(InputSize::Large.height(&theme) > InputSize::Medium.height(&theme));

        // Font sizes use typography tokens
        assert_eq!(InputSize::Small.font_size(&typography), typography.text_xs);
        assert_eq!(InputSize::Medium.font_size(&typography), typography.text_sm);
        assert_eq!(
            InputSize::Large.font_size(&typography),
            typography.text_base
        );
    }

    #[test]
    fn test_input_builder() {
        init_theme();
        let data = blinc_layout::widgets::text_input::text_input_data();
        let _input = input(&data)
            .label("Username")
            .placeholder("Enter username")
            .size(InputSize::Large);
    }
}
