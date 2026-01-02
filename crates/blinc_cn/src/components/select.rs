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
//!     let fruit_open = ctx.use_state_keyed("fruit_open", || false);
//!
//!     cn::select(&fruit, &fruit_open)
//!         .placeholder("Choose a fruit...")
//!         .option("apple", "Apple")
//!         .option("banana", "Banana")
//!         .option("cherry", "Cherry")
//!         .on_change(|value| println!("Selected: {}", value))
//! }
//!
//! // Different sizes
//! cn::select(&value, &open)
//!     .size(SelectSize::Large)
//!
//! // Disabled state
//! cn::select(&value, &open)
//!     .disabled(true)
//!
//! // With label
//! cn::select(&value, &open)
//!     .label("Favorite Fruit")
//! ```

use std::cell::OnceCell;
use std::sync::Arc;

use blinc_core::State;
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::{CursorStyle, RenderProps};
use blinc_layout::prelude::*;
use blinc_layout::stateful::ButtonState;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::{ColorToken, RadiusToken, SpacingToken, ThemeState};

use super::label::{label, LabelSize};

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

/// An option in the select dropdown
#[derive(Clone, Debug)]
pub struct SelectOption {
    /// The value (stored in state when selected)
    pub value: String,
    /// The display label shown in UI
    pub label: String,
    /// Whether this option is disabled
    pub disabled: bool,
}

