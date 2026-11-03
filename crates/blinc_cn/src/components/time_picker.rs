//! A controlled time editor with independently editable hour, minute and second fields.

use std::{cell::OnceCell, fmt, sync::Arc};

use blinc_core::reactive::{Signal, computed, effect};
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::{Div, ElementBuilder, ElementTypeId, div};
use blinc_layout::element::RenderProps;
use blinc_layout::text::Text;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_layout::widgets::text_input::{
    InputType, SharedTextInputData, refresh_text_input, text_input, text_input_data,
};

use super::input::InputSize;

/// A wall-clock time without a date or time zone, always stored in 24-hour form.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeOfDay {
    hour: u8,
    minute: u8,
    second: u8,
}

impl TimeOfDay {
    /// Returns `None` for an hour above 23 or a minute/second above 59.
    pub const fn new(hour: u8, minute: u8, second: u8) -> Option<Self> {
        if hour < 24 && minute < 60 && second < 60 {
            Some(Self {
                hour,
                minute,
                second,
            })
        } else {
            None
        }
    }

    pub const fn hour(self) -> u8 {
        self.hour
    }
    pub const fn minute(self) -> u8 {
        self.minute
    }
    pub const fn second(self) -> u8 {
        self.second
    }

    /// Format for a time field. Hiding seconds does not discard them.
    pub fn format(self, format: TimeFormat, seconds: bool) -> String {
        let hour = match format {
            TimeFormat::TwentyFourHour => self.hour,
            TimeFormat::TwelveHour => self.hour12(),
        };
        let mut text = format!("{hour:02}:{:02}", self.minute);
        if seconds {
            text.push_str(&format!(":{:02}", self.second));
        }
        if format == TimeFormat::TwelveHour {
            text.push_str(if self.hour < 12 { " AM" } else { " PM" });
        }
        text
    }

    fn hour12(self) -> u8 {
        (self.hour + 11) % 12 + 1
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
}

/// The displayed hour convention; the bound value remains in 24-hour form.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeFormat {
    #[default]
    TwentyFourHour,
    TwelveHour,
}

#[derive(Clone)]
struct Config {
    value: Signal<TimeOfDay>,
    format: TimeFormat,
    seconds: bool,
    minute_step: u8,
    second_step: u8,
    size: InputSize,
    disabled: Reactive<bool>,
    on_change: Option<Arc<dyn Fn(TimeOfDay) + Send + Sync>>,
}

impl Config {
    fn edit(&self, next: TimeOfDay) {
        if self.disabled.now().unwrap_or(false) || self.value.get() == next {
            return;
        }
        self.value.set(next);
        if let Some(callback) = &self.on_change {
            callback(next);
        }
    }
}

/// Builds a CSS-styled segmented time editor. Arrow keys and wheel input step a field.
pub struct TimePickerBuilder {
    config: Config,
    classes: Vec<String>,
    id: Option<String>,
    built: OnceCell<Div>,
}

impl TimePickerBuilder {
    pub fn new(value: Signal<TimeOfDay>) -> Self {
        Self {
            config: Config {
                value,
                format: TimeFormat::default(),
                seconds: false,
                minute_step: 1,
                second_step: 1,
                size: InputSize::Medium,
                disabled: false.into_reactive(),
                on_change: None,
            },
            classes: Vec::new(),
            id: None,
            built: OnceCell::new(),
        }
    }

