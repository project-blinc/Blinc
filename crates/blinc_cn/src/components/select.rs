//! Select component for dropdown value selection
//!
//! A themed select dropdown with click-to-open and keyboard navigation. It
//! reads and writes a `Signal<String>`; `label` and `disabled` are bound in
//! place.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let fruit = ctx.use_signal("apple".to_string());
//!
//!     cn::select(fruit)
//!         .placeholder("Choose a fruit...")
//!         .option("apple", "Apple")
//!         .option("banana", "Banana")
//!         .option("cherry", "Cherry")
//!         .on_change(|value| println!("Selected: {}", value))
//! }
//!
//! cn::select(value).size(SelectSize::Large)
//! cn::select(value).disabled(locked)
//! cn::select(value).label("Favorite Fruit")
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_core::context_state::BlincContextState;
use blinc_core::owner::Owner;
use blinc_core::reactive::{ReactiveGraph, Signal, computed};
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::click_outside;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::prelude::*;
use blinc_layout::stateful::ButtonState;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

use crate::ButtonVariant;

use super::label::{LabelSize, label};
use blinc_layout::InstanceKey;

/// Select size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectSize {
    /// Small select (height: 32px, text: 13px)
    Small,
    /// Medium select (height: 40px, text: 14px)
    #[default]
    Medium,
    /// Large select (height: 48px, text: 16px)
    Large,
}

impl SelectSize {
    /// Get the height for this size
    fn height(&self) -> f32 {
        match self {
            SelectSize::Small => 32.0,
            SelectSize::Medium => 40.0,
            SelectSize::Large => 48.0,
        }
    }

    /// Get the font size for this size
    fn font_size(&self) -> f32 {
        match self {
            SelectSize::Small => 13.0,
            SelectSize::Medium => 14.0,
            SelectSize::Large => 16.0,
        }
    }

    /// Get the padding for this size
    fn padding(&self) -> f32 {
        match self {
            SelectSize::Small => 8.0,
            SelectSize::Medium => 12.0,
            SelectSize::Large => 16.0,
        }
    }
}

/// Content builder for select options
pub type OptionContentFn = Arc<dyn Fn() -> Div + Send + Sync>;

/// An option in the select dropdown
#[derive(Clone)]
pub struct SelectOption {
    /// The value (stored in state when selected)
    pub value: String,
    /// The display label shown in UI (used for trigger display)
    pub label: String,
    /// Custom content builder for the dropdown item (if None, uses label)
    pub content: Option<OptionContentFn>,
    /// Whether this option is disabled
    pub disabled: bool,
}

impl std::fmt::Debug for SelectOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectOption")
            .field("value", &self.value)
            .field("label", &self.label)
            .field("content", &self.content.is_some())
            .field("disabled", &self.disabled)
            .finish()
    }
}

impl SelectOption {
    /// Create a new option with value and label
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            content: None,
            disabled: false,
        }
    }

    /// Set custom content for the dropdown item
    ///
    /// The content builder is called to render the dropdown item.
    /// The label is still used for the trigger display when selected.
    pub fn content<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Div + Send + Sync + 'static,
    {
        self.content = Some(Arc::new(f));
        self
    }

    /// Mark this option as disabled
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// Select component
///
/// A dropdown select with click-to-open and item selection.
/// Uses state-driven reactivity for proper persistence across UI rebuilds.
pub struct Select {
    /// The fully-built inner element
    inner: Div,
}

