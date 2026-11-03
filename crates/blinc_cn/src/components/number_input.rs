//! NumberInput — themed wrapper around [`blinc_layout::widgets::number_input`](mod@blinc_layout::widgets::number_input).
//!
//! Adds the cn surface (`.cn-input`-shaped focus ring, hover bg, the
//! `.cn-number-input` class hook for downstream override) plus a `+` /
//! `−` stepper pair flanking the field. All parse / clamp / format work
//! lives in the layout widget; the cn layer contributes look + the
//! stepper affordance.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
//!     let qty = ctx.use_signal(1.0);
//!     cn::number_input(qty).min(0.0).max(99.0).step(1.0).precision(0)
//! }
//! ```

use blinc_core::reactive::{ReactiveGraph, Signal, computed};
use blinc_icons::{icons, to_svg};
use blinc_layout::InstanceKey;
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::ElementBuilder;
use blinc_layout::prelude::*;
use blinc_layout::svg::svg;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::text_input::{
    InputType, SharedTextInputData, refresh_text_input, text_input, text_input_data,
};
use blinc_theme::{RadiusToken, ThemeState};
use std::sync::Arc;

use crate::components::input::InputSize;

/// Internal config for the cn number input.
#[allow(clippy::type_complexity)]
#[derive(Clone)]
struct Config {
    state: Signal<f64>,
    min: Option<f64>,
    max: Option<f64>,
    step: f64,
    precision: usize,
    size: InputSize,
    placeholder: Option<blinc_layout::binding::Reactive<String>>,
    disabled: Reactive<bool>,
    /// Explicit total width (steppers + field). `None` = auto-derive
    /// from precision + min / max bounds.
    width: Option<f32>,
    /// Cap on the auto-derived total width. Ignored when `width` is
    /// `Some(_)`. Defaults to 120 px.
    max_width: f32,
    on_change: Option<Arc<dyn Fn(f64) + Send + Sync>>,
}

/// Lazy builder for the cn number input.
pub struct NumberInputBuilder {
    key: InstanceKey,
    config: Config,
    built: std::cell::OnceCell<NumberInput>,
}

pub struct NumberInput {
    inner: Div,
}

impl NumberInputBuilder {
    /// A number input that reads and writes `state`.
    #[track_caller]
    pub fn new(state: Signal<f64>) -> Self {
        Self {
            key: InstanceKey::new("cn_number_input"),
            config: Config {
                state,
                min: None,
                max: None,
                step: 1.0,
                precision: 2,
                size: InputSize::Medium,
                placeholder: None,
                disabled: Reactive::Const(false),
                width: None,
                max_width: 200.0,
                on_change: None,
            },
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &NumberInput {
        ::blinc_layout::build_once::build_once(&self.built, || {
            NumberInput::from_config(&self.key, self.config.clone())
        })
    }

    pub fn min(mut self, min: f64) -> Self {
        self.config.min = Some(min);
        self
    }

    pub fn max(mut self, max: f64) -> Self {
        self.config.max = Some(max);
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        self.config.step = step;
        self
    }

    pub fn precision(mut self, precision: usize) -> Self {
        self.config.precision = precision;
        self
    }

    pub fn size(mut self, size: InputSize) -> Self {
        self.config.size = size;
        self
    }

    /// The text shown while the field is empty. A signal here replaces the
    /// text in place.
    pub fn placeholder(mut self, text: impl blinc_layout::binding::IntoReactive<String>) -> Self {
        self.config.placeholder = Some(text.into_reactive());
        self
    }

    /// Whether it is disabled: a bool, Signal or Computed.
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = disabled.into_reactive();
        self
    }

    /// Explicit total width (steppers + field). When unset, the width
    /// auto-derives from `precision` + `min` / `max` bounds, capped by
    /// [`Self::max_w`].
    pub fn w(mut self, px: f32) -> Self {
        self.config.width = Some(px);
        self
    }

    /// Cap on the auto-derived total width. Ignored when [`Self::w`]
    /// is set. Default 120 px.
    pub fn max_w(mut self, px: f32) -> Self {
        self.config.max_width = px;
        self
    }

    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(f64) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(handler));
        self
    }
}

/// A number input's dimensions at one size. Its scale is its own, a step
/// smaller than a text input's: the control sits beside other controls in a
/// row rather than taking one.
struct Metrics {
    row: f32,
    step: f32,
    /// The narrowest the field is, however short its bounds.
    field: f32,
    font: f32,
    padding: f32,
    radius: RadiusToken,
    /// The group's `data-size`.
    name: &'static str,
}

