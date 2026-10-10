//! Ready-to-use Radio Group widget
//!
//! A radio button group with built-in selection, hover states, and CSS styling support.
//! Each option's look is bound to the selected value and to the pointer
//! signals of an [`Interaction`], so a selection or a hover restyles it in
//! place.
//! Follows the lazy `OnceCell` initialization pattern from `blinc_cn`.
//!
//! # Example
//!
//! ```ignore
//! div().child(
//!     radio_group(&state)
//!         .label("Theme")
//!         .horizontal()
//!         .option("light", "Light")
//!         .option("dark", "Dark")
//!         .on_change(|value| println!("Selected: {}", value))
//! )
//! ```

use std::sync::Arc;

use blinc_core::reactive::{Computed, ReactiveGraph, Signal, computed};
use blinc_core::{Color, State, Transform};
use blinc_theme::{ColorToken, ThemeState};

use crate::css_parser::{ElementState, Stylesheet, active_stylesheet};
use crate::div::{ElementBuilder, div};
use crate::element::RenderProps;
use crate::element_style::ElementStyle;
use crate::interaction::Interaction;
use crate::key::InstanceKey;
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};

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

/// Radio group configuration
///
/// Color fields are `Option<Color>` — when `None`, theme defaults are resolved
/// fresh inside the `on_state` callback so the widget reacts to theme changes.
#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct RadioGroupConfig {
    /// Shared radio state (selected value)
    selected: State<String>,
    /// Radio options
    options: Vec<RadioOption>,
    /// Layout direction
    pub layout: RadioLayout,
    /// Label text above the group (optional)
    pub label: Option<String>,
    /// Whether all options are disabled
    pub disabled: bool,
    /// Gap between options
    pub gap: f32,
    /// Outer circle size
    pub outer_size: f32,
    /// Inner dot size
    pub inner_size: f32,
    /// Border width
    pub border_width: f32,
    /// Selected indicator color (None = theme Primary)
    pub selected_color: Option<Color>,
    /// Border color (None = theme BorderSecondary)
    pub border_color: Option<Color>,
    /// Hover border color (None = theme Primary)
    pub hover_border_color: Option<Color>,
    /// Label color (None = theme TextPrimary)
    pub label_color: Option<Color>,
    /// Label font size
    pub label_font_size: f32,
    /// CSS group ID for stylesheet matching
    pub css_group_id: Option<String>,
    /// Change handler
    pub on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

