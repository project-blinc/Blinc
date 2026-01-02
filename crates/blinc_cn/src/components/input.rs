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

use blinc_core::Color;
use blinc_layout::prelude::*;
use blinc_layout::widgets::text_input::{InputType, SharedTextInputData, TextInput};
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState, TypographyTokens};

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
            InputSize::Small => typography.text_xs,  // 12px
            InputSize::Medium => typography.text_sm, // 14px
            InputSize::Large => typography.text_base, // 16px
        }
    }
}

/// Styled Input component
pub struct Input {
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
    // Customization options
    border_colors: InputBorderColors,
    bg_colors: InputBgColors,
    text_color: Option<Color>,
    placeholder_color: Option<Color>,
    cursor_color: Option<Color>,
    selection_color: Option<Color>,
    border_width: Option<f32>,
    corner_radius: Option<f32>,
}

impl Input {
    /// Create a new input with the given data state
    pub fn new(data: &SharedTextInputData) -> Self {
        Self {
            data: data.clone(),
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
            full_width: true, // Default to full width like HTML inputs
            border_colors: InputBorderColors::default(),
            bg_colors: InputBgColors::default(),
            text_color: None,
            placeholder_color: None,
            cursor_color: None,
            selection_color: None,
            border_width: None,
            corner_radius: None,
        }
    }

    /// Set the input size
    pub fn size(mut self, size: InputSize) -> Self {
        self.size = size;
        self
    }

    /// Set a label above the input
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set a description/helper text below the input
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set an error message (shows in red, replaces description)
    pub fn error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Set the input type for validation
    pub fn input_type(mut self, input_type: InputType) -> Self {
        self.input_type = input_type;
        self
    }

    /// Set placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Make this a password input (masked)
    pub fn password(mut self) -> Self {
        self.password = true;
        self.input_type = InputType::Password;
        self
    }

    /// Disable the input
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Mark the input as required (shows asterisk on label)
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

    /// Make the input fill its parent width (default)
    pub fn w_full(mut self) -> Self {
        self.full_width = true;
        self.width = None;
        self
    }

    // ========== Border Color Customization ==========

    /// Set the idle border color (when not hovered or focused)
    pub fn idle_border_color(mut self, color: impl Into<Color>) -> Self {
        self.border_colors.idle = Some(color.into());
        self
    }

    /// Set the hover border color
    pub fn hover_border_color(mut self, color: impl Into<Color>) -> Self {
        self.border_colors.hover = Some(color.into());
        self
    }

    /// Set the focused border color
    pub fn focused_border_color(mut self, color: impl Into<Color>) -> Self {
        self.border_colors.focused = Some(color.into());
        self
    }

