//! NumberGauge: a number over a range, shown as a fill with the value centred
//! on it.
//!
//! Drag the fill's edge or press anywhere to seek, scroll over it, or, once it
//! has been pressed, use the arrows, Page Up/Down and Home/End. Values snap to
//! `step` from the range's start. The width follows the formatted bounds rather than the
//! value, so it never changes while the value does; a narrower parent may
//! shrink it.
//!
//! Every colour, the border, radius and cursor are `.cn-number-gauge` rules in
//! `CN_STYLES`; sizes are layout and set here.
//!
//! ```ignore
//! let level = ctx.use_signal(40.0);
//! cn::number_gauge(level).range(0.0..=100.0).format(|n| format!("{n} %"))
//! ```

use blinc_core::BlincContextState;
use blinc_core::reactive::{Computed, ReactiveGraph, Signal, computed};
use blinc_layout::InstanceKey;
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::ElementBuilder;
use blinc_layout::event_handler::EventContext;
use blinc_layout::prelude::*;
use blinc_layout::svg::svg;
use blinc_layout::text::Text;
use blinc_layout::text_measure::{TextLayoutOptions, measure_text_with_options};
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};

/// Gauge size: height 24, 32 or 40 with a 12, 13 or 14 px value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GaugeSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl GaugeSize {
    fn height(self) -> f32 {
        match self {
            GaugeSize::Small => 24.0,
            GaugeSize::Medium => 32.0,
            GaugeSize::Large => 40.0,
        }
    }

    fn font_size(self) -> f32 {
        match self {
            GaugeSize::Small => 12.0,
            GaugeSize::Medium => 13.0,
            GaugeSize::Large => 14.0,
        }
    }

    /// The narrowest the gauge is made, however short its values.
    fn base_width(self) -> f32 {
        match self {
            GaugeSize::Small => 90.0,
            GaugeSize::Medium => 114.0,
            GaugeSize::Large => 126.0,
        }
    }

    /// The class that carries this size's radius in `CN_STYLES`.
    pub(crate) fn css_class(self) -> &'static str {
        match self {
            GaugeSize::Small => "cn-number-gauge--sm",
            GaugeSize::Medium => "cn-number-gauge--md",
            GaugeSize::Large => "cn-number-gauge--lg",
        }
    }
}

pub(crate) const BORDER: f32 = 1.0;
pub(crate) const GRIP_WIDTH: f32 = 11.0;
/// Room kept beside the widest value so it never meets the border.
const VALUE_SPACE: f32 = 48.0;
const VALUE_WEIGHT: u16 = 500;

/// Six dots, coloured by the grip's `color`.
const GRIP_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="7" height="12" viewBox="0 0 7 12" fill="currentColor"><circle cx="1.5" cy="1.5" r="1.25"/><circle cx="5.5" cy="1.5" r="1.25"/><circle cx="1.5" cy="6" r="1.25"/><circle cx="5.5" cy="6" r="1.25"/><circle cx="1.5" cy="10.5" r="1.25"/><circle cx="5.5" cy="10.5" r="1.25"/></svg>"#;

pub(crate) type Format = Arc<dyn Fn(f64) -> String + Send + Sync>;
pub(crate) type OnChange = Arc<dyn Fn(f64) + Send + Sync>;

