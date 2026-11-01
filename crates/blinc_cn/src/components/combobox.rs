//! Combobox component - searchable dropdown selection
//!
//! A themed combobox with text input filtering and keyboard navigation.
//! Uses state-driven reactivity for proper persistence across UI rebuilds.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let country = ctx.use_signal(String::new());
//!
//!     cn::combobox(country)
//!         .placeholder("Search countries...")
//!         .option("us", "United States")
//!         .option("uk", "United Kingdom")
//!         .option("de", "Germany")
//!         .option("fr", "France")
//!         .on_change(|value| println!("Selected: {}", value))
//! }
//!
//! // Different sizes
//! cn::combobox(value)
//!     .size(ComboboxSize::Large)
//!
//! // Disabled while `locked` holds
//! cn::combobox(value)
//!     .disabled(locked)
//!
//! // With label
//! cn::combobox(value)
//!     .label("Country")
//!
//! // Allow custom values (not just from options)
//! cn::combobox(value)
//!     .allow_custom(true)
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_core::context_state::BlincContextState;
use blinc_core::owner::Owner;
use blinc_core::reactive::{ReactiveGraph, Signal, computed};
use blinc_layout::IntoReactive;
use blinc_layout::binding::Reactive;
use blinc_layout::click_outside;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::text_input::SharedTextInputData;
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

use super::label::{LabelSize, label};
use blinc_layout::InstanceKey;

/// Combobox size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ComboboxSize {
    /// Small combobox (height: 32px, text: 13px)
    Small,
    /// Medium combobox (height: 40px, text: 14px)
    #[default]
    Medium,
    /// Large combobox (height: 48px, text: 16px)
    Large,
}

impl ComboboxSize {
    /// Get the height for this size
    fn height(&self) -> f32 {
        match self {
            ComboboxSize::Small => 32.0,
            ComboboxSize::Medium => 40.0,
            ComboboxSize::Large => 48.0,
        }
    }

    /// Get the font size for this size
    fn font_size(&self) -> f32 {
        match self {
            ComboboxSize::Small => 13.0,
            ComboboxSize::Medium => 14.0,
            ComboboxSize::Large => 16.0,
        }
    }

    /// Get the padding for this size
    fn padding(&self) -> f32 {
        match self {
            ComboboxSize::Small => 8.0,
            ComboboxSize::Medium => 12.0,
            ComboboxSize::Large => 16.0,
        }
    }
}

/// Content builder for combobox options
pub type OptionContentFn = Arc<dyn Fn() -> Div + Send + Sync>;

/// An option in the combobox dropdown
#[derive(Clone)]
pub struct ComboboxOption {
    /// The value (stored in state when selected)
    pub value: String,
    /// The display label shown in UI (used for trigger display and filtering)
    pub label: String,
    /// Custom content builder for the dropdown item (if None, uses label)
    pub content: Option<OptionContentFn>,
    /// Whether this option is disabled
    pub disabled: bool,
}

impl std::fmt::Debug for ComboboxOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComboboxOption")
            .field("value", &self.value)
            .field("label", &self.label)
            .field("content", &self.content.is_some())
            .field("disabled", &self.disabled)
            .finish()
    }
}

impl ComboboxOption {
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

    /// Check if this option matches a search query (case-insensitive)
    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query_lower = query.to_lowercase();
        self.label.to_lowercase().contains(&query_lower)
            || self.value.to_lowercase().contains(&query_lower)
    }
}

/// Combobox component
///
/// A searchable dropdown with text input filtering and item selection.
/// Uses state-driven reactivity for proper persistence across UI rebuilds.
pub struct Combobox {
    /// The fully-built inner element
    inner: Div,
}