    /// Set the error border color
    pub fn error_border_color(mut self, color: impl Into<Color>) -> Self {
        self.border_colors.error = Some(color.into());
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
        self.border_colors = InputBorderColors {
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
        self.bg_colors.idle = Some(color.into());
        self
    }

    /// Set the hover background color
    pub fn hover_bg_color(mut self, color: impl Into<Color>) -> Self {
        self.bg_colors.hover = Some(color.into());
        self
    }

    /// Set the focused background color
    pub fn focused_bg_color(mut self, color: impl Into<Color>) -> Self {
        self.bg_colors.focused = Some(color.into());
        self
    }

    /// Set all background colors at once
    pub fn bg_colors(
        mut self,
        idle: impl Into<Color>,
        hover: impl Into<Color>,
        focused: impl Into<Color>,
    ) -> Self {
        self.bg_colors = InputBgColors {
            idle: Some(idle.into()),
            hover: Some(hover.into()),
            focused: Some(focused.into()),
        };
        self
    }

    // ========== Text Color Customization ==========

    /// Set the text color
    pub fn text_color(mut self, color: impl Into<Color>) -> Self {
        self.text_color = Some(color.into());
        self
    }

    /// Set the placeholder text color
    pub fn placeholder_color(mut self, color: impl Into<Color>) -> Self {
        self.placeholder_color = Some(color.into());
        self
    }

    /// Set the cursor color
    pub fn cursor_color(mut self, color: impl Into<Color>) -> Self {
        self.cursor_color = Some(color.into());
        self
    }

    /// Set the selection highlight color
    pub fn selection_color(mut self, color: impl Into<Color>) -> Self {
        self.selection_color = Some(color.into());
        self
    }

    // ========== Other Customization ==========

    /// Set the border width
    pub fn border_width(mut self, width: f32) -> Self {
        self.border_width = Some(width);
        self
    }

    /// Set the corner radius (overrides size-based radius)
    pub fn rounded(mut self, radius: f32) -> Self {
        self.corner_radius = Some(radius);
        self
    }

    /// Build the input element
    fn build_input(&self) -> TextInput {
        let theme = ThemeState::get();
        let typography = theme.typography();

        // Sync error state to underlying data's is_valid field
        // This makes the input show error border styling
        if self.error.is_some() {
            if let Ok(mut data) = self.data.lock() {
                data.is_valid = false;
            }
        }

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

        let radius = self.corner_radius.unwrap_or_else(|| theme.radius(RadiusToken::Md));

        let mut input = blinc_layout::widgets::text_input::text_input(&self.data)
            .h(self.size.height(&theme))
            .text_size(self.size.font_size(&typography))
            .rounded(radius)
            .input_type(self.input_type)
            .disabled(self.disabled)
            .masked(self.password);

        // Apply border colors (user overrides or theme defaults)
        input = input.idle_border_color(self.border_colors.idle.unwrap_or(default_border));
        input = input.hover_border_color(self.border_colors.hover.unwrap_or(default_border_hover));
        input = input.focused_border_color(self.border_colors.focused.unwrap_or(default_border_focus));
        input = input.error_border_color(self.border_colors.error.unwrap_or(default_border_error));

        // Apply background colors
        input = input.idle_bg_color(self.bg_colors.idle.unwrap_or(default_bg));
        input = input.hover_bg_color(self.bg_colors.hover.unwrap_or(default_bg_hover));
        input = input.focused_bg_color(self.bg_colors.focused.unwrap_or(default_bg_focus));

        // Apply text colors
        input = input.text_color(self.text_color.unwrap_or(default_text));
        input = input.placeholder_color(self.placeholder_color.unwrap_or(default_placeholder));
        input = input.cursor_color(self.cursor_color.unwrap_or(default_cursor));
        input = input.selection_color(self.selection_color.unwrap_or(default_selection));

        // Apply border width if specified
        if let Some(width) = self.border_width {
            input = input.border_width(width);
        }

        if let Some(ref placeholder) = self.placeholder {
            input = input.placeholder(placeholder.clone());
        }

        // Apply width
        if self.full_width {
            input = input.w_full();
        } else if let Some(w) = self.width {
            input = input.w(w);
        }

        input
    }
}

impl ElementBuilder for Input {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
        let theme = ThemeState::get();

        // If no label, description, or error, just return the input directly
        if self.label.is_none() && self.description.is_none() && self.error.is_none() {
            return self.build_input().build(tree);
        }

        // Build a container with label, input, and description/error
        let spacing = theme.spacing_value(SpacingToken::Space2);
        let mut container = div().flex_col().gap(spacing);

        // Apply width to container
        if self.full_width {
            container = container.w_full();
        } else if let Some(w) = self.width {
            container = container.w(w);
        }

        let typography = theme.typography();

        // Label (reuses Label component)
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

        // Input
        container = container.child(self.build_input());

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
            self.build_input().render_props()
        }
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        blinc_layout::div::ElementTypeId::Div
    }
}

/// Create a styled input component
pub fn input(data: &SharedTextInputData) -> Input {
    Input::new(data)
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
        assert_eq!(InputSize::Large.font_size(&typography), typography.text_base);
    }

    #[test]
    fn test_input_builder() {
        init_theme();
        let data = blinc_layout::widgets::text_input::text_input_data();
        let input = Input::new(&data)
            .label("Username")
            .placeholder("Enter username")
            .size(InputSize::Large);

        assert_eq!(input.size, InputSize::Large);
        assert_eq!(input.label, Some("Username".to_string()));
    }
}
