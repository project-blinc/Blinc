//! Button component with shadcn-style variants
//!
//! A themed button component built on `blinc_layout::Stateful<ButtonState>`.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Primary button (default)
//! cn::button("Click me")
//!
//! // Destructive button
//! cn::button("Delete")
//!     .variant(ButtonVariant::Destructive)
//!
//! // Outline button with custom size
//! cn::button("Cancel")
//!     .variant(ButtonVariant::Outline)
//!     .size(ButtonSize::Small)
//!
//! // Ghost button (minimal styling)
//! cn::button("More")
//!     .variant(ButtonVariant::Ghost)
//!
//! // Button with click handler and custom margin
//! cn::button("Submit")
//!     .on_click(|_| println!("Submitted!"))
//!     .m(8.0)  // All Stateful/Div methods are available
//!
//! // Custom styling via Deref to Stateful<ButtonState>
//! cn::button("Custom")
//!     .shadow_lg()
//!     .gap(8.0)
//! ```

use std::ops::{Deref, DerefMut};

use blinc_core::{Color, Transform};
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, Stateful};
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

/// Button visual variants (like shadcn)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Primary action button - filled with primary color
    #[default]
    Primary,
    /// Secondary action - muted background
    Secondary,
    /// Destructive action - red/danger styling
    Destructive,
    /// Outline button - border only, transparent background
    Outline,
    /// Ghost button - no background, minimal styling
    Ghost,
    /// Link button - appears as a link, no button styling
    Link,
}

impl ButtonVariant {
    /// Get the background color for this variant and state
    fn background(&self, theme: &ThemeState, state: ButtonState) -> Color {
        match (self, state) {
            // Disabled state
            (_, ButtonState::Disabled) => {
                let base = self.base_background(theme);
                base.with_alpha(0.5)
            }
            // Pressed state
            (ButtonVariant::Primary, ButtonState::Pressed) => {
                theme.color(ColorToken::PrimaryActive)
            }
            (ButtonVariant::Secondary, ButtonState::Pressed) => {
                theme.color(ColorToken::SecondaryActive)
            }
            (ButtonVariant::Destructive, ButtonState::Pressed) => {
                darken(theme.color(ColorToken::Error), 0.15)
            }
            (ButtonVariant::Outline | ButtonVariant::Ghost, ButtonState::Pressed) => {
                theme.color(ColorToken::TextPrimary).with_alpha(0.1)
            }
            (ButtonVariant::Link, ButtonState::Pressed) => Color::TRANSPARENT,
            // Hovered state
            (ButtonVariant::Primary, ButtonState::Hovered) => theme.color(ColorToken::PrimaryHover),
            (ButtonVariant::Secondary, ButtonState::Hovered) => {
                theme.color(ColorToken::SecondaryHover)
            }
            (ButtonVariant::Destructive, ButtonState::Hovered) => {
                darken(theme.color(ColorToken::Error), 0.1)
            }
            (ButtonVariant::Outline | ButtonVariant::Ghost, ButtonState::Hovered) => {
                theme.color(ColorToken::TextPrimary).with_alpha(0.05)
            }
            (ButtonVariant::Link, ButtonState::Hovered) => Color::TRANSPARENT,
            // Idle state (default)
            _ => self.base_background(theme),
        }
    }

    fn base_background(&self, theme: &ThemeState) -> Color {
        match self {
            ButtonVariant::Primary => theme.color(ColorToken::Primary),
            ButtonVariant::Secondary => theme.color(ColorToken::Secondary),
            ButtonVariant::Destructive => theme.color(ColorToken::Error),
            ButtonVariant::Outline | ButtonVariant::Ghost | ButtonVariant::Link => {
                Color::TRANSPARENT
            }
        }
    }

    /// Get the foreground (text) color for this variant
    fn foreground(&self, theme: &ThemeState) -> Color {
        match self {
            ButtonVariant::Primary | ButtonVariant::Secondary | ButtonVariant::Destructive => {
                theme.color(ColorToken::TextInverse)
            }
            ButtonVariant::Outline | ButtonVariant::Ghost => theme.color(ColorToken::TextPrimary),
            ButtonVariant::Link => theme.color(ColorToken::TextLink),
        }
    }