impl Combobox {
    /// Create from a full configuration
    fn from_config(instance_key: &str, config: ComboboxConfig) -> Self {
        let theme = ThemeState::get();
        let height = config.size.height();
        let font_size = config.size.font_size();
        let padding = config.size.padding();
        let radius = theme.radius(RadiusToken::Sm);

        // Colors
        let text_color = theme.color(ColorToken::TextPrimary);
        let text_tertiary = theme.color(ColorToken::TextTertiary);

        let disabled = config.disabled.clone();

        // Whether the list is showing, the search field and what is typed in
        // it; kept per instance across rebuilds.
        let ctx = BlincContextState::get();
        let open_state = ctx.use_signal_keyed(&format!("{instance_key}_open"), || false);
        let search_input_data: SharedTextInputData = ctx
            .use_signal_keyed(
                &format!("{instance_key}_search"),
                blinc_layout::widgets::text_input::text_input_data,
            )
            .try_get()
            .unwrap_or_else(blinc_layout::widgets::text_input::text_input_data);
        let search_query_state =
            ctx.use_signal_keyed(&format!("{instance_key}_search_query"), String::new);

        let dropdown_width = config.width.unwrap_or(200.0);

        // The combobox is built once. The trigger's text and colour follow the
        // value and the search, and its border is `.cn-combobox-trigger` rules
        // over `:hover`, `:disabled` and `data-state="open"`; the list is built
        // while it is open, and its rows follow the search.
        let (value, open, query) = (config.value, open_state, search_query_state);

        let options_for_display = config.options.clone();
        let placeholder = config
            .placeholder
            .clone()
            .unwrap_or_else(|| "Search...".to_string());
        let display_text = computed(move |g: &ReactiveGraph| {
            let current = g.get(value).unwrap_or_default();
            if let Some(opt) = options_for_display.iter().find(|o| o.value == current) {
                return opt.label.clone();
            }
            if !current.is_empty() {
                return current;
            }
            let search = g.get(query).unwrap_or_default();
            if !search.is_empty() && g.get(open).unwrap_or(false) {
                search
            } else {
                placeholder.clone()
            }
        });
        // Nothing chosen: the trigger carries `data-placeholder`, and its rule
        // dims the shown text, which inherits the trigger's colour.
        let options_for_placeholder = config.options.clone();
        let placeholder_shown = computed(move |g: &ReactiveGraph| {
            let current = g.get(value).unwrap_or_default();
            current.is_empty() && !options_for_placeholder.iter().any(|o| o.value == current)
        });
        // Chevron SVG (down arrow)
        let chevron_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

        // Trigger button — click handler is on the trigger itself (not the wrapper)
        // so clicking dropdown items does NOT re-toggle the dropdown.
        let open_state_trigger = open_state;
        let search_data_trigger = search_input_data.clone();
        let search_query_trigger = search_query_state;
        let disabled_for_click = disabled.clone();
        let trigger = div()
            .class("cn-combobox-trigger")
            .attr_when("data-state", "open", open_state)
            .attr_when("disabled", "", disabled.clone())
            .attr_when("data-placeholder", "", &placeholder_shown)
            .flex_row()
            .w_full()
            .items_center()
            .h(height)
            .p_px(padding)
            .border_width(1.0)
            .rounded(radius)
            .child(
                div().flex_1().overflow_clip().child(
                    Text::bound(display_text.into_reactive())
                        .size(font_size)
                        .no_cursor(),
                ),
            )
            .flex_shrink_0()
            // Turns while the list is open: `.cn-chevron` rules key it on the
            // trigger's `data-state`, so it eases and does not flip.
            .child(
                div()
                    .class("cn-chevron")
                    .ml(1.0)
                    .flex_shrink_0()
                    .child(svg(chevron_svg).size(16.0, 16.0)),
            )
            .cursor_pointer()
            .on_click(move |_ctx| {
                if disabled_for_click.now().unwrap_or(false) {
                    return;
                }
                let is_currently_open = open_state_trigger.try_get().unwrap_or(false);
                if is_currently_open {
                    // Closing: clear search text
                    if let Ok(mut data) = search_data_trigger.lock() {
                        data.value.clear();
                        data.cursor = 0;
                    }
                    search_query_trigger.set(String::new());
                } else {
                    // Opening: focus the search input so the user can
                    // start typing at once.
                    blinc_layout::widgets::text_input::focus_text_input(&search_data_trigger);
                }
                open_state_trigger.set(!is_currently_open);
            });

        // Unique element ID for click-outside detection
        let wrapper_id = format!("cn-combobox-{}", instance_key);
        let wrapper_id_for_list = wrapper_id.clone();
        let options_for_dropdown = config.options.clone();
        let on_change_for_dropdown = config.on_change.clone();
        let value_for_dropdown = config.value;
        let open_state_for_dropdown = open_state;
        let search_data_for_dropdown = search_input_data.clone();
        let search_query_for_dropdown = search_query_state;
        let allow_custom = config.allow_custom;
        let placeholder_for_content = config.placeholder.clone();
        let instance_key_owned = instance_key.to_string();

        // The wrapper is positioned so the list can sit absolutely below the
        // trigger.
        let combobox_element = div()
            .class("cn-combobox")
            .id(&wrapper_id)
            .relative()
            .overflow_visible()
            .w(dropdown_width)
            .child(trigger)
            .show(open_state, move || {
                // A click outside closes the list, and clears the search, for
                // as long as it is open.
                let dismiss_state = open_state_for_dropdown;
                let dismiss_search_data = search_data_for_dropdown.clone();
                let dismiss_search_query = search_query_for_dropdown;
                click_outside::register_click_outside(
                    &wrapper_id_for_list,
                    &wrapper_id_for_list,
                    move || {
                        dismiss_state.set(false);
                        if let Ok(mut data) = dismiss_search_data.lock() {
                            data.value.clear();
                            data.cursor = 0;
                        }
                        dismiss_search_query.set(String::new());
                    },
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
                    &search_data_for_dropdown,
                    search_query_for_dropdown,
                    dropdown_width,
                    height,
                    font_size,
                    padding,
                    radius,
                    text_color,
                    text_tertiary,
                    allow_custom,
                    &placeholder_for_content,
                )
            });

        // Build the outer container with optional label
        let container_width = config.width.unwrap_or(dropdown_width);
        // Dimmed by `.cn-combobox-box:disabled`.
        let combobox_container = div()
            .class("cn-combobox-box")
            .attr_when("disabled", "", disabled.clone())
            .w(container_width)
            .child(combobox_element);

        let inner = if let Some(label_text) = config.label {
            let spacing = theme.spacing_value(SpacingToken::Space2);
            let mut outer = div().flex_col().gap_px(spacing).w(container_width).h_fit();

            let lbl = label(label_text).size(LabelSize::Medium).disabled(disabled);

            outer = outer.child(lbl).child(combobox_container);
            outer
        } else {
            combobox_container
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

impl ElementBuilder for Combobox {
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

/// Internal configuration for building a Combobox
#[derive(Clone)]
#[allow(clippy::type_complexity)]
struct ComboboxConfig {
    value: Signal<String>,
    options: Vec<ComboboxOption>,
    placeholder: Option<String>,
    label: Option<Reactive<String>>,
    size: ComboboxSize,
    disabled: Reactive<bool>,
    width: Option<f32>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    /// Allow entering custom values not in the options list
    allow_custom: bool,
}

impl ComboboxConfig {
    fn new(value: Signal<String>) -> Self {
        Self {
            value,
            options: Vec::new(),
            placeholder: None,
            label: None,
            size: ComboboxSize::default(),
            disabled: Reactive::Const(false),
            width: None,
            on_change: None,
            allow_custom: false,
        }
    }
}

/// Builder for creating Combobox components with fluent API
pub struct ComboboxBuilder {
    key: InstanceKey,
    config: ComboboxConfig,
    /// Cached built Combobox - built lazily on first access
    built: OnceCell<Combobox>,
}

impl ComboboxBuilder {
    /// A combobox that reads and writes `value`.
    ///
    /// Whether the list is open, and the search, are kept per instance, keyed
    /// by the call site.
    #[track_caller]
    pub fn new(value: Signal<String>) -> Self {
        Self {
            key: InstanceKey::new("combobox"),
            config: ComboboxConfig::new(value),
            built: OnceCell::new(),
        }
    }

    /// Create a combobox builder with an explicit key
    pub fn with_key(key: impl Into<String>, value: Signal<String>) -> Self {
        Self {
            key: InstanceKey::explicit(key),
            config: ComboboxConfig::new(value),
            built: OnceCell::new(),
        }
    }

    /// Get or build the inner Combobox
    fn get_or_build(&self) -> &Combobox {
        ::blinc_layout::build_once::build_once(&self.built, || {
            Combobox::from_config(self.key.get(), self.config.clone())
        })
    }

    /// Add an option with value and label
    pub fn option(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.config.options.push(ComboboxOption::new(value, label));
        self
    }

    /// Add a disabled option
    pub fn option_disabled(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.config
            .options
            .push(ComboboxOption::new(value, label).disabled());
        self
    }

    /// Add multiple options
    pub fn options(mut self, options: impl IntoIterator<Item = ComboboxOption>) -> Self {
        self.config.options.extend(options);
        self
    }

    /// Set the placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.config.placeholder = Some(placeholder.into());
        self
    }

    /// A label above the combobox: a string, Signal or Computed.
    pub fn label(mut self, label: impl IntoReactive<String>) -> Self {
        self.config.label = Some(label.into_reactive());
        self
    }

    /// Set the combobox size
    pub fn size(mut self, size: ComboboxSize) -> Self {
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

    /// Allow custom values not in the options list
    pub fn allow_custom(mut self, allow: bool) -> Self {
        self.config.allow_custom = allow;
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

impl ElementBuilder for ComboboxBuilder {
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

/// A combobox that reads and writes `value`.
///
/// The shown text follows the value and the search in place, and `label` and
/// `disabled` are bound. Whether the list is open is kept per instance.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
///     let country = ctx.use_signal(String::new());
///
///     cn::combobox(country)
///         .placeholder("Search countries...")
///         .option("us", "United States")
///         .option("uk", "United Kingdom")
///         .on_change(|v| println!("Selected: {}", v))
/// }
/// ```
#[track_caller]
pub fn combobox(value: Signal<String>) -> ComboboxBuilder {
    ComboboxBuilder::new(value)
}

/// A row of the open list.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ListRow {
    /// The option at this index.
    Option(usize),
    /// Nothing matches the search.
    NoResults,
    /// Use the search text as the value.
    Custom(String),
}

/// Build the dropdown content as an absolutely positioned child.
///
/// This includes a search input and filtered options list.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn build_dropdown_content(
    options: &[ComboboxOption],
    current_selected: &str,
    value_state: Signal<String>,
    open_state: Signal<bool>,
    on_change: &Option<Arc<dyn Fn(&str) + Send + Sync>>,
    key: &str,
    search_data: &SharedTextInputData,
    search_query_state: Signal<String>,
    width: f32,
    trigger_height: f32,
    font_size: f32,
    padding: f32,
    radius: f32,
    text_color: blinc_core::Color,
    text_tertiary: blinc_core::Color,
    allow_custom: bool,
    placeholder: &Option<String>,
) -> Div {
    let theme = ThemeState::get();
    let dropdown_id = key;

    let mut dropdown_div = div()
        .class("cn-combobox-content")
        .id(dropdown_id)
        .flex_col()
        .w(width)
        // Fill, border, radius and shadow are `.cn-combobox-content` rules; the
        // radius stays here too because the panel clips its rows to it.
        .rounded(radius)
        .overflow_clip()
        // Absolutely positioned below the trigger, rendered in foreground pass
        // so it appears above all sibling content regardless of tree order
        .absolute()
        .top(trigger_height + 4.0)
        .left(0.0)
        .foreground();

    // Search input at the top
    let search_placeholder = placeholder
        .clone()
        .unwrap_or_else(|| "Type to search...".to_string());

    let search_query_for_sync = search_query_state;

    // No `.w_full()` — inside the flex_row container `flex_grow()` distributes
    // the remaining width after the container's padding. `w_full()` would
    // make the input claim 100% of the container's outer (padded) width.
    let search_input = blinc_layout::widgets::text_input::text_input(search_data)
        .id(&format!("{}_search_input", key))
        .h(trigger_height)
        .w(width - (width * padding / width)) // Subtract padding from width to account for container padding (since input is direct child of container)
        .text_size(font_size)
        .rounded(theme.radii().radius_md)
        .placeholder(search_placeholder)
        .class("cn-combobox-search")
        .text_color(text_color)
        .placeholder_color(text_tertiary)
        .flex_grow()
        .on_change(move |new_value: &str| {
            search_query_for_sync.set(new_value.to_string());
        });

    // Container is flex_row so the input is a true flex item — without an
    // explicit direction, `w_full()` on the input sizes to the container's
    // full width *including* its padding (border-box), so the right edge
    // got cropped at the dropdown's right border. Flex layout distributes
    // remaining space after padding instead.
    let search_container = div()
        .class("cn-combobox-search-row")
        .w_full()
        .flex_shrink_0()
        .flex_row()
        .items_center()
        .justify_center()
        .px(padding / 8.0)
        .py(padding / 8.0)
        .child(search_input);

    dropdown_div = dropdown_div.child(search_container);

    // The rows follow the search in place: the options that match, else a
    // "no results" row and, when allowed, one to use the search as the value.
    let options_content_key = format!("{}_options_content", key);
    let query = search_query_state;
    let options_for_rows = options.to_vec();
    let rows = computed(move |g: &ReactiveGraph| {
        let search = g.get(query).unwrap_or_default();
        let mut rows: Vec<ListRow> = options_for_rows
            .iter()
            .enumerate()
            .filter(|(_, opt)| opt.matches(&search))
            .map(|(idx, _)| ListRow::Option(idx))
            .collect();
        if rows.is_empty() {
            rows.push(ListRow::NoResults);
            if allow_custom && !search.is_empty() {
                rows.push(ListRow::Custom(search));
            }
        }
        rows
    });

    let options = options.to_vec();
    let current_selected = current_selected.to_string();
    let on_change = on_change.clone();
    let search_data = search_data.clone();
    let key = key.to_string();
    // Choosing a value closes the list and clears the search.
    let choose = Arc::new(move |chosen: String| {
        value_state.set(chosen.clone());
        open_state.set(false);
        if let Ok(mut data) = search_data.lock() {
            data.value.clear();
            data.cursor = 0;
        }
        search_query_state.set(String::new());
        if let Some(ref cb) = on_change {
            cb(&chosen);
        }
    });

    let options_content = div()
        .id(&options_content_key)
        .flex_col()
        .max_h(200.0)
        .overflow_y_scroll()
        .w_full()
        .for_each(
            rows,
            |row: &ListRow| row.clone(),
            move |row: ListRow| -> Div {
                match row {
                    ListRow::NoResults => div().w_full().p_px(padding).child(
                        text("No results found")
                            .class("cn-combobox-empty")
                            .size(font_size),
                    ),
                    ListRow::Custom(custom_value) => {
                        let choose = choose.clone();
                        div()
                            .id(format!("{}_custom", key))
                            .class("cn-combobox-item")
                            .w_full()
                            .h_fit()
                            .flex_row()
                            .items_center()
                            .child(
                                div().child(
                                    text(format!("Use \"{}\"", custom_value))
                                        .size(font_size)
                                        .no_cursor(),
                                ),
                            )
                            .on_click(move |_ctx| choose(custom_value.clone()))
                    }
                    ListRow::Option(idx) => {
                        let opt = &options[idx];
                        let is_opt_disabled = opt.disabled;
                        // Fill, text colour and cursor, selected, hovered or
                        // disabled, are `.cn-combobox-item` rules.
                        let mut option_item = div()
                            .id(format!("{}_opt_{}", key, idx))
                            .class("cn-combobox-item")
                            .w_full()
                            .h_fit()
                            .attr_when("disabled", "", is_opt_disabled)
                            .flex_row()
                            .items_center();
                        if opt.value == current_selected {
                            option_item = option_item.class("cn-combobox-item--selected");
                        }
                        let choose = choose.clone();
                        let value = opt.value.clone();
                        option_item
                            .child(if let Some(ref content_fn) = opt.content {
                                content_fn()
                            } else {
                                div().child(text(&opt.label).size(font_size).no_cursor())
                            })
                            .on_click(move |_ctx| {
                                if !is_opt_disabled {
                                    choose(value.clone());
                                }
                            })
                    }
                }
            },
        );

    dropdown_div = dropdown_div.child(options_content);

    dropdown_div
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combobox_sizes() {
        assert_eq!(ComboboxSize::Small.height(), 32.0);
        assert_eq!(ComboboxSize::Medium.height(), 40.0);
        assert_eq!(ComboboxSize::Large.height(), 48.0);
    }

    #[test]
    fn test_combobox_font_sizes() {
        assert_eq!(ComboboxSize::Small.font_size(), 13.0);
        assert_eq!(ComboboxSize::Medium.font_size(), 14.0);
        assert_eq!(ComboboxSize::Large.font_size(), 16.0);
    }

    #[test]
    fn test_combobox_option() {
        let opt = ComboboxOption::new("value", "Label");
        assert_eq!(opt.value, "value");
        assert_eq!(opt.label, "Label");
        assert!(!opt.disabled);

        let disabled_opt = opt.disabled();
        assert!(disabled_opt.disabled);
    }

    #[test]
    fn test_combobox_option_matches() {
        let opt = ComboboxOption::new("us", "United States");

        // Empty query matches everything
        assert!(opt.matches(""));

        // Case-insensitive label match
        assert!(opt.matches("united"));
        assert!(opt.matches("STATES"));
        assert!(opt.matches("Unit"));

        // Value match
        assert!(opt.matches("us"));
        assert!(opt.matches("US"));

        // No match
        assert!(!opt.matches("canada"));
        assert!(!opt.matches("xyz"));
    }
}