    pub fn format(mut self, format: TimeFormat) -> Self {
        self.config.format = format;
        self
    }
    pub fn seconds(mut self, show: bool) -> Self {
        self.config.seconds = show;
        self
    }
    /// Step used by arrows and wheel input; typing can still select any minute.
    pub fn minute_step(mut self, step: u8) -> Self {
        self.config.minute_step = step.clamp(1, 59);
        self
    }
    pub fn second_step(mut self, step: u8) -> Self {
        self.config.second_step = step.clamp(1, 59);
        self
    }
    pub fn size(mut self, size: InputSize) -> Self {
        self.config.size = size;
        self
    }
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = disabled.into_reactive();
        self
    }
    /// Called once per changed user edit, after the bound signal is written.
    pub fn on_change(mut self, callback: impl Fn(TimeOfDay) + Send + Sync + 'static) -> Self {
        self.config.on_change = Some(Arc::new(callback));
        self
    }
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
    pub fn id(mut self, id: &str) -> Self {
        self.id = Some(id.to_owned());
        self
    }

    fn element(&self) -> &Div {
        blinc_layout::build_once::build_once(&self.built, || {
            let c = &self.config;
            let size = match c.size {
                InputSize::Small => "sm",
                InputSize::Medium => "md",
                InputSize::Large => "lg",
            };
            let mut row = div()
                .class("cn-time-picker")
                .attr("data-size", size)
                .attr(
                    "data-format",
                    if c.format == TimeFormat::TwelveHour {
                        "12"
                    } else {
                        "24"
                    },
                )
                .attr_when("disabled", "", c.disabled.clone())
                .flex_row()
                .items_center()
                .w_fit()
                .child(segment(c, Segment::Hour))
                .child(Text::new(":").class("cn-time-picker-separator"))
                .child(segment(c, Segment::Minute));
            if c.seconds {
                row = row
                    .child(Text::new(":").class("cn-time-picker-separator"))
                    .child(segment(c, Segment::Second));
            }
            if c.format == TimeFormat::TwelveHour {
                let value = c.value;
                let label = computed(move |g| {
                    if g.get(value).unwrap_or_default().hour < 12 {
                        "AM".into()
                    } else {
                        "PM".into()
                    }
                });
                let edit = c.clone();
                row = row.child(
                    div()
                        .class("cn-time-picker-period")
                        .attr("tabindex", "0")
                        .attr_when("disabled", "", c.disabled.clone())
                        .child(Text::bound(label.into_reactive()))
                        .on_click(move |_| {
                            let mut next = edit.value.get();
                            next.hour = (next.hour + 12) % 24;
                            edit.edit(next);
                        }),
                );
            }
            for class in &self.classes {
                row = row.class(class);
            }
            if let Some(id) = &self.id {
                row = row.id(id);
            }
            row
        })
    }
}

#[derive(Clone, Copy)]
enum Segment {
    Hour,
    Minute,
    Second,
}

impl Segment {
    fn name(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::Minute => "minute",
            Self::Second => "second",
        }
    }
    fn get(self, c: &Config, time: TimeOfDay) -> u8 {
        match self {
            Self::Hour if c.format == TimeFormat::TwelveHour => time.hour12(),
            Self::Hour => time.hour,
            Self::Minute => time.minute,
            Self::Second => time.second,
        }
    }
    fn bounds(self, c: &Config) -> (u8, u8) {
        match self {
            Self::Hour if c.format == TimeFormat::TwelveHour => (1, 12),
            Self::Hour => (0, 23),
            _ => (0, 59),
        }
    }
    fn set(self, c: &Config, part: u8) {
        let mut next = c.value.get();
        match self {
            Self::Hour if c.format == TimeFormat::TwelveHour => {
                next.hour = part % 12 + (next.hour / 12) * 12
            }
            Self::Hour => next.hour = part,
            Self::Minute => next.minute = part,
            Self::Second => next.second = part,
        }
        c.edit(next);
    }
    fn step(self, c: &Config, delta: i32) {
        let step = match self {
            Self::Hour => 1,
            Self::Minute => c.minute_step,
            Self::Second => c.second_step,
        } as i32;
        let (min, max) = self.bounds(c);
        let next = (self.get(c, c.value.get()) as i32 - min as i32 + delta * step)
            .rem_euclid((max - min + 1) as i32)
            + min as i32;
        self.set(c, next as u8);
    }
}

