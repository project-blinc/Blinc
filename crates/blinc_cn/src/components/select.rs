//! Select component for dropdown value selection
//!
//! A themed select dropdown with click-to-open and keyboard navigation.
//! Uses state-driven reactivity for proper persistence across UI rebuilds.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let fruit = ctx.use_state_keyed("fruit", || "apple".to_string());
//!
//!     cn::select(&fruit)
//!         .placeholder("Choose a fruit...")
//!         .option("apple", "Apple")
//!         .option("banana", "Banana")
//!         .option("cherry", "Cherry")
//!         .on_change(|value| println!("Selected: {}", value))
//! }
//!
//! // Different sizes
//! cn::select(&value)
//!     .size(SelectSize::Large)
//!
//! // Disabled state
//! cn::select(&value)
//!     .disabled(true)
//!
//! // With label
//! cn::select(&value)
//!     .label("Favorite Fruit")
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
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

use crate::button::use_button_state;
use crate::ButtonVariant;

use super::label::{label, LabelSize};
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
        let bg = theme.color(ColorToken::Surface);
        let border = theme.color(ColorToken::Border);
        let border_hover = theme.color(ColorToken::BorderHover);
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

        // Store dropdown width for overlay
        let dropdown_width = config.width.unwrap_or(200.0);

        // Clones for closures
        let value_state_for_display = config.value_state.clone();
        let open_state_for_display = open_state.clone();
        let options_for_display = config.options.clone();
        let placeholder_for_display = config.placeholder.clone();

        // Chevron SVG (down arrow)
        let chevron_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

        // Build dropdown options
        let options = config.options.clone();
        let on_change = config.on_change.clone();
        let value_state_for_options = config.value_state.clone();
        let open_state_for_click = open_state.clone();
        let overlay_handle_for_click = overlay_handle_state.clone();
        let btn_variant = ButtonVariant::Outline;
        let select_btn_state = use_button_state(&format!("{}_btn", instance_key));
        // Clone instance_key for use in closures (it's a &str that needs to outlive 'static)
        let instance_key_owned = instance_key.to_string();
        // The click handler is on the Stateful itself (not the inner div) so it gets registered
        // Use w_full() to ensure the Stateful takes the same width as its parent container
        let select_element = Stateful::with_shared_state(select_btn_state)
            .deps(&[config.value_state.signal_id(), open_state.signal_id()])
            .w_full()
            .h(height)
            .cursor_pointer()
            .on_state(move |state, container: &mut Div| {
                let is_open = open_state_for_display.get();
                let bg = btn_variant.background(theme, *state);
                // Get current display value and selected option
                let current_val = value_state_for_display.get();
                let selected_option = options_for_display
                    .iter()
                    .find(|opt| opt.value == current_val);

                let is_placeholder = selected_option.is_none();
                let text_clr = if is_placeholder {
                    text_tertiary
                } else {
                    text_color
                };
                let bdr = if is_open { border_hover } else { border };

                // Build the content to display in the trigger
                // Use custom content if available, otherwise fall back to label text
                let display_content: Div = if let Some(opt) = selected_option {
                    if let Some(ref content_fn) = opt.content {
                        // Use custom content builder for the selected option
                        content_fn()
                    } else {
                        // Fall back to label text
                        div().child(text(&opt.label).size(font_size).no_cursor().color(text_clr))
                    }
                } else {
                    // Show placeholder
                    let placeholder_text = placeholder_for_display
                        .clone()
                        .unwrap_or_else(|| "Select...".to_string());
                    div().child(text(&placeholder_text).size(font_size).no_cursor().color(text_clr))
                };

                // Build trigger (visual only - click handler is on Stateful)
                // Wrap content in a flex-1 overflow-hidden container to prevent growing
                let content_wrapper = div()
                    .flex_1()
                    .overflow_clip()
                    .child(display_content);

                let trigger = div()
                    .flex_row()
                    .w_full()
                    .items_center()
                    .h(height)
                    .p_px(padding)
                    .bg(bg)
                    .border(1.0, bdr)
                    .rounded(radius)
                    .child(content_wrapper)
                    .flex_shrink_0()
                    .child(
                        svg(chevron_svg)
                            .size(16.0, 16.0)
                            .tint(text_tertiary)
                            .ml(1.0)
                            .flex_shrink_0(),
                    ).cursor_pointer();

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
                } else {
                    // Use EventContext bounds which are computed absolutely by the event router
                    // These are set during hit testing and represent the actual screen position
                    let (trigger_x, trigger_y, trigger_w, trigger_h) =
                        (ctx.bounds_x, ctx.bounds_y, ctx.bounds_width, ctx.bounds_height);

                    // Position dropdown directly below the trigger, left-aligned (same as DropdownMenuBuilder)
                    let offset = 4.0;
                    let dropdown_x = trigger_x;
                    let dropdown_y = trigger_y + trigger_h + offset;
                    tracing::debug!(
                        "Select dropdown position: x={:.1}, y={:.1} (trigger bounds: {:.1}, {:.1}, {:.1}, {:.1})",
                        dropdown_x, dropdown_y, trigger_x, trigger_y, trigger_w, trigger_h
                    );

                    // Clone values for the dropdown content closure
                    let opts = options.clone();
                    let val_state = value_state_for_options.clone();
                    let open_st = open_state_for_click.clone();
                    let handle_st = overlay_handle_for_click.clone();
                    let on_chg = on_change.clone();
                    let current_selected = val_state.get();
                    let dw = dropdown_width;
                    let key_for_content = instance_key_owned.clone();



                    // Clone for on_close callback
                    let open_state_for_close = open_state_for_click.clone();
                    let handle_state_for_close = overlay_handle_for_click.clone();

                    // Show dropdown via overlay manager
                    let mgr = get_overlay_manager();
                    let handle = mgr
                        .dropdown()
                        .at(dropdown_x, dropdown_y)
                        // .size(dropdown_width, estimated_height)
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
                                dw,
                                font_size,
                                padding,
                                radius,
                                bg,
                                border,
                                text_color,
                                text_tertiary,
                                surface_elevated,
                            )
                        })
                        .on_close(move || {
                            // Sync state when dropdown is dismissed externally
                            open_state_for_close.set(false);
                            handle_state_for_close.set(None);
                        })
                        .show();

                    open_state_for_click.set(true);
                    overlay_handle_for_click.set(Some(handle.id()));
                }
            });

        // Build the outer container with optional label
        // Use explicit width to maintain consistent size (don't shrink to content)
        let container_width = config.width.unwrap_or(dropdown_width);
        let mut select_container = div().w(container_width).child(select_element);

        if disabled {
            select_container = select_container.opacity(0.5);
        }

        // If there's a label, wrap in a container
        let inner = if let Some(ref label_text) = config.label {
            let spacing = theme.spacing_value(SpacingToken::Space2);
            // Use same width as container for consistency
            let mut outer = div().flex_col().gap_px(spacing).w(container_width);

            let mut lbl = label(label_text).size(LabelSize::Medium);
            if disabled {
                lbl = lbl.disabled(true);
            }

            outer = outer.child(lbl).child(select_container);
            outer
        } else {
            select_container
        };

        Self { inner }
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
}

