//! ToggleGroup — orchestrated row of `cn::toggle` items with shared
//! selection state. Mirrors shadcn/ui's `<ToggleGroup>` for the
//! `type="single"` case (radio-style — one value at a time).
//!
//! Compositional: each item renders a `Stateful<ButtonState>` whose
//! `is_on` derives from `group_state.get() == item.value`. Same theme-
//! token defaults as `cn::toggle` because the visual rules are
//! re-resolved per item inside the callback.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let align = ctx.use_state_keyed("text-align", || "left".to_string());
//!     cn::toggle_group(&align)
//!         .variant(cn::ToggleVariant::Outline)
//!         .item(cn::toggle_item("left").icon(to_svg(icons::ALIGN_LEFT, 16.0)))
//!         .item(cn::toggle_item("center").icon(to_svg(icons::ALIGN_CENTER, 16.0)))
//!         .item(cn::toggle_item("right").icon(to_svg(icons::ALIGN_RIGHT, 16.0)))
//! }
//! ```
//!
//! Multi-select (`type="multiple"`) is a follow-up — needs a
//! `State<Vec<String>>` overload + a different click-handler that
//! toggles set membership.

use blinc_core::{Color, State};
use blinc_layout::div::ElementBuilder;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, stateful_with_key};
use blinc_layout::svg::svg;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::{InstanceKey, units};
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
struct ToggleGroupConfig {
    state: State<String>,
    items: Vec<ToggleItem>,
    variant: ToggleVariant,
    size: ToggleSize,
    disabled: bool,
    gap: f32,
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
            .flex_row()
            .gap(config.gap)
            .items_center();

        for item in &config.items {
            row = row.child(build_item(instance_key, &config, item));
        }

        Self { inner: row }
    }
}

fn build_item(
    instance_key: &InstanceKey,
    config: &ToggleGroupConfig,
    item: &ToggleItem,
) -> impl ElementBuilder + 'static {
    let group_state = config.state.clone();
    let group_state_for_click = config.state.clone();
    let item_value = item.value.clone();
    let item_value_for_click = item.value.clone();
    let item_label = item.label.clone();
    let item_icon = item.icon.clone();
    let item_disabled = item.disabled || config.disabled;
    let variant = config.variant;
    let size = config.size;
    let on_change = config.on_change.clone();

    let height = size.height();
    let padding_x = size.padding_x();
    let icon_size = size.icon_size();
    let font_size_token = size.font_size(&ThemeState::get().typography());

    let key = instance_key.derive(&item.value);
    let _ = item.aria_label.clone(); // future a11y plumb

    let mut item_el = stateful_with_key::<ButtonState>(&key)
        .deps([group_state.signal_id()])
        .on_state(move |ctx| {
            let button_state = ctx.state();
            let is_hovered = matches!(button_state, ButtonState::Hovered | ButtonState::Pressed);
            let is_pressed = matches!(button_state, ButtonState::Pressed);
            let is_on = group_state.get() == item_value;

            let theme = ThemeState::get();
            // Match `cn::toggle`'s per-size radius — Small uses
            // `radius_sm`, Medium / Large use `radius_default`. Pre-fix,
            // group items always used `radius_default` so a Small icon
            // item rendered with noticeably more rounding than a
            // matching Small standalone toggle (which picks up
            // `radius_sm` via the `.cn-toggle--sm` CSS rule).
            let radius = theme.radius(size.radius_token());
            let surface = theme.color(ColorToken::Background);
            let text_primary = theme.color(ColorToken::TextPrimary);

            let on_bg = mix(surface, text_primary, 0.10);
            let off_bg = Color::TRANSPARENT;
            let border_color = theme.color(ColorToken::BorderSecondary);

            let bg = if is_on {
                if is_pressed && !item_disabled {
                    mix(on_bg, text_primary, 0.08)
                } else if is_hovered && !item_disabled {
                    mix(on_bg, text_primary, 0.04)
                } else {
                    on_bg
                }
            } else if is_pressed && !item_disabled {
                mix(off_bg, text_primary, 0.10)
            } else if is_hovered && !item_disabled {
                mix(off_bg, text_primary, 0.05)
            } else {
                off_bg
            };

            let fg = text_primary;

            let mut body = div()
                .h(height)
                .padding_x(units::px(padding_x))
                .flex_row()
                .items_center()
                .justify_center()
                .gap(6.0)
                .bg(bg)
                .rounded(radius)
                .cursor_pointer();

            if matches!(variant, ToggleVariant::Outline) {
                body = body.border(1.0, border_color);
            }

            if let Some(ref icon_svg) = item_icon {
                body = body.child(
                    svg(icon_svg)
                        .size(icon_size, icon_size)
                        .color(fg)
                        .internal(),
                );
            }
            if let Some(ref label_text) = item_label {
                body = body.child(text(label_text).size(font_size_token).color(fg));
            }
            if item_disabled {
                body = body.opacity(0.5);
            }
            body
        });

    item_el = item_el.on_click(move |_| {
        if item_disabled {
            return;
        }
        // Single-select: clicking the on item is a no-op (matches
        // shadcn's `type="single"`); clicking another item promotes it.
        // To clear selection, the parent state must be set to "" by
        // the caller.
        if group_state_for_click.get() != item_value_for_click {
            group_state_for_click.set(item_value_for_click.clone());
            if let Some(ref cb) = on_change {
                cb(&item_value_for_click);
            }
        }
    });

    item_el
}

/// Porter-Duff "over" with the source alpha scaled by `amount`.
/// See `blinc_layout::widgets::toggle::mix` for the rationale — same
/// logic, kept inline here to avoid a public re-export of a 10-line
/// helper.
fn mix(bottom: Color, top: Color, amount: f32) -> Color {
    let src_a = amount.clamp(0.0, 1.0) * top.a;
    let bg_a = bottom.a;
    let out_a = src_a + bg_a * (1.0 - src_a);
    if out_a < 1.0e-6 {
        return Color::rgba(0.0, 0.0, 0.0, 0.0);
    }
    let r = (top.r * src_a + bottom.r * bg_a * (1.0 - src_a)) / out_a;
    let g = (top.g * src_a + bottom.g * bg_a * (1.0 - src_a)) / out_a;
    let b = (top.b * src_a + bottom.b * bg_a * (1.0 - src_a)) / out_a;
    Color::rgba(r, g, b, out_a)
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
    #[track_caller]
    pub fn new(state: &State<String>) -> Self {
        Self {
            key: InstanceKey::new("toggle_group"),
            config: ToggleGroupConfig {
                state: state.clone(),
                items: Vec::new(),
                variant: ToggleVariant::default(),
                size: ToggleSize::default(),
                disabled: false,
                gap: 4.0,
                on_change: None,
            },
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &ToggleGroup {
        ::blinc_layout::build_once::build_once(&self.built, || {
            ToggleGroup::with_config(&self.key, self.clone_config())
        })
    }

    fn clone_config(&self) -> ToggleGroupConfig {
        ToggleGroupConfig {
            state: self.config.state.clone(),
            items: self.config.items.clone(),
            variant: self.config.variant,
            size: self.config.size,
            disabled: self.config.disabled,
            gap: self.config.gap,
            on_change: self.config.on_change.clone(),
        }
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

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.config.gap = gap;
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

/// Create a single-select toggle group bound to a [`State<String>`].
///
/// Clicking an item sets the group state to that item's value. Clicking
/// the already-selected item is a no-op (matches shadcn's `type="single"`).
#[track_caller]
pub fn toggle_group(state: &State<String>) -> ToggleGroupBuilder {
    ToggleGroupBuilder::new(state)
}
