#![cfg(feature = "headless")]
//! The calendar: a month turns by sliding, the keys walk the days, events
//! mark days in place, and the year list opens at the shown year.

use blinc_cn::{CalendarDay, CalendarEvent};
use blinc_core::reactive::{Signal, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 320;
const H: u32 = 420;
const DOWN: u32 = 40;
const RIGHT: u32 = 39;
const PAGE_DOWN: u32 = 34;
const ENTER: u32 = 13;

#[path = "cn_calendar/selected.rs"]
mod selected;

fn session() -> Option<Headless> {
    session_sized(W, H)
}

fn session_sized(w: u32, h: u32) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(w, h).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

fn day(d: u32) -> CalendarDay {
    CalendarDay::new(2026, 11, d)
}

fn mount(
    ui: &mut Headless,
    picked: Signal<Option<CalendarDay>>,
    events: Signal<Vec<CalendarEvent>>,
) {
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .p(1.0)
            .child(blinc_cn::calendar(picked).today(day(1)).events(events))
    });
    ui.leave().settle();
}

/// Every node with `class`, in document order.
fn all(ui: &Headless, class: &str) -> Vec<LayoutNodeId> {
    let tree = ui.tree();
    let registry = tree.element_registry();
    let mut found = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if registry.has_class(node, class) {
            found.push(node);
        }
        let mut children = tree.layout_tree.children(node);
        children.reverse();
        stack.extend(children);
    }
    found
}

fn click(ui: &mut Headless, node: LayoutNodeId) {
    let b = ui.tree().get_absolute_bounds(node).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0)
        .mouse_down()
        .mouse_up();
}

fn has(ui: &Headless, node: LayoutNodeId, attr: &str) -> bool {
    ui.tree().element_registry().attribute(node, attr).is_some()
}

/// The mean column of the pixels painted `colour`, if any are.
fn centre_of(ui: &mut Headless, colour: [u8; 4]) -> Option<f32> {
    let frame = ui.snapshot().expect("snapshot");
    let (mut sum, mut n) = (0u64, 0u64);
    for y in 0..frame.height {
        for x in 0..frame.width {
            let i = ((y * frame.width + x) * 4) as usize;
            let px = &frame.data[i..i + 4];
            if (0..3).all(|c| px[c].abs_diff(colour[c]) < 6) {
                sum += x as u64;
                n += 1;
            }
        }
    }
    (n > 50).then(|| sum as f32 / n as f32)
}

/// Turning to the next month slides the shown one out to the left over the
/// theme's normal duration, then lets it go.
#[test]
fn a_month_turns_by_sliding() {
    let Some(mut ui) = session() else { return };
    mount(&mut ui, signal(Some(day(12))), signal(Vec::new()));

    // The selected day's fill marks where November is drawn.
    let selected = all(&ui, "cn-calendar-day")
        .into_iter()
        .find(|d| has(&ui, *d, "data-selected"))
        .expect("the selected day");
    let b = ui.tree().get_absolute_bounds(selected).unwrap();
    let frame = ui.snapshot().unwrap();
    let fill = frame
        .get_pixel(b.x as u32 + 3, (b.y + b.height / 2.0) as u32)
        .expect("in frame");
    let start = centre_of(&mut ui, fill).expect("the fill is drawn");

    let next = all(&ui, "cn-calendar-nav")[1];
    click(&mut ui, next);
    ui.advance(80);
    assert_eq!(
        all(&ui, "cn-calendar-grid").len(),
        2,
        "both months mid-turn"
    );
    let partway = centre_of(&mut ui, fill).unwrap_or(f32::MIN);
    assert!(
        partway < start - 10.0,
        "November slides out to the left: {partway} from {start}"
    );

    ui.settle();
    assert_eq!(
        all(&ui, "cn-calendar-grid").len(),
        1,
        "the old month is let go"
    );
    assert!(
        centre_of(&mut ui, fill).is_none(),
        "December shows no November selection"
    );
}

