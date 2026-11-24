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
use blinc_layout::div::{Div, ElementBuilder, div};
use blinc_layout::prelude::*;
use blinc_layout::selector::ScrollRef;
use blinc_layout::widgets::virtual_list::virtual_list;
use blinc_recorder::{Headless, MotionPaint};
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 360;
const H: u32 = 640;
const ROW_HEIGHT: f32 = 30.0;
/// Pixels a frame may differ by from a full paint: the edge of a glyph a fraction of a pixel apart.
/// Text a frame behind its row, or a row a pixel off, differs by hundreds.
const EDGE_PIXELS: usize = 40;

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
    // The state-style pass runs when hover or focus changes, as in a window.
    ui.theme(HybridTheme::bundle(), ColorScheme::Dark);
    ui.gate_state_styles();
    ui.load_font_data(
        include_bytes!("../../../../examples/blinc_component_demos/assets/NotoSans-Regular.ttf")
            .to_vec(),
    );
    Some(ui)
}

/// What one run cost, in milliseconds.
struct Report {
    rows: usize,
    build: f64,
    first_paint: f64,
    /// How much of the first paint was the render, the rest being the frame before it.
    first_render: f64,
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
            "  {:<24} build {:>8.0}  first paint {:>7.1} (render {:>6.1})  jump {:>7.1}  | frame median {:>7.2}  p90 {:>7.2}  p99 {:>7.2}  max {:>7.1}  ({} of {} above 3x the median)",
            way.name(),
            self.build,
            self.first_paint,
            self.first_render,
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

fn row(i: usize) -> Div {
    let hue = (i % 9) as f32 / 9.0;
    div()
        .w_full()
        .h(ROW_HEIGHT - 2.0)
        .mb(0.5)
        .px(3.0)
        .rounded(6.0)
        .bg(Color::rgb(0.2 + 0.35 * hue, 0.4, 0.9 - 0.4 * hue))
        .flex_row()
        .items_center()
        .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE))
}

/// Mount `rows` rows in a scroll container bound to `bound`: a child for every row, or a virtual
/// list that builds the rows in view.
fn mount_list(ui: &mut Headless, rows: usize, virtualized: bool, bound: &ScrollRef) {
    let bound = bound.clone();
    ui.mount(move |_| {
        let list: Box<dyn ElementBuilder> = if virtualized {
            // Only a window of rows is built; the rest of the list is a count.
            Box::new(
                virtual_list(rows, row)
                    .estimated_item_height(ROW_HEIGHT)
                    .bind(&bound)
                    .w(W as f32)
                    .h(H as f32)
                    .into_div(),
            )
        } else {
            let mut list = div().flex_col().w(W as f32);
            for i in 0..rows {
                list = list.child(row(i));
            }
            Box::new(scroll().bind(&bound).w(W as f32).h(H as f32).child(list))
        };
        div()
            .w(W as f32)
            .h(H as f32)
            .bg(Color::rgb(0.09, 0.1, 0.13))
            .child_box(list)
    });
}

