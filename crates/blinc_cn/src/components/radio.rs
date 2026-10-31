//! Radio Group component for single-selection from multiple options
//!
//! A themed radio button group. Each button is an element with a `checked`
//! attribute while it is selected and `disabled` while it cannot be chosen, so
//! its look in each state, and how it eases between them, is the `.cn-radio`
//! rules of `CN_STYLES`. Restyle it with rules of your own:
//!
//! ```css
//! .my-radio:checked { border-color: #16a34a; }
//! ```
//! Uses `State<String>` from context for reactive state management.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     // Create radio state from context
//!     let selected = ctx.use_state_for("color", "red".to_string());
//!
//!     cn::radio_group(&selected)
//!         .option("red", "Red")
//!         .option("green", "Green")
//!         .option("blue", "Blue")
//!         .on_change(|value| println!("Selected: {}", value))
//! }
//!
//! // Horizontal layout
//! cn::radio_group(&selected)
//!     .horizontal()
//!     .option("yes", "Yes")
//!     .option("no", "No")
//!
//! // With label
//! cn::radio_group(&size)
//!     .label("Select size")
//!     .option("sm", "Small")
//!     .option("md", "Medium")
//!     .option("lg", "Large")
//!
//! // Disabled
//! cn::radio_group(&selected)
//!     .disabled(true)
//!     .option("a", "Option A")
//!     .option("b", "Option B")
//! ```

use blinc_core::State;
use blinc_core::reactive::{ReactiveGraph, computed};
use blinc_layout::InstanceKey;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, SpacingToken, ThemeState};
use std::sync::Arc;

use super::label::{LabelSize, label};

/// Radio button size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RadioSize {
    /// Small radio (14px)
    Small,
    /// Medium radio (18px)
    #[default]
    Medium,
    /// Large radio (22px)
    Large,
}

impl RadioSize {
    fn outer_size(&self) -> f32 {
        match self {
            RadioSize::Small => 14.0,
            RadioSize::Medium => 18.0,
            RadioSize::Large => 22.0,
        }
    }

    fn inner_size(&self) -> f32 {
        match self {
            RadioSize::Small => 6.0,
            RadioSize::Medium => 8.0,
            RadioSize::Large => 10.0,
        }
    }

    fn border_width(&self) -> f32 {
        match self {
            RadioSize::Small => 1.5,
            RadioSize::Medium => 2.0,
            RadioSize::Large => 2.0,
        }
    }
}

/// Layout direction for radio options
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RadioLayout {
    /// Options stacked vertically (default)
    #[default]
    Vertical,
    /// Options arranged horizontally
    Horizontal,
}

/// A radio option with value and label
#[derive(Clone)]
struct RadioOption {
    value: String,
    label: String,
    disabled: bool,
}

/// Radio Group component
///
/// A group of radio buttons where only one can be selected at a time.
/// Uses `State<String>` from context for reactive state management.
pub struct RadioGroup {
    /// The fully-built inner element (Div containing radio buttons and optional label)
    inner: Div,
}

impl RadioGroup {
    /// Create a new radio group with state from context
    ///
    /// # Example
    /// ```ignore
    /// let selected = ctx.use_state_for("choice", "option1".to_string());
    /// cn::radio_group(&selected)
    ///     .option("option1", "First Option")
    ///     .option("option2", "Second Option")
    /// ```
    #[track_caller]
    pub fn new(selected: &State<String>) -> Self {
        Self::with_config(RadioGroupConfig::new(selected.clone()))
    }

    /// Create from a full configuration
    fn with_config(config: RadioGroupConfig) -> Self {
        let theme = ThemeState::get();
        let default_gap = theme.spacing_value(SpacingToken::Space1);
        let gap = config.gap.unwrap_or(default_gap);

        // Build options container
        let mut options_container = match config.layout {
            RadioLayout::Vertical => div().flex_col().gap(gap).h_fit(),
            RadioLayout::Horizontal => div().flex_row().gap(gap).flex_wrap().h_fit(),
        };

        // Add each radio button
        for option in &config.options {
            // Auto-derive per-button CSS ID: "{group_id}-{value}"
            let button_css_id = config.css_group_id.as_ref().map(|group_id| {
                let sanitized = option.value.replace(' ', "-");
                format!("{group_id}-{sanitized}")
            });
            options_container =
                options_container.child(build_radio_button(&config, option, theme, button_css_id));
        }

        // If there's a label, wrap everything
        let mut inner = if let Some(ref label_text) = config.label {
            let spacing = theme.spacing_value(SpacingToken::Space2);
            let mut lbl = label(label_text).size(LabelSize::Medium);
            if config.disabled {
                lbl = lbl.disabled(true);
            }

            div()
                .flex_col()
                .gap(spacing)
                .h_fit()
                .child(lbl)
                .child(options_container)
        } else {
            options_container
        };

        // Apply user classes
        for c in &config.classes {
            inner = inner.class(c);
        }

        Self { inner }
    }
}