/// After a click, the arrows move the ring, Page Down turns the month with
/// it, and Enter chooses the day it is on.
#[test]
fn the_keys_walk_the_days() {
    let Some(mut ui) = session() else { return };
    let picked = signal(None);
    mount(&mut ui, picked, signal(Vec::new()));
    let ringed = |ui: &Headless| -> Vec<u32> {
        all(ui, "cn-calendar-day")
            .into_iter()
            .enumerate()
            .filter(|(_, d)| has(ui, *d, "data-focus-visible"))
            .map(|(i, _)| i as u32)
            .collect()
    };

    // November 2026 starts on a Sunday: the 11th is cell 10.
    let eleventh = all(&ui, "cn-calendar-day")[10];
    click(&mut ui, eleventh);
    ui.settle();
    assert_eq!(picked.get(), Some(day(11)));
    assert!(ringed(&ui).is_empty(), "a click shows no ring");

    ui.key(DOWN).key(RIGHT).settle();
    assert_eq!(ringed(&ui), [18], "on the 19th, and only there");

    ui.key(PAGE_DOWN).settle();
    assert_eq!(all(&ui, "cn-calendar-grid").len(), 1);
    // December 2026 starts on a Tuesday: the 19th is cell 20.
    assert_eq!(ringed(&ui), [20], "on December 19th");
    ui.key(ENTER).settle();
    assert_eq!(picked.get(), Some(CalendarDay::new(2026, 12, 19)));
}

/// Events mark their days and follow the list in place: a dot, or the
/// event's icon.
#[test]
fn events_mark_their_days_in_place() {
    let Some(mut ui) = session() else { return };
    let icon = r#"<svg viewBox="0 0 24 24"><rect width="24" height="24"/></svg>"#;
    let events = signal(vec![
        CalendarEvent::new(day(3), "Standup").kind("meeting"),
        CalendarEvent::new(day(3), "Review").kind("meeting"),
    ]);
    mount(&mut ui, signal(None), events);
    let days = all(&ui, "cn-calendar-day");
    assert!(has(&ui, days[2], "data-events"));
    assert!(!has(&ui, days[5], "data-events"));
    assert_eq!(all(&ui, "cn-calendar-event").len(), 2);

    events.set(vec![
        CalendarEvent::new(day(3), "Standup").kind("meeting"),
        CalendarEvent::new(day(6), "Sun").kind("weather").icon(icon),
    ]);
    ui.settle();
    assert_eq!(
        all(&ui, "cn-calendar-grid").len(),
        1,
        "the month was not rebuilt"
    );
    assert!(has(&ui, days[5], "data-events"), "the 6th gained its mark");
    assert_eq!(all(&ui, "cn-calendar-event").len(), 2);
    assert_eq!(all(&ui, "cn-calendar-event--icon").len(), 1);
}

/// The year list builds only the years in view, opens scrolled to the shown
/// one, and builds the next ones as it scrolls.
#[test]
fn the_year_list_is_virtual_and_opens_at_the_shown_year() {
    let Some(mut ui) = session() else { return };
    mount(&mut ui, signal(None), signal(Vec::new()));
    let year_trigger = all(&ui, "cn-select-trigger")[1];
    click(&mut ui, year_trigger);
    ui.settle();

    // 201 years from 1926, 32px rows: 2026 is the 101st.
    let list = all(&ui, "cn-select-list")[0];
    let (_, y) = ui.tree().get_scroll_offset(list);
    assert!(y < -3000.0, "the list did not open at 2026: {y}");
    let rows = all(&ui, "cn-select-item");
    assert!(rows.len() < 30, "built {} of 201 rows", rows.len());
    let selected = all(&ui, "cn-select-item--selected");
    assert_eq!(selected.len(), 1, "2026's row is built and marked");

    let first_id = |ui: &Headless| {
        let row = all(ui, "cn-select-item")[0];
        ui.tree().element_registry().get_id(row).unwrap_or_default()
    };
    let before = first_id(&ui);
    let b = ui.tree().get_absolute_bounds(list).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0)
        .scroll(0.0, -400.0)
        .settle();
    assert_ne!(first_id(&ui), before, "scrolling built later years");
    assert!(all(&ui, "cn-select-item").len() < 30);
}

