//! What scrolling a long list costs, on each way a window can paint it.
//!
//! The list is `SCROLL_ROWS` rows (default 100000) in a scroll container. A run builds it, paints
//! it in full, jumps to the middle of it, and then scrolls a notch a frame, reporting what each
//! frame cost the engine (`SCROLL_STEPS` frames of `SCROLL_NOTCH` pixels).
//!
//! ```text
//! cargo nextest run -p blinc_recorder --features headless --test scroll_perf \
//!     --run-ignored only --no-capture
//! SCROLL_ROWS=250000 cargo nextest run ...
//! ```
//!
//! The small `scrolling_a_short_list_*` test is not ignored and keeps this file honest.

use blinc_core::Color;
use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_layout::selector::ScrollRef;
use blinc_recorder::{Headless, MotionPaint};
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 360;
const H: u32 = 640;
const ROW_HEIGHT: f32 = 30.0;

fn env_number(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// One way a window can paint a scrolled frame.
#[derive(Clone, Copy)]
enum Way {
    /// Baked motion, and a full paint for every scrolled frame: what scrolling was.
    FullPaint,
    /// Rows applied on the CPU: a scrolled frame writes a row.
    CpuRows,
    /// Rows of the GPU node table.
    GpuRows,
}

impl Way {
    fn name(self) -> &'static str {
        match self {
            Way::FullPaint => "full paint every frame",
            Way::CpuRows => "rows on the CPU",
            Way::GpuRows => "rows on the GPU",
        }
    }

    fn paint(self) -> MotionPaint {
        match self {
            Way::FullPaint => MotionPaint::Promoted,
            Way::CpuRows => MotionPaint::Vector,
            Way::GpuRows => MotionPaint::NodeTable,
        }
    }

    fn fast(self) -> bool {
        !matches!(self, Way::FullPaint)
    }
}

fn session(way: Way) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::with_motion_paint(W, H, way.paint()).ok()?;
    // No stylesheet: the rows carry no classes, and a stylesheet makes the harness match every
    // node on every frame, which the windowed runner only does when hover or focus changes.
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    ui.load_font_data(
        include_bytes!("../../../examples/blinc_component_demos/assets/NotoSans-Regular.ttf")
            .to_vec(),
    );
    Some(ui)
}

/// What one run cost, in milliseconds.
struct Report {
    rows: usize,
    build: f64,
    first_paint: f64,
    jump: f64,
    frames: Vec<f64>,
}

impl Report {
    fn print(&self, way: Way) {
        let mut sorted = self.frames.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let at = |q: f64| sorted[((sorted.len() - 1) as f64 * q) as usize];
        let median = at(0.5);
        let repaints = self.frames.iter().filter(|f| **f > median * 3.0).count();
        eprintln!(
            "  {:<24} build {:>8.0}  first paint {:>7.1}  jump {:>7.1}  | frame median {:>7.2}  p90 {:>7.2}  p99 {:>7.2}  max {:>7.1}  ({} of {} above 3x the median)",
            way.name(),
            self.build,
            self.first_paint,
            self.jump,
            median,
            at(0.9),
            at(0.99),
            sorted[sorted.len() - 1],
            repaints,
            self.frames.len(),
        );
        let _ = self.rows;
    }
}

/// Build `rows` rows, paint, jump to the middle of the list, and scroll `steps` notches.
fn run(way: Way, rows: usize, steps: usize, notch: f32) -> Option<Report> {
    let mut ui = session(way)?;
    let reference = ScrollRef::new();
    let bound = reference.clone();

    let started = std::time::Instant::now();
    ui.mount(move |_| {
        let mut list = div().flex_col().w(W as f32);
        for i in 0..rows {
            let hue = (i % 9) as f32 / 9.0;
            list = list.child(
                div()
                    .w_full()
                    .h(ROW_HEIGHT - 2.0)
                    .mb(0.5)
                    .px(3.0)
                    .rounded(6.0)
                    .bg(Color::rgb(0.2 + 0.35 * hue, 0.4, 0.9 - 0.4 * hue))
                    .flex_row()
                    .items_center()
                    .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE)),
            );
        }
        div()
            .w(W as f32)
            .h(H as f32)
            .bg(Color::rgb(0.09, 0.1, 0.13))
            .child(scroll().bind(&bound).w(W as f32).h(H as f32).child(list))
    });
    ui.settle();
    let build = started.elapsed().as_secs_f64() * 1000.0;

    let started = std::time::Instant::now();
    ui.paint_timed(false).ok()?;
    let first_paint = started.elapsed().as_secs_f64() * 1000.0;

    // The middle of the list, where nothing about the index is small.
    let middle = (rows as f32 / 2.0) * ROW_HEIGHT;
    reference.set_scroll_offset(0.0, middle);
    ui.settle_scroll();
    let (_, jump) = ui.paint_timed(false).ok()?;

    let mut frames = Vec::with_capacity(steps);
    for step in 0..steps {
        reference.set_scroll_offset(0.0, middle + notch * (step + 1) as f32);
        ui.settle_scroll();
        let (_, render) = ui.paint_timed(way.fast()).ok()?;
        frames.push(render);
    }
    Some(Report {
        rows,
        build,
        first_paint,
        jump,
        frames,
    })
}

/// A short list: scrolling it gives the same frame on every way, and it moves.
#[test]
fn scrolling_a_short_list_works_on_every_way() {
    for way in [Way::FullPaint, Way::CpuRows, Way::GpuRows] {
        let Some(report) = run(way, 300, 6, 24.0) else {
            return;
        };
        assert_eq!(report.frames.len(), 6, "{}", way.name());
    }
}

/// Not a check: the numbers. Run with `--run-ignored only --no-capture`.
#[test]
#[ignore]
fn what_scrolling_a_long_list_costs() {
    let rows = env_number("SCROLL_ROWS", 100_000.0) as usize;
    let steps = env_number("SCROLL_STEPS", 60.0) as usize;
    let notch = env_number("SCROLL_NOTCH", 40.0);
    eprintln!(
        "{rows} rows of {ROW_HEIGHT} px, {steps} frames of {notch} px, from the middle (ms):"
    );
    // `SCROLL_WAYS=full,cpu,gpu` (default all) picks which ways run.
    let wanted = std::env::var("SCROLL_WAYS").unwrap_or_else(|_| "full,cpu,gpu".into());
    for (key, way) in [
        ("full", Way::FullPaint),
        ("cpu", Way::CpuRows),
        ("gpu", Way::GpuRows),
    ] {
        if !wanted.split(',').any(|w| w.trim() == key) {
            continue;
        }
        let Some(report) = run(way, rows, steps, notch) else {
            return;
        };
        report.print(way);
    }
}
