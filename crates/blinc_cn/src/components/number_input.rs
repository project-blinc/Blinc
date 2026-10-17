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
//!     let qty = ctx.use_state_keyed("qty", || 1.0);
//!     cn::number_input(&qty).min(0.0).max(99.0).step(1.0).precision(0)
//! }
//! ```

use blinc_core::State;
use blinc_core::reactive::{ReactiveGraph, computed};
use blinc_icons::{icons, to_svg};
use blinc_layout::InstanceKey;
use blinc_layout::div::ElementBuilder;
use blinc_layout::interaction::Interaction;
use blinc_layout::prelude::*;
use blinc_layout::svg::svg;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::text_input::{
    InputType, SharedTextInputData, refresh_text_input, text_input, text_input_data,
};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};
use std::sync::Arc;

use crate::components::input::InputSize;

/// Internal config for the cn number input.
#[allow(clippy::type_complexity)]
struct Config {
    state: State<f64>,
    min: Option<f64>,
    max: Option<f64>,
    step: f64,
    precision: usize,
    size: InputSize,
    placeholder: Option<String>,
    disabled: bool,
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
    #[track_caller]
    pub fn new(state: &State<f64>) -> Self {
        Self {
            key: InstanceKey::new("cn_number_input"),
            config: Config {
                state: state.clone(),
                min: None,
                max: None,
                step: 1.0,
                precision: 2,
                size: InputSize::Medium,
                placeholder: None,
                disabled: false,
                width: None,
                max_width: 200.0,
                on_change: None,
            },
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &NumberInput {
        ::blinc_layout::build_once::build_once(&self.built, || {
            NumberInput::from_config(&self.key, self.clone_config())
        })
    }

    fn clone_config(&self) -> Config {
        Config {
            state: self.config.state.clone(),
            min: self.config.min,
            max: self.config.max,
            step: self.config.step,
            precision: self.config.precision,
            size: self.config.size,
            placeholder: self.config.placeholder.clone(),
            disabled: self.config.disabled,
            width: self.config.width,
            max_width: self.config.max_width,
            on_change: self.config.on_change.clone(),
        }
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

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.config.placeholder = Some(text.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
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

impl NumberInput {
    fn from_config(instance_key: &InstanceKey, config: Config) -> Self {
        let theme = ThemeState::get();
        let typography = theme.typography();
        let height = config.size.height(theme);
        let font_size = config.size.font_size(&typography);
        let radius = theme.radius(RadiusToken::Default);
        let border_color = theme.color(ColorToken::BorderSecondary);

        // Steppers are square at the row height.
        let button_w = height;

        // The width follows the bounds, not the value, so typing digits
        // never resizes the control.
        const DEFAULT_FIELD_W: f32 = 32.0;
        let auto_field_w = auto_field_width(
            config.min,
            config.max,
            config.precision,
            font_size,
            DEFAULT_FIELD_W,
        );
        let total_w = config
            .width
            .unwrap_or_else(|| (auto_field_w + button_w * 2.0).min(config.max_width));
        let field_w = (total_w - button_w * 2.0).max(DEFAULT_FIELD_W);

        let data = text_input_data();
        if let Ok(mut d) = data.lock() {
            d.value = format_number(config.state.get(), config.precision);
            d.cursor = d.value.chars().count();
            d.input_type = InputType::Number;
        }
        show_state_in_field(&config, &data);

        let field = number_field(&config, &data, field_w, height);
        let group = div()
            .w(total_w)
            .h(height)
            .flex_row()
            .items_center()
            .rounded(radius)
            .border(1.0, border_color)
            .overflow_clip()
            .class("cn-number-input-group")
            .child(stepper(
                &config,
                &instance_key.derive("step-dec"),
                -1.0,
                height,
            ))
            .child(field)
            .child(stepper(
                &config,
                &instance_key.derive("step-inc"),
                1.0,
                height,
            ));

        Self {
            inner: div().h_fit().w_fit().child(group),
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
    let signal = config.state.signal();
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
    height: f32,
) -> blinc_layout::widgets::text_input::TextInput {
    let mut field = text_input(data)
        .input_type(InputType::Number)
        .text_align(blinc_core::TextAlign::Center)
        .padding_x(4.0)
        .w(width)
        .h(height)
        .rounded(0.0)
        .class("cn-number-input-field")
        .disabled(config.disabled);

    // Keyboard stepping (↑ / ↓ / `+` / `−`): `+1` increments and `-1`
    // decrements, as the buttons do.
    {
        let state = config.state.clone();
        let (min, max, step) = (config.min, config.max, config.step);
        let on_change = config.on_change.clone();
        let data = data.clone();
        field = field.on_step(move |delta| {
            let next = clamp(state.get() + delta as f64 * step, min, max);
            if (next - state.get()).abs() < f64::EPSILON {
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
        let state = config.state.clone();
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
            if (clamped - state.get()).abs() > f64::EPSILON {
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

/// A `+` or `−` button: steps the state by `delta` steps. Its fill follows
/// the pointer, and its icon dims where a step would change nothing.
fn stepper(config: &Config, key: &str, delta: f64, height: f32) -> Div {
    let theme = ThemeState::get();
    let typography = theme.typography();
    let font_size = config.size.font_size(&typography);
    let border_color = theme.color(ColorToken::BorderSecondary);
    let (bg, bg_hover, bg_pressed) = (
        theme.color(ColorToken::InputBg),
        theme.color(ColorToken::InputBgHover),
        theme.color(ColorToken::InputBgFocus),
    );
    let (text_primary, text_tertiary) = (
        theme.color(ColorToken::TextPrimary),
        theme.color(ColorToken::TextTertiary),
    );
    let (min, max, step, disabled) = (config.min, config.max, config.step, config.disabled);
    let is_dec = delta < 0.0;

    let interaction = Interaction::keyed(key);
    let (hovered, pressed) = (
        interaction.hovered().signal(),
        interaction.pressed().signal(),
    );
    let fill = computed(move |g: &ReactiveGraph| {
        if disabled {
            bg
        } else if g.get(pressed).unwrap_or(false) {
            bg_pressed
        } else if g.get(hovered).unwrap_or(false) {
            bg_hover
        } else {
            bg
        }
    });
    let value = config.state.signal();
    let fg = computed(move |g: &ReactiveGraph| {
        let at_bound = g
            .get(value)
            .is_some_and(|v| clamp(v + delta * step, min, max) == v);
        if disabled || at_bound {
            text_tertiary
        } else {
            text_primary
        }
    });

    // SVG icons rather than glyphs: a glyph renders in whatever face and
    // weight resolve at run time, and shifted between paints.
    let icon_size = (font_size + 2.0).min(height * 0.45);
    let icon = to_svg(if is_dec { icons::MINUS } else { icons::PLUS }, icon_size);

    let state = config.state.clone();
    let on_change = config.on_change.clone();
    let cell = div()
        .w(height)
        .h(height)
        .flex_row()
        .items_center()
        .justify_center()
        .bg(&fill)
        .cursor_pointer()
        .track(&interaction)
        .class(if is_dec {
            "cn-number-input-step--dec"
        } else {
            "cn-number-input-step--inc"
        })
        .child(svg(&icon).size(icon_size, icon_size).color(&fg).internal())
        .on_click(move |_| {
            if disabled {
                return;
            }
            let next = clamp(state.get() + delta * step, min, max);
            if (next - state.get()).abs() < f64::EPSILON {
                return; // at bound — no-op
            }
            // The field picks the value up through `show_state_in_field`.
            state.set(next);
            if let Some(ref cb) = on_change {
                cb(next);
            }
        });
    if is_dec {
        cell.border_right(1.0, border_color)
    } else {
        cell.border_left(1.0, border_color)
    }
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

/// Create a cn-styled number input bound to a [`State<f64>`].
#[track_caller]
pub fn number_input(state: &State<f64>) -> NumberInputBuilder {
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

        INIT_CONTEXT.call_once(|| {
            BlincContextState::init(
                Arc::new(std::sync::Mutex::new(ReactiveGraph::new())),
                Arc::new(std::sync::Mutex::new(HookState::new())),
                Arc::new(AtomicBool::new(false)),
            );
        });
    }

    fn test_state(initial: f64) -> State<f64> {
        use blinc_core::reactive::{global_dirty_flag, global_graph, signal};
        State::new(signal(initial), global_graph(), global_dirty_flag())
    }

    fn number_input_widths(value: f64) -> (f32, f32) {
        init_theme();

        let state = test_state(value);
        let input = number_input(&state)
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