/// Dragging across the days turns the month, leftward to the next, and does
/// not choose a day.
#[test]
fn a_drag_across_the_days_turns_the_month() {
    let Some(mut ui) = session() else { return };
    let picked = signal(None);
    mount(&mut ui, picked, signal(Vec::new()));
    let shown = |ui: &Headless| {
        // The first in-month day of the grid's first row.
        all(ui, "cn-calendar-day")
            .into_iter()
            .position(|d| !has(ui, d, "data-outside"))
            .unwrap()
    };
    // November 2026 starts on Sunday, December on Tuesday.
    assert_eq!(shown(&ui), 0);

    let start = all(&ui, "cn-calendar-day")[17];
    let b = ui.tree().get_absolute_bounds(start).unwrap();
    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    ui.move_to(x, y).mouse_down();
    for step in 1..=8 {
        ui.move_to(x - step as f32 * 10.0, y);
    }
    ui.mouse_up().settle();
    assert_eq!(all(&ui, "cn-calendar-grid").len(), 1);
    assert_eq!(shown(&ui), 2, "December is shown");
    assert_eq!(picked.get(), None, "a drag chooses no day");
}

/// In a page that culls what is outside its viewport, a year far down the
/// list can still be hovered and picked.
#[test]
fn a_year_can_be_picked_in_a_culling_page() {
    let Some(mut ui) = session() else { return };
    let picked = signal(None);
    ui.mount(move |_| {
        blinc_layout::widgets::scroll::scroll()
            .w(W as f32)
            .h(H as f32)
            .viewport_cull(true)
            .child(
                div()
                    .w(W as f32)
                    .p(1.0)
                    .child(blinc_cn::calendar(picked).today(day(1))),
            )
    });
    ui.leave().settle();
    let year_trigger = all(&ui, "cn-select-trigger")[1];
    click(&mut ui, year_trigger);
    ui.settle();
    let list = all(&ui, "cn-select-list")[0];
    let b = ui.tree().get_absolute_bounds(list).unwrap();
    // Two rows above 2026: 2024.
    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0 - 64.0);
    ui.move_to(x, y).mouse_down().mouse_up().settle();
    assert!(
        all(&ui, "cn-select-list").is_empty(),
        "the pick closed the list"
    );
    let first = all(&ui, "cn-calendar-day")
        .into_iter()
        .position(|d| !has(&ui, d, "data-outside"))
        .unwrap();
    // November 2024 starts on a Friday.
    assert_eq!(first, 5, "the calendar shows 2024");
}

/// A two-finger swipe over the days turns the month once, however far it
/// runs on; vertical scrolling over the calendar still scrolls the page.
#[test]
fn a_swipe_turns_one_month_and_vertical_scroll_passes_on() {
    let Some(mut ui) = session() else { return };
    ui.mount(move |_| {
        blinc_layout::widgets::scroll::scroll()
            .w(W as f32)
            .h(H as f32)
            .child(
                div()
                    .w(W as f32)
                    .h(H as f32 * 3.0)
                    .p(1.0)
                    .child(blinc_cn::calendar(signal(None)).today(day(1))),
            )
    });
    ui.leave().settle();
    let first = |ui: &Headless| {
        all(ui, "cn-calendar-day")
            .into_iter()
            .position(|d| !has(ui, d, "data-outside"))
            .unwrap()
    };
    assert_eq!(first(&ui), 0, "November");
    let day = all(&ui, "cn-calendar-day")[17];
    let b = ui.tree().get_absolute_bounds(day).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0);

    // One gesture, with momentum: one month on.
    for _ in 0..10 {
        ui.scroll(-20.0, 0.0);
    }
    ui.settle();
    assert_eq!(first(&ui), 2, "December, and no further");

    // A second swipe, after a pause and with the pointer where it was, turns
    // another month.
    std::thread::sleep(std::time::Duration::from_millis(200));
    for _ in 0..10 {
        ui.scroll(-20.0, 0.0);
    }
    ui.settle();
    // January 2027 starts on a Friday.
    assert_eq!(first(&ui), 5, "January, on the second swipe");

    // Vertical scroll over the days moves the page.
    let page = ui.tree().root().unwrap();
    let scroller = {
        let tree = ui.tree();
        let mut stack = vec![page];
        let mut found = None;
        while let Some(n) = stack.pop() {
            if tree.is_scroll_container(n) {
                found = Some(n);
                break;
            }
            stack.extend(tree.layout_tree.children(n));
        }
        found.expect("the page scroll")
    };
    let before = ui.tree().get_scroll_offset(scroller).1;
    ui.scroll(0.0, -60.0).settle();
    let after = ui.tree().get_scroll_offset(scroller).1;
    assert!(
        after < before - 1.0,
        "the page did not scroll: {before} to {after}"
    );
    assert_eq!(first(&ui), 5, "vertical scroll turned the month");
}

