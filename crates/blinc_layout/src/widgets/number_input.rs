//! Number input — typed numeric field built on `text_input`.
//!
//! Wraps [`text_input`](mod@super::text_input) with `InputType::Number`
//! and a parse + clamp + format pipeline so callers work in `f64`
//! rather than `String`. The visible field is a regular text input —
//! same focus ring, same theme tokens, same CSS hooks (`.cn-input` etc.
//! work as-is on top). The number-specific surface (`State<f64>`,
//! `min` / `max` / `step` / `precision`) lives in this thin wrapper.
//!
//! # Example
//!
//! ```ignore
//! let count = ctx.use_state_keyed("count", || 1.0);
//! number_input(&count)
//!     .min(0.0)
//!     .max(100.0)
//!     .step(1.0)
//!     .precision(0)
//!     .placeholder("0")
//!     .on_change(|v| println!("value: {v}"))
//! ```
//!
//! # Stepping
//!
//! v1 doesn't intercept `↑` / `↓` / `PageUp` / `PageDown` keys —
//! `text_input`'s `on_key_down` already owns the keyboard for cursor
//! / selection / clipboard, and forking it here would require an
//! escape hatch that doesn't exist yet. Use [`NumberInputBuilder::step`]
//! together with `cn::number_input`'s `+` / `−` buttons (or any
//! caller-supplied buttons that call [`step_up`] / [`step_down`]) to
//! step the value. Keyboard stepping is a follow-up, tracked as
//! git-bug e750fa9.

use std::sync::Arc;

use blinc_core::State;

use crate::div::{ElementBuilder, ElementTypeId};
use crate::element::RenderProps;
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::text_input::{
    InputType, SharedTextInputData, TextInput, text_input, text_input_data,
};

/// Reactive numeric value bound to a [`State<f64>`]. Use `f64` for the
/// widest range — `cn::number_input` can layer a precision/format
/// hint on top if integer semantics are wanted. Generic-over-numeric
/// is intentionally NOT done here: it'd multiply config types four
/// ways for marginal benefit, and the `.precision(0)` path handles
/// integer-display fine.
#[derive(Clone)]
pub struct NumberInputConfig {
    state: State<f64>,
    data: SharedTextInputData,
    /// Lower bound (inclusive). `None` = no lower bound.
    pub min: Option<f64>,
    /// Upper bound (inclusive). `None` = no upper bound.
    pub max: Option<f64>,
    /// Step amount used by [`step_up`] / [`step_down`] (and by cn's
    /// `+` / `−` buttons). Default `1.0`.
    pub step: f64,
    /// Decimal places used when formatting the value into the text
    /// field. `0` renders as an integer. Default `2`.
    pub precision: usize,
    /// Caller's change callback, fires whenever the value commits a
    /// new parsed-and-clamped value to `state`.
    on_change: Option<Arc<dyn Fn(f64) + Send + Sync>>,
}

/// Increment `state` by `step`, clamping against `[min, max]`. Used by
/// caller-supplied `+` buttons (notably `cn::number_input`).
pub fn step_up(config: &NumberInputConfig) {
    let next = clamp(config.state.get() + config.step, config.min, config.max);
    set_value(config, next);
}

/// Decrement `state` by `step`, clamping against `[min, max]`.
pub fn step_down(config: &NumberInputConfig) {
    let next = clamp(config.state.get() - config.step, config.min, config.max);
    set_value(config, next);
}

fn set_value(config: &NumberInputConfig, next: f64) {
    config.state.set(next);
    if let Ok(mut d) = config.data.lock() {
        d.value = format_value(next, config.precision);
        d.cursor = d.value.chars().count();
        d.selection_start = None;
    }
    if let Some(ref cb) = config.on_change {
        cb(next);
    }
}

fn clamp(value: f64, min: Option<f64>, max: Option<f64>) -> f64 {
    let v = if let Some(lo) = min {
        value.max(lo)
    } else {
        value
    };
    if let Some(hi) = max { v.min(hi) } else { v }
}

fn format_value(value: f64, precision: usize) -> String {
    if precision == 0 {
        // Integer rendering — round half-to-even, then strip any
        // negative-zero artifact `-0` produces.
        let rounded = value.round() as i64;
        return rounded.to_string();
    }
    format!("{value:.precision$}")
}

/// The fully-built number input. Just a `TextInput` under the hood —
/// the wrapper's only job is wiring the parse / clamp / format
/// pipeline into `on_change`.
pub struct NumberInput {
    inner: TextInput,
}

/// Lazy builder for [`NumberInput`]. Materialises on first `ElementBuilder`
/// access — matches the `OnceCell` pattern used by `checkbox` /
/// `radio_group` so `track_caller`'s `InstanceKey` stays stable across
/// the builder-chain → render boundary.
pub struct NumberInputBuilder {
    config: NumberInputConfig,
    // Direct passthroughs to TextInput so callers can chain `.w()` /
    // `.h()` / `.placeholder()` etc. without us re-implementing every
    // text_input setter.
    width: Option<f32>,
    height: Option<f32>,
    placeholder: Option<String>,
    disabled: bool,
    rounded: Option<f32>,
    css_element_id: Option<String>,
    css_classes: Vec<String>,
    built: std::cell::OnceCell<NumberInput>,
}