impl Select {
    /// Create from a full configuration
    fn from_config(instance_key: &str, config: SelectConfig) -> Self {
        let theme = ThemeState::get();
        let height = config.size.height();
        let font_size = config.size.font_size();
        let padding = config.size.padding();
        let radius = theme.radius(RadiusToken::Md);

        // Colors

        let disabled = config.disabled.clone();

        // Whether the list is showing; kept per instance across rebuilds.
        let open_key = format!("{}_open", instance_key);
        let open_state = BlincContextState::get().use_signal_keyed(&open_key, || false);

        let dropdown_width = config.width.unwrap_or(200.0);

        // The select is built once. Its fill and border are `.cn-select-trigger`
        // rules over `:hover`, `:active`, `:disabled` and `data-state="open"`;
        // the shown value is rebuilt when the value changes, and the list is
        // built while it is open.

        // Chevron SVG (down arrow)
        let chevron_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

        // The value shown, as a one-row list keyed by the value. Its text
        // inherits the trigger's colour, which `[data-placeholder]` and
        // `:disabled` rules dim.
        let value = config.value;
        let shown = computed(move |g: &ReactiveGraph| vec![g.get(value).unwrap_or_default()]);
        let placeholder_shown = {
            let options = config.options.clone();
            computed(move |g: &ReactiveGraph| {
                let current = g.get(value).unwrap_or_default();
                !options.iter().any(|opt| opt.value == current)
            })
        };
        let options_for_display = config.options.clone();
        let placeholder = config
            .placeholder
            .clone()
            .unwrap_or_else(|| blinc_layout::binding::Reactive::Const("Select...".to_string()));
        let content_wrapper = div().overflow_clip().flex_1().for_each(
            shown,
            |value: &String| value.clone(),
            move |current: String| -> Div {
                let selected = options_for_display.iter().find(|opt| opt.value == current);
                match selected {
                    Some(opt) => match opt.content {
                        Some(ref content_fn) => content_fn(),
                        None => div()
                            .h_fit()
                            .overflow_clip()
                            .child(text(&opt.label).size(font_size).no_wrap().no_cursor()),
                    },
                    None => div().h_fit().overflow_clip().child(
                        blinc_layout::text::Text::bound(placeholder.clone())
                            .size(font_size)
                            .no_wrap()
                            .no_cursor(),
                    ),
                }
            },
        );

        // Trigger button — click handler is on the trigger itself (not the wrapper)
        // so clicking dropdown items does NOT re-toggle the dropdown.
        let open_state_trigger = open_state;
        let trigger = div()
            .class("cn-select-trigger")
            .flex_row()
            .w(dropdown_width)
            .items_center()
            .justify_between()
            .h(height)
            // Side padding only: the height and centring place the line.
            .padding_x_px(padding)
            .attr_when("data-state", "open", open_state)
            .attr_when("disabled", "", disabled.clone())
            .attr_when("data-placeholder", "", &placeholder_shown)
            .border_width(1.0)
            .rounded(radius)
            .overflow_clip()
            .flex_shrink_0()
            .child(content_wrapper)
            // Turns while the list is open: `.cn-chevron` rules key it on the
            // trigger's `data-state`, so it eases and does not flip.
            .child(
                div()
                    .class("cn-chevron")
                    .ml(1.0)
                    .flex_shrink_0()
                    .child(svg(chevron_svg).size(16.0, 16.0)),
            )
            .on_click({
                let disabled = disabled.clone();
                move |_ctx| {
                    if !disabled.now().unwrap_or(false) {
                        open_state_trigger.set(!open_state_trigger.try_get().unwrap_or(false));
                    }
                }
            });

        // Unique element ID for click-outside detection
        let wrapper_id = format!("cn-select-{}", instance_key);
        let options_for_dropdown = config.options.clone();
        let on_change_for_dropdown = config.on_change.clone();
        let virtual_rows = config.virtual_rows;
        let value_for_dropdown = config.value;
        let open_state_for_dropdown = open_state;
        let instance_key_owned = instance_key.to_string();
        let wrapper_id_for_list = wrapper_id.clone();

        // The wrapper is positioned so the list can sit absolutely below the
        // trigger.
        let select_element = div()
            .class("cn-select")
            .id(&wrapper_id)
            .relative()
            .overflow_visible()
            .w(dropdown_width)
            .child(trigger)
            .show(open_state, move || {
                // A click outside closes the list for as long as it is open.
                let dismiss = open_state_for_dropdown;
                click_outside::register_click_outside(
                    &wrapper_id_for_list,
                    &wrapper_id_for_list,
                    move || dismiss.set(false),
                );
                let registered = wrapper_id_for_list.clone();
                Owner::on_cleanup(move || click_outside::unregister_click_outside(&registered));

                build_dropdown_content(
                    &options_for_dropdown,
                    &value_for_dropdown.try_get().unwrap_or_default(),
                    value_for_dropdown,
                    open_state_for_dropdown,
                    &on_change_for_dropdown,
                    &instance_key_owned,
                    dropdown_width,
                    height,
                    font_size,
                    radius,
                    virtual_rows,
                )
            });

        // If there's a label, wrap in a container
        let inner = if let Some(label_text) = config.label {
            let spacing = theme.spacing_value(SpacingToken::Space2);
            let mut outer = div().flex_col().gap_px(spacing).w(dropdown_width).h_fit();

            let lbl = label(label_text).size(LabelSize::Medium).disabled(disabled);

            outer = outer.child(lbl).child(select_element);
            outer
        } else {
            div().child(select_element)
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

impl ElementBuilder for Select {
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

/// Internal configuration for building a Select
#[derive(Clone)]
#[allow(clippy::type_complexity)]
struct SelectConfig {
    value: Signal<String>,
    options: Vec<SelectOption>,
    placeholder: Option<blinc_layout::binding::Reactive<String>>,
    label: Option<Reactive<String>>,
    size: SelectSize,
    disabled: Reactive<bool>,
    width: Option<f32>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    /// The fixed row height of a virtual list, which builds only the rows
    /// in view.
    virtual_rows: Option<f32>,
}

impl SelectConfig {
    fn new(value: Signal<String>) -> Self {
        Self {
            value,
            options: Vec::new(),
            placeholder: None,
            label: None,
            size: SelectSize::default(),
            disabled: Reactive::Const(false),
            width: None,
            on_change: None,
            virtual_rows: None,
        }
    }
}

/// Builder for creating Select components with fluent API
pub struct SelectBuilder {
    key: InstanceKey,
    config: SelectConfig,
    /// Cached built Select - built lazily on first access
    built: OnceCell<Select>,
}

impl SelectBuilder {
    /// A select that reads and writes `value`.
    ///
    /// Whether the list is open is kept per instance, keyed by the call site.
    #[track_caller]
    pub fn new(value: Signal<String>) -> Self {
        Self {
            key: InstanceKey::new("select"),
            config: SelectConfig::new(value),
            built: OnceCell::new(),
        }
    }

    /// Create a select builder with an explicit key
    pub fn with_key(key: impl Into<String>, value: Signal<String>) -> Self {
        Self {
            key: InstanceKey::explicit(key),
            config: SelectConfig::new(value),
            built: OnceCell::new(),
        }
    }

    /// Get or build the inner Select
    fn get_or_build(&self) -> &Select {
        ::blinc_layout::build_once::build_once(&self.built, || {
            Select::from_config(self.key.get(), self.config.clone())
        })
    }

    /// Add an option with value and label
    pub fn option(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.config.options.push(SelectOption::new(value, label));
        self
    }

    /// Add a disabled option
    pub fn option_disabled(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.config
            .options
            .push(SelectOption::new(value, label).disabled());
        self
    }

    /// Add multiple options
    pub fn options(mut self, options: impl IntoIterator<Item = SelectOption>) -> Self {
        self.config.options.extend(options);
        self
    }

    /// Set the placeholder text. A signal here replaces the text in place.
    pub fn placeholder(
        mut self,
        placeholder: impl blinc_layout::binding::IntoReactive<String>,
    ) -> Self {
        self.config.placeholder = Some(placeholder.into_reactive());
        self
    }

    /// A label above the select: a string, Signal or Computed.
    pub fn label(mut self, label: impl IntoReactive<String>) -> Self {
        self.config.label = Some(label.into_reactive());
        self
    }

    /// Set the select size
    pub fn size(mut self, size: SelectSize) -> Self {
        self.config.size = size;
        self
    }

    /// Whether it is disabled: a bool, Signal or Computed.
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = disabled.into_reactive();
        self
    }

    /// Set a fixed width
    pub fn w(mut self, width: f32) -> Self {
        self.config.width = Some(width);
        self
    }

    /// Set the change callback
    /// Build only the options in view, each `row_height` px tall, so a list
    /// of hundreds opens at once.
    pub fn virtual_rows(mut self, row_height: f32) -> Self {
        self.config.virtual_rows = Some(row_height.max(1.0));
        self
    }

    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(callback));
        self
    }
}

impl ElementBuilder for SelectBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().inner.element_type_id()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        Some(self.get_or_build().inner.event_handlers())
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().inner.element_classes()
    }
}