impl Metrics {
    fn of(size: InputSize) -> Self {
        let (row, step, field, font, padding, name) = match size {
            InputSize::Small => (24.0, 24.0, 40.0, 12.0, 4.0, "sm"),
            InputSize::Medium => (32.0, 32.0, 48.0, 13.0, 6.0, "md"),
            InputSize::Large => (40.0, 36.0, 52.0, 14.0, 8.0, "lg"),
        };
        let radius = match size {
            InputSize::Small => RadiusToken::Sm,
            _ => RadiusToken::Default,
        };
        Self {
            row,
            step,
            field,
            font,
            padding,
            radius,
            name,
        }
    }
}

impl NumberInput {
    fn from_config(instance_key: &InstanceKey, config: Config) -> Self {
        let theme = ThemeState::get();
        let m = Metrics::of(config.size);
        let radius = theme.radius(m.radius);

        // The width follows the bounds, not the value, so typing digits
        // never resizes the control.
        let auto_field_w =
            auto_field_width(config.min, config.max, config.precision, m.font, m.field);
        let total_w = config
            .width
            .unwrap_or_else(|| (auto_field_w + m.step * 2.0).min(config.max_width));
        let field_w = (total_w - m.step * 2.0).max(m.field);

        let data = text_input_data();
        if let Ok(mut d) = data.lock() {
            d.value = format_number(config.state.try_get().unwrap_or(0.0), config.precision);
            d.cursor = d.value.chars().count();
            d.input_type = InputType::Number;
        }
        show_state_in_field(&config, &data);

        let field = number_field(&config, &data, field_w, &m);
        let wheel_remainder = Arc::new(std::sync::Mutex::new(0.0f32));
        let group = div()
            .w(total_w)
            .h(m.row)
            .flex_row()
            .items_center()
            .rounded(radius)
            .border_width(1.0)
            .overflow_clip()
            .class("cn-number-input-group")
            .attr("data-size", m.name)
            .attr_when("disabled", "", config.disabled.clone())
            .on_scroll(wheel_stepper(&config, Arc::clone(&wheel_remainder)))
            .on_hover_leave(move |_| *wheel_remainder.lock().unwrap() = 0.0)
            .child(stepper(&config, -1.0, &m))
            .child(field)
            .child(stepper(&config, 1.0, &m));

        Self {
            inner: div().h_fit().w_fit().child(group),
        }
    }
}

/// Wheel stepping: vertical scrolling over the control steps the
/// value by its step, wheel up increasing it. Leaving the control clears a
/// partial step. A disabled control does nothing.
fn wheel_stepper(
    config: &Config,
    remainder: Arc<std::sync::Mutex<f32>>,
) -> impl Fn(&blinc_layout::event_handler::EventContext) + Send + Sync + 'static {
    let (state, on_change) = (config.state, config.on_change.clone());
    let (min, max, step) = (config.min, config.max, config.step);
    let disabled = config.disabled.clone();
    move |ctx| {
        let mut left = remainder.lock().unwrap();
        if disabled.now().unwrap_or(false) {
            *left = 0.0;
            return;
        }
        let Some(steps) =
            super::number_wheel::wheel_steps(&mut left, ctx.scroll_delta_x, ctx.scroll_delta_y)
        else {
            return;
        };
        if steps == 0 {
            return;
        }
        let current = state.try_get().unwrap_or(0.0);
        let next = clamp(current + steps as f64 * step, min, max);
        if (next - current).abs() < f64::EPSILON {
            return;
        }
        // The field picks the value up through `show_state_in_field`.
        state.set(next);
        if let Some(ref cb) = on_change {
            cb(next);
        }
    }
}