/// Build `rows` rows, paint, jump to the middle of the list, and scroll `steps` notches.
fn run(way: Way, rows: usize, steps: usize, notch: f32) -> Option<Report> {
    let mut ui = session(way)?;
    let reference = ScrollRef::new();
    let bound = reference.clone();

    let started = std::time::Instant::now();
    let virtualized = std::env::var("SCROLL_LIST").is_ok_and(|v| v == "virtual");
    mount_list(&mut ui, rows, virtualized, &bound);
    ui.settle();
    let build = started.elapsed().as_secs_f64() * 1000.0;

    // `SCROLL_PAUSE_FIRST_MS` waits before the first paint, to attach a profiler.
    if let Some(pause) = std::env::var("SCROLL_PAUSE_FIRST_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        eprintln!("READY_FIRST {}", way.name());
        std::thread::sleep(std::time::Duration::from_millis(pause));
    }
    let (first_frame, first_render) = ui.paint_timed(false).ok()?;
    let first_paint = first_frame + first_render;

    // The middle of the list, where nothing about the index is small.
    let middle = (rows as f32 / 2.0) * ROW_HEIGHT;
    reference.set_scroll_offset(0.0, middle);
    ui.settle_scroll();
    let (_, jump) = ui.paint_timed(false).ok()?;
    // `SCROLL_DUMP=<dir>` writes the frame after the jump as raw RGBA, to see what was drawn.
    if let Ok(dir) = std::env::var("SCROLL_DUMP") {
        let frame = ui.snapshot().ok()?;
        let _ = std::fs::write(
            format!(
                "{dir}/{}_{}x{}.rgba",
                virtualized, frame.width, frame.height
            ),
            &frame.data,
        );
    }

    // `SCROLL_PAUSE_MS` waits here before the frames that are timed, to attach a profiler.
    if let Some(pause) = std::env::var("SCROLL_PAUSE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        eprintln!("READY {}", way.name());
        std::thread::sleep(std::time::Duration::from_millis(pause));
    }

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
        first_render,
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

/// A list scrolled a frame at a time shows what a full paint of the same state shows, text with the
/// rows it belongs to (to within the anti-aliasing of an edge a fraction of a pixel apart): after
/// a full paint, `run` frames each scrolled a little and painted the way a window paints a frame
/// in which only the scroll moved, then one full paint of the last of them to compare with.
/// Several run lengths, so the window of painted content is outrun and painted again among them.
#[test]
fn a_list_scrolled_by_rows_shows_what_a_full_paint_shows() {
    for (virtualized, name) in [(false, "a child for every row"), (true, "a virtual list")] {
        let Some(mut ui) = session(Way::CpuRows) else {
            return;
        };
        let reference = ScrollRef::new();
        mount_list(&mut ui, 4000, virtualized, &reference);
        ui.settle();
        let mut y = 3000.0;
        reference.set_scroll_offset(0.0, y);
        ui.settle_scroll();
        let mut off = Vec::new();
        for run in [10, 25, 40, 55, 70, 90, 120] {
            ui.invalidate_render_cache("scroll_perf");
            ui.snapshot().expect("a full paint");
            let mut last = None;
            for _ in 0..run {
                y += 7.0;
                reference.set_scroll_offset(0.0, y);
                ui.frame(16);
                last = Some(ui.snapshot_fast().expect("a fast paint"));
            }
            ui.invalidate_render_cache("scroll_perf");
            let full = ui.snapshot().expect("a full paint");
            let differing =
                blinc_recorder::testing::difference(&last.expect("a frame"), &full, 100).differing;
            if differing > EDGE_PIXELS {
                off.push((run, differing));
            }
        }
        assert!(
            off.is_empty(),
            "{name}: after this many frames of scroll, pixels that differ from a full paint: {off:?}"
        );
    }
}

/// Not a check: what moving the pointer over a long list costs a frame when a stylesheet has class
/// rules and `:hover` rules (`SCROLL_ROWS`, default 20000). A rule such as `.list:hover .row` is
/// not in it: it restyles every row while the list is hovered, which is what the rule says. The state-style pass runs when the
/// hovered set changes, as in a window.
#[test]
#[ignore]
fn what_hovering_a_long_list_costs() {
    if !Headless::available() {
        return;
    }
    let rows = env_number("SCROLL_ROWS", 20_000.0) as usize;
    let css = ".row { border-radius: 6px; } .row:hover { background: #ffffff33; } \
               .row + .row:hover { border-color: #ffffff55; }";
    let mut ui = Headless::with_motion_paint(W, H, MotionPaint::Vector).expect("a session");
    ui.theme(HybridTheme::bundle().with_css(css), ColorScheme::Dark);
    ui.gate_state_styles();
    ui.load_font_data(
        include_bytes!("../../../../examples/blinc_component_demos/assets/NotoSans-Regular.ttf")
            .to_vec(),
    );
    ui.mount(move |_| {
        let mut list = div().flex_col().w(W as f32).class("group");
        for i in 0..rows {
            list = list.child(row(i).class("row"));
        }
        div()
            .w(W as f32)
            .h(H as f32)
            .bg(Color::rgb(0.09, 0.1, 0.13))
            .child(scroll().w(W as f32).h(H as f32).child(list))
    });
    ui.settle();
    ui.snapshot().expect("a first paint");
    // From the pointer move to the painted frame: the move (which runs the frame it lands in, the
    // state-style pass among it), and the paint of what a window would paint after it.
    let mut costs = Vec::new();
    let mut paints = Vec::new();
    let moves = env_number("SCROLL_HOVERS", 40.0) as usize;
    eprintln!("READY hovering");
    for step in 0..moves {
        // A different row under the pointer each time, so the hovered set changes.
        let started = std::time::Instant::now();
        ui.move_to(60.0, 15.0 + ROW_HEIGHT * (step % 18) as f32);
        ui.frame(16);
        costs.push(started.elapsed().as_secs_f64() * 1000.0);
        let (_, render) = ui.paint_timed(true).expect("a paint");
        if std::env::var("SCROLL_TRACE").is_ok() && render > 3.0 {
            eprintln!("slow hover paint at step {step}: {render:.1} ms");
        }
        paints.push(render);
    }
    paints.sort_by(|a, b| a.total_cmp(b));
    eprintln!(
        "{rows} rows, hover moves, paint ms: median {:.2}  p90 {:.2}  max {:.2}",
        paints[paints.len() / 2],
        paints[paints.len() * 9 / 10],
        paints[paints.len() - 1]
    );
    costs.sort_by(|a, b| a.total_cmp(b));
    eprintln!(
        "{rows} rows, hover moves, move and frame ms: median {:.2}  p90 {:.2}  max {:.2}",
        costs[costs.len() / 2],
        costs[costs.len() * 9 / 10],
        costs[costs.len() - 1]
    );
}