    /// Get the border color (if any) for this variant
    fn border(&self, theme: &ThemeState) -> Option<Color> {
        match self {
            ButtonVariant::Outline => Some(theme.color(ColorToken::Border)),
            _ => None,
        }
    }
}

/// Button size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    /// Small button - compact UI
    Small,
    /// Medium button - default size
    #[default]
    Medium,
    /// Large button - prominent actions
    Large,
    /// Icon-only button - square with icon
    Icon,
}

impl ButtonSize {
    /// Get the height for this size using theme tokens
    fn height(&self, theme: &ThemeState) -> f32 {
        match self {
            ButtonSize::Small => theme.spacing_value(SpacingToken::Space8), // 32px
            ButtonSize::Medium => theme.spacing_value(SpacingToken::Space10), // 40px
            ButtonSize::Large => theme.spacing_value(SpacingToken::Space12), // 48px
            ButtonSize::Icon => theme.spacing_value(SpacingToken::Space10), // 40px
        }
    }

    /// Get the horizontal padding
    fn padding_x(&self, theme: &ThemeState) -> f32 {
        match self {
            ButtonSize::Small => theme.spacing_value(SpacingToken::Space3), // 12px
            ButtonSize::Medium => theme.spacing_value(SpacingToken::Space4), // 16px
            ButtonSize::Large => theme.spacing_value(SpacingToken::Space6), // 24px
            ButtonSize::Icon => 0.0,
        }
    }

    /// Get the vertical padding
    fn padding_y(&self, theme: &ThemeState) -> f32 {
        match self {
            ButtonSize::Small => theme.spacing_value(SpacingToken::Space1_5), // 6px
            ButtonSize::Medium => theme.spacing_value(SpacingToken::Space2),  // 8px
            ButtonSize::Large => theme.spacing_value(SpacingToken::Space3),   // 12px
            ButtonSize::Icon => 0.0,
        }
    }

    /// Get the font size
    fn font_size(&self) -> f32 {
        match self {
            ButtonSize::Small => 13.0,
            ButtonSize::Medium => 14.0,
            ButtonSize::Large => 16.0,
            ButtonSize::Icon => 14.0,
        }
    }

    /// Get the border radius using theme tokens
    fn border_radius(&self, theme: &ThemeState) -> f32 {
        match self {
            ButtonSize::Small => theme.radius(RadiusToken::Sm),
            ButtonSize::Medium => theme.radius(RadiusToken::Md),
            ButtonSize::Large => theme.radius(RadiusToken::Lg),
            ButtonSize::Icon => theme.radius(RadiusToken::Md),
        }
    }
}

/// Button component with variants and sizes
///
/// Built on `blinc_layout::Stateful<ButtonState>` for hover/press interactions.
/// Implements `Deref` and `DerefMut` to `Stateful<ButtonState>` so all Div
/// methods are available for customization (margins, padding, shadows, etc.).
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// // All Stateful/Div methods are available via Deref
/// cn::button("Custom")
///     .variant(ButtonVariant::Primary)
///     .m(8.0)        // margin
///     .shadow_lg()   // shadow
///     .gap(4.0)      // flex gap
/// ```
pub struct Button {
    inner: Stateful<ButtonState>,
    label: String,
    variant: ButtonVariant,
    btn_size: ButtonSize,
    disabled: bool,
}

impl Button {
    /// Create a new button with a label
    pub fn new(label: impl Into<String>) -> Self {
        Self::with_options(
            label,
            ButtonVariant::default(),
            ButtonSize::default(),
            false,
        )
    }

