//! Themed OTP input wrapper around
//! [`blinc_layout::widgets::input_otp`](mod@blinc_layout::widgets::input_otp).

use std::sync::Arc;

use blinc_core::State;
use blinc_layout::InstanceKey;
use blinc_layout::div::ElementBuilder;
use blinc_layout::prelude::*;
use blinc_layout::stateful::{NoState, stateful_with_key};
use blinc_layout::widgets::input_otp::{sync_slots_from_value, wire_otp_slot};
use blinc_layout::widgets::text_input::{InputType, text_input, text_input_data};
use blinc_theme::{ColorToken, RadiusToken, ThemeState};

use crate::components::input::InputSize;

#[allow(clippy::type_complexity)]
struct Config {
    value: State<String>,
    length: usize,
    numeric_only: bool,
    masked: bool,
    disabled: bool,
    error: bool,
    size: InputSize,
    /// Insert a `•` separator every N slots. `None` disables grouping.
    group_every: Option<usize>,
    on_change: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    on_complete: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

pub struct InputOtpBuilder {
    key: InstanceKey,
    config: Config,
    built: std::cell::OnceCell<InputOtp>,
}

pub struct InputOtp {
    inner: Div,
}

impl InputOtpBuilder {
    #[track_caller]
    pub fn new(value: &State<String>, length: usize) -> Self {
        Self {
            key: InstanceKey::new("cn_input_otp"),
            config: Config {
                value: value.clone(),
                length,
                numeric_only: false,
                masked: false,
                disabled: false,
                error: false,
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
            InputOtp::from_config(&self.key, self.clone_config())
        })
    }

    fn clone_config(&self) -> Config {
        Config {
            value: self.config.value.clone(),
            length: self.config.length,
            numeric_only: self.config.numeric_only,
            masked: self.config.masked,
            disabled: self.config.disabled,
            error: self.config.error,
            size: self.config.size,
            group_every: self.config.group_every,
            on_change: self.config.on_change.clone(),
            on_complete: self.config.on_complete.clone(),
        }
    }

    /// Name this widget, so its group state is its own.
    ///
    /// The default identity is the call site, which is enough for two
    /// OTPs written in different places. A caller that builds several
    /// from ONE site -- a loop, or a DSL wrapper -- has to say which is
    /// which, or their groups share one stateful entry.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = InstanceKey::explicit(key);
        self
    }

    pub fn numeric_only(mut self, numeric_only: bool) -> Self {
        self.config.numeric_only = numeric_only;
        self
    }

    pub fn masked(mut self, masked: bool) -> Self {
        self.config.masked = masked;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
    }

    /// Show the error border/ring on every slot (invalid code, etc.).
    pub fn error(mut self, error: bool) -> Self {
        self.config.error = error;
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
    fn from_config(instance_key: &InstanceKey, config: Config) -> Self {
        let theme = ThemeState::get();
        let typography = theme.typography();
        let height = config.size.height(theme);
        let font_size = config.size.font_size(&typography);
        let radius = theme.radius(RadiusToken::Md);

        let border_idle = theme.color(ColorToken::Border);
        let border_hover = theme.color(ColorToken::BorderHover);
        let border_focus = theme.color(ColorToken::BorderFocus);
        let border_error = theme.color(ColorToken::BorderError);
        let bg = theme.color(ColorToken::InputBg);
        let bg_hover = theme.color(ColorToken::InputBgHover);
        let bg_focus = theme.color(ColorToken::InputBgFocus);
        let text_primary = theme.color(ColorToken::TextPrimary);
        let text_tertiary = theme.color(ColorToken::TextTertiary);

        let length = config.length;
        let numeric_only = config.numeric_only;

        // Preserve prefilled bound values on first paint.
        let initial = config.value.get();
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
                    d.disabled = config.disabled;
                    d.constraints.max_length = Some(1);
                }
                data
            })
            .collect();
        let slots = Arc::new(slots);

        let group_key = instance_key.derive("group");
        let value_for_group = config.value.clone();
        let value_dep = config.value.clone();
        let disabled = config.disabled;
        let error = config.error;
        let masked = config.masked;
        let group_every = config.group_every;
        let on_change = config.on_change.clone();
        let on_complete = config.on_complete.clone();

        let row = stateful_with_key::<NoState>(&group_key)
            .deps([value_dep.signal_id()])
            .on_state(move |_ctx| {
                sync_slots_from_value(&slots, &value_for_group.get());

                let mut row = div().flex_row().items_center().class("cn-input-otp");
                if disabled {
                    row = row.class("cn-input-otp--disabled");
                }

                for i in 0..length {
                    let mut field = text_input(&slots[i])
                        .input_type(if numeric_only {
                            InputType::Integer
                        } else {
                            InputType::Text
                        })
                        .text_align(blinc_core::TextAlign::Center)
                        .max_length(1)
                        .masked(masked)
                        .disabled(disabled)
                        .h(height)
                        .w(height)
                        .text_size(font_size)
                        .rounded(radius)
                        .idle_border_color(border_idle)
                        .hover_border_color(border_hover)
                        .focused_border_color(if error { border_error } else { border_focus })
                        .error_border_color(border_error)
                        .idle_bg_color(bg)
                        .hover_bg_color(bg_hover)
                        .focused_bg_color(bg_focus)
                        .text_color(text_primary)
                        .class("cn-input-otp-slot");
                    if error {
                        field = field.class("cn-input-otp-slot--error");
                    }

                    field = wire_otp_slot(
                        field,
                        i,
                        &slots,
                        numeric_only,
                        &value_for_group,
                        on_change.clone(),
                        on_complete.clone(),
                    );

                    row = row.child(field);

                    let is_last = i + 1 == length;
                    if !is_last {
                        let is_group_boundary =
                            group_every.map(|n| (i + 1) % n == 0).unwrap_or(false);
                        if is_group_boundary {
                            row = row.child(
                                div()
                                    .w(height * 0.5)
                                    .h(height)
                                    .flex_row()
                                    .items_center()
                                    .justify_center()
                                    .child(text("\u{2022}").size(font_size).color(text_tertiary))
                                    .class("cn-input-otp-separator"),
                            );
                        } else {
                            row = row.child(
                                div().w(theme.spacing_value(blinc_theme::SpacingToken::Space2)),
                            );
                        }
                    }
                }

                row
            });

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
/// bound to a reactive [`State<String>`] holding the joined value.
#[track_caller]
pub fn input_otp(value: &State<String>, length: usize) -> InputOtpBuilder {
    InputOtpBuilder::new(value, length)
}
