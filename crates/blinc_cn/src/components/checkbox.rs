//! Checkbox component for boolean selection
//!
//! A themed checkbox component with checked, unchecked, and hover states.
//! Uses motion animations for smooth state transitions.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     // Create checkbox state from context
//!     let checked = ctx.use_state_for("my_checkbox", false);
//!
//!     cn::checkbox(&checked)
//!         .label("Accept terms")
//!         .on_change(|is_checked| println!("Checked: {}", is_checked))
//! }
//!
//! // Pre-checked
//! let checked = ctx.use_state_for("agree", true);
//! cn::checkbox(&checked)
//!
//! // Disabled
//! cn::checkbox(&checked)
//!     .disabled(true)
//!
//! // Custom colors
//! cn::checkbox(&checked)
//!     .checked_color(Color::GREEN)
//!     .border_color(Color::GRAY)
//! ```

use blinc_core::reactive::{ReactiveGraph, computed};
use blinc_core::{Color, State};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, ThemeState};
use std::sync::Arc;

use blinc_layout::{InstanceKey, Interaction};

/// SVG checkmark path - simple checkmark that fits in a 16x16 viewBox
const CHECKMARK_SVG: &str = r#"<svg viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
    <path d="M3 8L6.5 11.5L13 4.5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
</svg>"#;

/// Checkbox size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckboxSize {
    /// Small checkbox (14px)
    Small,
    /// Medium checkbox (18px)
    #[default]
    Medium,
    /// Large checkbox (22px)
    Large,
}

impl CheckboxSize {
    fn size(&self) -> f32 {
        match self {
            CheckboxSize::Small => 14.0,
            CheckboxSize::Medium => 18.0,
            CheckboxSize::Large => 22.0,
        }
    }

    fn border_width(&self) -> f32 {
        match self {
            CheckboxSize::Small => 1.5,
            CheckboxSize::Medium => 2.0,
            CheckboxSize::Large => 2.0,
        }
    }

    fn checkmark_size(&self) -> f32 {
        match self {
            CheckboxSize::Small => 10.0,
            CheckboxSize::Medium => 12.0,
            CheckboxSize::Large => 16.0,
        }
    }
}

/// Checkbox component
///
/// A toggle checkbox with hover and press feedback.
/// Uses `State<bool>` from context for reactive state management.
pub struct Checkbox {
    /// The fully-built inner element (Div containing checkbox and optional label)
    inner: Div,
}

impl Checkbox {
    /// Create a new checkbox with state from context
    ///
    /// # Example
    /// ```ignore
    /// let checked = ctx.use_state_for("my_checkbox", false);
    /// cn::checkbox(&checked)
    /// ```
    #[track_caller]
    pub fn new(checked_state: &State<bool>) -> Self {
        Self::with_config(
            InstanceKey::new("checkbox").get(),
            CheckboxConfig::new(checked_state.clone()),
        )
    }

