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
//! // Custom idle colors, as inline style. Hover, focus and error looks are
//! // `.cn-input` rules: restyle them with rules of your own.
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

use super::label::{LabelSize, label};

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
    pub fn height(&self, theme: &ThemeState) -> f32 {
        // Use spacing tokens for consistent sizing
        match self {
            InputSize::Small => theme.spacing_value(SpacingToken::Space8), // 32px
            InputSize::Medium => theme.spacing_value(SpacingToken::Space10), // 40px
            InputSize::Large => theme.spacing_value(SpacingToken::Space12), // 48px
        }
    }

    pub fn font_size(&self, typography: &TypographyTokens) -> f32 {
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

        // Build the text input element
        let text_input = Self::build_text_input(&config, theme, &typography);

        // Determine the size-specific class
        let size_class = match config.size {
            InputSize::Small => "cn-input--sm",
            InputSize::Medium => "cn-input--md",
            InputSize::Large => "cn-input--lg",
        };

        // Add CSS classes directly to the TextInput (it supports .class())
        let mut text_input = text_input
            .class("cn-input")
            .class("tabindex=text")
            .class(size_class);
        if let Some(label) = config.label.as_ref() {
            text_input =
                text_input.accessibility_label(crate::reactive_props::clone_reactive(label));
        }
        // An error marks the box `data-invalid` while it is non-empty.
        if let Some(ref error) = config.error {
            text_input = text_input.invalid(crate::reactive_props::non_empty(error));
        }

        // If no label, description, or error, return the input directly in a minimal container
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
        let mut container = div()
            .class("cn-input-group")
            .flex_col()
            .gap_px(spacing)
            .h_fit();

        // Apply width to container
        if config.full_width {
            container = container.w_full();
        } else if let Some(w) = config.width {
            container = container.w(w);
        }

        // Label (reuses Label component)
        if let Some(label_text) = config.label.clone() {
            let mut lbl = label(label_text)
                .size(LabelSize::Medium)
                .disabled(config.disabled.clone());
            if config.required {
                lbl = lbl.required();
            }
            container = container.child(lbl);
        }

        // Input added directly (no wrapper div — TextInput has the cn-input class)
        container = container.child(text_input);

        // The error while it is non-empty, else the description.
        if let Some(message) = crate::reactive_props::field_message(
            &config.error,
            &config.description,
            typography.text_xs,
        ) {
            container = container.child(message);
        }

        Self { inner: container }
    }

    /// Build the text input element from config
    fn build_text_input(
        config: &InputConfig,
        theme: &ThemeState,
        typography: &TypographyTokens,
    ) -> TextInput {
        let radius = config
            .corner_radius
            .unwrap_or_else(|| theme.radius(RadiusToken::Md));

        let mut input = blinc_layout::widgets::text_input::text_input(&config.data);
        if let Some(input_ref) = &config.input_ref {
            input = input.bind(input_ref);
        }
        let mut input = input
            .h(config.size.height(theme))
            .text_size(config.size.font_size(typography))
            .rounded(radius)
            .input_type(config.input_type)
            .disabled(config.disabled.clone())
            .masked(config.password);

        // The box's look is the `.cn-input` rules; only an idle colour the
        // caller set is applied, as inline style.
        if let Some(color) = config.border_color {
            input = input.idle_border_color(color);
        }
        if let Some(color) = config.bg_color {
            input = input.idle_bg_color(color);
        }

        // The field's own theme colours unless the caller set one.
        if let Some(color) = config.text_color {
            input = input.text_color(color);
        }
        if let Some(color) = config.placeholder_color {
            input = input.placeholder_color(color);
        }
        if let Some(color) = config.cursor_color {
            input = input.cursor_color(color);
        }
        if let Some(color) = config.selection_color {
            input = input.selection_color(color);
        }

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

        if let Some(callback) = config.on_submit.clone() {
            input = input.on_submit(move |shift| callback(shift));
        }

        input
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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }

    // Forward layout_style / event_handlers / element_id to the inner
    // div so user-set width / height / event handlers / id reach the
    // renderer. Without these forwards the default trait impls return
    // `None` / `&[]`, so a `cn::input(&data).w(200.0)` chain sets
    // `config.width = Some(200.0)` and builds the inner div with the
    // correct taffy width — but the parent's layout query never sees
    // it and the input still stretches to fill its flex container.
    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.inner)
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(&self.inner)
    }
}