/// Keep the field showing the bound value.
///
/// Every change to the state is written into the field, formatted, except
/// while the field is focused: the user is typing there, and writing the
/// formatted string back would canonicalise mid-edit (`"3"` to `"3.0"` at
/// precision 1, so the next `"0"` lands after the `.0`). A step taken with
/// the field focused sets `force_sync_once`, so it still shows.
fn show_state_in_field(config: &Config, data: &SharedTextInputData) {
    let signal = config.state;
    let precision = config.precision;
    let data = data.clone();
    blinc_core::reactive::effect(move |graph| {
        // Through the passed graph: the first run holds the global one.
        let Some(value) = graph.get(signal) else {
            return;
        };
        let formatted = format_number(value, precision);
        let changed = match data.lock() {
            Ok(mut d) => {
                let force = std::mem::take(&mut d.force_sync_once);
                if (force || !d.visual.is_focused()) && d.value != formatted {
                    d.value = formatted;
                    d.cursor = d.value.chars().count();
                    d.selection_start = None;
                    d.scroll_offset_x = 0.0;
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        };
        if changed {
            refresh_text_input(&data);
        }
    });
}

/// The text field between the steppers: parses what is typed into the
/// state and steps it from the keyboard.
fn number_field(
    config: &Config,
    data: &SharedTextInputData,
    width: f32,
    m: &Metrics,
) -> blinc_layout::widgets::text_input::TextInput {
    let mut field = text_input(data)
        .input_type(InputType::Number)
        .text_align(blinc_core::TextAlign::Center)
        .text_size(m.font)
        .padding_x(m.padding)
        .w(width)
        .h(m.row)
        .rounded(0.0)
        .border_width(0.0)
        .idle_bg_color(blinc_core::Color::TRANSPARENT)
        .class("cn-number-input-field")
        .disabled(config.disabled.clone());

    // Keyboard stepping (↑ / ↓ / `+` / `−`): `+1` increments and `-1`
    // decrements, as the buttons do.
    {
        let state = config.state;
        let (min, max, step) = (config.min, config.max, config.step);
        let on_change = config.on_change.clone();
        let data = data.clone();
        field = field.on_step(move |delta| {
            let current = state.try_get().unwrap_or(0.0);
            let next = clamp(current + delta as f64 * step, min, max);
            if (next - current).abs() < f64::EPSILON {
                return; // at bound — no-op
            }
            // Before the write: the field shows the value even though it
            // is focused.
            if let Ok(mut d) = data.lock() {
                d.force_sync_once = true;
            }
            state.set(next);
            if let Some(ref cb) = on_change {
                cb(next);
            }
        });
    }
    if let Some(ref p) = config.placeholder {
        field = field.placeholder(p.clone());
    }
    // Parse + clamp on every keystroke. Empty / partial strings (`""`,
    // `"-"`, `"."`, `"-."`) leave `state` untouched so the user can finish
    // typing without the wrapper "correcting" mid-edit.
    {
        let state = config.state;
        let (min, max, precision) = (config.min, config.max, config.precision);
        let data = data.clone();
        let on_change = config.on_change.clone();
        field = field.on_change(move |text| {
            let trimmed = text.trim();
            if trimmed.is_empty() || trimmed == "-" || trimmed == "." || trimmed == "-." {
                return;
            }
            let Ok(parsed) = trimmed.parse::<f64>() else {
                return;
            };
            let clamped = clamp(parsed, min, max);
            // Only a change is written: every write wakes the dependents.
            if (clamped - state.try_get().unwrap_or(0.0)).abs() > f64::EPSILON {
                state.set(clamped);
                if let Some(ref cb) = on_change {
                    cb(clamped);
                }
            }
            // A clamped value goes back into the field in its canonical form.
            if (clamped - parsed).abs() > f64::EPSILON {
                if let Ok(mut d) = data.lock() {
                    d.value = format_number(clamped, precision);
                    d.cursor = d.value.chars().count();
                }
                refresh_text_input(&data);
            }
        });
    }
    field
}

/// A `+` or `−` button: steps the state by `delta` steps. Its fill and icon
/// colour are `.cn-number-input-step` rules; it is `disabled` while the input
/// is, and where a step would change nothing.
fn stepper(config: &Config, delta: f64, m: &Metrics) -> Div {
    let (min, max, step) = (config.min, config.max, config.step);
    let is_dec = delta < 0.0;

    // Disabled with the input, and where a step would change nothing.
    let value = config.state;
    let at_bound = {
        let disabled = config.disabled.clone();
        computed(move |g: &ReactiveGraph| {
            disabled.read(g).unwrap_or(false)
                || g.get(value)
                    .is_some_and(|v| clamp(v + delta * step, min, max) == v)
        })
    };

    // SVG icons rather than glyphs: a glyph renders in whatever face and
    // weight resolve at run time, and shifted between paints.
    let icon_size = (m.font + 2.0).min(m.row * 0.45);
    let icon = to_svg(if is_dec { icons::MINUS } else { icons::PLUS }, icon_size);

    let state = config.state;
    let on_change = config.on_change.clone();
    let disabled = config.disabled.clone();
    div()
        .w(m.step)
        .h(m.row)
        .flex_row()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .class("cn-number-input-step")
        .class(if is_dec {
            "cn-number-input-step--dec"
        } else {
            "cn-number-input-step--inc"
        })
        .attr_when("disabled", "", &at_bound)
        .child(svg(&icon).size(icon_size, icon_size).internal())
        .on_click(move |_| {
            if disabled.now().unwrap_or(false) {
                return;
            }
            let current = state.try_get().unwrap_or(0.0);
            let next = clamp(current + delta * step, min, max);
            if (next - current).abs() < f64::EPSILON {
                return; // at bound — no-op
            }
            // The field picks the value up through `show_state_in_field`.
            state.set(next);
            if let Some(ref cb) = on_change {
                cb(next);
            }
        })
}

/// Mirror of `layout_number_input::format_value` — kept in sync via
/// the rule that integer precision rounds half-to-even and other
/// precisions go through the standard `{:.N}` format spec.
fn format_number(value: f64, precision: usize) -> String {
    if precision == 0 {
        return (value.round() as i64).to_string();
    }
    format!("{value:.precision$}")
}

fn clamp(value: f64, min: Option<f64>, max: Option<f64>) -> f64 {
    let v = if let Some(lo) = min {
        value.max(lo)
    } else {
        value
    };
    if let Some(hi) = max { v.min(hi) } else { v }
}

fn auto_field_width(
    min: Option<f64>,
    max: Option<f64>,
    precision: usize,
    font_size: f32,
    min_width: f32,
) -> f32 {
    let chars = estimate_max_chars(min, max, precision).max(2) as f32;
    (chars * font_size * 0.65 + 8.0).max(min_width)
}

/// Estimate the widest visible string length given precision + bounds.
/// Width is based on bounds, not the current value, so digit entry does
/// not resize the control.
fn estimate_max_chars(min: Option<f64>, max: Option<f64>, precision: usize) -> usize {
    let chars_for = |v: f64| -> usize {
        // Integer part length: `log10(|v|)` clamped to ≥ 1, plus sign.
        let abs = v.abs();
        let int_digits = if abs < 10.0 {
            1
        } else {
            (abs.log10().floor() as usize) + 1
        };
        let sign = if v < 0.0 { 1 } else { 0 };
        let frac = if precision > 0 { 1 + precision } else { 0 };
        sign + int_digits + frac
    };
    match (min, max) {
        (Some(lo), Some(hi)) => chars_for(lo).max(chars_for(hi)),
        (Some(v), None) | (None, Some(v)) => chars_for(v).max(6),
        (None, None) => 6,
    }
}

impl ElementBuilder for NumberInput {
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
        Some("number_input")
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

impl ElementBuilder for NumberInputBuilder {
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
        Some("number_input")
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

/// A cn number input that reads and writes `state`, following it in place.
#[track_caller]
pub fn number_input(state: Signal<f64>) -> NumberInputBuilder {
    NumberInputBuilder::new(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Once;
    use std::sync::atomic::AtomicBool;

    use blinc_core::{BlincContextState, HookState, ReactiveGraph};
    use taffy::{AvailableSpace, Size};

    static INIT_CONTEXT: Once = Once::new();

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });

        // The process-wide graph, as every other test module initialises it: one that
        // builds its own would leave signals made elsewhere in this process on a different graph.
        INIT_CONTEXT.call_once(|| {
            if !BlincContextState::is_initialized() {
                BlincContextState::init(
                    blinc_core::reactive::global_graph(),
                    Arc::new(std::sync::Mutex::new(HookState::new())),
                    Arc::new(AtomicBool::new(false)),
                );
            }
        });
    }

    fn test_state(initial: f64) -> Signal<f64> {
        blinc_core::reactive::signal(initial)
    }

    fn number_input_widths(value: f64) -> (f32, f32) {
        init_theme();

        let state = test_state(value);
        let input = number_input(state)
            .min(0.0)
            .max(99.0)
            .step(1.0)
            .precision(0);

        let mut tree = LayoutTree::new();
        let root = input.build(&mut tree);
        tree.compute_layout(
            root,
            Size {
                width: AvailableSpace::Definite(400.0),
                height: AvailableSpace::Definite(120.0),
            },
        );

        let group = tree.children(root)[0];
        let field = tree.children(group)[1];
        (
            tree.get_bounds(group, (0.0, 0.0)).unwrap().width,
            tree.get_bounds(field, (0.0, 0.0)).unwrap().width,
        )
    }

    #[test]
    fn auto_width_covers_configured_range() {
        assert_eq!(estimate_max_chars(Some(0.0), Some(99.0), 0), 2);
        assert_eq!(estimate_max_chars(Some(-50.0), Some(60.0), 1), 5);
        assert!(
            auto_field_width(Some(-50.0), Some(60.0), 1, 14.0, 32.0)
                > auto_field_width(Some(0.0), Some(99.0), 0, 14.0, 32.0)
        );
    }

    #[test]
    fn group_width_does_not_grow_with_digits() {
        let short = number_input_widths(1.0);
        let long = number_input_widths(99.0);

        assert_eq!(short, long);
    }
}