/// Internal configuration for building a Select
#[derive(Clone)]
struct SelectConfig {
    value_state: State<String>,
    options: Vec<SelectOption>,
    placeholder: Option<String>,
    label: Option<String>,
    size: SelectSize,
    disabled: bool,
    width: Option<f32>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

impl SelectConfig {
    fn new(value_state: State<String>) -> Self {
        Self {
            value_state,
            options: Vec::new(),
            placeholder: None,
            label: None,
            size: SelectSize::default(),
            disabled: false,
            width: None,
            on_change: None,
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
    /// Create a new select builder with value state
    ///
    /// The open state is managed internally using the global context singleton.
    /// Uses `#[track_caller]` to generate a unique instance key based on the call site.
    #[track_caller]
    pub fn new(value_state: &State<String>) -> Self {
        Self {
            key: InstanceKey::new("select"),
            config: SelectConfig::new(value_state.clone()),
            built: OnceCell::new(),
        }
    }

    /// Create a select builder with an explicit key
    pub fn with_key(key: impl Into<String>, value_state: &State<String>) -> Self {
        Self {
            key: InstanceKey::explicit(key),
            config: SelectConfig::new(value_state.clone()),
            built: OnceCell::new(),
        }
    }

    /// Get or build the inner Select
    fn get_or_build(&self) -> &Select {
        self.built
            .get_or_init(|| Select::from_config(self.key.get(), self.config.clone()))
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

    /// Set the placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.config.placeholder = Some(placeholder.into());
        self
    }

    /// Add a label above the select
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Set the select size
    pub fn size(mut self, size: SelectSize) -> Self {
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

    /// Set the change callback
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
}

/// Create a select with value state
///
/// The select uses state-driven reactivity - changes to the value state
/// will trigger a rebuild of the component. The open/closed state is
/// managed internally using the global context singleton.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
///     let fruit = ctx.use_state_keyed("fruit", || "apple".to_string());
///
///     cn::select(&fruit)
///         .placeholder("Choose a fruit...")
///         .option("apple", "Apple")
///         .option("banana", "Banana")
///         .on_change(|v| println!("Selected: {}", v))
/// }
/// ```
#[track_caller]
pub fn select(value_state: &State<String>) -> SelectBuilder {
    SelectBuilder::new(value_state)
}

/// Build the dropdown content for the overlay
///
/// This is extracted as a separate function to be called from the overlay content closure.
#[allow(clippy::too_many_arguments)]
fn build_dropdown_content(
    options: &[SelectOption],
    current_selected: &str,
    value_state: &State<String>,
    open_state: &State<bool>,
    overlay_handle_state: &State<Option<u64>>,
    on_change: &Option<Arc<dyn Fn(&str) + Send + Sync>>,
    key: &str,
    width: f32,
    font_size: f32,
    padding: f32,
    radius: f32,
    bg: blinc_core::Color,
    border: blinc_core::Color,
    text_color: blinc_core::Color,
    text_tertiary: blinc_core::Color,
    surface_elevated: blinc_core::Color,
) -> Div {
    // Generate a unique ID for the dropdown based on the key
    let dropdown_id = key;

    let mut dropdown_div = div()
        .id(dropdown_id)
        .flex_col()
        .w(width)
        .bg(bg)
        .border(1.0, border)
        .rounded(radius)
        .shadow_lg()
        .overflow_clip()
        .h_fit();

    // Note: on_ready callback removed - overlay manager now uses initial size estimation
    // If accurate sizing is needed, register via ctx.query("select-dropdown-{handle}").on_ready(...)

    for (idx, opt) in options.iter().enumerate() {
        let opt_value = opt.value.clone();
        let opt_label = opt.label.clone();
        let opt_content = opt.content.clone();
        let is_selected = opt_value == current_selected;
        let is_opt_disabled = opt.disabled;

        let value_state_for_opt = value_state.clone();
        let open_state_for_opt = open_state.clone();
        let handle_state_for_opt = overlay_handle_state.clone();
        let on_change_for_opt = on_change.clone();
        let opt_value_for_click = opt_value.clone();

        let option_text_color = if is_opt_disabled {
            text_tertiary
        } else {
            text_color
        };

        // Background color - selected items get elevated bg, others get normal bg
        let base_bg = if is_selected { surface_elevated } else { bg };

        // Create a stable key for this option's button state
        let item_key = format!("{}_opt-{}", key, idx);
        let button_state = use_button_state(&item_key);

        // Build option item with Stateful for hover visual updates
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
                // Apply hover background based on button state
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
                    .child(
                        // Use custom content if available, otherwise fall back to label text
                        if let Some(ref content_fn) = opt_content {
                            content_fn()
                        } else {
                            div().child(
                                text(&opt_label)
                                    .size(font_size)
                                    .no_cursor()
                                    .color(option_text_color),
                            )
                        },
                    );

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

                    // Call on_change callback
                    if let Some(ref cb) = on_change_for_opt {
                        cb(&opt_value_for_click);
                    }
                }
            });

        dropdown_div = dropdown_div.child(option_item);
    }

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
