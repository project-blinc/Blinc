//! A formatted date trigger backed by the calendar's selection model.

use super::{
    calendar::{CalendarDay, CalendarEvent, calendar},
    icon::{IconSize, icon},
    input::InputSize,
    picker_popover::picker_popover,
};
use blinc_core::reactive::{Signal, computed, effect, signal};
use blinc_layout::widgets::overlay_stack::OverlayHandle;
use blinc_layout::{
    InstanceKey,
    binding::{IntoReactive, Reactive},
    div::{Div, ElementBuilder, ElementTypeId, div},
    element::RenderProps,
    text::Text,
    tree::{LayoutNodeId, LayoutTree},
};
use std::{cell::OnceCell, sync::Arc};

type Format = Arc<dyn Fn(CalendarDay) -> String + Send + Sync>;
type OnChange = Arc<dyn Fn(Option<CalendarDay>) + Send + Sync>;

#[derive(Clone)]
struct Config {
    value: Signal<Option<CalendarDay>>,
    placeholder: Reactive<String>,
    clear_text: Reactive<String>,
    disabled: Reactive<bool>,
    events: Reactive<Vec<CalendarEvent>>,
    format: Format,
    on_change: Option<OnChange>,
    size: InputSize,
    clearable: bool,
    week_start: u32,
    min_year: i32,
    max_year: i32,
    today: Option<CalendarDay>,
}

impl Config {
    fn edit(&self, next: Option<CalendarDay>) {
        if self.disabled.now().unwrap_or(false) || self.value.try_get().flatten() == next {
            return;
        }
        self.value.set(next);
        if let Some(callback) = &self.on_change {
            callback(next);
        }
    }
}

/// A calendar popover controlled by a `Signal<Option<CalendarDay>>`.
pub struct DatePickerBuilder {
    config: Config,
    classes: Vec<String>,
    id: Option<String>,
    key: InstanceKey,
    built: OnceCell<Div>,
}

impl DatePickerBuilder {
    #[track_caller]
    pub fn new(value: Signal<Option<CalendarDay>>) -> Self {
        Self {
            config: Config {
                value,
                placeholder: "Pick a date".into_reactive(),
                clear_text: "Clear".into_reactive(),
                disabled: false.into_reactive(),
                events: Vec::new().into_reactive(),
                format: Arc::new(|date| {
                    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
                }),
                on_change: None,
                size: InputSize::Medium,
                clearable: true,
                week_start: 0,
                min_year: 1900,
                max_year: 2100,
                today: None,
            },
            classes: Vec::new(),
            id: None,
            key: InstanceKey::new("date_picker"),
            built: OnceCell::new(),
        }
    }
    pub fn placeholder(mut self, text: impl IntoReactive<String>) -> Self {
        self.config.placeholder = text.into_reactive();
        self
    }
    pub fn clear_text(mut self, text: impl IntoReactive<String>) -> Self {
        self.config.clear_text = text.into_reactive();
        self
    }
    pub fn disabled(mut self, disabled: impl IntoReactive<bool>) -> Self {
        self.config.disabled = disabled.into_reactive();
        self
    }
    pub fn size(mut self, size: InputSize) -> Self {
        self.config.size = size;
        self
    }
    /// Whether the popup includes a clear button (default: true).
    pub fn clearable(mut self, clearable: bool) -> Self {
        self.config.clearable = clearable;
        self
    }
    /// Format the selected date; the default is `YYYY-MM-DD`.
    pub fn format(
        mut self,
        format: impl Fn(CalendarDay) -> String + Send + Sync + 'static,
    ) -> Self {
        self.config.format = Arc::new(format);
        self
    }
    /// Called after a changed user selection or clear. External writes are silent.
    pub fn on_change(
        mut self,
        callback: impl Fn(Option<CalendarDay>) + Send + Sync + 'static,
    ) -> Self {
        self.config.on_change = Some(Arc::new(callback));
        self
    }
    pub fn events(mut self, events: impl IntoReactive<Vec<CalendarEvent>>) -> Self {
        self.config.events = events.into_reactive();
        self
    }
    /// First weekday, 0 Sunday through 6 Saturday.
    pub fn week_start(mut self, weekday: u32) -> Self {
        self.config.week_start = weekday % 7;
        self
    }
    pub fn min_year(mut self, year: i32) -> Self {
        self.config.min_year = year;
        self
    }
    pub fn max_year(mut self, year: i32) -> Self {
        self.config.max_year = year;
        self
    }
    /// Override the calendar's local-today marker and initial empty month.
    pub fn today(mut self, today: CalendarDay) -> Self {
        self.config.today = Some(today);
        self
    }
    /// Applies to the trigger. The popup lives in the overlay layer.
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
            let c = self.config.clone();
            let value = c.value;
            let placeholder = c.placeholder.clone();
            let format = c.format.clone();
            let shown = computed(move |g| match g.get(value).flatten() {
                Some(day) => format(day),
                None => placeholder.read(g).unwrap_or_default(),
            });
            let size = match c.size {
                InputSize::Small => "sm",
                InputSize::Medium => "md",
                InputSize::Large => "lg",
            };
            let trigger = div()
                .class("cn-date-picker-trigger")
                .attr("data-size", size)
                .attr_when(
                    "data-empty",
                    "",
                    computed(move |g| g.get(value).flatten().is_none()),
                )
                .flex_row()
                .items_center()
                .w_fit()
                .child(
                    icon(blinc_icons::icons::CALENDAR)
                        .size(IconSize::Small)
                        .build()
                        .class("cn-date-picker-icon"),
                )
                .child(
                    Text::bound(shown.into_reactive())
                        .no_wrap()
                        .class("cn-date-picker-value"),
                );
            let id = self
                .id
                .clone()
                .unwrap_or_else(|| format!("cn-date-picker-{}", self.key.get()));
            let disabled = c.disabled.clone();
            let mut root = picker_popover(trigger, id, disabled, (352.0, 400.0), move |active| {
                content(&c, active)
            });
            for class in &self.classes {
                root = root.class(class);
            }
            root
        })
    }
}

