//! Switch component for boolean toggle
//!
//! A themed toggle switch. It is an element with a `checked` attribute while it
//! is on, so how it looks off, on, hovered, pressed and disabled, and how it
//! eases between them, is the `.cn-switch` rules of `CN_STYLES`. Restyle it
//! with rules of your own:
//!
//! ```css
//! .my-switch:checked { background: #16a34a; }
//! ```
//!
//! It reads and writes a `Signal<bool>`; `label` and `disabled` are bound in
//! place.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let enabled = ctx.use_signal(false);
//!
//!     cn::switch(enabled)
//!         .label("Enable notifications")
//!         .on_change(|is_on| println!("Switch: {}", is_on))
//! }
//!
//! cn::switch(dark_mode).size(SwitchSize::Small)
//! cn::switch(enabled).disabled(locked)
//! ```

use blinc_core::reactive::Signal;
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::style;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::ThemeState;
use std::sync::Arc;

/// Switch size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SwitchSize {
    /// Small switch (32x18)
    Small,
    /// Medium switch (44x24)
    #[default]
    Medium,
    /// Large switch (52x28)
    Large,
}

impl SwitchSize {
    /// Get the track width for this size
    fn track_width(&self) -> f32 {
        match self {
            SwitchSize::Small => 32.0,
            SwitchSize::Medium => 44.0,
            SwitchSize::Large => 52.0,
        }
    }

    /// Get the track height for this size
    fn track_height(&self) -> f32 {
        match self {
            SwitchSize::Small => 18.0,
            SwitchSize::Medium => 24.0,
            SwitchSize::Large => 28.0,
        }
    }

    /// The class that carries this size's thumb travel in `CN_STYLES`.
    fn css_class(&self) -> &'static str {
        match self {
            SwitchSize::Small => "cn-switch--sm",
            SwitchSize::Medium => "cn-switch--md",
            SwitchSize::Large => "cn-switch--lg",
        }
    }

    /// Get the thumb diameter for this size
    fn thumb_size(&self) -> f32 {
        match self {
            SwitchSize::Small => 14.0,
            SwitchSize::Medium => 20.0,
            SwitchSize::Large => 24.0,
        }
    }
}

/// Switch component
///
/// A toggle switch whose thumb slides between its ends.
pub struct Switch {
    /// The fully-built inner element (Div containing switch and optional label)
    inner: Div,
}

impl Switch {
    /// A switch that reads and writes `on`.
    pub fn new(on: Signal<bool>) -> Self {
        Self::with_config(SwitchConfig::new(on))
    }

