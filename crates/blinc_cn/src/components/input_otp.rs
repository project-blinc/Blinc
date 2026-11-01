//! Themed OTP input wrapper around
//! [`blinc_layout::widgets::input_otp`](mod@blinc_layout::widgets::input_otp).

use std::sync::Arc;

use blinc_core::reactive::Signal;
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::ElementBuilder;
use blinc_layout::prelude::*;
use blinc_layout::widgets::input_otp::{show_value_in_slots, wire_otp_slot};
use blinc_layout::widgets::text_input::{InputType, text_input, text_input_data};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

use crate::components::input::InputSize;

#[allow(clippy::type_complexity)]
#[derive(Clone)]
struct Config {
    value: Signal<String>,
    length: usize,
    numeric_only: bool,
    masked: bool,
    disabled: Reactive<bool>,
    error: Reactive<bool>,
    size: InputSize,
    /// Insert a `•` separator every N slots. `None` disables grouping.
    group_every: Option<usize>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    on_complete: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

pub struct InputOtpBuilder {
    config: Config,
    built: std::cell::OnceCell<InputOtp>,
}

pub struct InputOtp {
    inner: Div,
}

impl InputOtpBuilder {
    /// An OTP input with `length` slots that reads and writes `value`.
    pub fn new(value: Signal<String>, length: usize) -> Self {
        Self {
            config: Config {
                value,
                length,
                numeric_only: false,
                masked: false,
                disabled: Reactive::Const(false),
                error: Reactive::Const(false),
                size: InputSize::Medium,
                group_every: Some(3),
                on_change: None,
                on_complete: None,
            },
            built: std::cell::OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &InputOtp {
        ::blinc_layout::build_once::build_once(&self.built, || {
            InputOtp::from_config(self.config.clone())
        })
    }

    pub fn numeric_only(mut self, numeric_only: bool) -> Self {
        self.config.numeric_only = numeric_only;
        self
    }

    pub fn masked(mut self, masked: bool) -> Self {
        self.config.masked = masked;
        self
    }

    /// Whether every slot is disabled: a bool, Signal or Computed.
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = disabled.into_reactive();
        self
    }

    /// Show the error border on every slot (an invalid code) while `error`
    /// holds: a bool, Signal or Computed.
    pub fn error(mut self, error: impl IntoReactive<bool>) -> Self {
        self.config.error = error.into_reactive();
        self
    }

    pub fn size(mut self, size: InputSize) -> Self {
        self.config.size = size;
        self
    }

    /// Insert a `•` separator every `n` slots. Pass `0` to disable
    /// grouping (uniform gap throughout).
    pub fn group_every(mut self, n: usize) -> Self {
        self.config.group_every = if n == 0 { None } else { Some(n) };
        self
    }

    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(handler));
        self
    }

    pub fn on_complete<F>(mut self, handler: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.config.on_complete = Some(Arc::new(handler));
        self
    }
}

impl InputOtp {
    fn from_config(config: Config) -> Self {
        let theme = ThemeState::get();
        let typography = theme.typography();
        let height = config.size.height(theme);
        let font_size = config.size.font_size(&typography);
        let radius = theme.radius(RadiusToken::Md);

        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_tertiary = theme.color(ColorToken::TextTertiary);

        let length = config.length;
        let numeric_only = config.numeric_only;

        // Preserve prefilled bound values on first paint.
        let initial = config.value.try_get().unwrap_or_default();
        let mut initial_chars = initial.chars();
        let slots: Vec<_> = (0..length)
            .map(|_| {
                let data = text_input_data();
                if let Ok(mut d) = data.lock() {
                    if let Some(c) = initial_chars.next() {
                        d.value = c.to_string();
                        d.cursor = 1;
                    }
                    d.input_type = if numeric_only {
                        InputType::Integer
                    } else {
                        InputType::Text
                    };
                    d.masked = config.masked;
                    d.constraints.max_length = Some(1);
                }
                data
            })
            .collect();
        let slots = Arc::new(slots);

        show_value_in_slots(&slots, config.value);

        let mut row = div()
            .flex_row()
            .items_center()
            .class("cn-input-otp")
            .class_when("cn-input-otp--disabled", config.disabled.clone());
        for i in 0..length {
            let field = text_input(&slots[i])
                .input_type(if numeric_only {
                    InputType::Integer
                } else {
                    InputType::Text
                })
                .text_align(blinc_core::TextAlign::Center)
                .max_length(1)
                .masked(config.masked)
                .disabled(config.disabled.clone())
                .h(height)
                .w(height)
                .text_size(font_size)
                .rounded(radius)
                .text_color(text_primary)
                .class("cn-input-otp-slot")
                .class_when("cn-input-otp-slot--error", config.error.clone());
            row = row.child(wire_otp_slot(
                field,
                i,
                &slots,
                numeric_only,
                config.value,
                config.on_change.clone(),
                config.on_complete.clone(),
            ));

            if i + 1 == length {
                continue;
            }
            let group_boundary = config.group_every.is_some_and(|n| (i + 1) % n == 0);
            row = row.child(if group_boundary {
                div()
                    .w(height * 0.5)
                    .h(height)
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .child(text("\u{2022}").size(font_size).color(text_tertiary))
                    .class("cn-input-otp-separator")
            } else {
                div().w(theme.spacing_value(blinc_theme::SpacingToken::Space2))
            });
        }

        Self {
            inner: div().h_fit().w_fit().child(row),
        }
    }
}

impl ElementBuilder for InputOtp {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
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
        Some("input_otp")
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

impl ElementBuilder for InputOtpBuilder {
    fn build(&self, tree: &mut blinc_layout::tree::LayoutTree) -> blinc_layout::tree::LayoutNodeId {
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
        Some("input_otp")
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

/// Create a cn-styled one-time-passcode input with `length` slots,
/// bound to a [`Signal<String>`] holding the joined value.
pub fn input_otp(value: Signal<String>, length: usize) -> InputOtpBuilder {
    InputOtpBuilder::new(value, length)
}