fn content(c: &Config, active: Signal<Option<u64>>) -> Div {
    let selected = signal(c.value.try_get().flatten());
    let value = c.value;
    effect(move |g| {
        let next = g.get(value).flatten();
        if g.get_untracked(selected).flatten() != next {
            selected.set(next);
        }
    });
    let edit = c.clone();
    let mut calendar = calendar(selected)
        .week_start(c.week_start)
        .min_year(c.min_year)
        .max_year(c.max_year.max(c.min_year))
        .events(c.events.clone())
        .on_change(move |date| {
            edit.edit(Some(date));
            close(active);
        });
    if let Some(today) = c.today {
        calendar = calendar.today(today);
    }
    let mut panel = div()
        .class("cn-date-picker-content")
        .attr("role", "dialog")
        .flex_col()
        .h_fit()
        .child(calendar);
    if c.clearable {
        let edit = c.clone();
        let disabled = c.disabled.clone();
        panel = panel.child(
            div()
                .class("cn-date-picker-footer")
                .w_full()
                .flex_row()
                .justify_end()
                .child(
                    div()
                        .w_fit()
                        .class("cn-date-picker-clear")
                        .attr("tabindex", "0")
                        .attr("role", "button")
                        .attr_when(
                            "disabled",
                            "",
                            computed(move |g| {
                                g.get(value).flatten().is_none()
                                    || disabled.read(g).unwrap_or(false)
                            }),
                        )
                        .child(Text::bound(c.clear_text.clone()))
                        .on_click(move |_| {
                            if edit.disabled.now().unwrap_or(false)
                                || edit.value.try_get().flatten().is_none()
                            {
                                return;
                            }
                            edit.edit(None);
                            close(active);
                        }),
                ),
        );
    }
    panel
}

fn close(active: Signal<Option<u64>>) {
    if let Some(raw) = active.get() {
        OverlayHandle::from_raw(raw).close();
    }
}

impl ElementBuilder for DatePickerBuilder {
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
        Some("date_picker")
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

/// Pick a calendar date. `None` shows the placeholder; selection closes the popover.
#[track_caller]
pub fn date_picker(value: Signal<Option<CalendarDay>>) -> DatePickerBuilder {
    DatePickerBuilder::new(value)
}
