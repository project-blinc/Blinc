//! Slider: a number picked along a track.
//!
//! A fill, the thumb and the rest of the track sit in a row. The fill and the
//! rest grow in proportion to the value, so layout places the thumb and it
//! follows the bound number in place, whoever writes it. A press seeks, or
//! takes hold of the thumb where it was grabbed; a drag follows; the wheel
//! steps; once pressed, the arrows, Page Up/Down and Home/End move it.
//!
//! Every colour, the thumb's border and shadow, and how it grows under the
//! pointer are `.cn-slider` rules in `CN_STYLES`; sizes are layout and set
//! here.
//!
//! ```ignore
//! let volume = ctx.use_signal(50.0);
//! cn::slider(volume)
//!     .max(100.0)
//!     .step(1.0)
//!     .label("Volume")
//!     .show_value()
//!     .disabled(muted)
//! ```

use blinc_core::reactive::{Computed, ReactiveGraph, computed};
use blinc_layout::InstanceKey;
use blinc_layout::accessibility::{AccessibilityAction, AccessibilityBinding, AccessibilityValue};
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::text::Text;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use std::sync::{Arc, Mutex};

use super::number_gauge::{Edits, bound_flag, interactive};
use crate::reactive_props::NumberValue;

/// The track's length when no width is set.
const DEFAULT_WIDTH: f32 = 300.0;

/// Slider size variants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SliderSize {
    /// Small slider (track: 3px, thumb: 12px)
    Small,
    /// Medium slider (track: 4px, thumb: 16px)
    #[default]
    Medium,
    /// Large slider (track: 6px, thumb: 22px)
    Large,
}

impl SliderSize {
    /// The height of the row the thumb sits in.
    fn height(self) -> f32 {
        match self {
            SliderSize::Small => 16.0,
            SliderSize::Medium => 20.0,
            SliderSize::Large => 28.0,
        }
    }

    fn track_height(self) -> f32 {
        match self {
            SliderSize::Small => 3.0,
            SliderSize::Medium => 4.0,
            SliderSize::Large => 6.0,
        }
    }

    fn thumb_size(self) -> f32 {
        match self {
            SliderSize::Small => 12.0,
            SliderSize::Medium => 16.0,
            SliderSize::Large => 22.0,
        }
    }

    /// The label and value's size.
    fn font_size(self) -> f32 {
        match self {
            SliderSize::Small => 12.0,
            SliderSize::Medium => 14.0,
            SliderSize::Large => 16.0,
        }
    }

    /// Between the label row and the track.
    fn gap(self) -> f32 {
        match self {
            SliderSize::Small => 8.0,
            SliderSize::Medium => 10.0,
            SliderSize::Large => 12.0,
        }
    }

    /// The `data-size` its rules key on.
    fn attr(self) -> &'static str {
        match self {
            SliderSize::Small => "sm",
            SliderSize::Medium => "md",
            SliderSize::Large => "lg",
        }
    }
}

/// The bounds, the step and the thumb, and the arithmetic over them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Travel {
    min: f32,
    max: f32,
    step: Option<f32>,
    thumb: f32,
}

impl Travel {
    /// `v` on the step grid from `min`, within the bounds.
    fn snap(&self, v: f32) -> f32 {
        let v = if v.is_finite() { v } else { self.min };
        match self.step {
            Some(s) if s > 0.0 => {
                (self.min + ((v - self.min) / s).round() * s).clamp(self.min, self.max)
            }
            _ => v.clamp(self.min, self.max),
        }
    }

