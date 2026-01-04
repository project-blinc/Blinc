//! Button component with shadcn-style variants
//!
//! A themed button component using Stateful<ButtonState> for hover/press interactions.
//! The button manages its own state internally using `#[track_caller]` for unique key generation.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! // Primary button (default) - state managed internally via track_caller
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
//! // Button with click handler
//! cn::button("Submit")
//!     .on_click(|_| println!("Submitted!"))
//! ```

use blinc_core::context_state::BlincContextState;
use blinc_core::Color;
use blinc_layout::div::ElementBuilder;
use blinc_layout::element::CursorStyle;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, SharedState, Stateful, StatefulInner};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};
use std::sync::{Arc, Mutex};

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

    /// Get the base (idle) background color
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
            ButtonVariant::Primary | ButtonVariant::Destructive => {
                theme.color(ColorToken::TextInverse)
            }
            ButtonVariant::Secondary => theme.color(ColorToken::TextPrimary),
            ButtonVariant::Outline | ButtonVariant::Ghost => theme.color(ColorToken::TextPrimary),
            ButtonVariant::Link => theme.color(ColorToken::Primary),
        }
    }

    /// Get the border color for this variant (if any)
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
    /// Small button
    Small,
    /// Default size
    #[default]
    Medium,
    /// Large button
    Large,
    /// Icon-only button (square)
    Icon,
}

impl ButtonSize {
    /// Get height
    fn height(&self) -> f32 {
        match self {
            ButtonSize::Small => 32.0,
            ButtonSize::Medium => 40.0,
            ButtonSize::Large => 44.0,
            ButtonSize::Icon => 40.0,
        }
    }

    /// Get horizontal padding
    fn padding_x(&self) -> f32 {
        match self {
            ButtonSize::Small => 12.0,
            ButtonSize::Medium => 16.0,
            ButtonSize::Large => 24.0,
            ButtonSize::Icon => 8.0,
        }
    }

    /// Get vertical padding
    fn padding_y(&self) -> f32 {
        match self {
            ButtonSize::Small => 4.0,
            ButtonSize::Medium => 8.0,
            ButtonSize::Large => 12.0,
            ButtonSize::Icon => 8.0,
        }
    }

    /// Get font size
    fn font_size(&self) -> f32 {
        match self {
            ButtonSize::Small => 13.0,
            ButtonSize::Medium => 14.0,
            ButtonSize::Large => 16.0,
            ButtonSize::Icon => 14.0,
        }
    }

    /// Get border radius using theme tokens
    fn border_radius(&self, theme: &ThemeState) -> f32 {
        theme.radius(RadiusToken::Md)
    }
}

/// Helper to darken a color
fn darken(color: Color, amount: f32) -> Color {
    Color::rgba(
        (color.r * (1.0 - amount)).max(0.0),
        (color.g * (1.0 - amount)).max(0.0),
        (color.b * (1.0 - amount)).max(0.0),
        color.a,
    )
}

/// Icon position within the button
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconPosition {
    /// Icon appears before the label (left in LTR)
    #[default]
    Start,
    /// Icon appears after the label (right in LTR)
    End,
}

/// Get or create a persistent SharedState<ButtonState> for the given key
///
/// This bridges BlincContextState (which stores arbitrary values via signals)
/// with SharedState<S> (which Stateful needs for FSM state management).
fn use_button_state(key: &str) -> SharedState<ButtonState> {
    let ctx = BlincContextState::get();

    // We store the SharedState wrapped in an Arc inside the signal
    // This way it persists across rebuilds
    let state: blinc_core::State<Option<SharedState<ButtonState>>> =
        ctx.use_state_keyed(key, || None);

    let existing = state.get();
    if let Some(shared) = existing {
        shared
    } else {
        // First time - create the SharedState and store it
        let shared: SharedState<ButtonState> =
            Arc::new(Mutex::new(StatefulInner::new(ButtonState::Idle)));
        state.set(Some(shared.clone()));
        shared
    }
}

/// Create a button with a label
///
/// Uses `#[track_caller]` to generate a unique instance key based on the call site.
/// State is managed internally and persists across rebuilds.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// cn::button("OK")
///     .variant(ButtonVariant::Primary)
///     .on_click(|_| println!("Confirmed!"))
/// ```
#[track_caller]
pub fn button(label: impl Into<String>) -> ButtonBuilder {
    let loc = std::panic::Location::caller();
    let instance_key = format!("{}:{}:{}", loc.file(), loc.line(), loc.column());
    ButtonBuilder::new(&instance_key, label)
}

/// Internal configuration for ButtonBuilder
#[derive(Clone)]
struct ButtonConfig {
    instance_key: String,
    label: String,
    variant: ButtonVariant,
    btn_size: ButtonSize,
    disabled: bool,
    icon: Option<String>,
    icon_position: IconPosition,
    on_click: Option<Arc<dyn Fn(&blinc_layout::event_handler::EventContext) + Send + Sync>>,
}

