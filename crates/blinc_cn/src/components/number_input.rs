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
use blinc_icons::{icons, to_svg};
use blinc_layout::InstanceKey;
use blinc_layout::div::ElementBuilder;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{ButtonState, NoState, stateful_with_key};
use blinc_layout::svg::svg;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::text_input::{
    InputType, SharedTextInputData, text_input, text_input_data,
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
        let bg = theme.color(ColorToken::InputBg);
        let bg_hover = theme.color(ColorToken::InputBgHover);
        let bg_pressed = theme.color(ColorToken::InputBgFocus);
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_tertiary = theme.color(ColorToken::TextTertiary);

        // Steppers are square at the row height.
        let button_w = height;

        // SVG icons for the steppers — using glyph characters would
        // pull whatever font happens to render at the run-time
        // weight / family and made the `+` and `−` shift visually
        // between paints. SVG locks the geometry to Lucide.
        let icon_size = (font_size + 2.0).min(height * 0.45);
        let icon_minus = to_svg(icons::MINUS, icon_size);
        let icon_plus = to_svg(icons::PLUS, icon_size);

        // SharedTextInputData persists across re-renders triggered by
        // the outer Stateful's `state.signal_id()` deps. Without
        // hoisting the data here, each re-render would create a fresh
        // `text_input_data()` (losing cursor / focus / scroll state on
        // every stepper click).
        let data = text_input_data();
        if let Ok(mut d) = data.lock() {
            d.value = format_number(config.state.get(), config.precision);
            d.cursor = d.value.chars().count();
            d.input_type = InputType::Number;
        }

        // Pre-clone everything the outer Stateful callback captures so
        // the `move` closure stays `Fn`. Each `Arc`-clone is cheap.
        let group_key = instance_key.derive("group");
        let cfg_state = config.state.clone();
        let cfg_min = config.min;
        let cfg_max = config.max;
        let cfg_step = config.step;
        let cfg_precision = config.precision;
        let cfg_disabled = config.disabled;
        let cfg_max_width = config.max_width;
        let cfg_explicit_w = config.width;
        let cfg_placeholder = config.placeholder.clone();
        let cfg_on_change = config.on_change.clone();
        let data_for_render = data.clone();
        // Pre-derive stepper key strings outside the outer closure —
        // `InstanceKey` isn't `Send + Sync` so we can't capture it into
        // a `Fn` closure crossed by the Stateful's bg thread access.
        let stepper_dec_key: String = instance_key.derive("step-dec");
        let stepper_inc_key: String = instance_key.derive("step-inc");

        // Outer Stateful watches the bound `State<f64>`. When the
        // value changes (stepper click OR user typing committed via
        // on_change), the callback re-runs and the field width
        // recomputes against the *current* formatted value — fits the
        // content rather than the max possible value.
        let group = stateful_with_key::<NoState>(&group_key)
            .deps([cfg_state.signal_id()])
            .on_state(move |_ctx| {
                let current = cfg_state.get();
                let formatted = format_number(current, cfg_precision);

                // Sync the shared text-input data with the current
                // formatted state value on every re-render. Any
                // code path that calls `state.set(…)` (steppers,
                // keyboard stepping, external callers) bumps the
                // signal, the outer `.deps([state.signal_id()])`
                // fires this callback, and the field picks up the
                // new value next paint.
                //
                // SKIP the sync while the field is focused (user is
                // typing). The on_change handler still pushes parsed
                // values into `state`, but we don't write the
                // *formatted* string back into the visible field —
                // doing so canonicalises mid-edit (`"3"` → `"3.0"`
                // at precision=1) so the next keystroke (`"0"`)
                // lands after the auto-inserted `.0` instead of
                // building up `"30"`. Stepper / external updates
                // happen with the field blurred, so they still sync
                // through this path.
                if let Ok(mut d) = data_for_render.lock() {
                    let is_focused = d.visual.is_focused();
                    // Step changes (see on_step below) must still sync
                    // while focused — only typing should skip this.
                    let force = std::mem::take(&mut d.force_sync_once);
                    if (force || !is_focused) && d.value != formatted {
                        d.value = formatted.clone();
                        d.cursor = d.value.chars().count();
                        d.selection_start = None;
                        d.scroll_offset_x = 0.0;
                    }
                }

                const DEFAULT_FIELD_W: f32 = 32.0;
                let auto_field_w =
                    auto_field_width(cfg_min, cfg_max, cfg_precision, font_size, DEFAULT_FIELD_W);
                let total_w = cfg_explicit_w
                    .unwrap_or_else(|| (auto_field_w + button_w * 2.0).min(cfg_max_width));
                let field_w = (total_w - button_w * 2.0).max(DEFAULT_FIELD_W);

                // Field — text_input directly so cn owns the data
                // lifecycle (the outer Stateful needs the same
                // `Arc<Mutex<…>>` instance across re-renders to keep
                // cursor / focus state).
                let mut field = text_input(&data_for_render)
                    .input_type(InputType::Number)
                    .text_align(blinc_core::TextAlign::Center)
                    .padding_x(4.0)
                    .w(field_w)
                    .h(height)
                    .rounded(0.0)
                    .class("cn-number-input-field")
                    .disabled(cfg_disabled);

                // Wire keyboard stepping (↑ / ↓ / `+` / `−`). The hook
                // receives `+1` for increment, `-1` for decrement —
                // the same direction the `+` / `−` buttons use.
                {
                    let state = cfg_state.clone();
                    let min = cfg_min;
                    let max = cfg_max;
                    let step = cfg_step;
                    let on_change = cfg_on_change.clone();
                    let data_for_step = data_for_render.clone();
                    field = field.on_step(move |delta| {
                        let direction = delta as f64;
                        let next = clamp(state.get() + direction * step, min, max);
                        if (next - state.get()).abs() < f64::EPSILON {
                            return; // at bound — no-op
                        }
                        // Set before state.set() — it can synchronously
                        // trigger the sync above, so the flag must
                        // already be visible when that runs.
                        if let Ok(mut d) = data_for_step.lock() {
                            d.force_sync_once = true;
                        }
                        state.set(next);
                        if let Some(ref cb) = on_change {
                            cb(next);
                        }
                    });
                }
                if let Some(ref p) = cfg_placeholder {
                    field = field.placeholder(p.clone());
                }
                // Parse + clamp on every keystroke. Empty / partial
                // strings (`""`, `"-"`, `"."`, `"-."`) leave `state`
                // untouched so the user can finish typing without the
                // wrapper "correcting" mid-edit.
                {
                    let state = cfg_state.clone();
                    let min = cfg_min;
                    let max = cfg_max;
                    let precision = cfg_precision;
                    let data_ref = data_for_render.clone();
                    let on_change = cfg_on_change.clone();
                    field = field.on_change(move |text| {
                        let trimmed = text.trim();
                        if trimmed.is_empty() || trimmed == "-" || trimmed == "." || trimmed == "-."
                        {
                            return;
                        }
                        if let Ok(parsed) = trimmed.parse::<f64>() {
                            let clamped = clamp(parsed, min, max);
                            // Only mutate `state` if the value actually
                            // changed — otherwise we re-fire the
                            // outer Stateful's deps every keystroke
                            // (state.set always bumps the signal) and
                            // re-render mid-typing, which thrashes the
                            // cursor.
                            if (clamped - state.get()).abs() > f64::EPSILON {
                                state.set(clamped);
                                if let Some(ref cb) = on_change {
                                    cb(clamped);
                                }
                            }
                            // If clamp moved the value, push the
                            // canonical form back into the field.
                            if (clamped - parsed).abs() > f64::EPSILON {
                                if let Ok(mut d) = data_ref.lock() {
                                    d.value = format_number(clamped, precision);
                                    d.cursor = d.value.chars().count();
                                }
                            }
                        }
                    });
                }

                // Stepper buttons — each its own Stateful so hover /
                // pressed bg shifts work, keyed off the parent so
                // identity stays stable across outer re-renders.
                let make_stepper =
                    |label_svg: &str, delta: f64, is_left: bool, stepper_key: &str| {
                        let state = cfg_state.clone();
                        let state_click = cfg_state.clone();
                        let min = cfg_min;
                        let max = cfg_max;
                        let step = cfg_step;
                        let disabled = cfg_disabled;
                        let precision = cfg_precision;
                        let data_click = data_for_render.clone();
                        let on_change = cfg_on_change.clone();
                        let icon_svg = label_svg.to_string();

                        let mut btn = stateful_with_key::<ButtonState>(stepper_key)
                            .deps([cfg_state.signal_id()])
                            .on_state(move |ctx| {
                                let s = ctx.state();
                                let cell_bg = if disabled {
                                    bg
                                } else {
                                    match s {
                                        ButtonState::Pressed => bg_pressed,
                                        ButtonState::Hovered => bg_hover,
                                        _ => bg,
                                    }
                                };
                                // Disable when stepping in this direction
                                // wouldn't change the clamped value.
                                let at_bound = if disabled {
                                    true
                                } else {
                                    let v = state.get();
                                    clamp(v + delta * step, min, max) == v
                                };
                                let fg = if disabled || at_bound {
                                    text_tertiary
                                } else {
                                    text_primary
                                };

                                let mut cell = div()
                                    .w(button_w)
                                    .h(height)
                                    .flex_row()
                                    .items_center()
                                    .justify_center()
                                    .bg(cell_bg)
                                    .cursor_pointer()
                                    .child(
                                        svg(&icon_svg)
                                            .size(icon_size, icon_size)
                                            .color(fg)
                                            .internal(),
                                    );
                                if is_left {
                                    cell = cell.border_right(1.0, border_color);
                                } else {
                                    cell = cell.border_left(1.0, border_color);
                                }
                                cell.class(if is_left {
                                    "cn-number-input-step--dec"
                                } else {
                                    "cn-number-input-step--inc"
                                })
                            });

                        btn = btn.on_click(move |_| {
                            if disabled {
                                return;
                            }
                            let next = clamp(state_click.get() + delta * step, min, max);
                            if (next - state_click.get()).abs() < f64::EPSILON {
                                return; // at bound — no-op
                            }
                            // `state.set` is enough: it bumps the signal,
                            // the outer Stateful's `.deps` re-renders,
                            // and the render-time sync above pushes the
                            // formatted value into `data_for_render`.
                            // Pre-fix the click handler also wrote
                            // directly to `data`, which raced with the
                            // outer re-render and produced the first-
                            // click-doesn't-update bug.
                            state_click.set(next);
                            let _ = data_click; // capture for Send-bound; sync handled by outer Stateful
                            let _ = precision;
                            if let Some(ref cb) = on_change {
                                cb(next);
                            }
                        });

                        btn
                    };

                div()
                    .w(total_w)
                    .h(height)
                    .flex_row()
                    .items_center()
                    .rounded(radius)
                    .border(1.0, border_color)
                    .overflow_clip()
                    .class("cn-number-input-group")
                    .child(make_stepper(&icon_minus, -1.0, true, &stepper_dec_key))
                    .child(field)
                    .child(make_stepper(&icon_plus, 1.0, false, &stepper_inc_key))
            });

        Self {
            inner: div().h_fit().w_fit().child(group),
        }
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
        let graph = Arc::new(std::sync::Mutex::new(ReactiveGraph::new()));
        let signal = graph.lock().unwrap().create_signal(initial);
        State::new(signal, graph, Arc::new(AtomicBool::new(false)))
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
