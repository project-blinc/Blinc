//! Calendar component: a month of days to pick a date from
//!
//! A header with previous and next buttons around month and year selects, the
//! days of the week, and six weeks of days in a grid of seven. Days of the
//! months either side are muted, today is marked and the selected day is
//! filled. Choosing a day of a month either side turns to that month, and a
//! turning month slides in from its side while the old one slides out the
//! other.
//!
//! Days carry `data-outside`, `data-today`, `data-selected`, `data-events`,
//! `disabled` outside the allowed years, and `data-focus-visible` while the
//! keys are on them. A month's grid carries `data-enter` while it slides in
//! and `data-leaving` while it slides out, `"next"` or `"prev"`; the slide is
//! the `cn-calendar-*` keyframes. The navigation buttons carry `data-step`.
//! The look is the `.cn-calendar*` rules.
//!
//! A horizontal drag or two-finger swipe across the days turns the month,
//! leftward to the next; vertical scrolling passes on to the page.
//! Once a day has been clicked, the arrows move by a day or a week, Page Up
//! and Page Down by a month, Home and End to the ends of the week, and Enter
//! or Space choose the day the keys are on.
//!
//! Events mark days: up to three per day show along its bottom edge, below
//! the number, which stays where it is, as a dot or the event's icon, coloured by `.cn-calendar-event` rules keyed on the
//! event's `data-kind`.
//!
//! # Example
//!
//! ```ignore
//! use blinc_cn::prelude::*;
//!
//! let picked = ctx.use_signal(None::<CalendarDay>);
//!
//! cn::calendar(picked)
//!     .week_start(1)
//!     .events(vec![
//!         CalendarEvent::new(CalendarDay::new(2026, 11, 3), "Standup").kind("meeting"),
//!         CalendarEvent::new(CalendarDay::new(2026, 11, 4), "Rain")
//!             .kind("weather")
//!             .icon(to_svg(icons::CLOUD_RAIN, 12.0)),
//!     ])
//!     .on_change(|day| println!("{day:?}"))
//! ```

use std::cell::OnceCell;
use std::sync::{Arc, Mutex};

use blinc_animation::{Easing, Keyframe, KeyframeAnimation, KeyframeId, SchedulerHandle};
use blinc_core::context_state::BlincContextState;
use blinc_core::reactive::{ReactiveGraph, Signal, computed, signal};
use blinc_layout::InstanceKey;
use blinc_layout::binding::{IntoReactive, Reactive};
use blinc_layout::div::ElementTypeId;
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::region::Row;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_theme::ThemeState;

pub use super::calendar_date::CalendarDay;
use super::calendar_date::Month;
use super::number_gauge::GaugeFocus;
use super::select::{SelectBuilder, SelectOption, SelectSize};

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
const PREV_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>"#;
const NEXT_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;

/// How many years the year select lists, centred on the shown year.
const YEARS_LISTED: i32 = 201;
/// The height of a row of the year list, which builds only the rows in view.
const YEAR_ROW_HEIGHT: f32 = 32.0;
/// How far a press moves before its release stops being a click, in px.
const DRAG_SLOP: f32 = 8.0;
/// How far a two-finger swipe scrolls before it turns the month, in px.
const SWIPE_SCROLL: f32 = 60.0;
/// How long a swipe's scroll must pause before another can turn the month.
const SWIPE_QUIET_MS: u32 = 150;
/// How far a drag across the days goes before it turns the month, in px.
const SWIPE_DISTANCE: f32 = 40.0;
/// The most events a day shows.
const MARKS_PER_DAY: usize = 3;

/// Something on a day: a meeting, the weather, a deadline.
///
/// `kind` becomes the mark's `data-kind`, which rules colour it by; `icon`
/// (SVG markup in `currentColor`) draws in place of the dot.
#[derive(Clone, Debug, PartialEq)]
pub struct CalendarEvent {
    pub day: CalendarDay,
    pub title: String,
    pub kind: Option<String>,
    pub icon: Option<String>,
}

impl CalendarEvent {
    pub fn new(day: CalendarDay, title: impl Into<String>) -> Self {
        Self {
            day,
            title: title.into(),
            kind: None,
            icon: None,
        }
    }

    /// The mark's `data-kind`.
    pub fn kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }

    /// An icon drawn in place of the dot.
    pub fn icon(mut self, svg: impl Into<String>) -> Self {
        self.icon = Some(svg.into());
        self
    }
}