/// A select that reads and writes `value`.
///
/// The shown value follows the signal in place, and `label` and `disabled`
/// are bound. Whether the list is open is kept per instance.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
///     let fruit = ctx.use_signal("apple".to_string());
///
///     cn::select(fruit)
///         .placeholder("Choose a fruit...")
///         .option("apple", "Apple")
///         .option("banana", "Banana")
///         .on_change(|v| println!("Selected: {}", v))
/// }
/// ```
#[track_caller]
pub fn select(value: Signal<String>) -> SelectBuilder {
    SelectBuilder::new(value)
}

/// The tallest the open list grows before it scrolls.
const LIST_MAX_HEIGHT: f32 = 300.0;

/// Build the dropdown content as an absolutely positioned child.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn build_dropdown_content(
    options: &[SelectOption],
    current_selected: &str,
    value: Signal<String>,
    open_state: Signal<bool>,
    on_change: &Option<Arc<dyn Fn(&str) + Send + Sync>>,
    key: &str,
    width: f32,
    trigger_height: f32,
    font_size: f32,
    radius: f32,
    virtual_rows: Option<f32>,
) -> Div {
    let dropdown_id = key;

    let dropdown_div = div()
        .class("cn-select-content")
        .id(dropdown_id)
        .flex_col()
        .w(width)
        // Fill, border, radius and shadow are `.cn-select-content` rules; the
        // radius stays here too because the panel clips its rows to it.
        .rounded(radius)
        .overflow_clip()
        .h_fit()
        // Absolutely positioned below the trigger, rendered in foreground pass
        // so it appears above all sibling content regardless of tree order
        .absolute()
        .top(trigger_height + 4.0)
        .left(0.0)
        .foreground();

    // A long list scrolls, and opens with the chosen option in the middle.
    let reveal = blinc_layout::selector::ScrollRef::new();
    let list = div()
        .class("cn-select-list")
        .flex_col()
        .w_full()
        .max_h(LIST_MAX_HEIGHT)
        .overflow_y_scroll()
        .bind_scroll(&reveal);
    let row = OptionRow {
        options: Arc::new(options.to_vec()),
        current: current_selected.to_string(),
        value,
        open_state,
        on_change: on_change.clone(),
        key: key.to_string(),
        font_size,
    };

    let Some(row_height) = virtual_rows else {
        let mut list = list;
        for (idx, option) in options.iter().enumerate() {
            if option.value == current_selected {
                reveal.scroll_to(&row.id(idx));
            }
            list = list.child(row.build(idx));
        }
        return dropdown_div.child(list);
    };

    // Virtual: only the rows in view, and a few either side, exist, placed
    // over a spacer as tall as every row.
    let count = options.len();
    let total = count as f32 * row_height;
    let view = LIST_MAX_HEIGHT.min(total);
    let initial = options
        .iter()
        .position(|o| o.value == current_selected)
        .map_or(0.0, |i| {
            (i as f32 * row_height + row_height / 2.0 - view / 2.0).clamp(0.0, total - view)
        });
    if initial > 0.0 {
        reveal.set_scroll_offset(0.0, initial);
    }
    let offset = reveal.watch_y(initial);
    let window = computed(move |g: &ReactiveGraph| {
        let y = g.get(offset).unwrap_or(initial);
        let first = ((y / row_height) as usize).saturating_sub(OVERSCAN);
        let last = (((y + view) / row_height).ceil() as usize + OVERSCAN).min(count);
        (first..last).collect::<Vec<usize>>()
    });
    // Full height, never shrunk to the list: the culling of an enclosing
    // `viewport_cull` scroll tests this box, and the rows sit inside it.
    let rows = div().relative().w_full().h(total).flex_shrink_0().for_each(
        &window,
        |idx: &usize| *idx,
        move |idx: usize| {
            row.build(idx)
                .absolute()
                .left(0.0)
                .right(0.0)
                .top(idx as f32 * row_height)
                .h(row_height)
        },
    );
    dropdown_div.child(list.h(view).child(rows))
}

