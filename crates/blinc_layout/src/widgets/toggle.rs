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
//! let bold_on = ctx.use_signal(false);
//! toggle(bold_on)
//!     .label("B")
//!     .on_change(|on| println!("bold: {}", on))
//! ```

use std::sync::Arc;

use blinc_core::reactive::{ReactiveGraph, Signal, computed};

use crate::binding::{IntoReactive, Reactive};
use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::svg::svg;
use crate::text::Text;
use crate::tree::{LayoutNodeId, LayoutTree};

/// The class whose rules in [`crate::ua_styles::UA_STYLES`] give a toggle its
/// default look.
pub const DEFAULT_CLASS: &str = "blinc-toggle";

/// Toggle configuration.
#[derive(Clone)]
pub struct ToggleConfig {
    /// Whether it is on.
    on: Signal<bool>,
    /// Whether it is disabled; bound in place.
    disabled: Reactive<bool>,
    /// Text label; bound in place.
    label: Option<Reactive<String>>,
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
    fn new(on: Signal<bool>) -> Self {
        Self {
            on,
            disabled: Reactive::Const(false),
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
        let on_change = config.on_change.clone();
        let disabled = config.disabled.clone();

        // `data-state` is the on/off pair ashui's stylesheet and Radix write;
        // `checked` is the attribute `:checked` rules match.
        let on = config.on;
        let off = computed(move |g: &ReactiveGraph| !g.get(on).unwrap_or(false));

        let mut body = div()
            .class(DEFAULT_CLASS)
            .attr("data-variant", &config.variant)
            .attr("data-size", &config.size)
            .attr_when("data-state", "on", on)
            .attr_when("data-state", "off", &off)
            .attr_when("checked", "", on)
            .attr_when("disabled", "", config.disabled.clone())
            .cursor_pointer();
        for class in &config.css_classes {
            body = body.class(class);
        }
        if let Some(id) = &config.css_element_id {
            body = body.id(id);
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
        if let Some(label) = config.label {
            body = body.child(Text::bound(label));
        }

        let inner = body.on_click(move |_| {
            if disabled.now().unwrap_or(false) {
                return;
            }
            let next = !on.try_get().unwrap_or(false);
            on.set(next);
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
    /// A toggle that reads and writes `on`.
    pub fn new(on: Signal<bool>) -> Self {
        Self {
            config: ToggleConfig::new(on),
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

    /// The label: a string, Signal or Computed.
    pub fn label(mut self, label: impl IntoReactive<String>) -> Self {
        self.config.label = Some(label.into_reactive());
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

    /// Whether it is disabled: a bool, Signal or Computed.
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = disabled.into_reactive();
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

/// A toggle that reads and writes `on`, following it in place.
///
/// ```ignore
/// let bold = ctx.use_signal(false);
/// toggle(bold).label("B").on_change(|on| /* … */)
/// ```
pub fn toggle(on: Signal<bool>) -> ToggleBuilder {
    ToggleBuilder::new(on)
}