impl SelectOption {
    /// Create a new option with value and label
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: false,
        }
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
    fn from_config(config: SelectConfig) -> Self {
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

        // Clones for closures
        let value_state_for_display = config.value_state.clone();
        let open_state_for_display = config.open_state.clone();
        let options_for_display = config.options.clone();
        let placeholder_for_display = config.placeholder.clone();

        // Chevron SVG (down arrow)
        let chevron_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;

        // Build dropdown options
        let options = config.options.clone();
        let on_change = config.on_change.clone();
        let value_state_for_options = config.value_state.clone();
        let open_state_for_options = config.open_state.clone();

        // Use Stateful with () as we only need signal deps, not FSM state transitions
        let select_element = Stateful::<()>::new(())
            .deps(&[
                config.value_state.signal_id(),
                config.open_state.signal_id(),
            ])
            .on_state(move |_state: &(), container: &mut Div| {
                let is_open = open_state_for_display.get();

                // Get current display value
                let current_val = value_state_for_display.get();
                let current_lbl = options_for_display
                    .iter()
                    .find(|opt| opt.value == current_val)
                    .map(|opt| opt.label.clone());

                let disp_text = current_lbl.clone().unwrap_or_else(|| {
                    placeholder_for_display
                        .clone()
                        .unwrap_or_else(|| "Select...".to_string())
                });
                let is_placeholder = current_lbl.is_none();
                let text_clr = if is_placeholder {
                    text_tertiary
                } else {
                    text_color
                };
                let bdr = if is_open { border_hover } else { border };

                // Clone for trigger click handler
                let open_state_for_trigger = open_state_for_options.clone();

                // Build trigger
                let trigger = div()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(height)
                    .p_px(padding)
                    .bg(bg)
                    .border(1.0, bdr)
                    .rounded(radius)
                    .cursor_pointer()
                    .child(text(&disp_text).size(font_size).color(text_clr))
                    .child(
                        svg(chevron_svg)
                            .size(16.0, 16.0)
                            .tint(text_tertiary)
                            .ml(1.0),
                    )
                    .on_click(move |_ctx| {
                        // Toggle open state
                        let current = open_state_for_trigger.get();
                        open_state_for_trigger.set(!current);
                    });

                // Build dropdown if open
                let mut main_container = div().relative().w_full().child(trigger);

                if is_open {
                    let mut dropdown_div = div()
                        .flex_col()
                        .absolute()
                        .left(0.0)
                        .top(height + 4.0)
                        .w_full()
                        .bg(bg)
                        .border(1.0, border)
                        .rounded(radius)
                        .shadow_md()
                        .overflow_clip()
                        .max_h(200.0);

                    let current_selected = value_state_for_options.get();

                    for opt in &options {
                        let opt_value = opt.value.clone();
                        let opt_label = opt.label.clone();
                        let is_selected = opt_value == current_selected;
                        let is_opt_disabled = opt.disabled;

                        let value_state_for_opt = value_state_for_options.clone();
                        let open_state_for_opt = open_state_for_options.clone();
                        let on_change_for_opt = on_change.clone();
                        let opt_value_for_click = opt_value.clone();

                        // Background colors for different states
                        let idle_bg = if is_selected { surface_elevated } else { bg };
                        let hover_bg = surface_elevated;

                        let option_text_color = if is_opt_disabled {
                            text_tertiary
                        } else {
                            text_color
                        };

                        // Use Stateful for hover effect on each option
                        let option_item = Stateful::new(ButtonState::Idle)
                            .on_state(move |state: &ButtonState, opt_div: &mut Div| {
                                let bg_color = if is_opt_disabled {
                                    idle_bg
                                } else {
                                    match state {
                                        ButtonState::Hovered | ButtonState::Pressed => hover_bg,
                                        _ => idle_bg,
                                    }
                                };

                                opt_div.merge(
                                    div()
                                        .flex_row()
                                        .items_center()
                                        .h(36.0)
                                        .p_px(padding)
                                        .bg(bg_color)
                                        .cursor(if is_opt_disabled {
                                            CursorStyle::NotAllowed
                                        } else {
                                            CursorStyle::Pointer
                                        })
                                        .child(
                                            text(&opt_label)
                                                .size(font_size)
                                                .color(option_text_color),
                                        ),
                                );
                            })
                            .on_click(move |_ctx| {
                                if !is_opt_disabled {
                                    // Set the new value
                                    value_state_for_opt.set(opt_value_for_click.clone());
                                    // Close the dropdown
                                    open_state_for_opt.set(false);

                                    if let Some(ref cb) = on_change_for_opt {
                                        cb(&opt_value_for_click);
                                    }
                                }
                            });

                        dropdown_div = dropdown_div.child(option_item);
                    }

                    main_container = main_container.child(dropdown_div);
                }

                container.merge(main_container);
            });

        // Build the outer container with optional label
        let mut select_container = div().w_full().child(select_element);

        // Apply width if specified
        if let Some(w) = config.width {
            select_container = select_container.w(w);
        }

        if disabled {
            select_container = select_container.opacity(0.5);
        }

        // If there's a label, wrap in a container
        let inner = if let Some(ref label_text) = config.label {
            let spacing = theme.spacing_value(SpacingToken::Space2);
            let mut outer = div().flex_col().gap_px(spacing);

            if let Some(w) = config.width {
                outer = outer.w(w);
            } else {
                outer = outer.w_full();
            }

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
    open_state: State<bool>,
    options: Vec<SelectOption>,
    placeholder: Option<String>,
    label: Option<String>,
    size: SelectSize,
    disabled: bool,
    width: Option<f32>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

impl SelectConfig {
    fn new(value_state: State<String>, open_state: State<bool>) -> Self {
        Self {
            value_state,
            open_state,
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
    config: SelectConfig,
    /// Cached built Select - built lazily on first access
    built: OnceCell<Select>,
}

impl SelectBuilder {
    /// Create a new select builder with value state and open state
    pub fn new(value_state: &State<String>, open_state: &State<bool>) -> Self {
        Self {
            config: SelectConfig::new(value_state.clone(), open_state.clone()),
            built: OnceCell::new(),
        }
    }

    /// Get or build the inner Select
    fn get_or_build(&self) -> &Select {
        self.built
            .get_or_init(|| Select::from_config(self.config.clone()))
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
}

/// Create a select with value state and open state
///
/// The select uses state-driven reactivity - changes to either state
/// will trigger a rebuild of the component.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
///     let fruit = ctx.use_state_keyed("fruit", || "apple".to_string());
///     let open = ctx.use_state_keyed("fruit_open", || false);
///
///     cn::select(&fruit, &open)
///         .placeholder("Choose a fruit...")
///         .option("apple", "Apple")
///         .option("banana", "Banana")
///         .on_change(|v| println!("Selected: {}", v))
/// }
/// ```
pub fn select(value_state: &State<String>, open_state: &State<bool>) -> SelectBuilder {
    SelectBuilder::new(value_state, open_state)
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
