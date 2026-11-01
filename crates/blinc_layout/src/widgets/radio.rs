//! Ready-to-use Radio Group widget
//!
//! A radio button group. Each option's ring is an element with a `checked`
//! attribute while its option is selected and `disabled` while it cannot be
//! chosen, so its look, and how it eases between states, is the `.blinc-radio`
//! rules of [`crate::ua_styles::UA_STYLES`]. Restyle it with rules of your own:
//!
//! ```css
//! .my-radio:checked { border-color: #16a34a; }
//! ```
//!
//! # Example
//!
//! ```ignore
//! div().child(
//!     radio_group(choice)
//!         .label("Theme")
//!         .horizontal()
//!         .option("light", "Light")
//!         .option("dark", "Dark")
//!         .on_change(|value| println!("Selected: {}", value))
//! )
//! ```

use std::sync::Arc;

use blinc_core::reactive::{ReactiveGraph, Signal, computed};

use crate::binding::{IntoReactive, Reactive};
use crate::div::{ElementBuilder, div};
use crate::element::RenderProps;
use crate::element_style::ElementStyle;
use crate::key::InstanceKey;
use crate::text::{Text, text};
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
    /// The selected value.
    selected: Signal<String>,
    /// Radio options
    options: Vec<RadioOption>,
    /// Layout direction
    pub layout: RadioLayout,
    /// Label text above the group; bound in place.
    label: Option<Reactive<String>>,
    /// Whether every option is disabled; bound in place.
    disabled: Reactive<bool>,
    /// Gap between options
    pub gap: f32,
    /// Outer circle size
    pub outer_size: f32,
    /// Inner dot size
    pub inner_size: f32,
    /// Border width
    pub border_width: f32,
    /// Label font size
    pub label_font_size: f32,
    /// CSS group ID for stylesheet matching
    pub css_group_id: Option<String>,
    /// Change handler
    pub on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

impl RadioGroupConfig {
    fn new(selected: Signal<String>) -> Self {
        Self {
            selected,
            options: Vec::new(),
            layout: RadioLayout::default(),
            label: None,
            disabled: Reactive::Const(false),
            gap: 8.0,
            outer_size: 18.0,
            inner_size: 8.0,
            border_width: 2.0,
            label_font_size: 14.0,
            css_group_id: None,
            on_change: None,
        }
    }
}

/// Build a single radio button: a row holding the ring and the label. The
/// ring follows the selected value in place, and its rules do the rest.
fn build_radio_button(
    config: &RadioGroupConfig,
    option: &RadioOption,
    button_css_id: Option<String>,
) -> crate::div::Div {
    let outer_size = config.outer_size;
    let inner_size = config.inner_size;

    let option_value = option.value.clone();
    let option_label = option.label.clone();
    let selected = config.selected;
    let on_change = config.on_change.clone();
    let value_for_click = option.value.clone();
    // An option is disabled on its own, or with the group.
    let disabled = if option.disabled {
        Reactive::Const(true)
    } else {
        config.disabled.clone()
    };

    let is_selected =
        computed(move |g: &ReactiveGraph| g.get(selected).unwrap_or_default() == option_value);

    // The ring is the element; the dot is always there and shown while
    // checked. Sizes and the border width are layout, so they are set here.
    let ring = div()
        .class(RING_CLASS)
        .attr_when("checked", "", &is_selected)
        .attr_when("disabled", "", disabled.clone())
        .w(outer_size)
        .h(outer_size)
        .rounded(outer_size / 2.0)
        .border_width(config.border_width)
        .items_center()
        .justify_center()
        .child(
            div()
                .class("blinc-radio-dot")
                .w(inner_size)
                .h(inner_size)
                .rounded(inner_size / 2.0),
        );

    // The row carries `disabled` too, so the label's colour is a rule.
    let mut radio = div()
        .class("blinc-radio-row")
        .attr_when("disabled", "", disabled.clone())
        .flex_row()
        .gap(config.gap)
        .items_center()
        .cursor_pointer()
        .child(ring)
        .child(
            text(&option_label)
                .class("blinc-radio-label")
                .size(config.label_font_size),
        );
    if let Some(ref css_id) = button_css_id {
        radio = radio.id(css_id);
    }

    radio.on_click(move |_| {
        if disabled.now().unwrap_or(false) {
            return;
        }
        if selected.try_get().as_deref() != Some(value_for_click.as_str()) {
            selected.set(value_for_click.clone());
            if let Some(ref callback) = on_change {
                callback(&value_for_click);
            }
        }
    })
}

/// The class whose rules in [`crate::ua_styles::UA_STYLES`] give a radio
/// ring its default look.
pub const RING_CLASS: &str = "blinc-radio";

/// The fully-built radio group component (Div containing radio buttons and optional label)
pub struct RadioGroup {
    inner: crate::div::Div,
}

impl RadioGroup {
    /// Build from a full configuration
    fn with_config(_instance_key: &InstanceKey, config: RadioGroupConfig) -> Self {
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
            options_container =
                options_container.child(build_radio_button(&config, option, button_css_id));
        }

        // The group carries `disabled`, so its label's colour is a rule.
        let inner = if let Some(label) = config.label {
            div()
                .flex_col()
                .gap(6.0)
                .child(
                    Text::bound(label)
                        .class("blinc-radio-group-label")
                        .size(13.0),
                )
                .child(options_container)
        } else {
            options_container
        }
        .class("blinc-radio-group")
        .attr_when("disabled", "", config.disabled);

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
    /// A radio group that reads and writes `selected`.
    #[track_caller]
    pub fn new(selected: Signal<String>) -> Self {
        Self {
            key: InstanceKey::new("radio"),
            config: RadioGroupConfig::new(selected),
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

    /// A label above the group: a string, Signal or Computed.
    pub fn label(mut self, label: impl IntoReactive<String>) -> Self {
        self.config.label = Some(label.into_reactive());
        self
    }

    /// Whether every option is disabled: a bool, Signal or Computed.
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = disabled.into_reactive();
        self
    }

    /// Set the gap between options
    pub fn gap(mut self, gap: f32) -> Self {
        self.config.gap = gap;
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

/// A radio group that reads and writes `selected`.
///
/// The options restyle in place from the selected value, `disabled` and their
/// pointer signals; the group does not rebuild when any of them changes.
/// Use `.id("name")` to enable CSS styling via `:hover`, `:checked`, `:disabled`.
///
/// # Example
///
/// ```ignore
/// let choice = ctx.use_signal("a".to_string());
/// radio_group(choice)
///     .label("Pick one")
///     .option("a", "Option A")
///     .option("b", "Option B")
///     .on_change(|value| println!("Selected: {}", value))
/// ```
#[track_caller]
pub fn radio_group(selected: Signal<String>) -> RadioGroupBuilder {
    RadioGroupBuilder::new(selected)
}
