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

use blinc_animation::AnimationPreset;
use blinc_core::context_state::BlincContextState;
use blinc_core::State;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::motion::motion;
use blinc_layout::overlay_state::get_overlay_manager;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, Stateful};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::overlay::{OverlayHandle, OverlayManagerExt};
use blinc_layout::widgets::scroll::scroll;
use blinc_layout::widgets::text_input::SharedTextInputData;
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

use crate::button::use_button_state;

use super::label::{label, LabelSize};
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
        let radius = theme.radius(RadiusToken::Md);

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

        // Store overlay handle to track the dropdown overlay
        let handle_key = format!("{}_handle", instance_key);
        let overlay_handle_state: State<Option<u64>> =
            BlincContextState::get().use_state_keyed(&handle_key, || None);

        // Create search input data - use the text_input_data helper function
        // Note: We use a State to persist across rebuilds, storing the Arc<Mutex<TextInputData>>
        let search_key = format!("{}_search", instance_key);
        let search_input_data: SharedTextInputData = BlincContextState::get()
            .use_state_keyed(&search_key, || {
                blinc_layout::widgets::text_input::text_input_data()
            })
            .get();

        // Create a State<String> for reactive filtering - this is updated from SharedTextInputData
        // and used by the options list Stateful to trigger re-renders when search text changes
        let search_query_key = format!("{}_search_query", instance_key);
        let search_query_state: State<String> =
            BlincContextState::get().use_state_keyed(&search_query_key, || String::new());

        // Store dropdown width for overlay
        let dropdown_width = config.width.unwrap_or(200.0);

        // Clones for closures
        let value_state_for_display = config.value_state.clone();
        let open_state_for_display = open_state.clone();
        let options_for_display = config.options.clone();
        let placeholder_for_display = config.placeholder.clone();
        let search_data_for_display = search_input_data.clone();

        // Chevron SVG (down arrow)
        let chevron_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

        // Search icon SVG
        let _search_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>"#;

        // Build dropdown options
        let options = config.options.clone();
        let on_change = config.on_change.clone();
        let value_state_for_options = config.value_state.clone();
        let open_state_for_click = open_state.clone();
        let overlay_handle_for_click = overlay_handle_state.clone();
        let select_btn_state = use_button_state(&format!("{}_btn", instance_key));
        let search_data_for_click = search_input_data.clone();
        let allow_custom = config.allow_custom;
        let placeholder_for_content = config.placeholder.clone();
        // Clone instance_key for use in closures (it's a &str that needs to outlive 'static)
        let instance_key_owned = instance_key.to_string();

        // The click handler is on the Stateful itself (not the inner div) so it gets registered
        // Use w_full() to ensure the Stateful takes the same width as its parent container
        let combobox_element = Stateful::with_shared_state(select_btn_state)
            .deps(&[config.value_state.signal_id(), open_state.signal_id()])
            .w_full()
            .h(height)
            .cursor_pointer()
            .on_state(move |state, container: &mut Div| {
                let is_open = open_state_for_display.get();
                let current_val = value_state_for_display.get();

                // Get display text: if we have a selected value, show its label
                // Otherwise show placeholder or search text
                let selected_option = options_for_display
                    .iter()
                    .find(|opt| opt.value == current_val);

                let display_text = if let Some(opt) = selected_option {
                    opt.label.clone()
                } else if !current_val.is_empty() {
                    // Custom value entered
                    current_val.clone()
                } else {
                    // Get search text if dropdown is open
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
                } else if *state == ButtonState::Hovered {
                    border_hover
                } else {
                    border
                };

                // Build trigger with text display
                let display_content = div().flex_1().overflow_clip().child(
                    text(&display_text)
                        .size(font_size)
                        .no_cursor()
                        .color(text_clr),
                );

                let trigger = div()
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
                    .child(
                        svg(chevron_svg)
                            .size(16.0, 16.0)
                            .tint(text_tertiary)
                            .ml(1.0)
                            .flex_shrink_0(),
                    )
                    .cursor_pointer();

                let main_container = div().relative().w_full().child(trigger);
                container.merge(main_container);
            })
            .on_click(move |ctx| {
                let is_currently_open = open_state_for_click.get();

                if is_currently_open {
                    // Close the dropdown
                    if let Some(handle_id) = overlay_handle_for_click.get() {
                        let mgr = get_overlay_manager();
                        mgr.close(OverlayHandle::from_raw(handle_id));
                    }
                    open_state_for_click.set(false);
                    overlay_handle_for_click.set(None);

                    // Clear search text
                    if let Ok(mut data) = search_data_for_click.lock() {
                        data.value.clear();
                        data.cursor = 0;
                    }
                } else {
                    // Use EventContext bounds which are computed absolutely by the event router
                    let (trigger_x, trigger_y, _trigger_w, trigger_h) = (
                        ctx.bounds_x,
                        ctx.bounds_y,
                        ctx.bounds_width,
                        ctx.bounds_height,
                    );

                    // Position dropdown directly below the trigger, left-aligned
                    let offset = 4.0;
                    let dropdown_x = trigger_x;
                    let dropdown_y = trigger_y + trigger_h + offset;

                    // Clone values for the dropdown content closure
                    let opts = options.clone();
                    let val_state = value_state_for_options.clone();
                    let open_st = open_state_for_click.clone();
                    let handle_st = overlay_handle_for_click.clone();
                    let on_chg = on_change.clone();
                    let current_selected = val_state.get();
                    let dw = dropdown_width;
                    let key_for_content = instance_key_owned.clone();
                    let search_data = search_data_for_click.clone();
                    let placeholder_for_dropdown = placeholder_for_content.clone();
                    let search_query_for_dropdown = search_query_state.clone();

                    // Clone for on_close callback
                    let open_state_for_close = open_state_for_click.clone();
                    let handle_state_for_close = overlay_handle_for_click.clone();
                    let search_data_for_close = search_data_for_click.clone();
                    let search_query_for_close = search_query_state.clone();

                    // Show dropdown via overlay manager
                    let mgr = get_overlay_manager();
                    let handle = mgr
                        .dropdown()
                        .at(dropdown_x, dropdown_y)
                        .dismiss_on_escape(true)
                        .content(move || {
                            build_dropdown_content(
                                &opts,
                                &current_selected,
                                &val_state,
                                &open_st,
                                &handle_st,
                                &on_chg,
                                &key_for_content,
                                &search_data,
                                &search_query_for_dropdown,
                                dw,
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
                                &placeholder_for_dropdown,
                            )
                        })
                        .on_close(move || {
                            // Sync state when dropdown is dismissed externally
                            open_state_for_close.set(false);
                            handle_state_for_close.set(None);

                            // Clear search text
                            if let Ok(mut data) = search_data_for_close.lock() {
                                data.value.clear();
                                data.cursor = 0;
                            }
                            search_query_for_close.set(String::new());
                        })
                        .show();

                    open_state_for_click.set(true);
                    overlay_handle_for_click.set(Some(handle.id()));
                }
            });

        // Build the outer container with optional label
        let container_width = config.width.unwrap_or(dropdown_width);
        let mut combobox_container = div().w(container_width).child(combobox_element);

        if disabled {
            combobox_container = combobox_container.opacity(0.5);
        }

        // If there's a label, wrap in a container
        let inner = if let Some(ref label_text) = config.label {
            let spacing = theme.spacing_value(SpacingToken::Space2);
            let mut outer = div().flex_col().gap_px(spacing).w(container_width);

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
}

