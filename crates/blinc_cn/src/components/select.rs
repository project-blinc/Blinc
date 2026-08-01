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

use blinc_core::State;
use blinc_core::context_state::BlincContextState;
use blinc_layout::click_outside;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, stateful_with_key};
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

        let dropdown_width = config.width.unwrap_or(200.0);

        // Clones for closures
        let value_state_for_display = config.value_state.clone();
        let open_state_for_display = open_state.clone();
        let options_for_display = config.options.clone();
        let placeholder_for_display = config.placeholder.clone();
        let options_for_dropdown = config.options.clone();
        let on_change_for_dropdown = config.on_change.clone();
        let value_state_for_dropdown = config.value_state.clone();
        let open_state_for_dropdown = open_state.clone();

        // Chevron SVG (down arrow)
        let chevron_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

        let btn_variant = ButtonVariant::Outline;
        let select_btn_key = format!("{}_btn", instance_key);
        let instance_key_owned = instance_key.to_string();
        // Unique element ID for click-outside detection
        let wrapper_id = format!("cn-select-{}", instance_key);
        let wrapper_id_for_state = wrapper_id.clone();
        let open_state_for_dismiss = open_state.clone();

        let select_element = stateful_with_key::<ButtonState>(&select_btn_key)
            .deps([config.value_state.signal_id(), open_state.signal_id()])
            .on_state(move |ctx| {
                let state = ctx.state();
                let is_open = open_state_for_display.get();

                // Register/unregister click-outside based on open state
                if is_open {
                    let dismiss_state = open_state_for_dismiss.clone();
                    click_outside::register_click_outside(
                        &wrapper_id_for_state,
                        &wrapper_id_for_state,
                        move || {
                            dismiss_state.set(false);
                        },
                    );
                } else {
                    click_outside::unregister_click_outside(&wrapper_id_for_state);
                }

                // Disabled gets the same tonal-fill treatment as cn::button —
                // solid `InputBgDisabled` bg, `BorderSecondary` outline,
                // `TextTertiary` text. Matches the disabled-surface family
                // (button / select / switch) so the inert states all read
                // as the same UI tier.
                let bg = if disabled {
                    theme.color(ColorToken::InputBgDisabled)
                } else {
                    btn_variant.background(theme, state)
                };
                let current_val = value_state_for_display.get();
                let selected_option = options_for_display
                    .iter()
                    .find(|opt| opt.value == current_val);

                let is_placeholder = selected_option.is_none();
                let text_clr = if disabled || is_placeholder {
                    text_tertiary
                } else {
                    text_color
                };
                let bdr = if disabled {
                    theme.color(ColorToken::BorderSecondary)
                } else if is_open {
                    border_hover
                } else {
                    border
                };

                let display_content: Div = if let Some(opt) = selected_option {
                    if let Some(ref content_fn) = opt.content {
                        content_fn()
                    } else {
                        div()
                            .h_fit()
                            .overflow_clip()
                            .child(text(&opt.label).size(font_size).no_cursor().color(text_clr))
                    }
                } else {
                    let placeholder_text = placeholder_for_display
                        .clone()
                        .unwrap_or_else(|| "Select...".to_string());
                    div().h_fit().overflow_clip().child(
                        text(&placeholder_text)
                            .size(font_size)
                            .no_cursor()
                            .color(text_clr),
                    )
                };

                let content_wrapper = div().overflow_clip().flex_1().child(display_content);

                // Wrapper uses relative positioning so the dropdown can be absolutely positioned
                let mut wrapper = div()
                    .class("cn-select")
                    .id(&wrapper_id)
                    .relative()
                    .overflow_visible()
                    .w(dropdown_width);

                // Trigger button — click handler is on the trigger itself (not the wrapper)
                // so clicking dropdown items does NOT re-toggle the dropdown.
                let open_state_trigger = open_state_for_display.clone();
                let trigger = div()
                    .class("cn-select-trigger")
                    .flex_row()
                    .w(dropdown_width)
                    .items_center()
                    .justify_between()
                    .h(height)
                    .p_px(padding)
                    .bg(bg)
                    .border(1.0, bdr)
                    .rounded(radius)
                    .overflow_clip()
                    .flex_shrink_0()
                    // No shadow when disabled — matches the flat,
                    // non-interactive look the cn::button disabled state uses.
                    .when(!disabled, |t| t.shadow_sm())
                    .child(content_wrapper)
                    .child(
                        svg(chevron_svg)
                            .size(16.0, 16.0)
                            .tint(text_tertiary)
                            .ml(1.0)
                            .flex_shrink_0(),
                    )
                    .when(!disabled, |t| t.cursor_pointer())
                    .when(disabled, |t| t.cursor_not_allowed())
                    .on_click(move |_ctx| {
                        if !disabled {
                            open_state_trigger.set(!open_state_trigger.get());
                        }
                    });

                wrapper = wrapper.child(trigger);

                // Dropdown content (only when open)
                if is_open {
                    let current_selected = value_state_for_dropdown.get();
                    // Use fixed Surface color for dropdown items — not the
                    // trigger's state-dependent bg which changes on hover/press
                    let dropdown_bg = theme.color(ColorToken::Surface);
                    let dropdown = build_dropdown_content(
                        &options_for_dropdown,
                        &current_selected,
                        &value_state_for_dropdown,
                        &open_state_for_dropdown,
                        &on_change_for_dropdown,
                        &instance_key_owned,
                        dropdown_width,
                        height,
                        font_size,
                        padding,
                        radius,
                        dropdown_bg,
                        border,
                        text_color,
                        text_tertiary,
                        surface_elevated,
                    );

                    wrapper = wrapper.child(dropdown);
                }

                wrapper
            });

        // If there's a label, wrap in a container
        let inner = if let Some(ref label_text) = config.label {
            let spacing = theme.spacing_value(SpacingToken::Space2);
            let mut outer = div().flex_col().gap_px(spacing).w(dropdown_width).h_fit();

            let mut lbl = label(label_text).size(LabelSize::Medium);
            if disabled {
                lbl = lbl.disabled(true);
            }

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

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().inner.element_classes()
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

/// Build the dropdown content as an absolutely positioned child.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn build_dropdown_content(
    options: &[SelectOption],
    current_selected: &str,
    value_state: &State<String>,
    open_state: &State<bool>,
    on_change: &Option<Arc<dyn Fn(&str) + Send + Sync>>,
    key: &str,
    width: f32,
    trigger_height: f32,
    font_size: f32,
    padding: f32,
    radius: f32,
    bg: blinc_core::Color,
    border: blinc_core::Color,
    text_color: blinc_core::Color,
    text_tertiary: blinc_core::Color,
    surface_elevated: blinc_core::Color,
) -> Div {
    let dropdown_id = key;

    let mut dropdown_div = div()
        .class("cn-select-content")
        .id(dropdown_id)
        .flex_col()
        .w(width)
        .bg(bg)
        .border(1.0, border)
        .rounded(radius)
        .lock_corner_shape()
        .shadow_lg()
        .overflow_clip()
        .h_fit()
        // Absolutely positioned below the trigger, rendered in foreground pass
        // so it appears above all sibling content regardless of tree order
        .absolute()
        .top(trigger_height + 4.0)
        .left(0.0)
        .foreground();

    for (idx, opt) in options.iter().enumerate() {
        let opt_value = opt.value.clone();
        let opt_label = opt.label.clone();
        let opt_content = opt.content.clone();
        let is_selected = opt_value == current_selected;
        let is_opt_disabled = opt.disabled;

        let value_state_for_opt = value_state.clone();
        let open_state_for_opt = open_state.clone();
        let on_change_for_opt = on_change.clone();
        let opt_value_for_click = opt_value.clone();

        let option_text_color = if is_opt_disabled {
            text_tertiary
        } else {
            text_color
        };

        // No inline bg — `.w_full()` items would otherwise blanket the
        // dropdown panel's `--surface-elevated` CSS bg with pure-white
        // `Surface` for every non-selected row, leaving only the thin
        // `var(--space-1)` padding edge of the panel visible. CSS
        // `.cn-select-item:hover` / `.cn-select-item--selected` set
        // the bg when interactive; everything else stays transparent
        // so the panel's own surface-elevated fill reads through.
        let _ = surface_elevated;

        // Plain div with element ID for proper event registration during subtree rebuilds.
        // Hover styles come from `.cn-select-item:hover` in cn_styles.rs.
        let item_id = format!("{}_opt_{}", key, idx);
        let mut item = div()
            .id(&item_id)
            .class("cn-select-item")
            .w_full()
            .h_fit()
            .cursor(if is_opt_disabled {
                CursorStyle::NotAllowed
            } else {
                CursorStyle::Pointer
            })
            .flex_row()
            .items_center()
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

                    if let Some(ref cb) = on_change_for_opt {
                        cb(&opt_value_for_click);
                    }
                }
            });

        if is_selected {
            item = item.class("cn-select-item--selected");
        }

        dropdown_div = dropdown_div.child(item);
    }

    dropdown_div
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