/// Build a single radio button
fn build_radio_button(
    config: &RadioGroupConfig,
    option: &RadioOption,
    theme: &ThemeState,
    button_css_id: Option<String>,
) -> Div {
    let outer_size = config.size.outer_size();
    let inner_size = config.size.inner_size();
    let border_width = config.size.border_width();

    let option_value = option.value.clone();
    let option_disabled = option.disabled || config.disabled;
    let option_label = option.label.clone();
    let selected_state = config.selected.clone();
    let selected_state_for_click = config.selected.clone();
    let on_change = config.on_change.clone();
    let value_for_click = option.value.clone();

    let label_color = if option_disabled {
        theme.color(ColorToken::TextTertiary)
    } else {
        theme.color(ColorToken::TextPrimary)
    };

    // Hover, press, selection and disabled belong to CSS (`.cn-radio:hover`,
    // `:active`, `:checked`, `:disabled`), so nothing here follows the
    // pointer; the selection only moves the `checked` attribute, in place.
    let selected = selected_state.signal();
    let is_selected =
        computed(move |g: &ReactiveGraph| g.get(selected).unwrap_or_default() == option_value);

    // The ring is the element; the dot is always there and shown while
    // checked. Sizes and the border width vary per size, so they are set here.
    let mut circle = div()
        .class("cn-radio")
        // Added and taken away with the selection, so the stylesheet's
        // `.cn-radio--selected` rules follow it in place.
        .class_when("cn-radio--selected", &is_selected)
        .attr_when("checked", "", &is_selected)
        .w(outer_size)
        .h(outer_size)
        .rounded(outer_size / 2.0)
        .border_width(border_width)
        .items_center()
        .justify_center()
        .child(
            div()
                .class("cn-radio-dot")
                .w(inner_size)
                .h(inner_size)
                .rounded(inner_size / 2.0),
        );

    if option_disabled {
        circle = circle.class("cn-radio--disabled").attr("disabled", "");
    }

    let mut radio = div()
        .flex_row()
        .gap_px(theme.spacing_value(SpacingToken::Space4))
        .items_center()
        .cursor_pointer()
        .child(circle)
        .child(text(&option_label).size(14.0).color(label_color));

    // Set CSS element ID for element registry matching
    if let Some(ref css_id) = button_css_id {
        radio = radio.id(css_id);
    }

    // Click handler
    radio.on_click(move |_| {
        if option_disabled {
            return;
        }

        let current = selected_state_for_click.get();
        if current != value_for_click {
            selected_state_for_click.set(value_for_click.clone());
            if let Some(ref callback) = on_change {
                callback(&value_for_click);
            }
        }
    })
}

/// Internal configuration for building a RadioGroup
#[derive(Clone)]
#[allow(clippy::type_complexity)]
struct RadioGroupConfig {
    selected: State<String>,
    options: Vec<RadioOption>,
    size: RadioSize,
    layout: RadioLayout,
    label: Option<String>,
    disabled: bool,
    gap: Option<f32>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    css_group_id: Option<String>,
    /// Distinguishes this group's per-option state from another
    /// group's. Two groups offering the same values would otherwise key
    /// their options identically, and one group's hover would drive the
    /// other's.
    instance_key: InstanceKey,
    /// User-added CSS classes
    classes: Vec<std::sync::Arc<str>>,
}

impl RadioGroupConfig {
    #[track_caller]
    fn new(selected: State<String>) -> Self {
        Self {
            selected,
            instance_key: InstanceKey::new("radio-group"),
            options: Vec::new(),
            size: RadioSize::default(),
            layout: RadioLayout::default(),
            label: None,
            disabled: false,
            gap: None,
            on_change: None,
            css_group_id: None,
            classes: Vec::new(),
        }
    }
}

