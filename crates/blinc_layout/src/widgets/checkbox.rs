//! Ready-to-use Checkbox widget
//!
//! A checkbox is an element with a `checked` attribute while it is checked and
//! `disabled` while it cannot be used, so how it looks in each state, and how it
//! eases between them, is the `.blinc-checkbox` rules of
//! [`crate::ua_styles::UA_STYLES`]. Restyle it with rules of your own:
//!
//! ```css
//! .my-checkbox:checked { background: #16a34a; border-color: #16a34a; }
//! ```
//!
//! # Example
//!
//! ```ignore
//! let checked = ctx.use_state_keyed("my_checkbox", || false);
//! div().child(
//!     checkbox(&checked)
//!         .label("Remember me")
//!         .on_change(|checked| println!("Checked: {}", checked))
//! )
//! ```

use std::sync::Arc;

use blinc_core::State;
use blinc_theme::{ColorToken, ThemeState};

use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::svg::svg;
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};

/// The class whose rules in [`crate::ua_styles::UA_STYLES`] give a checkbox
/// its default look.
pub const DEFAULT_CLASS: &str = "blinc-checkbox";

/// The mark: ashui's check, a bold round-capped stroke on a 24x24 grid that
/// spans about two thirds of the mark's size, drawn in the box's `color`.
const CHECKMARK_SVG: &str = r#"<svg viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
    <path d="M5 12l5 5L20 7" stroke="currentColor" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round"/>
</svg>"#;