    /// Create from a full configuration
    fn with_config(config: SwitchConfig) -> Self {
        let theme = ThemeState::get();
        let track_width = config.size.track_width();
        let track_height = config.size.track_height();
        let thumb_size = config.size.thumb_size();
        let padding = 2.0; // Padding from track edge
        let radius = track_height / 2.0; // Fully rounded track
        let on = config.on;

        // The thumb sits at the track's start; `:checked` rules move it. Its
        // size and the track's are layout, set here; every colour, the
        // shadow and the travel are rules.
        let thumb = div()
            .class("cn-switch-thumb")
            .w(thumb_size)
            .h(thumb_size)
            .style(&style! { rounded: thumb_size / 2.0 });

        let switch = div()
            .class("cn-switch")
            .class("tabindex=0")
            .class(config.size.css_class())
            .w(track_width)
            .h(track_height)
            .style(&style! { rounded: radius })
            .cursor_pointer()
            .items_center()
            .padding_x(blinc_layout::units::px(padding))
            .child(thumb)
            // `:checked` and `:disabled` rules follow the switch in place.
            .attr_when("checked", "", on)
            .attr_when("disabled", "", config.disabled.clone());

        // The track and its label both toggle; the bound `disabled` is read
        // when the click arrives.
        let toggle = {
            let disabled = config.disabled.clone();
            let on_change = config.on_change.clone();
            move || {
                if disabled.now().unwrap_or(false) {
                    return;
                }
                let next = !on.try_get().unwrap_or(false);
                on.set(next);
                if let Some(ref callback) = on_change {
                    callback(next);
                }
            }
        };

        let inner = if let Some(label) = config.label {
            let row_toggle = toggle.clone();
            let switch = switch.on_click(move |_| toggle());
            // The row carries `disabled` too, so the label's colour is a rule.
            div()
                .class("cn-switch-row")
                .attr_when("disabled", "", config.disabled)
                .flex_row()
                .gap(theme.spacing().space_1)
                .items_center()
                .cursor_pointer()
                .child(switch)
                .child(
                    blinc_layout::text::Text::bound(label)
                        .class("cn-switch-label")
                        .size(theme.typography().text_sm),
                )
                .on_click(move |_| row_toggle())
        } else {
            // A single switch in a row, so it lays out as a unit.
            div().child(switch.on_click(move |_| toggle()))
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

impl ElementBuilder for Switch {
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

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
}

/// Internal configuration for building a Switch
#[derive(Clone)]
struct SwitchConfig {
    on: Signal<bool>,
    size: SwitchSize,
    label: Option<Reactive<String>>,
    disabled: Reactive<bool>,
    on_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
}

impl SwitchConfig {
    fn new(on: Signal<bool>) -> Self {
        Self {
            on,
            size: SwitchSize::default(),
            label: None,
            disabled: Reactive::Const(false),
            on_change: None,
        }
    }
}

/// Builder for creating Switch components with fluent API
pub struct SwitchBuilder {
    config: SwitchConfig,
    /// Cached built Switch - built lazily on first access
    built: std::cell::OnceCell<Switch>,
}

impl SwitchBuilder {
    /// A switch that reads and writes `on`.
    pub fn new(on: Signal<bool>) -> Self {
        Self {
            config: SwitchConfig::new(on),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Get or build the inner Switch
    fn get_or_build(&self) -> &Switch {
        ::blinc_layout::build_once::build_once(&self.built, || {
            Switch::with_config(self.config.clone())
        })
    }

    /// Set the switch size
    pub fn size(mut self, size: SwitchSize) -> Self {
        self.config.size = size;
        self
    }

    /// The label beside the track: a string, Signal or Computed.
    pub fn label(mut self, label: impl IntoReactive<String>) -> Self {
        self.config.label = Some(label.into_reactive());
        self
    }

    /// Whether it is disabled: a bool, Signal or Computed.
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = disabled.into_reactive();
        self
    }

    /// Set the change callback
    ///
    /// Called when the switch is toggled, with the new state.
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(callback));
        self
    }

    /// Build the final Switch component
    pub fn build_component(self) -> Switch {
        Switch::with_config(self.config)
    }
}

impl ElementBuilder for SwitchBuilder {
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

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// A switch that reads and writes `on`, following it in place.
///
/// # Example
///
/// ```ignore
/// use blinc_cn::prelude::*;
///
/// fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
///     let dark_mode = ctx.use_signal(false);
///
///     cn::switch(dark_mode)
///         .label("Dark mode")
///         .on_change(|enabled| println!("Dark mode: {}", enabled))
/// }
/// ```
pub fn switch(on: Signal<bool>) -> SwitchBuilder {
    SwitchBuilder::new(on)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_switch_sizes() {
        assert_eq!(SwitchSize::Small.track_width(), 32.0);
        assert_eq!(SwitchSize::Medium.track_width(), 44.0);
        assert_eq!(SwitchSize::Large.track_width(), 52.0);
    }

    #[test]
    fn test_switch_track_heights() {
        assert_eq!(SwitchSize::Small.track_height(), 18.0);
        assert_eq!(SwitchSize::Medium.track_height(), 24.0);
        assert_eq!(SwitchSize::Large.track_height(), 28.0);
    }

    #[test]
    fn test_switch_thumb_sizes() {
        assert_eq!(SwitchSize::Small.thumb_size(), 14.0);
        assert_eq!(SwitchSize::Medium.thumb_size(), 20.0);
        assert_eq!(SwitchSize::Large.thumb_size(), 24.0);
    }

    #[test]
    fn test_thumb_travel() {
        let size = SwitchSize::Medium;
        let padding = 2.0;
        let travel = size.track_width() - size.thumb_size() - (padding * 2.0);
        // Travel should be: 44 - 20 - 4 = 20
        assert_eq!(travel, 20.0);
    }
}
