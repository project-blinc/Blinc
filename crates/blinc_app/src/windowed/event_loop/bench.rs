//! A scripted swipe for measuring a window, off unless `BLINC_BENCH_WHEEL` is set.
//!
//! - `BLINC_BENCH_WHEEL=<.class|#id>`: wheel the page down until the first element that matches
//!   is in view, wait for the page to settle, then swipe left over its centre: six horizontal
//!   deltas, a lift, and the end of momentum, once every `BLINC_BENCH_WHEEL_PERIOD` steps (36 by
//!   default).
//! - `BLINC_BENCH_WHEEL_TURNS`: how many swipes before the process exits (40 by default).
//!
//! The script takes 60 steps a second of wall time, whatever the window draws in between, and
//! its events go through the same input path as the platform's. Two builds fed the script get
//! the same input over the same time and can be compared by instruction count.

use blinc_layout::prelude::*;
use blinc_platform::{InputEvent, MouseEvent, ScrollPhase};
use std::cell::RefCell;

const STEP: std::time::Duration = std::time::Duration::from_micros(16_667);
const SETTLE_STEPS: u64 = 120;
const REST_STEPS: u64 = 30;
/// How long the page must be still before the element is measured.
const STILL: std::time::Duration = std::time::Duration::from_secs(3);
const SEEK_LIMIT: u64 = 400;

struct Script {
    selector: String,
    period: u64,
    turns: u64,
    step: u64,
    due: Option<std::time::Instant>,
    stage: Stage,
}

#[derive(Clone, Copy)]
enum Stage {
    Settle,
    Seek { tries: u64 },
    Still { since: std::time::Instant },
    Rest { until: u64 },
    Swipe { start: u64 },
}

thread_local! {
    static SCRIPT: RefCell<Option<Script>> = RefCell::new(
        std::env::var("BLINC_BENCH_WHEEL").ok().map(|selector| Script {
            selector,
            period: number("BLINC_BENCH_WHEEL_PERIOD", 36).max(12),
            turns: number("BLINC_BENCH_WHEEL_TURNS", 40).max(1),
            step: 0,
            due: None,
            stage: Stage::Settle,
        }),
    );
}

fn number(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Whether a script is running.
pub(super) fn on() -> bool {
    SCRIPT.with(|s| s.borrow().is_some())
}

/// The first node in document order that matches `.class` or `#id`.
fn find(tree: &RenderTree, selector: &str) -> Option<LayoutNodeId> {
    if let Some(id) = selector.strip_prefix('#') {
        return tree.query_by_id(id);
    }
    let class = selector.strip_prefix('.')?;
    let index = tree.element_registry().class_to_nodes_index();
    let matching = index.get(class)?;
    let mut stack = vec![tree.root()?];
    while let Some(node) = stack.pop() {
        if matching.contains(&node) {
            return Some(node);
        }
        stack.extend(tree.layout_tree.children(node).into_iter().rev());
    }
    None
}

/// The events to feed now, given the tree, the window's logical size and its scale, and how long
/// until the next step is due.
pub(super) fn next(
    tree: &RenderTree,
    logical: (f32, f32),
    scale: f32,
) -> (Vec<InputEvent>, std::time::Duration) {
    SCRIPT.with(|s| {
        let mut guard = s.borrow_mut();
        let Some(script) = guard.as_mut() else {
            return (Vec::new(), STEP);
        };
        let now = std::time::Instant::now();
        let due = *script.due.get_or_insert(now);
        if now < due {
            return (Vec::new(), due - now);
        }
        script.due = Some(due + STEP);
        let wait = (due + STEP).saturating_duration_since(now);
        (step(script, tree, logical, scale), wait)
    })
}

fn step(
    script: &mut Script,
    tree: &RenderTree,
    logical: (f32, f32),
    scale: f32,
) -> Vec<InputEvent> {
    {
        let n = script.step;
        script.step += 1;
        let moved = |x: f32, y: f32| {
            InputEvent::Mouse(MouseEvent::Moved {
                x: x * scale,
                y: y * scale,
            })
        };
        let scroll = |dx: f32, dy: f32, phase: ScrollPhase| InputEvent::Scroll {
            delta_x: dx,
            delta_y: dy,
            phase,
        };
        match script.stage {
            Stage::Settle => {
                if n >= SETTLE_STEPS {
                    script.stage = Stage::Seek { tries: 0 };
                }
                Vec::new()
            }
            Stage::Seek { tries } => {
                let bounds =
                    find(tree, &script.selector).and_then(|node| tree.get_absolute_bounds(node));
                if bounds.is_some_and(|b| b.y > 60.0 && b.y + b.height < logical.1 - 20.0) {
                    script.stage = Stage::Still {
                        since: std::time::Instant::now(),
                    };
                    return Vec::new();
                }
                if tries >= SEEK_LIMIT {
                    tracing::error!("bench wheel: {} never came into view", script.selector);
                    std::process::exit(2);
                }
                script.stage = Stage::Seek { tries: tries + 1 };
                vec![
                    moved(4.0, logical.1 / 2.0),
                    scroll(0.0, -120.0, ScrollPhase::Moved),
                ]
            }
            // The page glides on after the last wheel and its scrollbar fades later; measure
            // where the element stops once both are over.
            Stage::Still { since } => {
                if since.elapsed() < STILL
                    || tree.has_animating_scroll_physics()
                    || tree.scroll_off_rows_animating()
                {
                    return Vec::new();
                }
                let bounds =
                    find(tree, &script.selector).and_then(|node| tree.get_absolute_bounds(node));
                if let Some(b) = bounds
                    && b.y > 60.0
                    && b.y + b.height < logical.1 - 20.0
                {
                    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
                    tracing::info!("bench wheel: {} at {b:?}", script.selector);
                    script.stage = Stage::Rest {
                        until: n + REST_STEPS,
                    };
                    return vec![moved(x, y)];
                }
                script.stage = Stage::Seek { tries: 0 };
                Vec::new()
            }
            Stage::Rest { until } => {
                if n >= until {
                    script.stage = Stage::Swipe { start: n };
                }
                Vec::new()
            }
            Stage::Swipe { start } => {
                let turn = (n - start) / script.period;
                if turn >= script.turns {
                    tracing::info!(
                        "bench wheel: {} swipes in {} steps",
                        script.turns,
                        n - start
                    );
                    std::process::exit(0);
                }
                match (n - start) % script.period {
                    0 => vec![scroll(-8.0, 0.0, ScrollPhase::Started)],
                    1..=5 => vec![scroll(-8.0, 0.0, ScrollPhase::Moved)],
                    6 => vec![scroll(0.0, 0.0, ScrollPhase::Ended)],
                    f if f == script.period - 1 => {
                        vec![scroll(0.0, 0.0, ScrollPhase::MomentumEnded)]
                    }
                    _ => Vec::new(),
                }
            }
        }
    }
}