    /// Create a button with specific variant and size
    fn with_options(
        label: impl Into<String>,
        variant: ButtonVariant,
        btn_size: ButtonSize,
        disabled: bool,
    ) -> Self {
        let theme = ThemeState::get();
        let label = label.into();

        // Get colors for this variant
        let fg = variant.foreground(&theme);
        let border = variant.border(&theme);

        // Get sizes using theme tokens
        let height = btn_size.height(&theme);
        let px = btn_size.padding_x(&theme);
        let py = btn_size.padding_y(&theme);
        let font_size = btn_size.font_size();
        let radius = btn_size.border_radius(&theme);

        // Build the stateful button
        let initial_state = if disabled {
            ButtonState::Disabled
        } else {
            ButtonState::Idle
        };

        let mut btn = Stateful::new(initial_state)
            .h(height)
            .padding_x(blinc_layout::units::px(px))
            .padding_y(blinc_layout::units::px(py))
            .rounded(radius)
            .items_center()
            .justify_center()
            .cursor_pointer();

        // Handle border for outline variant
        if let Some(border_color) = border {
            btn = btn.border(1.0, border_color);
        }

        // Default to fit content width
        btn = btn.w_fit();

        // Clone label for the closure
        let label_clone = label.clone();

        // State callback for hover/press visual changes
        btn = btn.on_state(move |state, container| {
            let theme = ThemeState::get();
            let bg = variant.background(&theme, *state);
            let scale = if matches!(state, ButtonState::Pressed) {
                0.98
            } else {
                1.0
            };

            container.merge(
                div()
                    .bg(bg)
                    .transform(Transform::scale(scale, scale))
                    .cursor_pointer()
                    .child(text(&label_clone).size(font_size).color(fg)),
            );
        });

        Self {
            inner: btn,
            label,
            variant,
            btn_size,
            disabled,
        }
    }

    /// Set the button variant (rebuilds with new styling)
    pub fn variant(self, variant: ButtonVariant) -> Self {
        Self::with_options(self.label, variant, self.btn_size, self.disabled)
    }

    /// Set the button size (rebuilds with new sizing)
    pub fn size(self, size: ButtonSize) -> Self {
        Self::with_options(self.label, self.variant, size, self.disabled)
    }

    /// Make the button disabled
    pub fn disabled(self, disabled: bool) -> Self {
        Self::with_options(self.label, self.variant, self.btn_size, disabled)
    }

    /// Make the button full width
    pub fn full_width(mut self) -> Self {
        self.inner = self.inner.w_full();
        self
    }

    /// Set click handler
    ///
    /// This method is provided on Button directly to allow chaining with
    /// `.variant()` and `.size()` methods.
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&blinc_layout::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_click(handler);
        self
    }
}

// Implement Deref to expose all Stateful<ButtonState> methods
impl Deref for Button {
    type Target = Stateful<ButtonState>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Button {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

// Implement ElementBuilder so Button can be used directly
impl ElementBuilder for Button {
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
        self.inner.event_handlers()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.inner.element_type_id()
    }
}

/// Create a button with a label
///
/// The button supports fluent chaining and exposes all `Stateful<ButtonState>`
/// methods via `Deref`, allowing full customization.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// // Basic button
/// cn::button("Click me")
///     .variant(ButtonVariant::Primary)
///     .on_click(|_| println!("Clicked!"))
///
/// // With additional styling
/// cn::button("Custom")
///     .m(8.0)        // margin (via Deref)
///     .shadow_md()   // shadow (via Deref)
/// ```
pub fn button(label: impl Into<String>) -> Button {
    Button::new(label)
}

/// Helper to darken a color
fn darken(color: Color, amount: f32) -> Color {
    Color::rgba(
        (color.r - amount).max(0.0),
        (color.g - amount).max(0.0),
        (color.b - amount).max(0.0),
        color.a,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_button_default() {
        init_theme();
        let btn = button("Test");
        assert_eq!(btn.state(), ButtonState::Idle);
    }

    #[test]
    fn test_button_variants() {
        init_theme();

        // All variants should build without error
        let _ = button("Primary").variant(ButtonVariant::Primary);
        let _ = button("Secondary").variant(ButtonVariant::Secondary);
        let _ = button("Destructive").variant(ButtonVariant::Destructive);
        let _ = button("Outline").variant(ButtonVariant::Outline);
        let _ = button("Ghost").variant(ButtonVariant::Ghost);
        let _ = button("Link").variant(ButtonVariant::Link);
    }

    #[test]
    fn test_button_sizes() {
        init_theme();

        // All sizes should build without error
        let _ = button("Small").size(ButtonSize::Small);
        let _ = button("Medium").size(ButtonSize::Medium);
        let _ = button("Large").size(ButtonSize::Large);
        let _ = button("Icon").size(ButtonSize::Icon);
    }

    #[test]
    fn test_button_disabled() {
        init_theme();
        let btn = button("Disabled").disabled(true);
        assert_eq!(btn.state(), ButtonState::Disabled);
    }

    #[test]
    fn test_button_deref() {
        init_theme();
        // Test that Deref works - can access state() method
        let btn = button("Test");
        let _ = btn.state(); // This should compile thanks to Deref
    }
}