    /// Where `v` sits between the bounds, from 0 to 1.
    fn fraction(&self, v: f32) -> f32 {
        if self.max > self.min && v.is_finite() {
            ((v - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// The value whose thumb centre is `x` from the left of a slider `width`
    /// wide.
    fn at(&self, x: f32, width: f32) -> f32 {
        let travel = width - self.thumb;
        let t = if travel > 0.0 {
            ((x - self.thumb / 2.0) / travel).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.min + t * (self.max - self.min)
    }

    /// Where the thumb's centre is for `v` in a slider `width` wide.
    fn centre(&self, v: f32, width: f32) -> f32 {
        self.thumb / 2.0 + self.fraction(v) * (width - self.thumb).max(0.0)
    }

    /// What an arrow or a wheel step moves by: the step, or a hundredth of
    /// the range without one.
    fn nudge(&self) -> f32 {
        self.step
            .filter(|s| *s > 0.0)
            .unwrap_or((self.max - self.min) / 100.0)
    }

    /// The value a navigation key moves `v` to, or `None` for any other key.
    fn key(&self, v: f32, key_code: u32) -> Option<f32> {
        let page = self.nudge().max((self.max - self.min) / 10.0);
        Some(match key_code {
            39 | 38 => v + self.nudge(),
            37 | 40 => v - self.nudge(),
            33 => v + page,
            34 => v - page,
            36 => self.min,
            35 => self.max,
            _ => return None,
        })
    }

    /// The value as the readout shows it: whole numbers for a whole step,
    /// two places otherwise.
    fn format(&self, v: f32) -> String {
        if self.step.is_some_and(|s| s >= 1.0) {
            format!("{v:.0}")
        } else {
            format!("{v:.2}")
        }
    }
}

/// Slider component. See the module docs.
pub struct Slider {
    inner: Div,
    accessibility: AccessibilityBinding,
}

impl Slider {
    /// A slider over `0.0..=1.0` reading and writing `value`.
    #[track_caller]
    pub fn new(value: impl Into<NumberValue>) -> Self {
        Self::with_config(InstanceKey::new("slider"), SliderConfig::new(value.into()))
    }

    fn with_config(key: InstanceKey, config: SliderConfig) -> Self {
        let size = config.size;
        let value = config.value_state;
        let travel = Travel {
            min: config.min,
            max: config.max.max(config.min),
            step: config.step,
            thumb: size.thumb_size(),
        };
        let width = config.width.unwrap_or(DEFAULT_WIDTH);
        let disabled = config.disabled.clone();
        let instance = key.get().to_string();

        let part = |class: &str| div().class(class).pointer_events_none();
        let row = div()
            .class("cn-slider")
            .class("tabindex=0")
            .attr("data-size", size.attr())
            .attr_when("disabled", "", disabled.clone())
            .flex_row()
            .items_center()
            .flex_shrink_0()
            .w(width)
            .h(size.height())
            .child(
                part("cn-slider-fill")
                    .w(0.0)
                    .h(size.track_height())
                    .flex_grow_value(computed(move |g: &ReactiveGraph| {
                        travel.fraction(value.read(g))
                    })),
            )
            .child(
                part("cn-slider-thumb")
                    .w(travel.thumb)
                    .h(travel.thumb)
                    .flex_shrink_0(),
            )
            .child(
                part("cn-slider-rest")
                    .w(0.0)
                    .h(size.track_height())
                    .flex_grow_value(computed(move |g: &ReactiveGraph| {
                        1.0 - travel.fraction(value.read(g))
                    })),
            );

        // A user edit: snapped, written only when it changes, then reported.
        let set: Arc<dyn Fn(f32) + Send + Sync> = {
            let on_change = config.on_change.clone();
            Arc::new(move |v: f32| {
                let v = travel.snap(v);
                if (v - value.get()).abs() <= f32::EPSILON {
                    return;
                }
                value.set(v);
                if let Some(ref cb) = on_change {
                    cb(v);
                }
            })
        };
        let accessibility = {
            let label = config.label.clone();
            let disabled = disabled.clone();
            let set = set.clone();
            AccessibilityBinding {
                read: Arc::new(move || AccessibilityValue {
                    label: label.as_ref().and_then(|label| label.now()),
                    value: travel.format(value.get()),
                    ..Default::default()
                }),
                action: Arc::new(move |action| {
                    if disabled.try_get().unwrap_or(true) {
                        return false;
                    }
                    let next = match action {
                        AccessibilityAction::Increment => value.get() + travel.nudge(),
                        AccessibilityAction::Decrement => value.get() - travel.nudge(),
                        _ => return false,
                    };
                    set(next);
                    true
                }),
            }
        };
        // How far from the thumb's centre a press on it landed, kept for the
        // drag so the thumb does not jump to the pointer.
        let grab = Arc::new(Mutex::new(0.0f32));
        let slider = interactive(
            row,
            &instance,
            &format!("cn-slider-{instance}"),
            disabled.clone(),
            Edits {
                seek: Arc::new({
                    let set = Arc::clone(&set);
                    move |x, width, press| {
                        let mut grab = grab.lock().unwrap();
                        if press {
                            let off = x - travel.centre(value.get(), width);
                            *grab = if off.abs() <= travel.thumb / 2.0 {
                                off
                            } else {
                                0.0
                            };
                        }
                        set(travel.at(x - *grab, width))
                    }
                }),
                wheel: Arc::new({
                    let set = Arc::clone(&set);
                    move |steps, _x, _width| set(value.get() + steps as f32 * travel.nudge())
                }),
                key: Arc::new(move |key_code| match travel.key(value.get(), key_code) {
                    Some(next) => {
                        set(next);
                        true
                    }
                    None => false,
                }),
            },
        );

        let inner = if config.label.is_some() || config.show_value {
            let caption = |class: &str, text: Reactive<String>| {
                div()
                    .class(class)
                    .child(Text::bound(text).size(size.font_size()).no_wrap())
            };
            let mut header = div()
                .class("cn-slider-header")
                .flex_row()
                .justify_between()
                .items_center();
            if let Some(label) = config.label.clone() {
                header = header.child(caption("cn-slider-label", label));
            }
            if config.show_value {
                let shown = computed(move |g: &ReactiveGraph| travel.format(value.read(g)));
                header = header.child(caption("cn-slider-value", Reactive::Computed(shown)));
            }
            div()
                .class("cn-slider-field")
                .attr("data-size", size.attr())
                .attr_when("data-disabled", "", disabled.clone())
                .flex_col()
                .gap_px(size.gap())
                .w(width)
                .child(header)
                .child(slider)
        } else {
            slider
        };
        let inner = config
            .classes
            .iter()
            .fold(inner, |d, c| d.class(c))
            .attr_when("disabled", "", disabled);

        Self {
            inner,
            accessibility,
        }
    }
}

impl ElementBuilder for Slider {
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

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("slider")
    }

    fn accessibility_binding(&self) -> Option<AccessibilityBinding> {
        Some(self.accessibility.clone())
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.inner)
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(&self.inner)
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.inner.element_classes()
    }
}

/// Internal configuration for building a Slider
#[derive(Clone)]
struct SliderConfig {
    value_state: NumberValue,
    min: f32,
    max: f32,
    step: Option<f32>,
    size: SliderSize,
    label: Option<Reactive<String>>,
    show_value: bool,
    disabled: Computed<bool>,
    width: Option<f32>,
    classes: Vec<String>,
    on_change: Option<Arc<dyn Fn(f32) + Send + Sync>>,
}

impl SliderConfig {
    fn new(value_state: NumberValue) -> Self {
        Self {
            value_state,
            min: 0.0,
            max: 1.0,
            step: None,
            size: SliderSize::default(),
            label: None,
            show_value: false,
            disabled: bound_flag(false),
            width: None,
            classes: Vec::new(),
            on_change: None,
        }
    }
}

/// Builder for [`Slider`], usable inline as an element.
pub struct SliderBuilder {
    key: InstanceKey,
    config: SliderConfig,
    built: std::cell::OnceCell<Slider>,
}

impl SliderBuilder {
    /// A slider reading and writing `value`.
    #[track_caller]
    pub fn new(value: impl Into<NumberValue>) -> Self {
        Self {
            key: InstanceKey::new("slider"),
            config: SliderConfig::new(value.into()),
            built: std::cell::OnceCell::new(),
        }
    }

    /// A slider with an explicit key.
    pub fn with_key(key: impl Into<String>, value: impl Into<NumberValue>) -> Self {
        Self {
            key: InstanceKey::explicit(key),
            config: SliderConfig::new(value.into()),
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &Slider {
        ::blinc_layout::build_once::build_once(&self.built, || {
            Slider::with_config(self.key.clone(), self.config.clone())
        })
    }

    /// The minimum value (default: 0.0)
    pub fn min(mut self, min: f32) -> Self {
        self.config.min = min;
        self
    }

    /// The maximum value (default: 1.0)
    pub fn max(mut self, max: f32) -> Self {
        self.config.max = max;
        self
    }

    /// The step values snap to, counted from the minimum.
    pub fn step(mut self, step: f32) -> Self {
        self.config.step = Some(step);
        self
    }

    pub fn size(mut self, size: SliderSize) -> Self {
        self.config.size = size;
        self
    }

    /// A label above the track: a string, Signal or Computed.
    pub fn label(mut self, label: impl IntoReactive<String>) -> Self {
        self.config.label = Some(label.into_reactive());
        self
    }

    /// Show the current value above the track's far end.
    pub fn show_value(mut self) -> Self {
        self.config.show_value = true;
        self
    }

    /// Whether it is inert: a value, a Signal or a Computed, followed in place.
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = bound_flag(disabled);
        self
    }

    /// The track's length; 300px unless set.
    pub fn w(mut self, width: f32) -> Self {
        self.config.width = Some(width);
        self
    }

    /// A class for rules of your own to match.
    pub fn class(mut self, name: impl Into<String>) -> Self {
        self.config.classes.push(name.into());
        self
    }

    /// Called with each value the user sets; a value set from code is not
    /// reported.
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(f32) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(callback));
        self
    }
}

impl ElementBuilder for SliderBuilder {
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

    fn semantic_type_name(&self) -> Option<&'static str> {
        self.get_or_build().semantic_type_name()
    }

    fn accessibility_binding(&self) -> Option<AccessibilityBinding> {
        self.get_or_build().accessibility_binding()
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

/// A slider reading and writing `value`, a `Signal<f32>` or `Signal<f64>`.
/// See the module docs.
#[track_caller]
pub fn slider(value: impl Into<NumberValue>) -> SliderBuilder {
    SliderBuilder::new(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Travel = Travel {
        min: 0.0,
        max: 100.0,
        step: Some(5.0),
        thumb: 16.0,
    };

    #[test]
    fn values_snap_to_the_step_within_the_bounds() {
        assert_eq!(T.snap(12.0), 10.0);
        assert_eq!(T.snap(13.0), 15.0);
        assert_eq!(T.snap(-4.0), 0.0);
        assert_eq!(T.snap(140.0), 100.0);
    }

    #[test]
    fn the_thumb_centre_maps_to_the_value() {
        let width = 116.0;
        assert_eq!(T.at(8.0, width), 0.0);
        assert_eq!(T.at(108.0, width), 100.0);
        assert_eq!(T.at(58.0, width), 50.0);
        assert_eq!(T.centre(50.0, width), 58.0);
    }

    #[test]
    fn keys_step_page_and_jump() {
        assert_eq!(T.key(50.0, 39), Some(55.0));
        assert_eq!(T.key(50.0, 37), Some(45.0));
        assert_eq!(T.key(50.0, 33), Some(60.0));
        assert_eq!(T.key(50.0, 36), Some(0.0));
        assert_eq!(T.key(50.0, 35), Some(100.0));
        assert_eq!(T.key(50.0, 65), None);
        let free = Travel { step: None, ..T };
        assert_eq!(free.key(50.0, 39), Some(51.0));
    }
}