/// The bounds and step of a gauge, and the arithmetic over them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GaugeRange {
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl GaugeRange {
    /// The bounds of `bounds`, and `step`.
    pub fn over(bounds: &RangeInclusive<f64>, step: f64) -> Self {
        Self::new(*bounds.start(), *bounds.end(), step)
    }

    /// `max` below `min` is raised to it, and a step that is not positive is 1.
    pub fn new(min: f64, max: f64, step: f64) -> Self {
        assert!(
            min.is_finite() && max.is_finite() && step.is_finite(),
            "a gauge's bounds and step must be finite"
        );
        Self {
            min,
            max: max.max(min),
            step: if step > 0.0 { step } else { 1.0 },
        }
    }

    /// `v` on the step grid from `min`, within the bounds and rounded to the
    /// precision of the step and `min`. A value past `max` takes the last grid
    /// point at or below it.
    pub fn snap(&self, v: f64) -> f64 {
        if !v.is_finite() {
            return self.min;
        }
        let (base, step) = (self.min, self.step);
        let scale = 10f64.powi((decimals(step) + decimals(base)) as i32);
        let grid = |k: f64| ((base + k * step) * scale).round() / scale;
        let mut out = grid(((v - base) / step).round());
        if out > self.max {
            let mut last = ((self.max - base) / step).floor();
            // A subtraction such as 2.05 - 1.05 can fall just below a whole step.
            if grid(last + 1.0) <= self.max {
                last += 1.0;
            }
            out = grid(last);
        }
        out.max(self.min)
    }

    /// Where `v` sits between the bounds, from 0 to 1.
    pub fn fraction(&self, v: f64) -> f32 {
        if self.max > self.min && v.is_finite() {
            ((v - self.min) / (self.max - self.min)).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }

    /// What Page Up and Page Down move by: a tenth of the range, at least a step.
    pub fn page(&self) -> f64 {
        self.step.max((self.max - self.min) / 10.0)
    }

    /// The value whose grip centre is `x` from the gauge's left edge, in a
    /// gauge `width` wide.
    pub fn at(&self, x: f32, width: f32) -> f64 {
        let travel = width - 2.0 * BORDER - GRIP_WIDTH;
        let along = x - BORDER - GRIP_WIDTH / 2.0;
        let t = if travel > 0.0 { along / travel } else { 0.0 };
        self.min + t as f64 * (self.max - self.min)
    }

    /// The value a navigation key moves `v` to, or `None` for any other key.
    pub fn key(&self, v: f64, key_code: u32) -> Option<f64> {
        Some(match key_code {
            39 | 38 => v + self.step,
            37 | 40 => v - self.step,
            33 => v + self.page(),
            34 => v - self.page(),
            36 => self.min,
            35 => self.max,
            _ => return None,
        })
    }
}

/// Digits after the point in `x` as `{}` prints it.
fn decimals(x: f64) -> usize {
    let s = format!("{x}");
    s.find('.').map_or(0, |dot| s.len() - dot - 1)
}

/// The default format: the shortest form of the number.
pub(crate) fn plain(n: f64) -> String {
    format!("{n}")
}

/// The width that holds the widest of `samples`, or the size's base width.
pub(crate) fn width_for(size: GaugeSize, samples: &[String]) -> f32 {
    let mut options = TextLayoutOptions::new();
    options.font_weight = VALUE_WEIGHT;
    let advance = samples
        .iter()
        .map(|s| measure_text_with_options(s, size.font_size(), &options).width)
        .fold(0.0f32, f32::max);
    size.base_width().max((advance + VALUE_SPACE).ceil())
}

/// The bounds and the values a step in from each, formatted: whichever is
/// widest sets the width.
pub(crate) fn width_samples(range: GaugeRange, format: &Format) -> Vec<String> {
    [
        range.min,
        range.max,
        range.snap(range.min + range.step),
        range.snap(range.max - range.step),
    ]
    .into_iter()
    .map(|n| format(n))
    .collect()
}

/// Adds what every gauge face shares to `row`: a fill and a rest growing in
/// proportion to `fraction`, with the grip between them.
pub(crate) fn face(
    row: Div,
    fraction: impl Fn(&ReactiveGraph) -> f32 + Send + Sync + 'static,
) -> Div {
    let fraction = Arc::new(fraction);
    let rest_fraction = Arc::clone(&fraction);
    let fill = div()
        .class("cn-number-gauge-fill")
        .w(0.0)
        .h_full()
        .pointer_events_none()
        .flex_grow_value(computed(move |g: &ReactiveGraph| fraction(g)));
    let grip = div()
        .class("cn-number-gauge-thumb")
        .w(GRIP_WIDTH)
        .h_full()
        .flex_shrink_0()
        .flex_col()
        .items_center()
        .justify_center()
        .pointer_events_none()
        .child(
            svg(GRIP_SVG)
                .size(7.0, 12.0)
                .internal()
                .class("cn-number-gauge-grip"),
        );
    let rest = div()
        .class("cn-number-gauge-rest")
        .w(0.0)
        .h_full()
        .pointer_events_none()
        .flex_grow_value(computed(move |g: &ReactiveGraph| 1.0 - rest_fraction(g)));
    row.child(fill).child(grip).child(rest)
}

/// The formatted value, centred over the gauge and letting the pointer through.
pub(crate) fn value_label(
    size: GaugeSize,
    text: impl Fn(&ReactiveGraph) -> String + Send + Sync + 'static,
) -> Div {
    div()
        .class("cn-number-gauge-value")
        .absolute()
        .inset(0.0)
        .flex_row()
        .items_center()
        .justify_center()
        .pointer_events_none()
        .child(
            Text::bound(Reactive::Computed(computed(text)))
                .size(size.font_size())
                .medium()
                .no_wrap(),
        )
}

/// The shell every gauge shares: its size, border, width rule and the state
/// attributes its rules key on.
/// A bound flag as a computed, whatever form it was given in, so it can both
/// bind an attribute and be read when an event arrives.
pub(crate) fn bound_flag(flag: impl IntoReactive<bool>) -> Computed<bool> {
    match flag.into_reactive() {
        Reactive::Const(on) => computed(move |_: &ReactiveGraph| on),
        Reactive::Bound(state) => {
            let signal = state.signal();
            computed(move |g: &ReactiveGraph| g.get(signal).unwrap_or(false))
        }
        Reactive::Computed(c) => c,
    }
}

pub(crate) fn shell(
    size: GaugeSize,
    width: Option<f32>,
    fit: f32,
    disabled: &Computed<bool>,
) -> Div {
    let shell = div()
        .class("cn-number-gauge")
        .class("tabindex=0")
        .class(size.css_class())
        .relative()
        .flex_row()
        .h(size.height())
        .border_width(BORDER)
        // The border is painted over the box, so the parts are inset by it
        // here, as a CSS border would inset them.
        .padding(blinc_layout::units::px(BORDER))
        .overflow_clip()
        .align_self_start()
        .flex_shrink()
        .attr_when("disabled", "", disabled.clone());
    match width {
        Some(w) => shell.w(w),
        // The width the bounds need, unless the parent is narrower.
        None => shell.w(fit).max_w_full(),
    }
}

/// Pointer focus for a gauge: set by a press on it, cleared by a press
/// anywhere else. Keys act only while it holds, and the ring shows only once a
/// key has been used, as `:focus-visible` would.
#[derive(Clone, Copy)]
pub(crate) struct GaugeFocus {
    pub focused: Signal<bool>,
    pub keyed: Signal<bool>,
}

impl GaugeFocus {
    pub fn new(instance: &str, element_id: &str) -> Self {
        let ctx = BlincContextState::get();
        let focused = ctx.use_signal_keyed(&format!("{instance}_focused"), || false);
        let keyed = ctx.use_signal_keyed(&format!("{instance}_keyed"), || false);
        let lost = focused;
        blinc_layout::click_outside::register_click_outside(instance, element_id, move || {
            if lost.get() {
                lost.set(false);
            }
        });
        Self { focused, keyed }
    }

    /// A pointer press takes focus and hides the ring.
    pub fn press(&self) {
        if !self.focused.get() {
            self.focused.set(true);
        }
        if self.keyed.get() {
            self.keyed.set(false);
        }
    }

    /// A key that moved the gauge shows the ring.
    pub fn key(&self) {
        if !self.keyed.get() {
            self.keyed.set(true);
        }
    }

    pub fn visible(&self) -> blinc_core::reactive::Computed<bool> {
        let (focused, keyed) = (self.focused, self.keyed);
        computed(move |g: &ReactiveGraph| {
            g.get(focused).unwrap_or(false) && g.get(keyed).unwrap_or(false)
        })
    }
}

/// What a gauge does with the pointer, the wheel and the keyboard.
pub(crate) struct Edits {
    /// The pointer is `x` from the gauge's left edge in a gauge `width` wide;
    /// `press` is true for the press that starts a drag.
    pub seek: Arc<dyn Fn(f32, f32, bool) + Send + Sync>,
    /// Whole wheel steps, with the pointer's `x` and the gauge's width.
    pub wheel: Arc<dyn Fn(i32, f32, f32) + Send + Sync>,
    /// A key while focused; true when it moved the gauge.
    pub key: Arc<dyn Fn(u32) -> bool + Send + Sync>,
}

/// Wires `gauge` to `edits`: a press seeks and takes focus, a drag keeps
/// seeking, the wheel steps, and keys act while it is focused. Adds the
/// `data-state="dragging"` and `data-focus-visible` attributes its rules use,
/// and `element_id`, which a press elsewhere is told apart by.
pub(crate) fn interactive(
    gauge: Div,
    instance: &str,
    element_id: &str,
    disabled: Computed<bool>,
    edits: Edits,
) -> Div {
    let dragging =
        BlincContextState::get().use_signal_keyed(&format!("{instance}_dragging"), || false);
    let focus = GaugeFocus::new(instance, element_id);
    let wheel_left = Arc::new(Mutex::new(0.0f32));
    // The gauge's left edge and width as of the press: a drag event that
    // leaves the gauge carries no bounds of it.
    let pressed_on = Arc::new(Mutex::new((0.0f32, 0.0f32)));
    let Edits { seek, wheel, key } = edits;
    gauge
        .id(element_id)
        .attr_when("data-state", "dragging", dragging)
        .attr_when("data-focus-visible", "", focus.visible())
        .on_mouse_down({
            let seek = Arc::clone(&seek);
            let (pressed_on, disabled) = (Arc::clone(&pressed_on), disabled.clone());
            move |ctx: &EventContext| {
                if disabled.get() {
                    return;
                }
                focus.press();
                dragging.set(true);
                *pressed_on.lock().unwrap() = (ctx.bounds_x, ctx.bounds_width);
                seek(ctx.local_x, ctx.bounds_width, true);
            }
        })
        .on_drag({
            let disabled = disabled.clone();
            move |ctx: &EventContext| {
                if !disabled.get() {
                    let (left, width) = *pressed_on.lock().unwrap();
                    seek(ctx.mouse_x - left, width, false);
                }
            }
        })
        .on_drag_end(move |_| dragging.set(false))
        .on_mouse_up(move |_| {
            if dragging.get() {
                dragging.set(false);
            }
        })
        .on_scroll({
            let (left, disabled) = (Arc::clone(&wheel_left), disabled.clone());
            move |ctx: &EventContext| {
                let mut left = left.lock().unwrap();
                if disabled.get() {
                    *left = 0.0;
                    return;
                }
                if let Some(steps) = super::number_wheel::wheel_steps(
                    &mut left,
                    ctx.scroll_delta_x,
                    ctx.scroll_delta_y,
                ) {
                    if steps != 0 {
                        wheel(steps, ctx.local_x, ctx.bounds_width);
                    }
                }
            }
        })
        .on_hover_leave(move |_| *wheel_left.lock().unwrap() = 0.0)
        .on_key_down(move |ctx: &EventContext| {
            let tabbed = blinc_layout::event_router::keyboard_focused_node() == Some(ctx.node_id);
            if !disabled.get() && (focus.focused.get() || tabbed) && key(ctx.key_code) {
                focus.key();
            }
        })
}

struct Config {
    value: Signal<f64>,
    bounds: RangeInclusive<f64>,
    step: f64,
    size: GaugeSize,
    format: Option<Format>,
    disabled: Computed<bool>,
    width: Option<f32>,
    classes: Vec<String>,
    on_change: Option<OnChange>,
}

/// Lazy builder for [`NumberGauge`].
pub struct NumberGaugeBuilder {
    key: InstanceKey,
    config: Config,
    built: std::cell::OnceCell<NumberGauge>,
}

pub struct NumberGauge {
    inner: Div,
}

impl NumberGaugeBuilder {
    #[track_caller]
    pub fn new(value: Signal<f64>) -> Self {
        Self {
            key: InstanceKey::new("cn_number_gauge"),
            config: Config {
                value,
                bounds: 0.0..=100.0,
                step: 1.0,
                size: GaugeSize::Medium,
                format: None,
                disabled: bound_flag(false),
                width: None,
                classes: Vec::new(),
                on_change: None,
            },
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &NumberGauge {
        ::blinc_layout::build_once::build_once(&self.built, || {
            NumberGauge::from_config(self.key.get(), &self.config)
        })
    }

    /// The values it covers, `0.0..=100.0` unless set.
    pub fn range(mut self, bounds: RangeInclusive<f64>) -> Self {
        self.config.bounds = bounds;
        self
    }

    /// The step values snap to, counted from the range's start; 1 unless set.
    pub fn step(mut self, step: f64) -> Self {
        self.config.step = step;
        self
    }

    pub fn size(mut self, size: GaugeSize) -> Self {
        self.config.size = size;
        self
    }

    /// How the centred value is written, for example `|n| format!("{n} %")`.
    pub fn format<F>(mut self, format: F) -> Self
    where
        F: Fn(f64) -> String + Send + Sync + 'static,
    {
        self.config.format = Some(Arc::new(format));
        self
    }

    /// Whether it is inert: a value, a Signal or a Computed, followed in place.
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = bound_flag(disabled);
        self
    }

    /// A fixed width in place of the one the formatted bounds give.
    pub fn w(mut self, px: f32) -> Self {
        self.config.width = Some(px);
        self
    }

    /// A class for rules of your own to match.
    pub fn class(mut self, name: impl Into<String>) -> Self {
        self.config.classes.push(name.into());
        self
    }

    /// Called with each value the user sets; a value set from code is not
    /// reported.
    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(f64) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(handler));
        self
    }
}

