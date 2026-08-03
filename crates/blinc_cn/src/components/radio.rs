//! Radio Group component for single-selection from multiple options
//!
//! A themed radio button group with smooth selection transitions.
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

use blinc_core::{Color, State};
use blinc_layout::InstanceKey;
use blinc_layout::css_parser::{ElementState, Stylesheet, active_stylesheet};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::element_style::ElementStyle;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{NoState, stateful_with_key};
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
) -> Stateful<NoState> {
    let outer_size = config.size.outer_size();
    let inner_size = config.size.inner_size();
    let border_width = config.size.border_width();

    // Get colors
    let selected_color = config
        .selected_color
        .unwrap_or_else(|| theme.color(ColorToken::Primary));
    let border = config
        .border_color
        .unwrap_or_else(|| theme.color(ColorToken::BorderSecondary));
    let hover_border = config
        .hover_border_color
        .unwrap_or_else(|| theme.color(ColorToken::Primary));

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

    let css_id_for_state = button_css_id.clone();

    // Unique per option AND per group. Keyed on the value alone, two
    // groups offering the same values shared one entry, so hovering an
    // option in one drove a rebuild that the other answered too.
    let stateful_key = button_css_id.as_deref().map_or_else(
        || format!("radio-{}-{}", config.instance_key.get(), option.value),
        |id| format!("radio-{id}"),
    );

    // Build the radio button circle — CSS handles border, scale, opacity via classes
    let has_custom_colors = config.selected_color.is_some()
        || config.border_color.is_some()
        || config.hover_border_color.is_some();

    // `NoState`, not `ButtonState`: hover and press belong to CSS
    // (`.cn-radio:hover` / `:active`), so pointer movement must not
    // reach an FSM here. It did, and every transition rebuilt the
    // option — structurally, because selection decides whether the dot
    // is a child. Five options subscribed to one signal then rebuilt
    // together on a hover nobody clicked.
    //
    // What remains is a subscription: the dot appears and disappears
    // with the bound value, and that IS a change of children.
    let mut radio = stateful_with_key::<NoState>(&stateful_key)
        .deps([selected_state.signal_id()])
        .on_state(move |_ctx| {
            let theme = ThemeState::get();
            let is_selected = selected_state.get() == option_value;

            // Start with builder-configured colors for CSS override resolution
            let mut overrides = RadioStyleOverrides {
                selected_color,
                border_color: border,
                hover_border_color: hover_border,
                label_color,
                opacity: None,
                background: None,
            };

            // Apply CSS overrides if we have an element ID
            if let Some(ref css_id) = css_id_for_state {
                if let Some(stylesheet) = active_stylesheet() {
                    apply_css_overrides_radio(
                        &stylesheet,
                        css_id,
                        is_selected,
                        option_disabled,
                        &mut overrides,
                    );
                }
            }

            // Border color based on state. Disabled wins over selected so
            // the radio joins the shared disabled-surface palette
            // (BorderSecondary outline, same as button / select / checkbox).
            let border_color = if option_disabled {
                theme.color(ColorToken::BorderSecondary)
            } else if is_selected {
                overrides.selected_color
            } else {
                overrides.border_color
            };

            // Build the radio circle with builder properties + CSS classes
            let mut circle = div()
                .class("cn-radio")
                .w(outer_size)
                .h(outer_size)
                .rounded(outer_size / 2.0)
                .border(border_width, border_color)
                .items_center()
                .justify_center();

            if is_selected {
                circle = circle.class("cn-radio--selected");
            }

            if option_disabled {
                // Fill the circle with InputBgDisabled so it reads as part of
                // the shared disabled-surface palette. Override comes last
                // so a user-provided `overrides.background` still wins.
                circle = circle
                    .class("cn-radio--disabled")
                    .bg(theme.color(ColorToken::InputBgDisabled));
            }

            if let Some(bg) = overrides.background {
                circle = circle.bg(bg);
            }

            // Add inner dot if selected
            if is_selected {
                let inner_dot = div()
                    .class("cn-radio-dot")
                    .w(inner_size)
                    .h(inner_size)
                    .rounded(inner_size / 2.0)
                    .bg(overrides.selected_color);
                circle = circle.child(inner_dot);
            }

            // Build with label
            let mut visual = div()
                .flex_row()
                .gap_px(theme.spacing_value(SpacingToken::Space4))
                .items_center()
                .cursor_pointer()
                .child(circle)
                .child(text(&option_label).size(14.0).color(overrides.label_color));

            // Only apply opacity if a CSS rule explicitly set one. The
            // tonal disabled palette (InputBgDisabled / BorderSecondary /
            // TextTertiary via label_color) carries the inert state itself
            // without dimming.
            if let Some(opacity) = overrides.opacity {
                visual = visual.opacity(opacity);
            }

            visual
        });

    // Set CSS element ID on the Stateful for element registry matching
    if let Some(ref css_id) = button_css_id {
        radio = radio.id(css_id);
    }

    // Click handler
    radio = radio.on_click(move |_| {
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
    });

    radio
}

/// Mutable radio button style overrides, populated by CSS cascade
struct RadioStyleOverrides {
    selected_color: Color,
    border_color: Color,
    hover_border_color: Color,
    label_color: Color,
    opacity: Option<f32>,
    background: Option<Color>,
}

impl RadioStyleOverrides {
    /// Apply a single ElementStyle layer
    fn apply(&mut self, style: &ElementStyle) {
        if let Some(color) = style.accent_color {
            self.selected_color = color;
        }
        if let Some(color) = style.border_color {
            self.border_color = color;
            // border_color also overrides hover_border_color
            self.hover_border_color = color;
        }
        if let Some(color) = style.text_color {
            self.label_color = color;
        }
        if let Some(o) = style.opacity {
            self.opacity = Some(o);
        }
        if let Some(blinc_core::Brush::Solid(color)) = style.background {
            self.background = Some(color);
        }
    }
}

/// Apply CSS style overrides to radio button visual properties
///
/// Cascading order: base → :checked → :disabled (highest priority).
///
/// `:hover` is absent on purpose. The stylesheet applies it directly to
/// the element, so resolving it here too would mean rebuilding on every
/// pointer enter and leave.
fn apply_css_overrides_radio(
    stylesheet: &Stylesheet,
    element_id: &str,
    is_selected: bool,
    is_disabled: bool,
    overrides: &mut RadioStyleOverrides,
) {
    // 1. Base style
    if let Some(base) = stylesheet.get(element_id) {
        overrides.apply(base);
    }
    // 2. :checked (if selected)
    if is_selected {
        if let Some(s) = stylesheet.get_with_state(element_id, ElementState::Checked) {
            overrides.apply(s);
        }
    }
    // 3. :disabled (highest priority)
    if is_disabled {
        if let Some(s) = stylesheet.get_with_state(element_id, ElementState::Disabled) {
            overrides.apply(s);
        }
    }
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
    selected_color: Option<Color>,
    border_color: Option<Color>,
    hover_border_color: Option<Color>,
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
            selected_color: None,
            border_color: None,
            hover_border_color: None,
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

    /// Set the selected indicator color
    pub fn selected_color(mut self, color: impl Into<Color>) -> Self {
        self.config.selected_color = Some(color.into());
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