/// The events of `events` on `day`, in order.
pub fn events_on(events: &[CalendarEvent], day: CalendarDay) -> Vec<&CalendarEvent> {
    events.iter().filter(|e| e.day == day).collect()
}

/// Calendar component
pub struct Calendar {
    inner: Div,
}

impl ElementBuilder for Calendar {
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

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }
}

type ChangeCallback = Arc<dyn Fn(CalendarDay) + Send + Sync>;

/// Builder for a calendar bound to a selected day.
pub struct CalendarBuilder {
    key: InstanceKey,
    value: Signal<Option<CalendarDay>>,
    today: Option<CalendarDay>,
    week_start: u32,
    years: Years,
    events: Reactive<Vec<CalendarEvent>>,
    on_change: Option<ChangeCallback>,
    classes: Vec<std::sync::Arc<str>>,
    user_id: Option<String>,
    built: OnceCell<Calendar>,
}

impl CalendarBuilder {
    #[track_caller]
    pub fn new(value: Signal<Option<CalendarDay>>) -> Self {
        Self {
            key: InstanceKey::new("calendar"),
            value,
            today: None,
            week_start: 0,
            years: Years::default(),
            events: Reactive::Const(Vec::new()),
            on_change: None,
            classes: Vec::new(),
            user_id: None,
            built: OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &Calendar {
        ::blinc_layout::build_once::build_once(&self.built, || self.build_component())
    }

    /// The day marked as today; the local date by default.
    pub fn today(mut self, day: CalendarDay) -> Self {
        self.today = Some(day);
        self
    }

    /// The first day of the week: 0 for Sunday (the default), 1 for Monday.
    pub fn week_start(mut self, weekday: u32) -> Self {
        self.week_start = weekday % 7;
        self
    }

    /// The earliest year that can be shown or chosen.
    pub fn min_year(mut self, year: i32) -> Self {
        self.years.min = Some(year);
        self
    }

    /// The latest year that can be shown or chosen.
    pub fn max_year(mut self, year: i32) -> Self {
        self.years.max = Some(year);
        self
    }

    /// What is on which day: a list, or a Signal or Computed of one, whose
    /// changes the marks follow in place.
    pub fn events(mut self, events: impl IntoReactive<Vec<CalendarEvent>>) -> Self {
        self.events = events.into_reactive();
        self
    }

    /// Called with the day chosen.
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(CalendarDay) + Send + Sync + 'static,
    {
        self.on_change = Some(Arc::new(callback));
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: impl AsRef<str>) -> Self {
        self.classes.push(blinc_core::intern::intern(name.as_ref()));
        self
    }

    /// Set the element ID for CSS selector matching
    pub fn id(mut self, id: &str) -> Self {
        self.user_id = Some(id.to_string());
        self
    }

    fn build_component(&self) -> Calendar {
        let years = self.years;
        if let (Some(min), Some(max)) = (years.min, years.max) {
            assert!(
                min <= max,
                "calendar min_year {min} is after max_year {max}"
            );
        }
        let key = self.key.get().to_string();
        let ctx = BlincContextState::get();
        let today = self.today.unwrap_or_else(CalendarDay::today);
        let at = self.value.try_get().flatten().unwrap_or(today);
        let first = CalendarDay::new(years.clamp(at.year), at.month, at.day);
        let keyed = |name: &str| format!("{key}_{name}");

        let nav = Nav {
            shown: ctx.use_signal_keyed(&keyed("shown"), || Month::of(first)),
            side: ctx.use_signal_keyed(&keyed("side"), || None),
            month_pick: ctx.use_signal_keyed(&keyed("month"), || first.month.to_string()),
            year_pick: ctx.use_signal_keyed(&keyed("year"), || first.year.to_string()),
            year_from: ctx.use_signal_keyed(&keyed("years"), || years.window(first.year)),
            years,
        };
        let root_id = self
            .user_id
            .clone()
            .unwrap_or_else(|| format!("cn-calendar-{key}"));
        let typography = ThemeState::get().typography();
        let parts = Parts {
            value: self.value,
            cursor: ctx.use_signal_keyed(&keyed("cursor"), || first),
            nav,
            focus: GaugeFocus::new(&key, &root_id),
            on_change: self.on_change.clone(),
            events: self.events.clone(),
            today,
            week_start: self.week_start,
            day_size: typography.text_sm,
            swiped: Arc::default(),
        };

        let mut weekdays = div().class("cn-calendar-weekdays").grid();
        for i in 0..7 {
            let name = WEEKDAYS[((i + self.week_start) % 7) as usize];
            weekdays = weekdays.child(
                div()
                    .class("cn-calendar-weekday")
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .child(text(name).size(typography.text_xs).no_cursor()),
            );
        }

        let (shown, side) = (nav.shown, nav.side);
        let pages = computed(move |g: &ReactiveGraph| {
            vec![(
                g.get(shown).unwrap_or(Month::of(first)),
                g.get(side).flatten(),
            )]
        });
        let page_parts = parts.clone();
        let months = div()
            .class("cn-calendar-months")
            .relative()
            .overflow_clip()
            .for_each(
                &pages,
                |page: &(Month, Option<Side>)| page.0,
                move |(month, enter)| page(month, enter, &page_parts),
            );
        let months = swipeable(months, nav, Arc::clone(&parts.swiped));

        let keys = parts.clone();
        let body = div()
            .class("cn-calendar-body")
            .flex_col()
            .child(weekdays)
            .child(months)
            .on_key_down(move |ctx| {
                if keys.focus.focused.get() && keys.key(ctx.key_code) {
                    keys.focus.key();
                }
            });

        let mut root = div()
            .class("cn-calendar")
            .id(&root_id)
            .flex_col()
            .child(header(&key, &parts))
            .child(body);
        for c in &self.classes {
            root = root.class(c);
        }
        Calendar { inner: root }
    }
}

/// The years that may be shown; either end may be open.
#[derive(Clone, Copy, Default)]
struct Years {
    min: Option<i32>,
    max: Option<i32>,
}

impl Years {
    fn allows(self, year: i32) -> bool {
        self.min.is_none_or(|m| year >= m) && self.max.is_none_or(|m| year <= m)
    }

    fn clamp(self, year: i32) -> i32 {
        let year = self.min.map_or(year, |m| year.max(m));
        self.max.map_or(year, |m| year.min(m))
    }

    /// The first year the year select lists with `year` shown.
    fn window(self, year: i32) -> i32 {
        let mut from = year - YEARS_LISTED / 2;
        if let Some(max) = self.max {
            from = from.min(max - (YEARS_LISTED - 1));
        }
        if let Some(min) = self.min {
            from = from.max(min);
        }
        from
    }

    fn listed(self, from: i32) -> i32 {
        self.max
            .map_or(YEARS_LISTED, |max| YEARS_LISTED.min(max - from + 1))
    }
}

/// The side a turning month comes in from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Side {
    Next,
    Prev,
}

/// The month shown and the selects that name it.
#[derive(Clone, Copy)]
struct Nav {
    shown: Signal<Month>,
    side: Signal<Option<Side>>,
    month_pick: Signal<String>,
    year_pick: Signal<String>,
    /// The first year the year select lists.
    year_from: Signal<i32>,
    years: Years,
}

impl Nav {
    fn now(self) -> Option<Month> {
        self.shown.try_get()
    }