/// Checkbox configuration
#[derive(Clone)]
pub struct CheckboxConfig {
    /// Reactive checked state signal
    checked: State<bool>,
    /// Whether disabled
    pub disabled: bool,
    /// Label text (optional)
    pub label: Option<String>,
    /// Box size
    pub size: f32,
    /// The mark's size; the box's inner area (its size less the border) when not set.
    pub mark_size: Option<f32>,
    /// Gap between box and label, in 4px units; the theme's `space_1` when not set.
    pub gap: Option<f32>,
    /// Label font size (None = the theme's `text_sm`)
    pub label_font_size: Option<f32>,
    /// Corner radius (None = the theme's `RadiusToken::Sm`)
    pub corner_radius: Option<f32>,
    /// Border width
    pub border_width: f32,
    /// CSS element ID for stylesheet matching
    pub css_element_id: Option<String>,
    /// CSS class names for the box
    pub css_classes: Vec<Arc<str>>,
    /// A class the box has while it is checked.
    pub checked_class: Option<Arc<str>>,
    /// Change handler
    pub on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl CheckboxConfig {
    fn new(checked: State<bool>) -> Self {
        Self {
            checked,
            disabled: false,
            label: None,
            size: 20.0,
            mark_size: None,
            gap: None,
            label_font_size: None,
            corner_radius: None,
            border_width: 2.0,
            css_element_id: None,
            css_classes: Vec::new(),
            checked_class: None,
            on_change: None,
        }
    }
}

/// A built checkbox: the box, and a row holding it and its label.
pub struct Checkbox {
    inner: Div,
}

impl Checkbox {
    fn with_config(config: CheckboxConfig) -> Self {
        let theme = ThemeState::get();
        let checked_for_click = config.checked.clone();
        let on_change = config.on_change.clone();
        let disabled = config.disabled;
        let mark_size = config
            .mark_size
            .unwrap_or(config.size - 2.0 * config.border_width);
        let radius = config
            .corner_radius
            .unwrap_or_else(|| theme.radius(blinc_theme::RadiusToken::Sm));

        let mut checkbox = div()
            .class(DEFAULT_CLASS)
            .attr_when("checked", "", &config.checked)
            .w(config.size)
            .h(config.size)
            // Pin the box to its size: a narrow flex row (a settings list)
            // would otherwise let the layout squeeze it while the label keeps
            // its natural width.
            .flex_shrink_0()
            .rounded(radius)
            .cursor_pointer()
            .items_center()
            .justify_center()
            // The width varies per size, so it is set here; the fill, the
            // border colour, hover and the mark are rules.
            .border_width(config.border_width)
            // The mark is always there and shown while checked.
            .child(
                div()
                    .class("blinc-checkbox-mark")
                    .w(mark_size)
                    .h(mark_size)
                    .child(svg(CHECKMARK_SVG).size(mark_size, mark_size)),
            );
        for class in &config.css_classes {
            checkbox = checkbox.class(class);
        }
        if let Some(class) = &config.checked_class {
            checkbox = checkbox.class_when(class.as_ref(), &config.checked);
        }
        if let Some(id) = &config.css_element_id {
            checkbox = checkbox.id(id);
        }
        if disabled {
            checkbox = checkbox.attr("disabled", "");
        }

        let checkbox = checkbox.on_click(move |_| {
            if disabled {
                return;
            }
            let next = !checked_for_click.get();
            checked_for_click.set(next);
            if let Some(ref handler) = on_change {
                handler(next);
            }
        });

        let inner = if let Some(ref label) = config.label {
            let label_color = if disabled {
                theme.color(ColorToken::TextTertiary)
            } else {
                theme.color(ColorToken::TextPrimary)
            };
            let state = config.checked.clone();
            let on_change = config.on_change.clone();
            // The label toggles too, as the box does.
            div()
                .flex_row()
                .gap(config.gap.unwrap_or(theme.spacing().space_1))
                .items_center()
                .cursor_pointer()
                .child(checkbox)
                .child(
                    text(label)
                        .size(config.label_font_size.unwrap_or(theme.typography().text_sm))
                        .color(label_color),
                )
                .on_click(move |_| {
                    if disabled {
                        return;
                    }
                    let next = !state.get();
                    state.set(next);
                    if let Some(ref handler) = on_change {
                        handler(next);
                    }
                })
        } else {
            // A single box in a row, so it lays out as a unit.
            div().child(checkbox)
        };

        Self { inner }
    }
}

impl ElementBuilder for Checkbox {
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
        Some("checkbox")
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

/// Builder for creating Checkbox components with fluent API.
/// Config is accumulated via builder methods; the actual Checkbox is built lazily
/// on first access (build/render_props/children).
pub struct CheckboxBuilder {
    config: CheckboxConfig,
    /// Cached built Checkbox — built lazily on first access
    built: std::cell::OnceCell<Checkbox>,
}

impl CheckboxBuilder {
    /// Create a new checkbox builder with reactive checked state
    pub fn new(checked: &State<bool>) -> Self {
        Self {
            config: CheckboxConfig::new(checked.clone()),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Checkbox
    fn get_or_build(&self) -> &Checkbox {
        crate::build_once::build_once(&self.built, || Checkbox::with_config(self.config.clone()))
    }

    /// Set the CSS element ID for stylesheet matching
    pub fn id(mut self, id: &str) -> Self {
        self.config.css_element_id = Some(id.to_string());
        self
    }

    /// Add a class to the box, for your own rules.
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.config
            .css_classes
            .push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// A class the box has while it is checked.
    pub fn checked_class(mut self, name: impl AsRef<str>) -> Self {
        self.config.checked_class = Some(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Set label
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    /// Set checkbox box size
    pub fn checkbox_size(mut self, size: f32) -> Self {
        self.config.size = size;
        self
    }

    /// Set the mark's size.
    pub fn mark_size(mut self, size: f32) -> Self {
        self.config.mark_size = Some(size);
        self
    }

    /// Gap between the box and its label, in 4px units.
    pub fn gap(mut self, gap: f32) -> Self {
        self.config.gap = Some(gap);
        self
    }

    /// Set label font size
    pub fn label_font_size(mut self, size: f32) -> Self {
        self.config.label_font_size = Some(size);
        self
    }

    /// Set corner radius
    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.corner_radius = Some(radius);
        self
    }

    /// Set border width
    pub fn border_width(mut self, width: f32) -> Self {
        self.config.border_width = width;
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Set change handler
    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(handler));
        self
    }
}

impl ElementBuilder for CheckboxBuilder {
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
        Some("checkbox")
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

/// Create a checkbox with reactive checked state
///
/// The checkbox follows the `State<bool>` in place: its `checked` attribute
/// comes and goes with it, and the stylesheet's `:checked`, `:hover` and
/// `:disabled` rules do the rest.
///
/// # Example
///
/// ```ignore
/// let checked = ctx.use_state_keyed("remember_me", || false);
/// checkbox(&checked)
///     .label("Remember me")
///     .on_change(|checked| println!("Checked: {}", checked))
/// ```
pub fn checkbox(checked: &State<bool>) -> CheckboxBuilder {
    CheckboxBuilder::new(checked)
}

/// Create a checkbox with label and reactive state
pub fn checkbox_labeled(checked: &State<bool>, label: impl Into<String>) -> CheckboxBuilder {
    CheckboxBuilder::new(checked).label(label)
}