    /// Create from a full configuration
    fn with_config(instance_key: &str, config: CheckboxConfig) -> Self {
        let theme = ThemeState::get();
        let box_size = config.size.size();
        let border_width = config.size.border_width();
        let checkmark_size = config.size.checkmark_size();
        let radius = match config.size {
            CheckboxSize::Small => theme.radius(blinc_theme::RadiusToken::Sm) * 0.75,
            _ => theme.radius(blinc_theme::RadiusToken::Sm),
        };

        // Get colors — used as builder defaults, CSS can override via classes
        let checked_bg = config
            .checked_color
            .unwrap_or_else(|| theme.color(ColorToken::Primary));
        let unchecked_bg = config
            .unchecked_bg
            .unwrap_or_else(|| theme.color(ColorToken::InputBg));
        let border = config
            .border_color
            .unwrap_or_else(|| theme.color(ColorToken::Border));
        let hover_border = config
            .hover_border_color
            .unwrap_or_else(|| theme.color(ColorToken::BorderHover));
        let check_mark_color = config
            .check_color
            .unwrap_or_else(|| theme.color(ColorToken::TextInverse));

        let disabled = config.disabled;
        let on_change = config.on_change.clone();
        let checked_state = config.checked_state.clone();
        let checked_state_for_click = config.checked_state.clone();

        // What a state changes is bound: each is a computed over the checked
        // signal and the box's hover, so a change patches the node instead of
        // rebuilding it. Disabled uses the shared disabled-surface palette
        // (InputBgDisabled / BorderSecondary), which matches button / select
        // / switch so all inert controls read as the same UI tier.
        let interaction = Interaction::keyed(&format!("{instance_key}:box"));
        let (checked, hovered) = (checked_state.signal(), interaction.hovered().signal());
        let disabled_bg = theme.color(ColorToken::InputBgDisabled);
        let disabled_border = theme.color(ColorToken::BorderSecondary);

        let bg = computed(move |g: &ReactiveGraph| {
            if disabled {
                disabled_bg
            } else if g.get(checked).unwrap_or(false) {
                checked_bg
            } else {
                unchecked_bg
            }
        });
        let current_border = computed(move |g: &ReactiveGraph| {
            if disabled {
                disabled_border
            } else if g.get(hovered).unwrap_or(false) {
                hover_border
            } else {
                border
            }
        });
        let scale = computed(move |g: &ReactiveGraph| {
            let by = if g.get(hovered).unwrap_or(false) && !disabled {
                1.05
            } else {
                1.0
            };
            blinc_core::Transform::scale(by, by)
        });

        let mut checkbox = div()
            .class("cn-checkbox")
            // Added and taken away with the state, so the stylesheet's
            // `.cn-checkbox--checked` rules follow it in place.
            .class_when("cn-checkbox--checked", &checked_state)
            .w(box_size)
            .h(box_size)
            // Pin box to its explicit dimensions — without this a
            // narrow flex-row parent (e.g. a settings list) would let
            // taffy compress the checkbox while the label keeps its
            // natural width.
            .flex_shrink_0()
            .rounded(radius)
            .cursor_pointer()
            .items_center()
            .justify_center()
            .bg(&bg)
            .border_width(border_width)
            .border_color(&current_border)
            .transform(&scale)
            .track(&interaction)
            // The mark is always there and shown while checked.
            .child(
                div()
                    .w(checkmark_size)
                    .h(checkmark_size)
                    .visible(&checked_state)
                    .child(
                        svg(CHECKMARK_SVG)
                            .size(checkmark_size, checkmark_size)
                            .tint(check_mark_color),
                    ),
            );

        if disabled {
            // No opacity dim — the InputBgDisabled / BorderSecondary
            // palette above already conveys the inert state without
            // washing out the silhouette.
            checkbox = checkbox.class("cn-checkbox--disabled");
        }

        // Add click handler to toggle the state (only if not disabled)
        checkbox = checkbox.on_click(move |_| {
            if disabled {
                return;
            }

            let current = checked_state_for_click.get();
            let new_value = !current;
            checked_state_for_click.set(new_value);

            if let Some(ref callback) = on_change {
                callback(new_value);
            }
        });

        // If there's a label, wrap in a row with clickable container
        let inner = if let Some(ref label_text) = config.label {
            let label_color = if disabled {
                theme.color(ColorToken::TextTertiary)
            } else {
                theme.color(ColorToken::TextPrimary)
            };

            // Clone state for the container click handler
            let checked_state_for_label = config.checked_state.clone();
            let on_change_for_label = config.on_change.clone();

            div()
                .flex_row()
                .gap(theme.spacing().space_1)
                .items_center()
                .cursor_pointer()
                .child(checkbox)
                .child(
                    text(label_text)
                        .size(theme.typography().text_sm)
                        .color(label_color),
                )
                .on_click(move |_| {
                    if disabled {
                        return;
                    }
                    let current = checked_state_for_label.get();
                    let new_value = !current;
                    checked_state_for_label.set(new_value);
                    if let Some(ref callback) = on_change_for_label {
                        callback(new_value);
                    }
                })
        } else {
            // Wrap single checkbox in a div for consistent behavior
            div().child(checkbox)
        };

        Self { inner }
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

impl ElementBuilder for Checkbox {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Internal configuration for building a Checkbox
#[derive(Clone)]
struct CheckboxConfig {
    checked_state: State<bool>,
    size: CheckboxSize,
    label: Option<String>,
    disabled: bool,
    checked_color: Option<Color>,
    unchecked_bg: Option<Color>,
    border_color: Option<Color>,
    hover_border_color: Option<Color>,
    check_color: Option<Color>,
    on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl CheckboxConfig {
    fn new(checked_state: State<bool>) -> Self {
        Self {
            checked_state,
            size: CheckboxSize::default(),
            label: None,
            disabled: false,
            checked_color: None,
            unchecked_bg: None,
            border_color: None,
            hover_border_color: None,
            check_color: None,
            on_change: None,
        }
    }
}

/// Builder for creating Checkbox components with fluent API
pub struct CheckboxBuilder {
    key: InstanceKey,
    config: CheckboxConfig,
    /// Cached built Checkbox - built lazily on first access
    built: std::cell::OnceCell<Checkbox>,
}

impl CheckboxBuilder {
    /// Create a new checkbox builder with state from context
    #[track_caller]
    pub fn new(checked_state: &State<bool>) -> Self {
        Self {
            key: InstanceKey::new("checkbox"),
            config: CheckboxConfig::new(checked_state.clone()),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Create a checkbox builder with an explicit key
    pub fn with_key(key: impl Into<String>, checked_state: &State<bool>) -> Self {
        Self {
            key: InstanceKey::explicit(key),
            config: CheckboxConfig::new(checked_state.clone()),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Checkbox
    fn get_or_build(&self) -> &Checkbox {
        ::blinc_layout::build_once::build_once(&self.built, || {
            Checkbox::with_config(self.key.get(), self.config.clone())
        })
    }

    /// Set the checkbox size
    pub fn size(mut self, size: CheckboxSize) -> Self {
        self.config.size = size;
        self
    }

    /// Add a label to the checkbox
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Set the background color when checked
    pub fn checked_color(mut self, color: impl Into<Color>) -> Self {
        self.config.checked_color = Some(color.into());
        self
    }

    /// Set the background color when unchecked
    pub fn unchecked_bg(mut self, color: impl Into<Color>) -> Self {
        self.config.unchecked_bg = Some(color.into());
        self
    }

    /// Set the border color
    pub fn border_color(mut self, color: impl Into<Color>) -> Self {
        self.config.border_color = Some(color.into());
        self
    }

    /// Set the hover border color
    pub fn hover_border_color(mut self, color: impl Into<Color>) -> Self {
        self.config.hover_border_color = Some(color.into());
        self
    }

    /// Set the checkmark color
    pub fn check_color(mut self, color: impl Into<Color>) -> Self {
        self.config.check_color = Some(color.into());
        self
    }

    /// Set the change callback
    ///
    /// Called when the checkbox is toggled, with the new checked state.
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(callback));
        self
    }

    /// Build the final Checkbox component
    pub fn build_component(self) -> Checkbox {
        Checkbox::with_config(self.key.get(), self.config)
    }
}

impl ElementBuilder for CheckboxBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// Create a checkbox with state from context
///
/// The checkbox uses reactive `State<bool>` for its checked status.
/// State changes automatically trigger visual updates via signals.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
///     let checked = ctx.use_state_for("remember_me", false);
///
///     cn::checkbox(&checked)
///         .label("Remember me")
///         .on_change(|checked| println!("Checked: {}", checked))
/// }
/// ```
pub fn checkbox(state: &State<bool>) -> CheckboxBuilder {
    CheckboxBuilder::new(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkbox_sizes() {
        assert_eq!(CheckboxSize::Small.size(), 14.0);
        assert_eq!(CheckboxSize::Medium.size(), 18.0);
        assert_eq!(CheckboxSize::Large.size(), 22.0);
    }

    #[test]
    fn test_checkbox_checkmark_sizes() {
        assert_eq!(CheckboxSize::Small.checkmark_size(), 10.0);
        assert_eq!(CheckboxSize::Medium.checkmark_size(), 12.0);
        assert_eq!(CheckboxSize::Large.checkmark_size(), 16.0);
    }
}