impl RadioGroupConfig {
    fn new(selected: State<String>) -> Self {
        Self {
            selected,
            options: Vec::new(),
            layout: RadioLayout::default(),
            label: None,
            disabled: false,
            gap: 8.0,
            outer_size: 18.0,
            inner_size: 8.0,
            border_width: 2.0,
            selected_color: None,
            border_color: None,
            hover_border_color: None,
            label_color: None,
            label_font_size: 14.0,
            css_group_id: None,
            on_change: None,
        }
    }
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
/// Cascading order: base -> :checked -> :hover -> :disabled (highest priority)
fn apply_css_overrides_radio(
    stylesheet: &Stylesheet,
    element_id: &str,
    is_selected: bool,
    is_hovered: bool,
    is_disabled: bool,
    overrides: &mut RadioStyleOverrides,
) {
    if let Some(base) = stylesheet.get(element_id) {
        overrides.apply(base);
    }
    if is_selected {
        if let Some(s) = stylesheet.get_with_state(element_id, ElementState::Checked) {
            overrides.apply(s);
        }
    }
    if is_hovered {
        if let Some(s) = stylesheet.get_with_state(element_id, ElementState::Hover) {
            overrides.apply(s);
        }
    }
    if is_disabled {
        if let Some(s) = stylesheet.get_with_state(element_id, ElementState::Disabled) {
            overrides.apply(s);
        }
    }
}

/// What a radio button resolves its look from, apart from its state.
struct RadioContext {
    option_value: String,
    option_disabled: bool,
    css_id: Option<String>,
    selected_color: Option<Color>,
    border_color: Option<Color>,
    hover_border_color: Option<Color>,
    label_color: Option<Color>,
}

/// What a radio button draws in one state.
#[derive(Clone)]
struct Look {
    border_color: Color,
    background: Color,
    ring_scale: Transform,
    dot_color: Color,
    dot_scale: Transform,
    label_color: Color,
    opacity: f32,
}

impl Look {
    fn resolve(
        ctx: &RadioContext,
        selected_value: &str,
        is_hovered: bool,
        is_pressed: bool,
    ) -> Self {
        let theme = ThemeState::get();
        let is_selected = selected_value == ctx.option_value;

        // Resolve colors from theme
        let mut overrides = RadioStyleOverrides {
            selected_color: ctx
                .selected_color
                .unwrap_or_else(|| theme.color(ColorToken::Primary)),
            border_color: ctx
                .border_color
                .unwrap_or_else(|| theme.color(ColorToken::BorderSecondary)),
            hover_border_color: ctx
                .hover_border_color
                .unwrap_or_else(|| theme.color(ColorToken::Primary)),
            label_color: ctx.label_color.unwrap_or_else(|| {
                if ctx.option_disabled {
                    theme.color(ColorToken::TextTertiary)
                } else {
                    theme.color(ColorToken::TextPrimary)
                }
            }),
            opacity: None,
            background: None,
        };

        // Apply CSS overrides if we have an element ID
        if let Some(ref css_id) = ctx.css_id {
            if let Some(stylesheet) = active_stylesheet() {
                apply_css_overrides_radio(
                    &stylesheet,
                    css_id,
                    is_selected,
                    is_hovered,
                    ctx.option_disabled,
                    &mut overrides,
                );
            }
        }

        // Border color based on state
        let border_color = if is_selected {
            overrides.selected_color
        } else if is_hovered && !ctx.option_disabled {
            overrides.hover_border_color
        } else {
            overrides.border_color
        };

        // Scale effect on hover
        let scale = if is_hovered && !ctx.option_disabled {
            1.05
        } else {
            1.0
        };

        // Scale effect on press for inner dot
        let inner_scale = if is_pressed && !ctx.option_disabled {
            0.8
        } else {
            1.0
        };

        Self {
            border_color,
            background: overrides.background.unwrap_or(Color::TRANSPARENT),
            ring_scale: Transform::scale(scale, scale),
            dot_color: overrides.selected_color,
            dot_scale: Transform::scale(inner_scale, inner_scale),
            label_color: overrides.label_color,
            opacity: overrides
                .opacity
                .unwrap_or(if ctx.option_disabled { 0.5 } else { 1.0 }),
        }
    }
}

/// A value taken from the look, kept current by the signals it is resolved from.
fn follow_look<T: Clone + Send + 'static>(
    ctx: Arc<RadioContext>,
    selected: Signal<String>,
    hovered: Signal<bool>,
    pressed: Signal<bool>,
    pick: fn(&Look) -> T,
) -> Computed<T> {
    computed(move |g: &ReactiveGraph| {
        pick(&Look::resolve(
            &ctx,
            &g.get(selected).unwrap_or_default(),
            g.get(hovered).unwrap_or(false),
            g.get(pressed).unwrap_or(false),
        ))
    })
}