/// Builder for creating RadioGroup components with fluent API
pub struct RadioGroupBuilder {
    config: RadioGroupConfig,
    /// Cached built RadioGroup - built lazily on first access
    built: std::cell::OnceCell<RadioGroup>,
}

impl RadioGroupBuilder {
    /// Create a new radio group builder with state from context
    #[track_caller]
    pub fn new(selected: &State<String>) -> Self {
        Self {
            config: RadioGroupConfig::new(selected.clone()),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner RadioGroup
    fn get_or_build(&self) -> &RadioGroup {
        ::blinc_layout::build_once::build_once(&self.built, || {
            RadioGroup::with_config(self.config.clone())
        })
    }

    /// Name this group, so its per-option state is its own.
    ///
    /// The default identity is the call site, which is enough for two
    /// groups written in different places. A caller that builds several
    /// groups from ONE site -- a loop, or a DSL wrapper -- has to say
    /// which is which, or groups offering the same option values share
    /// their options' state.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.config.instance_key = InstanceKey::explicit(key);
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.config
            .classes
            .push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Set a CSS element ID for the radio group
    ///
    /// Individual radio buttons auto-derive IDs as `"{group_id}-{value}"`.
    /// For example, `.id("theme-radio")` with `.option("light", "Light")` produces
    /// a button with CSS ID `"theme-radio-light"`.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.config.css_group_id = Some(id.into());
        self
    }

    /// Add an option to the radio group
    pub fn option(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.config.options.push(RadioOption {
            value: value.into(),
            label: label.into(),
            disabled: false,
        });
        self
    }

    /// Add a disabled option to the radio group
    pub fn option_disabled(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.config.options.push(RadioOption {
            value: value.into(),
            label: label.into(),
            disabled: true,
        });
        self
    }

    /// Set the radio button size
    pub fn size(mut self, size: RadioSize) -> Self {
        self.config.size = size;
        self
    }

    /// Set the layout direction
    pub fn layout(mut self, layout: RadioLayout) -> Self {
        self.config.layout = layout;
        self
    }

    /// Use horizontal layout
    pub fn horizontal(mut self) -> Self {
        self.config.layout = RadioLayout::Horizontal;
        self
    }

    /// Use vertical layout (default)
    pub fn vertical(mut self) -> Self {
        self.config.layout = RadioLayout::Vertical;
        self
    }

    /// Add a label above the radio group
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Disable all options
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Set the gap between options
    pub fn gap(mut self, gap: f32) -> Self {
        self.config.gap = Some(gap);
        self
    }

    /// Set the change callback
    ///
    /// Called when a different option is selected, with the new value.
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(callback));
        self
    }

    /// Build the final RadioGroup component
    pub fn build_component(self) -> RadioGroup {
        RadioGroup::with_config(self.config)
    }
}

impl ElementBuilder for RadioGroup {
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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

impl ElementBuilder for RadioGroupBuilder {
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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// Create a radio group with state from context
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// let choice = ctx.use_state_for("choice", "a".to_string());
/// cn::radio_group(&choice)
///     .label("Pick one")
///     .option("a", "Option A")
///     .option("b", "Option B")
///     .option("c", "Option C")
/// ```
pub fn radio_group(selected: &State<String>) -> RadioGroupBuilder {
    RadioGroupBuilder::new(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_radio_sizes() {
        assert_eq!(RadioSize::Small.outer_size(), 14.0);
        assert_eq!(RadioSize::Medium.outer_size(), 18.0);
        assert_eq!(RadioSize::Large.outer_size(), 22.0);
    }

    #[test]
    fn test_radio_inner_sizes() {
        assert_eq!(RadioSize::Small.inner_size(), 6.0);
        assert_eq!(RadioSize::Medium.inner_size(), 8.0);
        assert_eq!(RadioSize::Large.inner_size(), 10.0);
    }

    #[test]
    fn test_radio_border_widths() {
        assert_eq!(RadioSize::Small.border_width(), 1.5);
        assert_eq!(RadioSize::Medium.border_width(), 2.0);
        assert_eq!(RadioSize::Large.border_width(), 2.0);
    }

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    // Note: RadioGroup builder test requires State which needs context
    // The RadioGroup API is tested through the size/layout tests above
}