    /// Show `to`, sliding in from its side.
    fn turn(self, to: Month) {
        let Some(now) = self.now() else { return };
        if to == now || !self.years.allows(to.year) {
            return;
        }
        self.side
            .set(Some(if to > now { Side::Next } else { Side::Prev }));
        self.shown.set(to);
        self.month_pick.set(to.month.to_string());
        self.year_pick.set(to.year.to_string());
        let from = self.year_from.try_get().unwrap_or(to.year);
        if to.year < from || to.year >= from + self.years.listed(from) {
            self.year_from.set(self.years.window(to.year));
        }
    }
}

/// What the days of every month share.
#[derive(Clone)]
struct Parts {
    value: Signal<Option<CalendarDay>>,
    /// The day the keys are on.
    cursor: Signal<CalendarDay>,
    nav: Nav,
    focus: GaugeFocus,
    on_change: Option<ChangeCallback>,
    events: Reactive<Vec<CalendarEvent>>,
    today: CalendarDay,
    week_start: u32,
    day_size: f32,
    /// Set by a drag across the days, so the click its release makes chooses
    /// nothing.
    swiped: Arc<std::sync::atomic::AtomicBool>,
}

impl Parts {
    fn choose(&self, day: CalendarDay) {
        if !self.nav.years.allows(day.year) {
            return;
        }
        self.value.set(Some(day));
        self.nav.turn(Month::of(day));
        self.cursor.set(day);
        if let Some(cb) = &self.on_change {
            cb(day);
        }
    }