impl NumberGauge {
    fn from_config(instance: &str, config: &Config) -> Self {
        let range = GaugeRange::over(&config.bounds, config.step);
        let format = config.format.clone().unwrap_or_else(|| Arc::new(plain));
        let size = config.size;
        let disabled = config.disabled.clone();

        let value = config.value;
        let fraction =
            move |g: &ReactiveGraph| range.fraction(range.snap(g.get(value).unwrap_or(range.min)));
        let label = {
            let format = Arc::clone(&format);
            move |g: &ReactiveGraph| format(range.snap(g.get(value).unwrap_or(range.min)))
        };

        // A user edit: snapped, written only when it changes, then reported.
        let set: Arc<dyn Fn(f64) + Send + Sync> = {
            let on_change = config.on_change.clone();
            Arc::new(move |v: f64| {
                let v = range.snap(v);
                if v == value.get() {
                    return;
                }
                value.set(v);
                if let Some(ref cb) = on_change {
                    cb(v);
                }
            })
        };

        let fit = width_for(size, &width_samples(range, &format));
        let shell = config
            .classes
            .iter()
            .fold(shell(size, config.width, fit, &disabled), |d, c| d.class(c));
        let gauge = interactive(
            face(shell, fraction).child(value_label(size, label)),
            instance,
            &format!("cn-number-gauge-{instance}"),
            disabled,
            Edits {
                seek: Arc::new({
                    let set = Arc::clone(&set);
                    move |x, width, _press| set(range.at(x, width))
                }),
                wheel: Arc::new({
                    let set = Arc::clone(&set);
                    move |steps, _x, _width| set(value.get() + steps as f64 * range.step)
                }),
                key: Arc::new({
                    move |key_code| match range.key(range.snap(value.get()), key_code) {
                        Some(next) => {
                            set(next);
                            true
                        }
                        None => false,
                    }
                }),
            },
        );

        Self { inner: gauge }
    }
}

