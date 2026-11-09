//! Dragging a trigger along the strip reorders the tabs. The held trigger follows the pointer
//! through a transform; the strip is a region over the order, so when the held trigger's middle
//! passes a neighbour's, the two swap places and the neighbour slides into the gap by its
//! `transform` transition (`.cn-tabs-list--reorderable`). Dropping reports the new order.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use blinc_core::Transform;
use blinc_core::reactive::{ReactiveGraph, Signal, computed, signal};
use blinc_layout::prelude::*;
use blinc_layout::selector::DivRef;

use super::{TabMenuItem, TabsSize, build_tab_trigger, close};

/// Called with the tabs' values in their new order when a drag ends.
pub(super) type OnReorder = Arc<dyn Fn(Vec<String>) + Send + Sync>;

/// Called with a tab's value when it is selected.
type OnChange = Option<Arc<dyn Fn(&str) + Send + Sync>>;

/// How far the pointer moves before a press becomes a drag, so a click still selects.
const DRAG_THRESHOLD: f32 = 4.0;

/// A press on a trigger, which may become a drag.
#[derive(Clone)]
struct Press {
    value: String,
    /// Where the pointer went down.
    start_x: f32,
    /// Where the held trigger was laid out when the press became a drag.
    origin: Option<f32>,
    /// How far the held trigger's laid-out place has moved since, by the swaps made.
    moved: f32,
}

/// What a drag shares between the triggers.
#[derive(Clone)]
struct Drag {
    order: Signal<Vec<String>>,
    /// The trigger being dragged, once the press has moved far enough.
    held: Signal<Option<String>>,
    /// The held trigger's offset from its laid-out place.
    offset: Signal<f32>,
    press: Arc<Mutex<Option<Press>>>,
    elements: Arc<Mutex<HashMap<String, DivRef>>>,
}

impl Drag {
    fn bounds(&self, value: &str) -> Option<blinc_core::context_state::Bounds> {
        self.elements
            .lock()
            .unwrap()
            .get(value)
            .and_then(DivRef::bounds)
    }

    /// The pointer is at `x` with the button held: follow it, swapping the held trigger with a
    /// neighbour whose middle its own middle has passed.
    fn follow(&self, x: f32) {
        let Some(mut press) = self.press.lock().unwrap().clone() else {
            return;
        };
        if press.origin.is_none() {
            if (x - press.start_x).abs() < DRAG_THRESHOLD {
                return;
            }
            press.origin = self.bounds(&press.value).map(|b| b.x);
            self.held.set(Some(press.value.clone()));
        }
        let mut order = self.order.try_get().unwrap_or_default();
        if let (Some(origin), Some(at), Some(own)) = (
            press.origin,
            order.iter().position(|v| *v == press.value),
            self.bounds(&press.value),
        ) {
            // Swap only once layout has caught up with the last swap, so the neighbours'
            // places are where they are now laid out. Until it has, the held trigger is laid out
            // a whole neighbour away from where the swaps put it; once it has, it is within a
            // pixel's rounding, and that place is the one to follow from.
            let laid_out = origin + press.moved;
            if (own.x - laid_out).abs() < own.width / 2.0 {
                press.moved = own.x - origin;
                let middle = origin + (x - press.start_x) + own.width / 2.0;
                let next = order.get(at + 1).and_then(|v| self.bounds(v));
                let prev = at.checked_sub(1).and_then(|i| self.bounds(&order[i]));
                if let Some(n) = next.filter(|n| middle > n.x + n.width / 2.0) {
                    press.moved = n.x + n.width - own.width - origin;
                    order.swap(at, at + 1);
                    self.order.set(order);
                } else if let Some(p) = prev.filter(|p| middle < p.x + p.width / 2.0) {
                    press.moved = p.x - origin;
                    order.swap(at, at - 1);
                    self.order.set(order);
                }
            }
        }
        self.offset.set(x - press.start_x - press.moved);
        *self.press.lock().unwrap() = Some(press);
    }

    /// The button is up: settle the held trigger in its place and report the order.
    fn drop(&self, on_reorder: &OnReorder) {
        *self.press.lock().unwrap() = None;
        if self.held.try_get().flatten().is_some() {
            self.held.set(None);
            self.offset.set(0.0);
            on_reorder(self.order.try_get().unwrap_or_default());
        }
    }
}

/// The trigger strip, its triggers draggable into a new order.
pub(super) fn strip(
    trigger_key: String,
    items: Vec<TabMenuItem>,
    size: TabsSize,
    state: Signal<String>,
    on_change: OnChange,
    on_reorder: OnReorder,
    on_close: Option<close::OnClose>,
) -> Div {
    let drag = Drag {
        order: signal(
            items
                .iter()
                .map(|item| item.value().to_string())
                .collect::<Vec<_>>(),
        ),
        held: signal(None),
        offset: signal(0.0),
        press: Arc::default(),
        elements: Arc::default(),
    };
    let items = Arc::new(items);

    div()
        .class("cn-tabs-list")
        .class("cn-tabs-list--reorderable")
        .w_full()
        .flex_row()
        .items_center()
        .for_each(
            drag.order,
            |value: &String| value.clone(),
            move |value: String| {
                let Some(item) = items.iter().find(|item| item.value() == value) else {
                    return div();
                };
                let element = DivRef::new();
                drag.elements
                    .lock()
                    .unwrap()
                    .insert(value.clone(), element.clone());
                let (held, offset) = (drag.held, drag.offset);
                let is_held = {
                    let value = value.clone();
                    move |g: &ReactiveGraph| {
                        g.get(held).flatten().as_deref() == Some(value.as_str())
                    }
                };
                let lifted = computed(is_held.clone());
                let shifted = computed(move |g: &ReactiveGraph| {
                    if is_held(g) {
                        Transform::translate(g.get(offset).unwrap_or(0.0), 0.0)
                    } else {
                        Transform::identity()
                    }
                });
                let (down, moving, up) = (drag.clone(), drag.clone(), drag.clone());
                let pressed = value.clone();
                let on_reorder = Arc::clone(&on_reorder);
                build_tab_trigger(
                    &trigger_key,
                    item,
                    size,
                    state,
                    on_change.clone(),
                    on_close.clone(),
                )
                // Given here, at build, so the id is part of the trigger's first stable id.
                .id(format!("{trigger_key}:{value}"))
                .bind(&element)
                .class_when("cn-tabs-trigger--dragging", lifted)
                .transform(shifted)
                .on_mouse_down(move |ctx| {
                    *down.press.lock().unwrap() = Some(Press {
                        value: pressed.clone(),
                        start_x: ctx.mouse_x,
                        origin: None,
                        moved: 0.0,
                    });
                })
                .on_drag(move |ctx| moving.follow(ctx.mouse_x))
                .on_drag_end(move |_| up.drop(&on_reorder))
            },
        )
}