    /// Keep the day the keys are on in the month picked, without choosing.
    fn jump(&self, to: Month) {
        if !self.nav.years.allows(to.year) {
            return;
        }
        let day = self.cursor.try_get().map_or(1, |d| d.day);
        self.cursor.set(CalendarDay::new(to.year, to.month, day));
        self.nav.turn(to);
    }

    /// A key while the calendar has focus; true when it did something.
    fn key(&self, code: u32) -> bool {
        const ENTER: u32 = 13;
        const SPACE: u32 = 32;
        const PAGE_UP: u32 = 33;
        const PAGE_DOWN: u32 = 34;
        const END: u32 = 35;
        const HOME: u32 = 36;
        const LEFT: u32 = 37;
        const UP: u32 = 38;
        const RIGHT: u32 = 39;
        const DOWN: u32 = 40;

        let Some(at) = self.cursor.try_get() else {
            return false;
        };
        let into_week = ((at.weekday() + 7 - self.week_start) % 7) as i64;
        let next = match code {
            LEFT => at.add_days(-1),
            RIGHT => at.add_days(1),
            UP => at.add_days(-7),
            DOWN => at.add_days(7),
            PAGE_UP => at.add_months(-1),
            PAGE_DOWN => at.add_months(1),
            HOME => at.add_days(-into_week),
            END => at.add_days(6 - into_week),
            ENTER | SPACE => {
                self.choose(at);
                return true;
            }
            _ => return false,
        };
        if self.nav.years.allows(next.year) {
            self.nav.turn(Month::of(next));
            self.cursor.set(next);
        }
        true
    }
}

/// The previous and next buttons around the month and year selects.
fn header(key: &str, parts: &Parts) -> Div {
    let nav = parts.nav;
    let pick_month = parts.clone();
    let months = SelectBuilder::with_key(format!("{key}_month_picker"), nav.month_pick)
        .size(SelectSize::Small)
        .w(106.0)
        .options((1..=12u32).map(|m| SelectOption::new(m.to_string(), MONTHS[m as usize - 1])))
        .on_change(move |v| {
            if let (Ok(month), Some(now)) = (v.parse(), nav.now()) {
                pick_month.jump(Month {
                    year: now.year,
                    month,
                });
            }
        });

    let year_key = format!("{key}_year_picker");
    let from = nav.year_from;
    let windows = computed(move |g: &ReactiveGraph| vec![g.get(from).unwrap_or(0)]);
    let pick_year = parts.clone();
    let years = div().class("cn-calendar-year-picker").for_each(
        &windows,
        |from: &i32| *from,
        move |from: i32| {
            let listed = (from..from + nav.years.listed(from))
                .map(|y| SelectOption::new(y.to_string(), y.to_string()));
            let pick = pick_year.clone();
            SelectBuilder::with_key(year_key.clone(), nav.year_pick)
                .size(SelectSize::Small)
                .w(72.0)
                .virtual_rows(YEAR_ROW_HEIGHT)
                .options(listed)
                .on_change(move |v| {
                    if let (Ok(year), Some(now)) = (v.parse(), nav.now()) {
                        pick.jump(Month {
                            year,
                            month: now.month,
                        });
                    }
                })
        },
    );

    div()
        .class("cn-calendar-header")
        .flex_row()
        .items_center()
        .justify_between()
        .child(step(nav, Side::Prev))
        .child(
            div()
                .class("cn-calendar-caption")
                .flex_row()
                .items_center()
                .child(div().class("cn-calendar-month-picker").child(months))
                .child(years),
        )
        .child(step(nav, Side::Next))
}

/// The button that turns to the previous or next month.
fn step(nav: Nav, side: Side) -> Div {
    let (n, name, icon) = match side {
        Side::Prev => (-1, "prev", PREV_SVG),
        Side::Next => (1, "next", NEXT_SVG),
    };
    let shown = nav.shown;
    let years = nav.years;
    let disabled = computed(move |g: &ReactiveGraph| {
        g.get(shown).is_none_or(|m| !years.allows(m.shift(n).year))
    });
    div()
        .class("cn-calendar-nav")
        .attr("data-step", name)
        .attr_when("disabled", "", &disabled)
        .flex_row()
        .items_center()
        .justify_center()
        .child(
            div()
                .pointer_events_none()
                .child(svg(icon).size(16.0, 16.0)),
        )
        .on_click(move |_| {
            if let Some(now) = nav.now() {
                nav.turn(now.shift(n));
            }
        })
}

/// A horizontal drag across the days turns the month: leftward to the next,
/// rightward to the previous. Once a press has moved, its release chooses no
/// day.
fn swipeable(months: Div, nav: Nav, swiped: Arc<std::sync::atomic::AtomicBool>) -> Div {
    use std::sync::atomic::Ordering::Relaxed;
    let dragged = Arc::new(Mutex::new(0.0f32));
    let follow = Arc::clone(&dragged);
    let moved = Arc::clone(&swiped);
    let swipe = Arc::new(Mutex::new(Swipe::default()));
    months
        .on_mouse_down(move |_| swiped.store(false, Relaxed))
        .on_drag(move |ctx| {
            *follow.lock().unwrap() = ctx.drag_delta_x;
            if ctx.drag_delta_x.abs() >= DRAG_SLOP {
                moved.store(true, Relaxed);
            }
        })
        .on_drag_end(move |ctx| {
            let dx = std::mem::take(&mut *dragged.lock().unwrap());
            let dx = if ctx.drag_delta_x != 0.0 {
                ctx.drag_delta_x
            } else {
                dx
            };
            if dx.abs() >= SWIPE_DISTANCE {
                turn_toward(nav, dx);
            }
        })
        .on_scroll_x(move |ctx| swipe.lock().unwrap().scroll(nav, ctx.scroll_delta_x))
}

/// Turn the month away from a movement `dx`: leftward to the next.
fn turn_toward(nav: Nav, dx: f32) {
    if let Some(now) = nav.now() {
        nav.turn(now.shift(if dx < 0.0 { 1 } else { -1 }));
    }
}

/// A two-finger swipe: its horizontal scroll adds up until it turns the
/// month once; the rest of it, momentum included, is spent until the scroll
/// has been quiet a moment. The gap is timed on the wall clock, as input
/// events arrive, since nothing animates between two swipes.
#[derive(Default)]
struct Swipe {
    travel: f32,
    spent: bool,
    last: Option<web_time::Instant>,
}

impl Swipe {
    fn scroll(&mut self, nav: Nav, dx: f32) {
        let now = web_time::Instant::now();
        let quiet = self
            .last
            .is_none_or(|t| now.duration_since(t).as_millis() >= SWIPE_QUIET_MS as u128);
        self.last = Some(now);
        if quiet {
            self.travel = 0.0;
            self.spent = false;
        }
        if self.spent {
            return;
        }
        self.travel += dx;
        if self.travel.abs() >= SWIPE_SCROLL {
            self.spent = true;
            turn_toward(nav, self.travel);
        }
    }
}

/// One month's grid, sliding in from `enter` when it has a side; it slides
/// out the other way once another month replaces it.
fn page(month: Month, enter: Option<Side>, parts: &Parts) -> Row {
    let leaving: Signal<Option<Side>> = signal(None);
    let out_to =
        |side: Side| computed(move |g: &ReactiveGraph| g.get(leaving).flatten() == Some(side));
    let gone = computed(move |g: &ReactiveGraph| g.get(leaving).flatten().is_some());
    let mut grid = div()
        .class("cn-calendar-grid")
        .grid()
        .attr_when("data-enter", "next", enter == Some(Side::Next))
        .attr_when("data-enter", "prev", enter == Some(Side::Prev))
        .attr_when("data-leaving", "next", out_to(Side::Next))
        .attr_when("data-leaving", "prev", out_to(Side::Prev))
        .absolute_when(&gone);
    for day in month.grid(parts.week_start) {
        grid = grid.child(day_cell(day, month, parts));
    }

    // Kept until its slide has played: the theme's normal duration, on the
    // animation clock.
    let side = parts.nav.side;
    let timer = Arc::new(Mutex::new(Timer::default()));
    let start = Arc::clone(&timer);
    Row::new(grid).on_leave(
        move || {
            leaving.set(Some(side.try_get().flatten().unwrap_or(Side::Next)));
            let ms = ThemeState::get().animations().duration_normal as u32;
            start.lock().unwrap().start(ms);
        },
        move || timer.lock().unwrap().done(),
    )
}

/// A keyframe on the animation scheduler, so a wait is timed on the clock
/// the animations are.
#[derive(Default)]
struct Timer(Option<(SchedulerHandle, KeyframeId)>);

impl Timer {
    /// Start, or start again, a wait of `ms`.
    fn start(&mut self, ms: u32) {
        if let Some((scheduler, id)) = self.0.take() {
            scheduler.remove_keyframe(id);
        }
        let Some(scheduler) = blinc_layout::render_state::get_global_scheduler() else {
            return;
        };
        let frames = vec![
            Keyframe {
                time: 0.0,
                value: 0.0,
                easing: Easing::Linear,
            },
            Keyframe {
                time: 1.0,
                value: 1.0,
                easing: Easing::Linear,
            },
        ];
        if let Some(id) = scheduler.register_keyframe(KeyframeAnimation::new(ms.max(1), frames)) {
            scheduler.start_keyframe(id);
            self.0 = Some((scheduler, id));
        }
    }

