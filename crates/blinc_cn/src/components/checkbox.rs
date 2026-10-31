//! Checkbox component for boolean selection
//!
//! A themed checkbox. It is an element with a `checked` attribute while it is
//! checked and `disabled` while it is not usable, so how it looks in each
//! state, and how it eases between them, is the `.cn-checkbox` rules of
//! `CN_STYLES`. Restyle it with rules of your own:
//!
//! ```css
//! .my-checkbox:checked { background: #16a34a; border-color: #16a34a; }
//! ```
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
//! ```

use blinc_core::State;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::checkbox as layout_checkbox;
use blinc_theme::ThemeState;
use std::sync::Arc;

use blinc_layout::InstanceKey;

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
            CheckboxSize::Small => 11.0,
            CheckboxSize::Medium => 14.0,
            CheckboxSize::Large => 18.0,
        }
    }
}

/// Checkbox component
///
/// A toggle checkbox with hover and press feedback.
/// Uses `State<bool>` from context for reactive state management.
pub struct Checkbox {
    /// The layout checkbox, with cn's classes, sizes and radius on it.
    inner: layout_checkbox::CheckboxBuilder,
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
    fn with_config(_instance_key: &str, config: CheckboxConfig) -> Self {
        let theme = ThemeState::get();
        let radius = match config.size {
            CheckboxSize::Small => theme.radius(blinc_theme::RadiusToken::Sm) * 0.75,
            _ => theme.radius(blinc_theme::RadiusToken::Sm),
        };

        // The look is the layout checkbox's `.blinc-checkbox` rules; cn adds
        // its classes, which `CN_STYLES` hangs its own hooks on, and its size
        // ladder.
        let mut inner = layout_checkbox::checkbox(&config.checked_state)
            .class("cn-checkbox")
            // Added and taken away with the state, so the stylesheet's
            // `.cn-checkbox--checked` rules follow it in place.
            .checked_class("cn-checkbox--checked")
            .checkbox_size(config.size.size())
            .mark_size(config.size.checkmark_size())
            .border_width(config.size.border_width())
            .rounded(radius)
            .disabled(config.disabled);
        if config.disabled {
            inner = inner.class("cn-checkbox--disabled");
        }
        if let Some(label) = config.label {
            inner = inner.label(label);
        }
        if let Some(on_change) = config.on_change {
            inner = inner.on_change(move |checked| on_change(checked));
        }

        Self { inner }
    }

    /// Add a CSS class to the box, for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.inner = self.inner.class(name);
        self
    }

    /// Set the element ID of the box for CSS selector matching
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

    fn semantic_type_name(&self) -> Option<&'static str> {
        self.inner.semantic_type_name()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.inner.event_handlers()
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
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
    on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl CheckboxConfig {
    fn new(checked_state: State<bool>) -> Self {
        Self {
            checked_state,
            size: CheckboxSize::default(),
            label: None,
            disabled: false,
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
        // The box's inner area: its size less the border on each side.
        for size in [
            CheckboxSize::Small,
            CheckboxSize::Medium,
            CheckboxSize::Large,
        ] {
            assert_eq!(
                size.checkmark_size(),
                size.size() - 2.0 * size.border_width()
            );
        }
    }
}