fn show(data: &SharedTextInputData, value: u8) {
    let mut d = data.lock().unwrap();
    d.value = format!("{value:02}");
    d.cursor = d.value.len();
    d.selection_start = None;
    drop(d);
    refresh_text_input(data);
}

fn segment(c: &Config, part: Segment) -> Div {
    let (width, height, font) = match c.size {
        InputSize::Small => (36.0, 28.0, 12.0),
        InputSize::Medium => (40.0, 36.0, 14.0),
        InputSize::Large => (48.0, 44.0, 16.0),
    };
    let data = text_input_data();
    show(&data, part.get(c, c.value.get()));
    let sync = data.clone();
    let config = c.clone();
    effect(move |g| {
        let value = part.get(&config, g.get(config.value).unwrap_or_default());
        let d = sync.lock().unwrap();
        let matches = d.value.parse::<u8>().ok() == Some(value);
        let formatted = d.value == format!("{value:02}");
        if (!d.visual.is_focused() && !formatted) || !matches {
            drop(d);
            show(&sync, value);
        }
    });
    let edit = c.clone();
    let step = c.clone();
    let stepped = data.clone();
    let submit = data.clone();
    let canonical = c.clone();
    let input = text_input(&data)
        .w(width)
        .h(height)
        .padding_x(4.0)
        .text_size(font)
        .input_type(InputType::Number)
        .max_length(2)
        .text_align(blinc_core::TextAlign::Center)
        .class("cn-time-picker-segment")
        .class("tabindex=text")
        .class(match c.size {
            InputSize::Small => "cn-time-picker-segment--sm",
            InputSize::Medium => "cn-time-picker-segment--md",
            InputSize::Large => "cn-time-picker-segment--lg",
        })
        .class(&format!("cn-time-picker-{}", part.name()))
        .disabled(c.disabled.clone())
        .on_change(move |text| {
            if let Ok(value) = text.parse::<u8>() {
                let (min, max) = part.bounds(&edit);
                if (min..=max).contains(&value) {
                    part.set(&edit, value);
                }
            }
        })
        .on_step(move |delta| {
            if step.disabled.now().unwrap_or(false) {
                return;
            }
            part.step(&step, delta);
            show(&stepped, part.get(&step, step.value.get()));
        })
        .on_submit(move |_| show(&submit, part.get(&canonical, canonical.value.get())));
    let wheel = c.clone();
    let field = data.clone();
    let remainder = std::sync::Mutex::new(0.0f32);
    div()
        .class("cn-time-picker-part")
        .w_fit()
        .h_fit()
        .child(input)
        .on_blur(move |_| show(&data, part.get(&wheel, wheel.value.get())))
        .on_scroll({
            let c = c.clone();
            move |event| {
                if c.disabled.now().unwrap_or(false) {
                    return;
                }
                if let Some(delta) = super::number_wheel::wheel_steps(
                    &mut remainder.lock().unwrap(),
                    event.scroll_delta_x,
                    event.scroll_delta_y,
                ) {
                    if delta != 0 {
                        part.step(&c, delta);
                        show(&field, part.get(&c, c.value.get()));
                    }
                }
            }
        })
}

impl ElementBuilder for TimePickerBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.element().build(tree)
    }
    fn render_props(&self) -> RenderProps {
        self.element().render_props()
    }
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.element().children_builders()
    }
    fn element_type_id(&self) -> ElementTypeId {
        self.element().element_type_id()
    }
    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("time_picker")
    }
    fn layout_style(&self) -> Option<&taffy::Style> {
        self.element().layout_style()
    }
    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(self.element())
    }
    fn element_id(&self) -> Option<&str> {
        self.element().element_id()
    }
    fn element_classes(&self) -> &[Arc<str>] {
        self.element().element_classes()
    }
}

/// Edit a bound time. Defaults to 24-hour format with hidden seconds.
pub fn time_picker(value: Signal<TimeOfDay>) -> TimePickerBuilder {
    TimePickerBuilder::new(value)
}