    fn done(&self) -> bool {
        self.0
            .as_ref()
            .is_none_or(|(scheduler, id)| !scheduler.is_keyframe_playing(*id))
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Some((scheduler, id)) = self.0.take() {
            scheduler.remove_keyframe(id);
        }
    }
}

/// One day: its number, and the marks of what is on it.
fn day_cell(day: CalendarDay, month: Month, parts: &Parts) -> Div {
    let value = parts.value;
    let selected = computed(move |g: &ReactiveGraph| g.get(value).flatten() == Some(day));
    let (cursor, focused, keyed) = (parts.cursor, parts.focus.focused, parts.focus.keyed);
    let ringed = computed(move |g: &ReactiveGraph| {
        g.get(focused).unwrap_or(false)
            && g.get(keyed).unwrap_or(false)
            && g.get(cursor) == Some(day)
    });
    let events = parts.events.clone();
    let marks = computed(move |g: &ReactiveGraph| {
        events
            .read(g)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| e.day == day)
            .take(MARKS_PER_DAY)
            .enumerate()
            .collect::<Vec<_>>()
    });
    let events = parts.events.clone();
    let has_marks = computed(move |g: &ReactiveGraph| {
        events
            .read(g)
            .is_some_and(|all| all.iter().any(|e| e.day == day))
    });

