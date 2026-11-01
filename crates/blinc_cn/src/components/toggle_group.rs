//! ToggleGroup — orchestrated row of `cn::toggle` items with shared
//! selection state. Mirrors shadcn/ui's `<ToggleGroup>` for the
//! `type="single"` case (radio-style — one value at a time).
//!
//! Compositional: each item is an element with `data-state` on while
//! `group_state` holds its value, so its look is the `.blinc-toggle` rules of
//! `blinc_layout::ua_styles`, the same as `cn::toggle`; the group sets
//! `data-variant` and `data-size` for its items.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let align = ctx.use_signal("left".to_string());
//!     cn::toggle_group(align)
//!         .variant(cn::ToggleVariant::Outline)
//!         .item(cn::toggle_item("left").icon(to_svg(icons::ALIGN_LEFT, 16.0)))
//!         .item(cn::toggle_item("center").icon(to_svg(icons::ALIGN_CENTER, 16.0)))
//!         .item(cn::toggle_item("right").icon(to_svg(icons::ALIGN_RIGHT, 16.0)))
//! }
//! ```
//!
//! Multi-select (`type="multiple"`) is a follow-up — needs a
//! `Signal<Vec<String>>` overload + a different click-handler that
//! toggles set membership.

use blinc_core::Color;
use blinc_core::reactive::{ReactiveGraph, Signal, computed};
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::ElementBuilder;
use blinc_layout::prelude::*;
use blinc_layout::svg::svg;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::{InstanceKey, Interaction, units};
use blinc_theme::{ColorToken, ThemeState};
use std::sync::Arc;

use crate::components::toggle::{ToggleSize, ToggleVariant};

/// One item in a [`ToggleGroup`]. Constructed via [`toggle_item`].
#[derive(Clone)]
pub struct ToggleItem {
    value: String,
    label: Option<String>,
    icon: Option<String>,
    aria_label: Option<String>,
    disabled: bool,
}

impl ToggleItem {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: None,
            icon: None,
            aria_label: None,
            disabled: false,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn icon(mut self, svg_markup: impl Into<String>) -> Self {
        self.icon = Some(svg_markup.into());
        self
    }