/// Internal configuration for building a Combobox
#[derive(Clone)]
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
        self.built
            .get_or_init(|| Combobox::from_config(self.key.get(), self.config.clone()))
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

/// Build the dropdown content for the overlay
///
/// This includes a search input and filtered options list.
#[allow(clippy::too_many_arguments)]
fn build_dropdown_content(
    options: &[ComboboxOption],
    current_selected: &str,
    value_state: &State<String>,
    open_state: &State<bool>,
    overlay_handle_state: &State<Option<u64>>,
    on_change: &Option<Arc<dyn Fn(&str) + Send + Sync>>,
    key: &str,
    search_data: &SharedTextInputData,
    search_query_state: &State<String>,
    width: f32,
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
    let dropdown_id = key;

    let mut dropdown_div = div()
        .id(dropdown_id)
        .flex_col()
        .w(width)
        .bg(bg)
        .border(1.0, border)
        .rounded(radius)
        .shadow_lg()
        .overflow_clip();

    // Search input at the top - use placeholder from config or default
    let search_placeholder = placeholder
        .clone()
        .unwrap_or_else(|| "Type to search...".to_string());

    // Clone search_query_state for the text input's on_change sync
    let search_query_for_sync = search_query_state.clone();

    // Use w_full() on the input and flex_grow() to fill the container
    // This prevents the input from shrinking when focused
    let search_input = blinc_layout::widgets::text_input::text_input(search_data)
        .w_full()
        .h(36.0)
        .text_size(font_size)
        .placeholder(search_placeholder)
        .idle_border_color(blinc_core::Color::TRANSPARENT)
        .hover_border_color(blinc_core::Color::TRANSPARENT)
        .focused_border_color(border_focus)
        .idle_bg_color(bg)
        .hover_bg_color(bg)
        .focused_bg_color(bg)
        .text_color(text_color)
        .placeholder_color(text_tertiary)
        .flex_grow() // Ensure it takes full width
        .on_change(move |new_value: &str| {
            // Sync the search query state when text changes - this triggers deps updates
            search_query_for_sync.set(new_value.to_string());
        });

    // Search container with explicit width and flex properties to prevent shrinking
    let search_container = div()
        .w(width) // Explicit width to prevent resize on focus
        .flex_shrink_0() // Prevent container from shrinking
        .p_px(padding / 2.0)
        .border_bottom(1.0, border)
        .child(search_input);

    dropdown_div = dropdown_div.child(search_container);

    // Create a Stateful container for the options list that reacts to search changes
    // Clone all values needed inside the on_state callback
    let options_for_filter = options.to_vec();
    let current_selected_owned = current_selected.to_string();
    let value_state_for_opts = value_state.clone();
    let open_state_for_opts = open_state.clone();
    let handle_state_for_opts = overlay_handle_state.clone();
    let on_change_for_opts = on_change.clone();
    let search_data_for_opts = search_data.clone();
    let search_query_for_opts = search_query_state.clone();
    let key_for_opts = key.to_string();

    // Create a button state for the options container (used for deps-based updates)
    let options_container_key = format!("{}_options_container", key);
    let options_container_state = use_button_state(&options_container_key);

    let options_stateful = Stateful::with_shared_state(options_container_state)
        .deps(&[search_query_state.signal_id()])
        .w_full()
        .on_state(move |_state, container: &mut Div| {
            // Get search text from State<String> (updated by text_input's on_change callback)
            let search_text = search_query_for_opts.get();

            // Filter options based on current search text
            let filtered_options: Vec<_> = options_for_filter
                .iter()
                .filter(|opt| opt.matches(&search_text))
                .collect();

            // Build options content
            let mut options_content = div().flex_col().w_full();

            if filtered_options.is_empty() {
                // Show "no results" message
                let no_results = div().w_full().p_px(padding).child(
                    text("No results found")
                        .size(font_size)
                        .color(text_tertiary),
                );
                options_content = options_content.child(no_results);

                // If allow_custom and there's search text, show option to use custom value
                if allow_custom && !search_text.is_empty() {
                    let custom_value = search_text.clone();
                    let value_state_for_custom = value_state_for_opts.clone();
                    let open_state_for_custom = open_state_for_opts.clone();
                    let handle_state_for_custom = handle_state_for_opts.clone();
                    let on_change_for_custom = on_change_for_opts.clone();
                    let search_data_for_custom = search_data_for_opts.clone();
                    let search_query_for_custom = search_query_for_opts.clone();

                    let custom_item_key = format!("{}_custom", key_for_opts);
                    let custom_button_state = use_button_state(&custom_item_key);

                    let custom_item = Stateful::with_shared_state(custom_button_state)
                        .w_full()
                        .h_fit()
                        .py(padding / 4.0)
                        .px(padding / 2.0)
                        .bg(bg)
                        .cursor(CursorStyle::Pointer)
                        .on_state(move |state, container: &mut Div| {
                            let item_bg = if *state == ButtonState::Hovered {
                                surface_elevated
                            } else {
                                bg
                            };

                            let content = div()
                                .w_full()
                                .h_fit()
                                .flex_row()
                                .items_center()
                                .bg(item_bg)
                                .child(
                                    div().child(
                                        text(&format!("Use \"{}\"", custom_value))
                                            .size(font_size)
                                            .no_cursor()
                                            .color(text_color),
                                    ),
                                );

                            container.merge(content);
                        })
                        .on_click(move |_ctx| {
                            // Set the custom value
                            let custom_val = search_data_for_custom
                                .lock()
                                .ok()
                                .map(|d| d.value.clone())
                                .unwrap_or_default();
                            value_state_for_custom.set(custom_val.clone());

                            // Close the overlay
                            if let Some(handle_id) = handle_state_for_custom.get() {
                                let mgr = get_overlay_manager();
                                mgr.close(OverlayHandle::from_raw(handle_id));
                            }
                            open_state_for_custom.set(false);
                            handle_state_for_custom.set(None);

                            // Clear search
                            if let Ok(mut data) = search_data_for_custom.lock() {
                                data.value.clear();
                                data.cursor = 0;
                            }
                            search_query_for_custom.set(String::new());

                            // Call on_change callback
                            if let Some(ref cb) = on_change_for_custom {
                                cb(&custom_val);
                            }
                        });

                    options_content = options_content.child(custom_item);
                }
            } else {
                // Render filtered options
                for (idx, opt) in filtered_options.iter().enumerate() {
                    let opt_value = opt.value.clone();
                    let opt_label = opt.label.clone();
                    let opt_content = opt.content.clone();
                    let is_selected = opt_value == current_selected_owned;
                    let is_opt_disabled = opt.disabled;

                    let value_state_for_opt = value_state_for_opts.clone();
                    let open_state_for_opt = open_state_for_opts.clone();
                    let handle_state_for_opt = handle_state_for_opts.clone();
                    let on_change_for_opt = on_change_for_opts.clone();
                    let opt_value_for_click = opt_value.clone();
                    let search_data_for_opt = search_data_for_opts.clone();
                    let search_query_for_opt = search_query_for_opts.clone();

                    let option_text_color = if is_opt_disabled {
                        text_tertiary
                    } else {
                        text_color
                    };

                    let base_bg = if is_selected { surface_elevated } else { bg };

                    let item_key = format!("{}_opt-{}", key_for_opts, idx);
                    let button_state = use_button_state(&item_key);

                    let option_item = Stateful::with_shared_state(button_state)
                        .w_full()
                        .h_fit()
                        .py(padding / 4.0)
                        .px(padding / 2.0)
                        .bg(base_bg)
                        .cursor(if is_opt_disabled {
                            CursorStyle::NotAllowed
                        } else {
                            CursorStyle::Pointer
                        })
                        .on_state(move |state, container: &mut Div| {
                            let item_bg = if *state == ButtonState::Hovered && !is_opt_disabled {
                                surface_elevated
                            } else {
                                base_bg
                            };

                            let content = div()
                                .w_full()
                                .h_fit()
                                .flex_row()
                                .items_center()
                                .bg(item_bg)
                                .child(if let Some(ref content_fn) = opt_content {
                                    content_fn()
                                } else {
                                    div().child(
                                        text(&opt_label)
                                            .size(font_size)
                                            .no_cursor()
                                            .color(option_text_color),
                                    )
                                });

                            container.merge(content);
                        })
                        .on_click(move |_ctx| {
                            if !is_opt_disabled {
                                // Set the new value
                                value_state_for_opt.set(opt_value_for_click.clone());

                                // Close the overlay
                                if let Some(handle_id) = handle_state_for_opt.get() {
                                    let mgr = get_overlay_manager();
                                    mgr.close(OverlayHandle::from_raw(handle_id));
                                }
                                open_state_for_opt.set(false);
                                handle_state_for_opt.set(None);

                                // Clear search
                                if let Ok(mut data) = search_data_for_opt.lock() {
                                    data.value.clear();
                                    data.cursor = 0;
                                }
                                search_query_for_opt.set(String::new());

                                // Call on_change callback
                                if let Some(ref cb) = on_change_for_opt {
                                    cb(&opt_value_for_click);
                                }
                            }
                        });

                    options_content = options_content.child(option_item);
                }
            }

            // Wrap in scroll container
            let scroll_content = div()
                .w_full()
                .max_h(200.0)
                .overflow_clip()
                .child(scroll().w_full().h_full().child(options_content));

            container.merge(scroll_content);
        });

    dropdown_div = dropdown_div.child(options_stateful);

    // Wrap dropdown in motion container for enter/exit animations
    div().child(
        motion()
            .enter_animation(AnimationPreset::dropdown_in(150))
            .exit_animation(AnimationPreset::dropdown_out(100))
            .child(dropdown_div),
    )
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