/// Build a single radio button. Selection and the pointer state are signals,
/// and what they change is bound, so a change patches the nodes in place.
fn build_radio_button(
    config: &RadioGroupConfig,
    option: &RadioOption,
    instance_key: &InstanceKey,
    button_css_id: Option<String>,
) -> crate::div::Div {
    let outer_size = config.outer_size;
    let inner_size = config.inner_size;
    let border_width = config.border_width;

    let option_disabled = option.disabled || config.disabled;
    let option_label = option.label.clone();
    let selected_state = config.selected.clone();
    let selected_state_for_click = config.selected.clone();
    let on_change = config.on_change.clone();
    let value_for_click = option.value.clone();

    let ctx = Arc::new(RadioContext {
        option_value: option.value.clone(),
        option_disabled,
        css_id: button_css_id.clone(),
        selected_color: config.selected_color,
        border_color: config.border_color,
        hover_border_color: config.hover_border_color,
        label_color: config.label_color,
    });
    let label_font_size = config.label_font_size;

    // Derive the interaction key from the group's instance key + option value
    let interaction = Interaction::keyed(&instance_key.derive(&option.value));

    let (selected, hovered, pressed) = (
        selected_state.signal(),
        interaction.hovered().signal(),
        interaction.pressed().signal(),
    );
    macro_rules! follow {
        ($pick:expr) => {
            follow_look(Arc::clone(&ctx), selected, hovered, pressed, $pick)
        };
    }
    let is_selected = {
        let ctx = Arc::clone(&ctx);
        computed(move |g: &ReactiveGraph| g.get(selected).unwrap_or_default() == ctx.option_value)
    };

    // Build the radio circle (outer ring)
    let circle = div()
        .w(outer_size)
        .h(outer_size)
        .rounded(outer_size / 2.0)
        .border_width(border_width)
        .border_color(follow!(|l| l.border_color))
        .bg(follow!(|l| l.background))
        .items_center()
        .justify_center()
        .transform(follow!(|l| l.ring_scale.clone()))
        // The dot is always there and shown while selected.
        .child(
            div()
                .w(inner_size)
                .h(inner_size)
                .rounded(inner_size / 2.0)
                .bg(follow!(|l| l.dot_color))
                .transform(follow!(|l| l.dot_scale.clone()))
                .visible(&is_selected),
        );

    // Build row with label
    let mut radio = div()
        .flex_row()
        .gap(8.0)
        .items_center()
        .cursor_pointer()
        .opacity(follow!(|l| l.opacity))
        .track(&interaction)
        .child(circle)
        .child(
            text(&option_label)
                .size(label_font_size)
                .color(follow!(|l| l.label_color)),
        );

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

/// The fully-built radio group component (Div containing radio buttons and optional label)
pub struct RadioGroup {
    inner: crate::div::Div,
}

impl RadioGroup {
    /// Build from a full configuration
    fn with_config(instance_key: &InstanceKey, config: RadioGroupConfig) -> Self {
        let gap = config.gap;

        // Build options container
        let mut options_container = match config.layout {
            RadioLayout::Vertical => div().flex_col().gap(gap),
            RadioLayout::Horizontal => div().flex_row().gap(gap).flex_wrap(),
        };

        // Add each radio button
        for option in &config.options {
            let button_css_id = config.css_group_id.as_ref().map(|group_id| {
                let sanitized = option.value.replace(' ', "-");
                format!("{group_id}-{sanitized}")
            });
            options_container = options_container.child(build_radio_button(
                &config,
                option,
                instance_key,
                button_css_id,
            ));
        }

        // If there's a label, wrap everything
        let inner = if let Some(ref label_text) = config.label {
            let theme = ThemeState::get();
            let label_color = if config.disabled {
                theme.color(ColorToken::TextTertiary)
            } else {
                theme.color(ColorToken::TextSecondary)
            };

            div()
                .flex_col()
                .gap(6.0)
                .child(text(label_text).size(13.0).color(label_color))
                .child(options_container)
        } else {
            options_container
        };

        Self { inner }
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

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        self.inner.element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("radio")
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Builder for creating RadioGroup components with fluent API.
/// Config is accumulated via builder methods; the actual RadioGroup is built lazily
/// via `OnceCell::get_or_init()` on first access (build/render_props/children).
pub struct RadioGroupBuilder {
    key: InstanceKey,
    config: RadioGroupConfig,
    /// Cached built RadioGroup — built lazily on first access
    built: std::cell::OnceCell<RadioGroup>,
}

impl RadioGroupBuilder {
    /// Create a new radio group builder with reactive state
    #[track_caller]
    pub fn new(selected: &State<String>) -> Self {
        Self {
            key: InstanceKey::new("radio"),
            config: RadioGroupConfig::new(selected.clone()),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner RadioGroup
    fn get_or_build(&self) -> &RadioGroup {
        self.built
            .get_or_init(|| RadioGroup::with_config(&self.key, self.config.clone()))
    }

    /// Set the CSS group ID for stylesheet matching
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
        self.config.gap = gap;
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
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(callback));
        self
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

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("radio")
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// Create a radio group with reactive state
///
/// The options restyle in place from the selected value and their pointer
/// signals; the group does not rebuild when either changes.
/// Use `.id("name")` to enable CSS styling via `:hover`, `:checked`, `:disabled`.
///
/// # Example
///
/// ```ignore
/// let state = ctx.use_state_keyed("choice", || "a".to_string());
/// radio_group(&state)
///     .label("Pick one")
///     .option("a", "Option A")
///     .option("b", "Option B")
///     .on_change(|value| println!("Selected: {}", value))
/// ```
#[track_caller]
pub fn radio_group(selected: &State<String>) -> RadioGroupBuilder {
    RadioGroupBuilder::new(selected)
}