impl NumberInputBuilder {
    #[track_caller]
    pub fn new(state: &State<f64>) -> Self {
        let data = text_input_data();
        // Seed the input with the current state value so first paint
        // shows the right thing — without this, the text input would
        // render empty until the user typed.
        if let Ok(mut d) = data.lock() {
            d.value = format_value(state.get(), 2);
            d.cursor = d.value.chars().count();
            d.input_type = InputType::Number;
        }
        Self {
            config: NumberInputConfig {
                state: state.clone(),
                data,
                min: None,
                max: None,
                step: 1.0,
                precision: 2,
                on_change: None,
            },
            width: None,
            height: None,
            placeholder: None,
            disabled: false,
            rounded: None,
            css_element_id: None,
            css_classes: Vec::new(),
            built: std::cell::OnceCell::new(),
        }
    }

    /// Borrow the resolved config — used by `cn::number_input`'s `+` /
    /// `−` buttons to call [`step_up`] / [`step_down`].
    pub fn config(&self) -> &NumberInputConfig {
        &self.config
    }

    /// Clone the shared text-input data backing this widget. External
    /// stepper buttons (notably `cn::number_input`'s `+` / `−`) need
    /// this to push a re-formatted value into the visible field after
    /// they mutate `state` — `state.set` alone doesn't reach the text
    /// input, since `text_input` reads from its own `SharedTextInputData`.
    pub fn data(&self) -> SharedTextInputData {
        self.config.data.clone()
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

    /// Decimal places used when rendering the value. Re-formats the
    /// current text immediately so the visible field matches.
    pub fn precision(mut self, precision: usize) -> Self {
        self.config.precision = precision;
        if let Ok(mut d) = self.config.data.lock() {
            d.value = format_value(self.config.state.get(), precision);
            d.cursor = d.value.chars().count();
        }
        self
    }

    pub fn w(mut self, px: f32) -> Self {
        self.width = Some(px);
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        self.height = Some(px);
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn rounded(mut self, radius: f32) -> Self {
        self.rounded = Some(radius);
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.css_element_id = Some(id.into());
        self
    }

    pub fn class(mut self, name: impl Into<String>) -> Self {
        self.css_classes.push(name.into());
        self
    }

    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(f64) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(handler));
        self
    }

    fn get_or_build(&self) -> &NumberInput {
        self.built.get_or_init(|| NumberInput::from_builder(self))
    }
}

impl NumberInput {
    fn from_builder(b: &NumberInputBuilder) -> Self {
        let state = b.config.state.clone();
        let min = b.config.min;
        let max = b.config.max;
        let precision = b.config.precision;
        let data_for_change = b.config.data.clone();
        let on_change = b.config.on_change.clone();

        // Parse-and-clamp wired into the text_input's on_change. Fires
        // on every keystroke — that's the only callback `text_input`
        // surfaces. Empty / mid-edit invalid strings (`""`, `"-"`,
        // `"3."`) leave `state` untouched so the user can keep
        // typing without the wrapper "correcting" them mid-edit. On
        // blur (or commit), `state` already holds the last successfully
        // parsed value, which is the source of truth.
        let mut input = text_input(&b.config.data).input_type(InputType::Number);

        if let Some(w) = b.width {
            input = input.w(w);
        }
        if let Some(h) = b.height {
            input = input.h(h);
        }
        if let Some(ref p) = b.placeholder {
            input = input.placeholder(p.clone());
        }
        if b.disabled {
            input = input.disabled(true);
        }
        if let Some(r) = b.rounded {
            input = input.rounded(r);
        }
        if let Some(ref id) = b.css_element_id {
            input = input.id(id);
        }
        for class in &b.css_classes {
            input = input.class(class);
        }

        input = input.on_change(move |text| {
            // Parse the visible text. Empty / partial inputs (lone `-`
            // or `.`) — return early so the user can keep typing.
            let trimmed = text.trim();
            if trimmed.is_empty() || trimmed == "-" || trimmed == "." || trimmed == "-." {
                return;
            }
            if let Ok(parsed) = trimmed.parse::<f64>() {
                let clamped = clamp(parsed, min, max);
                state.set(clamped);
                if let Some(ref cb) = on_change {
                    cb(clamped);
                }
                // Reformat IF the clamp actually changed the value —
                // otherwise the cursor jumps every keystroke. The
                // text_input's data is already in sync with what the
                // user typed.
                if (clamped - parsed).abs() > f64::EPSILON {
                    if let Ok(mut d) = data_for_change.lock() {
                        d.value = format_value(clamped, precision);
                        d.cursor = d.value.chars().count();
                    }
                }
            }
        });

        Self { inner: input }
    }
}

impl ElementBuilder for NumberInput {
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
        Some("number_input")
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

impl ElementBuilder for NumberInputBuilder {
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
        Some("number_input")
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

/// Create a number input bound to a reactive [`State<f64>`].
///
/// ```ignore
/// let count = ctx.use_state_keyed("count", || 0.0);
/// number_input(&count).min(0.0).max(99.0).step(1.0).precision(0)
/// ```
#[track_caller]
pub fn number_input(state: &State<f64>) -> NumberInputBuilder {
    NumberInputBuilder::new(state)
}