/// The shown month's horizontal offset, from its page's transform.
fn pulled(ui: &Headless) -> f32 {
    let grid = all(ui, "cn-calendar-page")[0];
    match ui
        .tree()
        .get_render_node(grid)
        .and_then(|n| n.props.transform.clone())
    {
        Some(blinc_core::Transform::Affine2D(a)) => a.elements[4],
        _ => 0.0,
    }
}

/// A swipe moves the month with the fingers: short of the distance it
/// springs back when they lift, past it the month turns.
#[test]
fn the_month_follows_a_swipe_and_turns_or_springs_back() {
    let Some(mut ui) = session() else { return };
    mount(&mut ui, signal(None), signal(Vec::new()));
    let first = |ui: &Headless| {
        all(ui, "cn-calendar-day")
            .into_iter()
            .position(|d| !has(ui, d, "data-outside"))
            .unwrap()
    };
    let day = all(&ui, "cn-calendar-day")[17];
    let b = ui.tree().get_absolute_bounds(day).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0);

    ui.scroll(-15.0, 0.0).scroll(-15.0, 0.0);
    assert!((pulled(&ui) + 30.0).abs() < 0.5, "follows: {}", pulled(&ui));
    ui.scroll_end().settle();
    assert_eq!(pulled(&ui), 0.0, "springs back");
    assert_eq!(first(&ui), 0, "still November");

    ui.scroll_start().scroll(-30.0, 0.0).scroll(-30.0, 0.0);
    assert!((pulled(&ui) + 60.0).abs() < 0.5, "follows: {}", pulled(&ui));
    ui.scroll_end().settle();
    assert_eq!(all(&ui, "cn-calendar-grid").len(), 1);
    assert_eq!(first(&ui), 2, "December");
    assert_eq!(pulled(&ui), 0.0, "December sits in place");
}

/// A drag moves the month while the button is down.
#[test]
fn the_month_follows_a_drag() {
    let Some(mut ui) = session() else { return };
    mount(&mut ui, signal(None), signal(Vec::new()));
    let day = all(&ui, "cn-calendar-day")[17];
    let b = ui.tree().get_absolute_bounds(day).unwrap();
    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    ui.move_to(x, y).mouse_down().move_to(x + 25.0, y);
    assert!((pulled(&ui) - 25.0).abs() < 0.5, "follows: {}", pulled(&ui));
    ui.mouse_up().settle();
    assert_eq!(pulled(&ui), 0.0, "springs back");
}

/// Swipes one after another, each followed by the platform's momentum, turn
/// a month each with no pause between them; the momentum moves nothing.
#[test]
fn back_to_back_swipes_each_turn_a_month() {
    let Some(mut ui) = session() else { return };
    mount(&mut ui, signal(None), signal(Vec::new()));
    let first = |ui: &Headless| {
        all(ui, "cn-calendar-day")
            .into_iter()
            .position(|d| !has(ui, d, "data-outside"))
            .unwrap()
    };
    let day = all(&ui, "cn-calendar-day")[17];
    let b = ui.tree().get_absolute_bounds(day).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0);
    // November, December, January: first in-month cells 0, 2, 5.
    for expected in [2, 5] {
        ui.scroll_start().scroll(-25.0, 0.0).scroll(-25.0, 0.0);
        assert!(pulled(&ui) < -40.0, "follows the fingers: {}", pulled(&ui));
        ui.scroll_end();
        // Momentum after the lift.
        for _ in 0..5 {
            ui.scroll(-10.0, 0.0);
        }
        ui.settle();
        assert_eq!(first(&ui), expected);
        assert_eq!(pulled(&ui), 0.0, "momentum moved the month");
    }
}

