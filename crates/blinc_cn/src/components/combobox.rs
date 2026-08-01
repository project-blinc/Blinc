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
//!     let country = ctx.use_state_keyed("country", || "".to_string());
//!
//!     cn::combobox(&country)
//!         .placeholder("Search countries...")
//!         .option("us", "United States")
//!         .option("uk", "United Kingdom")
//!         .option("de", "Germany")
//!         .option("fr", "France")
//!         .on_change(|value| println!("Selected: {}", value))
//! }
//!
//! // Different sizes
//! cn::combobox(&value)
//!     .size(ComboboxSize::Large)
//!
//! // Disabled state
//! cn::combobox(&value)
//!     .disabled(true)
//!
//! // With label
//! cn::combobox(&value)
//!     .label("Country")
//!
//! // Allow custom values (not just from options)
//! cn::combobox(&value)
//!     .allow_custom(true)
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_core::State;
use blinc_core::context_state::BlincContextState;
use blinc_layout::click_outside;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, stateful_with_key};
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
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let border_hover = theme.color(ColorToken::BorderHover);
        let border_focus = theme.color(ColorToken::BorderFocus);
        let text_color = theme.color(ColorToken::TextPrimary);
        let text_tertiary = theme.color(ColorToken::TextTertiary);
        let surface_elevated = theme.color(ColorToken::SurfaceElevated);

        let disabled = config.disabled;

        // Create internal open_state using the singleton (tracks whether dropdown should be shown)
        let open_key = format!("{}_open", instance_key);
        let open_state = BlincContextState::get().use_state_keyed(&open_key, || false);

        // Create search input data
        let search_key = format!("{}_search", instance_key);
        let search_input_data: SharedTextInputData = BlincContextState::get()
            .use_state_keyed(&search_key, || {
                blinc_layout::widgets::text_input::text_input_data()
            })
            .get();

        // Create a State<String> for reactive filtering
        let search_query_key = format!("{}_search_query", instance_key);
        let search_query_state: State<String> =
            BlincContextState::get().use_state_keyed(&search_query_key, String::new);

        let dropdown_width = config.width.unwrap_or(200.0);

        // Clones for closures
        let value_state_for_display = config.value_state.clone();
        let open_state_for_display = open_state.clone();
        let options_for_display = config.options.clone();
        let placeholder_for_display = config.placeholder.clone();
        let search_data_for_display = search_input_data.clone();
        let options_for_dropdown = config.options.clone();
        let on_change_for_dropdown = config.on_change.clone();
        let value_state_for_dropdown = config.value_state.clone();
        let open_state_for_dropdown = open_state.clone();
        let search_data_for_dropdown = search_input_data.clone();
        let search_query_for_dropdown = search_query_state.clone();
        let allow_custom = config.allow_custom;
        let placeholder_for_content = config.placeholder.clone();

        // Chevron SVG (down arrow)
        let chevron_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

        let select_btn_key = format!("{}_btn", instance_key);
        let instance_key_owned = instance_key.to_string();
        // Unique element ID for click-outside detection
        let wrapper_id = format!("cn-combobox-{}", instance_key);
        let wrapper_id_for_state = wrapper_id.clone();
        let open_state_for_dismiss = open_state.clone();
        let search_data_for_dismiss = search_input_data.clone();
        let search_query_for_dismiss = search_query_state.clone();

        let combobox_element = stateful_with_key::<ButtonState>(&select_btn_key)
            .deps([
                config.value_state.signal_id(),
                open_state.signal_id(),
                search_query_state.signal_id(),
            ])
            .on_state(move |ctx| {
                let state = ctx.state();
                let is_open = open_state_for_display.get();

                // Register/unregister click-outside based on open state
                if is_open {
                    let dismiss_state = open_state_for_dismiss.clone();
                    let dismiss_search_data = search_data_for_dismiss.clone();
                    let dismiss_search_query = search_query_for_dismiss.clone();
                    click_outside::register_click_outside(
                        &wrapper_id_for_state,
                        &wrapper_id_for_state,
                        move || {
                            dismiss_state.set(false);
                            // Clear search text on dismiss
                            if let Ok(mut data) = dismiss_search_data.lock() {
                                data.value.clear();
                                data.cursor = 0;
                            }
                            dismiss_search_query.set(String::new());
                        },
                    );
                } else {
                    click_outside::unregister_click_outside(&wrapper_id_for_state);
                }
                let current_val = value_state_for_display.get();

                let selected_option = options_for_display
                    .iter()
                    .find(|opt| opt.value == current_val);

                let display_text = if let Some(opt) = selected_option {
                    opt.label.clone()
                } else if !current_val.is_empty() {
                    current_val.clone()
                } else {
                    let search_text = search_data_for_display
                        .lock()
                        .ok()
                        .map(|d| d.value.clone())
                        .unwrap_or_default();
                    if !search_text.is_empty() && is_open {
                        search_text
                    } else {
                        placeholder_for_display
                            .clone()
                            .unwrap_or_else(|| "Search...".to_string())
                    }
                };

                let is_placeholder = selected_option.is_none() && current_val.is_empty();
                let text_clr = if is_placeholder {
                    text_tertiary
                } else {
                    text_color
                };

                let bdr = if is_open {
                    border_focus
                } else if state == ButtonState::Hovered {
                    border_hover
                } else {
                    border
                };

                let display_content = div().flex_1().overflow_clip().child(
                    text(&display_text)
                        .size(font_size)
                        .no_cursor()
                        .color(text_clr),
                );

                // Wrapper uses relative positioning so the dropdown can be absolutely positioned
                let mut wrapper = div()
                    .class("cn-combobox")
                    .id(&wrapper_id)
                    .relative()
                    .overflow_visible()
                    .w(dropdown_width);

                // Trigger button — click handler is on the trigger itself (not the wrapper)
                // so clicking dropdown items does NOT re-toggle the dropdown.
                let open_state_trigger = open_state_for_display.clone();
                let search_data_trigger = search_data_for_display.clone();
                let search_query_trigger = search_query_for_dropdown.clone();
                let trigger = div()
                    .class("cn-combobox-trigger")
                    .flex_row()
                    .w_full()
                    .items_center()
                    .h(height)
                    .p_px(padding)
                    .bg(bg)
                    .border(1.0, bdr)
                    .rounded(radius)
                    .child(display_content)
                    .flex_shrink_0()
                    .shadow_sm()
                    .child(
                        svg(chevron_svg)
                            .size(16.0, 16.0)
                            .tint(text_tertiary)
                            .ml(1.0)
                            .flex_shrink_0(),
                    )
                    .cursor_pointer()
                    .on_click(move |_ctx| {
                        if disabled {
                            return;
                        }
                        let is_currently_open = open_state_trigger.get();
                        if is_currently_open {
                            // Closing: clear search text
                            if let Ok(mut data) = search_data_trigger.lock() {
                                data.value.clear();
                                data.cursor = 0;
                            }
                            search_query_trigger.set(String::new());
                        } else {
                            // Opening: autofocus the search input so the
                            // user can start typing immediately. Without
                            // this they'd have to click the search field
                            // first — unexpected for a "type to search"
                            // affordance.
                            blinc_layout::widgets::text_input::focus_text_input(
                                &search_data_trigger,
                            );
                        }
                        open_state_trigger.set(!is_currently_open);
                    });

                wrapper = wrapper.child(trigger);

                // Dropdown content (only when open)
                if is_open {
                    let current_selected = value_state_for_dropdown.get();
                    let dropdown = build_dropdown_content(
                        &options_for_dropdown,
                        &current_selected,
                        &value_state_for_dropdown,
                        &open_state_for_dropdown,
                        &on_change_for_dropdown,
                        &instance_key_owned,
                        &search_data_for_dropdown,
                        &search_query_for_dropdown,
                        dropdown_width,
                        height,
                        font_size,
                        padding,
                        radius,
                        bg,
                        border,
                        border_focus,
                        text_color,
                        text_tertiary,
                        surface_elevated,
                        allow_custom,
                        &placeholder_for_content,
                    );

                    wrapper = wrapper.child(dropdown);
                }

                wrapper
            });

        // Build the outer container with optional label
        let container_width = config.width.unwrap_or(dropdown_width);
        let mut combobox_container = div().w(container_width).child(combobox_element);

        if disabled {
            combobox_container = combobox_container.opacity(0.5);
        }

        let inner = if let Some(ref label_text) = config.label {
            let spacing = theme.spacing_value(SpacingToken::Space2);
            let mut outer = div().flex_col().gap_px(spacing).w(container_width).h_fit();

            let mut lbl = label(label_text).size(LabelSize::Medium);
            if disabled {
                lbl = lbl.disabled(true);
            }

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
    value_state: State<String>,
    options: Vec<ComboboxOption>,
    placeholder: Option<String>,
    label: Option<String>,
    size: ComboboxSize,
    disabled: bool,
    width: Option<f32>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    /// Allow entering custom values not in the options list
    allow_custom: bool,
}