/// The built button element
pub struct Button {
    /// The fully-built inner element
    inner: Div,
}

impl Button {
    /// Build from a config
    fn from_config(config: ButtonConfig) -> Self {
        let theme = ThemeState::get();

        // Get persistent state for this button
        let state_key = format!("_cn_btn_{}", config.instance_key);
        let button_state = use_button_state(&state_key);

        // Get sizes from config
        let height = config.btn_size.height();
        let px = config.btn_size.padding_x();
        let py = config.btn_size.padding_y();
        let font_size = config.btn_size.font_size();
        let radius = config.btn_size.border_radius(&theme);
        let variant = config.variant;
        let label = config.label.clone();
        let icon = config.icon.clone();
        let icon_position = config.icon_position;
        let border = variant.border(&theme);
        let disabled = config.disabled;

        // Get initial colors for the label (will be updated by on_state)
        let initial_fg = variant.foreground(&theme);

        // Build content with icon + label or just label
        let mut content = blinc_layout::div::div().flex_row().items_center().gap(6.0);
        let label_text = text(&label).size(font_size).color(initial_fg).no_cursor();

        if let Some(ref icon_str) = icon {
            let icon_text = text(icon_str).size(font_size).color(initial_fg);
            match icon_position {
                IconPosition::Start => {
                    content = content.child(icon_text).child(label_text);
                }
                IconPosition::End => {
                    content = content.child(label_text).child(icon_text);
                }
            }
        } else {
            content = content.child(label_text);
        }

        // Create stateful container with FSM button state using persistent handle
        let mut stateful = Stateful::with_shared_state(button_state)
            .h(height)
            .padding_x(Length::Px(px))
            .padding_y(Length::Px(py))
            .rounded(radius)
            .items_center()
            .justify_center()
            .cursor(if disabled {
                CursorStyle::NotAllowed
            } else {
                CursorStyle::Pointer
            })
            .w_fit()
            .on_state(move |state: &ButtonState, container: &mut Div| {
                let theme = ThemeState::get();
                let bg = variant.background(&theme, *state);

                // Scale for pressed state
                let scale = if matches!(state, ButtonState::Pressed) && !disabled {
                    0.98
                } else {
                    1.0
                };

                // Build merge div with background and transform
                let mut merge_div = div()
                    .padding_x(Length::Px(px))
                    .padding_y(Length::Px(py))
                    .bg(bg)
                    .transform(blinc_core::Transform::scale(scale, scale))
                    .cursor_pointer();

                // Include border for outline variant (must be reapplied each state change)
                if let Some(border_color) = variant.border(&theme) {
                    merge_div = merge_div.border(1.0, border_color);
                }

                if variant != ButtonVariant::Link && variant != ButtonVariant::Ghost {
                    merge_div = merge_div.shadow_md();
                }

                container.merge(merge_div);
            })
            .child(content);

        // Add border for outline variant
        if let Some(border_color) = border {
            stateful = stateful.border(1.0, border_color);
        }

        // Add click handler if provided
        if let Some(handler) = config.on_click {
            stateful = stateful.on_click(move |ctx| handler(ctx));
        }

        // If disabled, set initial state to Disabled
        if disabled {
            stateful.set_state(ButtonState::Disabled);
        }

        if variant != ButtonVariant::Link && variant != ButtonVariant::Ghost {
            stateful = stateful.shadow_md();
        }

        // Wrap in a div for consistent ElementBuilder behavior
        Self {
            inner: div().child(stateful),
        }
    }
}

impl ElementBuilder for Button {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
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

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }
}

/// Button configuration for building buttons
pub struct ButtonBuilder {
    config: ButtonConfig,
    /// Cached built Button - built lazily on first access
    built: std::cell::OnceCell<Button>,
}

impl ButtonBuilder {
    /// Create a new button builder
    pub fn new(instance_key: &str, label: impl Into<String>) -> Self {
        Self {
            config: ButtonConfig {
                instance_key: instance_key.to_string(),
                label: label.into(),
                variant: ButtonVariant::default(),
                btn_size: ButtonSize::default(),
                disabled: false,
                icon: None,
                icon_position: IconPosition::Start,
                on_click: None,
            },
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Button
    fn get_or_build(&self) -> &Button {
        self.built
            .get_or_init(|| Button::from_config(self.config.clone()))
    }

    /// Set the button variant
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.config.variant = variant;
        self
    }

    /// Set the button size
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.config.btn_size = size;
        self
    }

    /// Make the button disabled
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Set an icon for the button
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.config.icon = Some(icon.into());
        self
    }

    /// Set the icon position
    pub fn icon_position(mut self, position: IconPosition) -> Self {
        self.config.icon_position = position;
        self
    }

    /// Set the click handler
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&blinc_layout::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.config.on_click = Some(Arc::new(handler));
        self
    }

    /// Build the final Button component
    pub fn build_component(self) -> Button {
        Button::from_config(self.config)
    }
}

impl ElementBuilder for ButtonBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
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

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }
}