/// Two calendars side by side, 300px apart.
fn mount_two(ui: &mut Headless) {
    ui.mount(move |_| {
        div()
            .w(640.0)
            .h(H as f32)
            .flex_row()
            .items_start()
            .child(
                div()
                    .w(320.0)
                    .child(blinc_cn::calendar(signal(None)).today(day(1))),
            )
            .child(
                div()
                    .w(320.0)
                    .child(blinc_cn::calendar(signal(None)).today(day(1))),
            )
    });
    ui.leave().settle();
}

/// Each calendar's first in-month cell: which month it shows.
fn firsts(ui: &Headless) -> Vec<usize> {
    all(ui, "cn-calendar")
        .into_iter()
        .map(|cal| {
            let tree = ui.tree();
            let mut stack = vec![cal];
            let mut days = Vec::new();
            while let Some(n) = stack.pop() {
                if tree.element_registry().has_class(n, "cn-calendar-day") {
                    days.push(n);
                }
                let mut c = tree.layout_tree.children(n);
                c.reverse();
                stack.extend(c);
            }
            days.iter()
                .position(|d| !has(ui, *d, "data-outside"))
                .unwrap()
        })
        .collect()
}

fn over_day(ui: &mut Headless, calendar: usize) {
    let day = all(ui, "cn-calendar-day")[calendar * 42 + 17];
    let b = ui.tree().get_absolute_bounds(day).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0);
}

/// A swipe belongs to the calendar it started on: its momentum, with the
/// pointer moved onto the other calendar, moves neither.
#[test]
fn a_swipe_belongs_to_its_calendar() {
    let Some(mut ui) = session_sized(640, H) else {
        return;
    };
    mount_two(&mut ui);
    assert_eq!(firsts(&ui), [0, 0]);
    over_day(&mut ui, 0);
    ui.scroll_start()
        .scroll(-30.0, 0.0)
        .scroll(-30.0, 0.0)
        .scroll_end();
    over_day(&mut ui, 1);
    for _ in 0..6 {
        ui.scroll(-20.0, 0.0);
    }
    ui.scroll_momentum_end().settle();
    assert_eq!(
        firsts(&ui),
        [2, 0],
        "the first turned once, the second not at all"
    );
}

/// The pointer leaving mid-swipe cancels it: the month springs back, and
/// what is left of the gesture moves neither calendar.
#[test]
fn leaving_cancels_a_swipe() {
    let Some(mut ui) = session_sized(640, H) else {
        return;
    };
    mount_two(&mut ui);
    over_day(&mut ui, 0);
    ui.scroll_start().scroll(-30.0, 0.0);
    assert!(pulled(&ui) < -20.0, "follows: {}", pulled(&ui));
    over_day(&mut ui, 1);
    assert_eq!(pulled(&ui), 0.0, "leaving put the month back");
    ui.scroll(-30.0, 0.0)
        .scroll(-30.0, 0.0)
        .scroll_end()
        .settle();
    assert_eq!(firsts(&ui), [0, 0], "neither turned");

    // A new touch on the second is its own swipe.
    ui.scroll_start()
        .scroll(-30.0, 0.0)
        .scroll(-30.0, 0.0)
        .scroll_end()
        .settle();
    assert_eq!(firsts(&ui), [0, 2]);
}

/// A drag that leaves the days springs back and does not turn the month.
#[test]
fn a_drag_leaving_the_days_is_cancelled() {
    let Some(mut ui) = session() else { return };
    mount(&mut ui, signal(None), signal(Vec::new()));
    let day = all(&ui, "cn-calendar-day")[17];
    let b = ui.tree().get_absolute_bounds(day).unwrap();
    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    ui.move_to(x, y).mouse_down();
    for step in 1..=4 {
        ui.move_to(x - step as f32 * 15.0, y);
    }
    assert!(pulled(&ui) < -40.0);
    // Off the bottom of the calendar.
    ui.move_to(x - 60.0, y + 400.0);
    assert_eq!(pulled(&ui), 0.0, "leaving put the month back");
    ui.mouse_up().settle();
    let first = all(&ui, "cn-calendar-day")
        .into_iter()
        .position(|d| !has(&ui, d, "data-outside"))
        .unwrap();
    assert_eq!(first, 0, "still November");
}