    let choose = parts.clone();
    div()
        .class("cn-calendar-day")
        .attr_when("data-outside", "", Month::of(day) != month)
        .attr_when("data-today", "", day == parts.today)
        .attr_when("data-selected", "", &selected)
        .attr_when("data-focus-visible", "", &ringed)
        .attr_when("data-events", "", &has_marks)
        .attr_when("disabled", "", !parts.nav.years.allows(day.year))
        .relative()
        .flex_row()
        .items_center()
        .justify_center()
        .child(
            text(day.day.to_string())
                .size(parts.day_size)
                .no_cursor()
                .pointer_events_none(),
        )
        .child(
            div()
                .class("cn-calendar-events")
                .absolute()
                .left(0.0)
                .right(0.0)
                .bottom(2.0)
                .h(8.0)
                .gap(0.5)
                .flex_row()
                .items_center()
                .justify_center()
                .pointer_events_none()
                .for_each(
                    &marks,
                    |(i, e): &(usize, CalendarEvent)| {
                        (*i, e.title.clone(), e.kind.clone(), e.icon.clone())
                    },
                    |(_, e): (usize, CalendarEvent)| mark(&e),
                ),
        )
        .on_click(move |_| {
            if choose
                .swiped
                .swap(false, std::sync::atomic::Ordering::Relaxed)
            {
                return;
            }
            choose.focus.press();
            choose.choose(day);
        })
}

/// An event's dot, or its icon.
fn mark(event: &CalendarEvent) -> Div {
    let mut m = div().class("cn-calendar-event").flex_shrink_0();
    if let Some(kind) = &event.kind {
        m = m.attr("data-kind", kind.as_str());
    }
    match &event.icon {
        Some(icon) => m
            .class("cn-calendar-event--icon")
            .child(svg(icon).size(8.0, 8.0)),
        None => m,
    }
}

impl ElementBuilder for CalendarBuilder {
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

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }
}

/// A calendar that reads and writes the selected day.
///
/// # Example
///
/// ```ignore
/// let picked = ctx.use_signal(None::<CalendarDay>);
/// cn::calendar(picked).on_change(|day| println!("{day:?}"))
/// ```
#[track_caller]
pub fn calendar(value: Signal<Option<CalendarDay>>) -> CalendarBuilder {
    CalendarBuilder::new(value)
}