impl ComboboxConfig {
    fn new(value_state: State<String>) -> Self {
        Self {
            value_state,
            options: Vec::new(),
            placeholder: None,
            label: None,
            size: ComboboxSize::default(),
            disabled: false,
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
    /// Create a new combobox builder with value state
    ///
    /// The open state is managed internally using the global context singleton.
    /// Uses `#[track_caller]` to generate a unique instance key based on the call site.
    #[track_caller]
    pub fn new(value_state: &State<String>) -> Self {
        Self {
            key: InstanceKey::new("combobox"),
            config: ComboboxConfig::new(value_state.clone()),
            built: OnceCell::new(),
        }
    }

    /// Create a combobox builder with an explicit key
    pub fn with_key(key: impl Into<String>, value_state: &State<String>) -> Self {
        Self {
            key: InstanceKey::explicit(key),
            config: ComboboxConfig::new(value_state.clone()),
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

    /// Add a label above the combobox
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Set the combobox size
    pub fn size(mut self, size: ComboboxSize) -> Self {
        self.config.size = size;
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
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

/// Create a combobox with value state
///
/// The combobox uses state-driven reactivity - changes to the value state
/// will trigger a rebuild of the component. The open/closed state is
/// managed internally using the global context singleton.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
///     let country = ctx.use_state_keyed("country", || "".to_string());
///
///     cn::combobox(&country)
///         .placeholder("Search countries...")
///         .option("us", "United States")
///         .option("uk", "United Kingdom")
///         .on_change(|v| println!("Selected: {}", v))
/// }
/// ```
#[track_caller]
pub fn combobox(value_state: &State<String>) -> ComboboxBuilder {
    ComboboxBuilder::new(value_state)
}

/// Build the dropdown content as an absolutely positioned child.
///
/// This includes a search input and filtered options list.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn build_dropdown_content(
    options: &[ComboboxOption],
    current_selected: &str,
    value_state: &State<String>,
    open_state: &State<bool>,
    on_change: &Option<Arc<dyn Fn(&str) + Send + Sync>>,
    key: &str,
    search_data: &SharedTextInputData,
    search_query_state: &State<String>,
    width: f32,
    trigger_height: f32,
    font_size: f32,
    padding: f32,
    radius: f32,
    bg: blinc_core::Color,
    border: blinc_core::Color,
    border_focus: blinc_core::Color,
    text_color: blinc_core::Color,
    text_tertiary: blinc_core::Color,
    surface_elevated: blinc_core::Color,
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
        .bg(bg)
        .border(1.0, border)
        .rounded(radius)
        .lock_corner_shape()
        .shadow_lg()
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

    let search_query_for_sync = search_query_state.clone();

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
        .idle_border_color(theme.color(ColorToken::Border))
        .hover_border_color(theme.color(ColorToken::BorderFocus))
        .focused_border_color(border_focus)
        .idle_bg_color(bg)
        .hover_bg_color(bg)
        .focused_bg_color(bg)
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
        .w_full()
        .flex_shrink_0()
        .flex_row()
        .items_center()
        .justify_center()
        .px(padding / 8.0)
        .py(padding / 8.0)
        .border_bottom(1.0, border)
        .child(search_input);

    dropdown_div = dropdown_div.child(search_container);

    // Build the options list inline. The outer combobox Stateful already lists
    // search_query_state.signal_id() in its deps, so search-driven rebuilds flow
    // through that — wrapping the option list in its own Stateful<NoState> just
    // added a redundant subtree-rebuild layer that left class registrations
    // out-of-sync with apply_complex_selector_styles' hover matching pass, so
    // `.cn-combobox-item:hover` never lit up.
    let options_content_key = format!("{}_options_content", key);
    let search_text = search_query_state.get();

    let filtered_options: Vec<_> = options
        .iter()
        .filter(|opt| opt.matches(&search_text))
        .collect();

    let mut options_content = div()
        .id(&options_content_key)
        .flex_col()
        .max_h(200.0)
        .overflow_y_scroll()
        .w_full();

    if filtered_options.is_empty() {
        let no_results = div().w_full().p_px(padding).child(
            text("No results found")
                .size(font_size)
                .color(text_tertiary),
        );
        options_content = options_content.child(no_results);

        if allow_custom && !search_text.is_empty() {
            let custom_value = search_text.clone();
            let value_state_for_custom = value_state.clone();
            let open_state_for_custom = open_state.clone();
            let on_change_for_custom = on_change.clone();
            let search_data_for_custom = search_data.clone();
            let search_query_for_custom = search_query_state.clone();

            let custom_item_id = format!("{}_custom", key);
            let custom_item = div()
                .id(&custom_item_id)
                .class("cn-combobox-item")
                .w_full()
                .h_fit()
                .cursor(CursorStyle::Pointer)
                .flex_row()
                .items_center()
                .child(
                    div().child(
                        text(format!("Use \"{}\"", custom_value))
                            .size(font_size)
                            .no_cursor()
                            .color(text_color),
                    ),
                )
                .on_click(move |_ctx| {
                    let custom_val = search_data_for_custom
                        .lock()
                        .ok()
                        .map(|d| d.value.clone())
                        .unwrap_or_default();
                    value_state_for_custom.set(custom_val.clone());
                    open_state_for_custom.set(false);

                    if let Ok(mut data) = search_data_for_custom.lock() {
                        data.value.clear();
                        data.cursor = 0;
                    }
                    search_query_for_custom.set(String::new());

                    if let Some(ref cb) = on_change_for_custom {
                        cb(&custom_val);
                    }
                });

            options_content = options_content.child(custom_item);
        }
    } else {
        for (idx, opt) in filtered_options.iter().enumerate() {
            let opt_value = opt.value.clone();
            let opt_label = opt.label.clone();
            let opt_content = opt.content.clone();
            let is_selected = opt_value == current_selected;
            let is_opt_disabled = opt.disabled;

            let value_state_for_opt = value_state.clone();
            let open_state_for_opt = open_state.clone();
            let on_change_for_opt = on_change.clone();
            let opt_value_for_click = opt_value.clone();
            let search_data_for_opt = search_data.clone();
            let search_query_for_opt = search_query_state.clone();

            let option_text_color = if is_opt_disabled {
                text_tertiary
            } else {
                text_color
            };

            // Background is owned by CSS (.cn-combobox-item /
            // .cn-combobox-item:hover / .cn-combobox-item--selected) — an
            // explicit `.bg(base_bg)` here overrides the :hover selector.
            let item_id = format!("{}_opt_{}", key, idx);
            let mut option_item = div()
                .id(&item_id)
                .class("cn-combobox-item")
                .w_full()
                .h_fit()
                .cursor(if is_opt_disabled {
                    CursorStyle::NotAllowed
                } else {
                    CursorStyle::Pointer
                })
                .flex_row()
                .items_center();
            if is_selected {
                option_item = option_item.class("cn-combobox-item--selected");
            }
            let option_item = option_item
                .child(if let Some(ref content_fn) = opt_content {
                    content_fn()
                } else {
                    div().child(
                        text(&opt_label)
                            .size(font_size)
                            .no_cursor()
                            .color(option_text_color),
                    )
                })
                .on_click(move |_ctx| {
                    if !is_opt_disabled {
                        value_state_for_opt.set(opt_value_for_click.clone());
                        open_state_for_opt.set(false);

                        if let Ok(mut data) = search_data_for_opt.lock() {
                            data.value.clear();
                            data.cursor = 0;
                        }
                        search_query_for_opt.set(String::new());

                        if let Some(ref cb) = on_change_for_opt {
                            cb(&opt_value_for_click);
                        }
                    }
                });

            options_content = options_content.child(option_item);
        }
    }

    let _ = surface_elevated;
    let _ = bg;

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