/// Internal configuration for building an Input
#[derive(Clone)]
struct InputConfig {
    data: SharedTextInputData,
    /// Caller's handle onto this field, if bound.
    input_ref: Option<blinc_layout::selector::InputRef>,
    size: InputSize,
    label: Option<blinc_layout::binding::Reactive<String>>,
    description: Option<blinc_layout::binding::Reactive<String>>,
    error: Option<blinc_layout::binding::Reactive<String>>,
    disabled: blinc_layout::binding::Reactive<bool>,
    required: bool,
    input_type: InputType,
    placeholder: Option<blinc_layout::binding::Reactive<String>>,
    password: bool,
    width: Option<f32>,
    full_width: bool,
    border_color: Option<Color>,
    bg_color: Option<Color>,
    text_color: Option<Color>,
    placeholder_color: Option<Color>,
    cursor_color: Option<Color>,
    selection_color: Option<Color>,
    border_width: Option<f32>,
    corner_radius: Option<f32>,
    on_change: Option<OnChangeCallback>,
    on_submit: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            data: blinc_layout::widgets::text_input::text_input_data(),
            input_ref: None,
            size: InputSize::default(),
            label: None,
            description: None,
            error: None,
            disabled: blinc_layout::binding::Reactive::Const(false),
            required: false,
            input_type: InputType::Text,
            placeholder: None,
            password: false,
            width: None,
            full_width: true,
            border_color: None,
            bg_color: None,
            text_color: None,
            placeholder_color: None,
            cursor_color: None,
            selection_color: None,
            border_width: None,
            corner_radius: None,
            on_change: None,
            on_submit: None,
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
        ::blinc_layout::build_once::build_once(&self.built, || {
            Input::with_config(self.config.clone())
        })
    }

    /// Set the input size
    pub fn size(mut self, size: InputSize) -> Self {
        self.config.size = size;
        self
    }

    /// Set a label above the input
    pub fn label(mut self, label: impl blinc_layout::binding::IntoReactive<String>) -> Self {
        self.config.label = Some(label.into_reactive());
        self
    }

    /// Set a description/helper text below the input. A signal here
    /// replaces the text in place.
    pub fn description(
        mut self,
        description: impl blinc_layout::binding::IntoReactive<String>,
    ) -> Self {
        self.config.description = Some(description.into_reactive());
        self
    }

    /// Set an error message: shown in place of the description, and the box
    /// carries `data-invalid`, while it is non-empty. A signal here is
    /// followed in place, so an empty one clears the error.
    pub fn error(mut self, error: impl blinc_layout::binding::IntoReactive<String>) -> Self {
        self.config.error = Some(error.into_reactive());
        self
    }

    /// Set the input type for validation
    /// Bind an [`blinc_layout::selector::InputRef`] to this field.
    ///
    /// The ref reads and writes the same state the field is built from,
    /// so `value()` and `set_value()` work without waiting for a render.
    pub fn bind(mut self, input_ref: &blinc_layout::selector::InputRef) -> Self {
        self.config.input_ref = Some(input_ref.clone());
        self
    }

    pub fn input_type(mut self, input_type: InputType) -> Self {
        self.config.input_type = input_type;
        self
    }

    /// Set placeholder text. A signal here replaces the text in place.
    pub fn placeholder(
        mut self,
        placeholder: impl blinc_layout::binding::IntoReactive<String>,
    ) -> Self {
        self.config.placeholder = Some(placeholder.into_reactive());
        self
    }

    /// Make this a password input (masked)
    pub fn password(mut self) -> Self {
        self.config.password = true;
        self.config.input_type = InputType::Password;
        self
    }

    /// Disable the input
    /// Whether it is disabled: a bool, Signal or Computed.
    pub fn disabled(mut self, disabled: impl blinc_layout::binding::IntoReactive<bool>) -> Self {
        self.config.disabled = disabled.into_reactive();
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

    // ========== Box Colour Customization ==========

    /// Set the idle border color, as inline style. How the border looks while
    /// hovered, focused or in error is the `.cn-input` rules.
    pub fn idle_border_color(mut self, color: impl Into<Color>) -> Self {
        self.config.border_color = Some(color.into());
        self
    }

    /// Set the idle background color, as inline style.
    pub fn idle_bg_color(mut self, color: impl Into<Color>) -> Self {
        self.config.bg_color = Some(color.into());
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

    /// Run when Enter is pressed, receiving whether Shift was held.
    pub fn on_submit(mut self, callback: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.config.on_submit = Some(Arc::new(callback));
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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(self.get_or_build())
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(self.get_or_build())
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(self.get_or_build())
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
        assert!(InputSize::Small.height(theme) > 0.0);
        assert!(InputSize::Medium.height(theme) > InputSize::Small.height(theme));
        assert!(InputSize::Large.height(theme) > InputSize::Medium.height(theme));

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
