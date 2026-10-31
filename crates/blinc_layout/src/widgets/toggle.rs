//! Toggle — a button-shaped binary on/off control.
//!
//! Visually a button that stays "pressed in" while `on`. Distinct from
//! `switch` (sliding track + thumb, lives in `blinc_cn::switch`) and
//! [`checkbox`](mod@super::checkbox) (small box with checkmark). The
//! canonical use is icon-only formatting toggles in a toolbar — bold /
//! italic / underline — or a single-key feature flag in settings.
//!
//! A toggle is one element. It carries `data-state="on"|"off"` (and a
//! `checked` attribute while on), `data-variant`, `data-size` and `disabled`,
//! so how it looks in each state, variant and size, and how it eases between
//! them, is the `.blinc-toggle` rules of [`crate::ua_styles::UA_STYLES`].
//! Restyle it with rules of your own:
//!
//! ```css
//! .my-toggle[data-state="on"] { background: #16a34a; }
//! ```
//!
//! # Example
//!
//! ```ignore
//! let bold_on = ctx.use_state_keyed("bold", || false);
//! toggle(&bold_on)
//!     .label("B")
//!     .on_change(|on| println!("bold: {}", on))
//! ```

use std::sync::Arc;

use blinc_core::State;
use blinc_core::reactive::{ReactiveGraph, computed};

use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::svg::svg;
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};

/// The class whose rules in [`crate::ua_styles::UA_STYLES`] give a toggle its
/// default look.
pub const DEFAULT_CLASS: &str = "blinc-toggle";

/// Toggle configuration.
#[derive(Clone)]
pub struct ToggleConfig {
    /// Reactive on/off state.
    on: State<bool>,
    /// Whether the toggle is disabled.
    pub disabled: bool,
    /// Optional text label.
    pub label: Option<String>,
    /// Optional SVG icon markup (rendered before the label).
    pub icon: Option<String>,
    /// Icon size in logical px.
    pub icon_size: f32,
    /// `data-variant`: `default` or `outline`.
    pub variant: String,
    /// `data-size`: `sm`, `md` or `lg`.
    pub size: String,
    /// CSS element ID for stylesheet matching.
    pub css_element_id: Option<String>,
    /// CSS class names.
    pub css_classes: Vec<Arc<str>>,
    /// Change handler.
    pub on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl ToggleConfig {
    fn new(on: State<bool>) -> Self {
        Self {
            on,
            disabled: false,
            label: None,
            icon: None,
            icon_size: 16.0,
            variant: "default".into(),
            size: "md".into(),
            css_element_id: None,
            css_classes: Vec::new(),
            on_change: None,
        }
    }
}

/// A built toggle: the one element, with its content and its click.
pub struct Toggle {
    inner: Div,
}

impl Toggle {
    fn with_config(config: ToggleConfig) -> Self {
        let on_state_for_click = config.on.clone();
        let on_change = config.on_change.clone();
        let disabled = config.disabled;

        // `data-state` is the on/off pair ashui's stylesheet and Radix write;
        // `checked` is the attribute `:checked` rules match.
        let on = config.on.signal();
        let off = computed(move |g: &ReactiveGraph| !g.get(on).unwrap_or(false));

        let mut body = div()
            .class(DEFAULT_CLASS)
            .attr("data-variant", &config.variant)
            .attr("data-size", &config.size)
            .attr_when("data-state", "on", &config.on)
            .attr_when("data-state", "off", &off)
            .attr_when("checked", "", &config.on)
            .cursor_pointer();
        for class in &config.css_classes {
            body = body.class(class);
        }
        if let Some(id) = &config.css_element_id {
            body = body.id(id);
        }
        if disabled {
            body = body.attr("disabled", "");
        }

        // The label and the icon take the toggle's colour and font from its
        // rules, so nothing here names either.
        if let Some(ref icon_svg) = config.icon {
            body = body.child(
                svg(icon_svg)
                    .size(config.icon_size, config.icon_size)
                    .internal(),
            );
        }
        if let Some(ref label_text) = config.label {
            body = body.child(text(label_text));
        }

        let inner = body.on_click(move |_| {
            if disabled {
                return;
            }
            let next = !on_state_for_click.get();
            on_state_for_click.set(next);
            if let Some(ref handler) = on_change {
                handler(next);
            }
        });

        Self { inner }
    }
}

impl ElementBuilder for Toggle {
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
        Some("toggle")
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

    fn element_classes(&self) -> &[Arc<str>] {
        self.inner.element_classes()
    }
}

/// Lazy builder for [`Toggle`]. Config accumulates via fluent methods,
/// the Toggle itself materialises on first `ElementBuilder` access.
pub struct ToggleBuilder {
    config: ToggleConfig,
    built: std::cell::OnceCell<Toggle>,
}

impl ToggleBuilder {
    pub fn new(on: &State<bool>) -> Self {
        Self {
            config: ToggleConfig::new(on.clone()),
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &Toggle {
        crate::build_once::build_once(&self.built, || Toggle::with_config(self.config.clone()))
    }

    pub fn id(mut self, id: &str) -> Self {
        self.config.css_element_id = Some(id.to_string());
        self
    }

    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.config
            .css_classes
            .push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.config.label = Some(label.into());
        self
    }

    pub fn icon(mut self, svg_markup: impl Into<String>) -> Self {
        self.config.icon = Some(svg_markup.into());
        self
    }

    /// The icon's size in logical px. The label's size comes from the
    /// toggle's rules.
    pub fn icon_size(mut self, px: f32) -> Self {
        self.config.icon_size = px;
        self
    }

    /// `data-variant`: `default` (borderless) or `outline` (a hairline in
    /// both states).
    pub fn variant(mut self, variant: impl Into<String>) -> Self {
        self.config.variant = variant.into();
        self
    }

    /// `data-size`: `sm`, `md` (default) or `lg`.
    pub fn size(mut self, size: impl Into<String>) -> Self {
        self.config.size = size.into();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(handler));
        self
    }
}

impl ElementBuilder for ToggleBuilder {
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
        Some("toggle")
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

    fn element_classes(&self) -> &[Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// Create a toggle bound to a reactive `State<bool>`.
///
/// ```ignore
/// let bold = ctx.use_state_keyed("bold", || false);
/// toggle(&bold).label("B").on_change(|on| /* … */)
/// ```
pub fn toggle(on: &State<bool>) -> ToggleBuilder {
    ToggleBuilder::new(on)
}