    pub fn aria_label(mut self, label: impl Into<String>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// Builder helper for an item in a toggle group.
pub fn toggle_item(value: impl Into<String>) -> ToggleItem {
    ToggleItem::new(value)
}

/// Toggle group config (internal — driven by the builder).
#[allow(clippy::type_complexity)]
#[derive(Clone)]
struct ToggleGroupConfig {
    value: Signal<String>,
    items: Vec<ToggleItem>,
    variant: ToggleVariant,
    size: ToggleSize,
    disabled: Reactive<bool>,
    /// Set by `.gap(..)`; the group's rules give 4px otherwise.
    gap: Option<f32>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

/// The fully-built ToggleGroup.
pub struct ToggleGroup {
    inner: Div,
}

impl ToggleGroup {
    fn with_config(instance_key: &InstanceKey, config: ToggleGroupConfig) -> Self {
        let mut row = div()
            .class("cn-toggle-group")
            .class("blinc-toggle-group")
            .attr("data-variant", config.variant.data_value())
            .attr("data-size", config.size.data_value())
            .flex_row()
            .items_center();
        if let Some(gap) = config.gap {
            row = row.gap(gap);
        }

        for item in &config.items {
            row = row.child(build_item(instance_key, &config, item));
        }

        Self { inner: row }
    }
}

fn build_item(
    _instance_key: &InstanceKey,
    config: &ToggleGroupConfig,
    item: &ToggleItem,
) -> impl ElementBuilder + 'static {
    let group = config.value;
    let item_value = item.value.clone();
    let item_value_for_click = item.value.clone();
    let item_label = item.label.clone();
    let item_icon = item.icon.clone();
    // An item is disabled on its own, or with the group.
    let disabled = if item.disabled {
        Reactive::Const(true)
    } else {
        config.disabled.clone()
    };
    let on_change = config.on_change.clone();
    let icon_size = config.size.icon_size();
    let _ = item.aria_label.clone(); // future a11y plumb

    // On while the group holds this item's value; the rules do the rest.
    let is_on = {
        let value = item_value.clone();
        computed(move |g: &ReactiveGraph| g.get(group).unwrap_or_default() == value)
    };
    let is_off = {
        let value = item_value;
        computed(move |g: &ReactiveGraph| g.get(group).unwrap_or_default() != value)
    };

    let mut body = div()
        .class("blinc-toggle")
        .class("cn-toggle")
        .attr_when("data-state", "on", &is_on)
        .attr_when("data-state", "off", &is_off)
        .attr_when("checked", "", &is_on)
        .attr_when("disabled", "", disabled.clone())
        .cursor_pointer();

    if let Some(ref icon_svg) = item_icon {
        body = body.child(svg(icon_svg).size(icon_size, icon_size).internal());
    }
    if let Some(ref label_text) = item_label {
        body = body.child(text(label_text));
    }

    body.on_click(move |_| {
        if disabled.now().unwrap_or(false) {
            return;
        }
        // Single-select: clicking the on item is a no-op (matches
        // shadcn's `type="single"`); clicking another item promotes it.
        // To clear selection, the parent state must be set to "" by
        // the caller.
        if group.try_get().as_deref() != Some(item_value_for_click.as_str()) {
            group.set(item_value_for_click.clone());
            if let Some(ref cb) = on_change {
                cb(&item_value_for_click);
            }
        }
    })
}

impl ElementBuilder for ToggleGroup {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.inner.element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("toggle_group")
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.inner.element_classes()
    }
}

/// Lazy builder for [`ToggleGroup`].
pub struct ToggleGroupBuilder {
    key: InstanceKey,
    config: ToggleGroupConfig,
    built: std::cell::OnceCell<ToggleGroup>,
}

impl ToggleGroupBuilder {
    /// A toggle group that reads and writes `value`.
    #[track_caller]
    pub fn new(value: Signal<String>) -> Self {
        Self {
            key: InstanceKey::new("toggle_group"),
            config: ToggleGroupConfig {
                value,
                items: Vec::new(),
                variant: ToggleVariant::default(),
                size: ToggleSize::default(),
                disabled: Reactive::Const(false),
                gap: None,
                on_change: None,
            },
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &ToggleGroup {
        ::blinc_layout::build_once::build_once(&self.built, || {
            ToggleGroup::with_config(&self.key, self.config.clone())
        })
    }

    /// Name this group's per-item state explicitly.
    ///
    /// The default key comes from the call site, which is enough in
    /// Rust. Callers that funnel every group through one call site (the
    /// DSL's FFI thunk does) must supply their own, or two groups share
    /// one key and hovering an item in either scales both.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = InstanceKey::explicit(key);
        self
    }

    /// Append an item to the group.
    pub fn item(mut self, item: ToggleItem) -> Self {
        self.config.items.push(item);
        self
    }

    pub fn variant(mut self, variant: ToggleVariant) -> Self {
        self.config.variant = variant;
        self
    }

    pub fn size(mut self, size: ToggleSize) -> Self {
        self.config.size = size;
        self
    }

    /// Whether every item is disabled: a bool, Signal or Computed.
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = disabled.into_reactive();
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.config.gap = Some(gap);
        self
    }

    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(handler));
        self
    }
}

impl ElementBuilder for ToggleGroupBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("toggle_group")
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// A single-select toggle group that reads and writes `value`.
///
/// Clicking an item sets the group state to that item's value. Clicking
/// the already-selected item is a no-op (matches shadcn's `type="single"`).
#[track_caller]
pub fn toggle_group(value: Signal<String>) -> ToggleGroupBuilder {
    ToggleGroupBuilder::new(value)
}