/// Rows built either side of those in view in a virtual list.
const OVERSCAN: usize = 4;

/// What an option row needs, shared by every row of one open list.
#[derive(Clone)]
struct OptionRow {
    options: Arc<Vec<SelectOption>>,
    current: String,
    value: Signal<String>,
    open_state: Signal<bool>,
    #[allow(clippy::type_complexity)]
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    key: String,
    font_size: f32,
}

impl OptionRow {
    fn id(&self, idx: usize) -> String {
        format!("{}_opt_{}", self.key, idx)
    }

    /// The row of option `idx`. Its fill and text colour, selected, hovered
    /// or disabled, are `.cn-select-item` rules.
    fn build(&self, idx: usize) -> Div {
        let opt = &self.options[idx];
        let chosen = opt.value.clone();
        let disabled = opt.disabled;
        let (value, open_state, on_change) = (self.value, self.open_state, self.on_change.clone());
        let mut item = div()
            .id(self.id(idx))
            .class("cn-select-item")
            .w_full()
            .h_fit()
            .attr_when("disabled", "", disabled)
            .flex_row()
            .items_center()
            .child(match opt.content {
                Some(ref content_fn) => content_fn(),
                None => div().child(text(&opt.label).size(self.font_size).no_cursor()),
            })
            .on_click(move |_ctx| {
                if !disabled {
                    value.set(chosen.clone());
                    open_state.set(false);
                    if let Some(ref cb) = on_change {
                        cb(&chosen);
                    }
                }
            });
        if opt.value == self.current {
            item = item.class("cn-select-item--selected");
        }
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_sizes() {
        assert_eq!(SelectSize::Small.height(), 32.0);
        assert_eq!(SelectSize::Medium.height(), 40.0);
        assert_eq!(SelectSize::Large.height(), 48.0);
    }

    #[test]
    fn test_select_font_sizes() {
        assert_eq!(SelectSize::Small.font_size(), 13.0);
        assert_eq!(SelectSize::Medium.font_size(), 14.0);
        assert_eq!(SelectSize::Large.font_size(), 16.0);
    }

    #[test]
    fn test_select_option() {
        let opt = SelectOption::new("value", "Label");
        assert_eq!(opt.value, "value");
        assert_eq!(opt.label, "Label");
        assert!(!opt.disabled);

        let disabled_opt = opt.disabled();
        assert!(disabled_opt.disabled);
    }
}