impl ElementBuilder for NumberGaugeBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().inner.build(tree)
    }

    fn render_props(&self) -> blinc_layout::element::RenderProps {
        self.get_or_build().inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().inner.children_builders()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.get_or_build().inner.element_type_id()
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(&self.get_or_build().inner)
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.get_or_build().inner)
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(&self.get_or_build().inner)
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.get_or_build().inner.element_classes()
    }
}

/// A number over a range, shown as a fill. See the module docs.
#[track_caller]
pub fn number_gauge(value: Signal<f64>) -> NumberGaugeBuilder {
    NumberGaugeBuilder::new(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_snap_to_the_step_from_min() {
        let r = GaugeRange::new(1.0, 10.0, 2.0);
        assert_eq!(r.snap(4.2), 5.0);
        assert_eq!(r.snap(-3.0), 1.0);
        // 10 is off the grid from 1, so the last point at or below it.
        assert_eq!(r.snap(12.0), 9.0);
    }

    #[test]
    fn snapping_rounds_to_the_step_precision() {
        let r = GaugeRange::new(0.0, 1.0, 0.1);
        assert_eq!(r.snap(0.30000000000000004), 0.3);
        let r = GaugeRange::new(1.05, 2.05, 0.5);
        assert_eq!(r.snap(2.05), 2.05);
    }

    #[test]
    fn keys_step_page_and_jump() {
        let r = GaugeRange::new(0.0, 200.0, 1.0);
        assert_eq!(r.key(50.0, 39), Some(51.0));
        assert_eq!(r.key(50.0, 40), Some(49.0));
        assert_eq!(r.key(50.0, 33), Some(70.0));
        assert_eq!(r.key(50.0, 36), Some(0.0));
        assert_eq!(r.key(50.0, 35), Some(200.0));
        assert_eq!(r.key(50.0, 65), None);
        // A page is never less than a step.
        assert_eq!(GaugeRange::new(0.0, 5.0, 1.0).page(), 1.0);
    }

    #[test]
    fn the_grip_centre_maps_to_the_value() {
        let r = GaugeRange::new(0.0, 100.0, 1.0);
        let width = 2.0 * BORDER + GRIP_WIDTH + 100.0;
        assert_eq!(r.at(BORDER + GRIP_WIDTH / 2.0, width), 0.0);
        assert_eq!(r.at(BORDER + GRIP_WIDTH / 2.0 + 100.0, width), 100.0);
    }

    #[test]
    fn a_reversed_range_collapses_to_min() {
        let r = GaugeRange::new(5.0, 1.0, 0.0);
        assert_eq!((r.min, r.max, r.step), (5.0, 5.0, 1.0));
        assert_eq!(r.fraction(5.0), 0.0);
    }
}
