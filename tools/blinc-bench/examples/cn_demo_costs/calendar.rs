//! Script the real page's calendar without driving a desktop window.
//!
//! `COSTS_CALENDAR=200 COSTS_CALENDAR_PHASE=pull` measures movement without month creation;
//! `turn` includes month creation and its slide, and `rest` measures the same viewport idle.
//! Omit the phase for all three. `COSTS_NO_TRACE=1` removes path tracing while profiling.
//! `COSTS_CALENDAR_SNAP=<prefix>` saves cached and full-paint references separately from timings;
//! with `COSTS_CALENDAR_SNAP_ALL=1`, at every frame of the first turn.
//! `COSTS_CALENDAR_INDEX=1` selects the right calendar. `COSTS_CALENDAR_FRAMES=12` and
//! `COSTS_CALENDAR_DT=8` turn months continuously without waiting for their slides to settle.
//! `COSTS_CALENDAR_TREE=1` counts root builds, structural updates and month-page lifetimes.

use super::{Headless, PathLog, Scene, bring_into_view, rest};
use std::time::Instant;

struct Samples<'a> {
    scene: Scene<'a>,
    input: Vec<f64>,
    total: Vec<f64>,
    step_ms: u32,
    tree_counts: Option<super::tree_counts::TreeCounts>,
}

impl<'a> Samples<'a> {
    fn new(ui: &'a mut Headless, log: PathLog) -> Self {
        Self {
            scene: Scene::new(ui, log),
            input: Vec::new(),
            total: Vec::new(),
            tree_counts: std::env::var_os("COSTS_CALENDAR_TREE")
                .map(|_| super::tree_counts::TreeCounts::default()),
            step_ms: std::env::var("COSTS_CALENDAR_DT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(16)
                .max(1),
        }
    }

    fn frame(&mut self, input: impl FnOnce(&mut Headless)) {
        let tree_before = self
            .tree_counts
            .as_ref()
            .map(|_| super::tree_counts::Snapshot::read(self.scene.ui));
        let start = Instant::now();
        input(self.scene.ui);
        self.input.push(start.elapsed().as_secs_f64() * 1000.0);
        self.scene.step_at(self.step_ms);
        self.total.push(start.elapsed().as_secs_f64() * 1000.0);
        if let Some(counts) = self.tree_counts.as_mut() {
            counts.record(tree_before.unwrap(), self.scene.ui);
        }
    }

    fn report(&self, name: &str) {
        self.scene.report(name);
        let quantiles = |samples: &[f64]| {
            let mut samples = samples.to_vec();
            samples.sort_by(f64::total_cmp);
            (samples[samples.len() / 2], samples[samples.len() * 9 / 10])
        };
        let (i50, i90) = quantiles(&self.input);
        let (t50, t90) = quantiles(&self.total);
        println!(
            "    {} frames, input {i50:.2} / {i90:.2} ms, complete headless step {t50:.2} / {t90:.2} ms",
            self.total.len()
        );
        let tail = |samples: &[f64]| {
            let mut samples = samples.to_vec();
            samples.sort_by(f64::total_cmp);
            [0.95, 0.99, 1.0].map(|p| samples[((samples.len() - 1) as f64 * p) as usize])
        };
        let [i95, i99, imax] = tail(&self.input);
        let [t95, t99, tmax] = tail(&self.total);
        println!(
            "    tail ms: input p95={i95:.2} p99={i99:.2} max={imax:.2}; complete step p95={t95:.2} p99={t99:.2} max={tmax:.2}"
        );
        let mut worst: Vec<_> = self.total.iter().copied().enumerate().collect();
        worst.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (frame, total) in worst.into_iter().take(3) {
            println!(
                "    slow step {}: input {:.2}ms total {total:.2}ms",
                frame + 1,
                self.input[frame],
            );
        }
        if let Some(counts) = &self.tree_counts {
            counts.report();
        }
    }
}

fn calendar_node(ui: &Headless, class: &str) -> Option<blinc_layout::LayoutNodeId> {
    let index = std::env::var("COSTS_CALENDAR_INDEX")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let tree = ui.tree();
    let mut nodes: Vec<_> = tree
        .element_registry()
        .class_to_nodes_index()
        .get(class)?
        .iter()
        .copied()
        .collect();
    nodes.sort_by(|a, b| {
        let x = |node| {
            tree.get_absolute_bounds(node)
                .map_or(f32::INFINITY, |b| b.x)
        };
        x(*a).total_cmp(&x(*b))
    });
    nodes.get(index).copied()
}

/// `COSTS_CALENDAR_DROPDOWN=<turns>`: turn that many months, open the year select, and save
/// its frame with `COSTS_CALENDAR_SNAP`.
fn open_year_picker(ui: &mut Headless, turns: usize) {
    for _ in 0..turns {
        ui.scroll_start();
        for _ in 0..6 {
            ui.scroll(-8.0, 0.0);
            ui.advance(8);
            let _ = ui.paint_timed(true);
        }
        ui.scroll_end();
        for _ in 0..6 {
            ui.advance(8);
            let _ = ui.paint_timed(true);
        }
        ui.scroll_momentum_end();
    }
    rest(ui);
    // `COSTS_CALENDAR_DROPDOWN_REST=<n>`: rest n times as long first, as a window left alone.
    let rests: usize = std::env::var("COSTS_CALENDAR_DROPDOWN_REST")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    for _ in 0..rests {
        rest(ui);
    }
    let picker = calendar_node(ui, "cn-calendar-year-picker").expect("year picker");
    let b = ui
        .tree()
        .get_absolute_bounds(picker)
        .expect("picker bounds");
    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    println!(
        "caption {:?}, clicking the year picker at ({x:.0}, {y:.0})",
        caption(ui)
    );
    // `COSTS_CALENDAR_DROPDOWN_OPENS=<n>`: open it n times, closing it in between.
    let opens: usize = std::env::var("COSTS_CALENDAR_DROPDOWN_OPENS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .max(1);
    // `COSTS_CALENDAR_DROPDOWN_CLOSE=outside|pick`: close it by a click outside, or on a year,
    // instead of on the trigger.
    let close = std::env::var("COSTS_CALENDAR_DROPDOWN_CLOSE").unwrap_or_default();
    for i in 0..opens * 2 - 1 {
        let (x, y) = match (i % 2, close.as_str()) {
            (1, "outside") => (x + 300.0, y - 60.0),
            (1, "pick") => (x, y + 120.0),
            _ => (x, y),
        };
        ui.move_to(x, y);
        ui.mouse_down();
        ui.advance(16);
        ui.mouse_up();
        for _ in 0..40 {
            ui.advance(16);
            let _ = ui.paint_timed(true);
        }
    }
    capture(ui, "dropdown");
}

fn caption(ui: &Headless) -> Vec<String> {
    let tree = ui.tree();
    let mut pending: Vec<_> = calendar_node(ui, "cn-calendar-caption")
        .into_iter()
        .collect();
    let mut text = Vec::new();
    while let Some(node) = pending.pop() {
        pending.extend(tree.layout_tree.children(node).into_iter().rev());
        if let Some(render) = tree.get_render_node(node)
            && let blinc_layout::renderer::ElementType::Text(data) = &render.element_type
        {
            text.push(data.content.clone());
        }
    }
    text
}

fn capture(ui: &mut Headless, phase: &str) {
    let Ok(prefix) = std::env::var("COSTS_CALENDAR_SNAP") else {
        return;
    };
    let frame = ui.snapshot_fast().expect("cached frame");
    blinc_recorder::ScreenshotExporter::save_png(&frame, format!("{prefix}-{phase}.png"))
        .expect("saved");
    let full = ui.snapshot().expect("full paint of the same pose");
    blinc_recorder::ScreenshotExporter::save_png(&full, format!("{prefix}-{phase}-full.png"))
        .expect("saved");
}

pub(super) fn run(ui: &mut Headless, log: PathLog, cycles: usize) {
    let phase = std::env::var("COSTS_CALENDAR_PHASE").unwrap_or_else(|_| "all".into());
    assert!(matches!(phase.as_str(), "all" | "rest" | "pull" | "turn"));
    assert!(bring_into_view(ui, ".cn-calendar"), "calendar in view");
    ui.scroll_end().scroll_momentum_end();
    rest(ui);
    let node = calendar_node(ui, "cn-calendar-months").expect("month viewport");
    let bounds = ui.tree().get_absolute_bounds(node).expect("month bounds");
    ui.move_to(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    rest(ui);
    println!("calendar at {bounds:?}, caption {:?}", caption(ui));

    // `COSTS_CALENDAR_TOAST=1`: show persistent toasts in the corners near the calendar and
    // save the frame with `COSTS_CALENDAR_SNAP`.
    if std::env::var_os("COSTS_CALENDAR_TOAST").is_some() {
        use blinc_layout::widgets::overlay::Corner;
        for corner in [Corner::BottomLeft, Corner::BottomRight, Corner::TopLeft] {
            for title in ["First Toast", "Second Toast", "Third Toast"] {
                blinc_cn::prelude::toast(title)
                    .description("A toast above the calendar.")
                    .corner(corner)
                    .persistent()
                    .show();
            }
        }
        rest(ui);
        rest(ui);
        capture(ui, "toast");
        return;
    }
    if let Ok(turns) = std::env::var("COSTS_CALENDAR_DROPDOWN") {
        open_year_picker(ui, turns.parse().unwrap_or(0));
        return;
    }

    if matches!(phase.as_str(), "all" | "rest") {
        let mut samples = Samples::new(ui, log.clone());
        let frames = if phase == "rest" { cycles * 16 } else { 60 };
        for _ in 0..frames {
            samples.frame(|_| {});
        }
        samples.report("calendar at rest");
    }

    if matches!(phase.as_str(), "all" | "pull") {
        let mut samples = Samples::new(ui, log.clone());
        let before = caption(samples.scene.ui);
        for cycle in 0..cycles {
            samples.scene.ui.scroll_start();
            for frame in 0..16 {
                let dx = if frame < 8 { -4.0 } else { 4.0 };
                samples.frame(|ui| {
                    ui.scroll(dx, 0.0);
                });
                if cycle == 0 && frame == 7 {
                    capture(samples.scene.ui, "pull");
                }
            }
            samples.scene.ui.scroll_end().scroll_momentum_end();
        }
        assert_eq!(
            caption(samples.scene.ui),
            before,
            "a short pull keeps its month"
        );
        samples.report("calendar pull, no month turn");
    }

    if matches!(phase.as_str(), "all" | "turn") {
        let frames = std::env::var("COSTS_CALENDAR_FRAMES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(36)
            .max(7);
        let mut samples = Samples::new(ui, log);
        for cycle in 0..cycles {
            let before = caption(samples.scene.ui);
            for frame in 0..frames {
                samples.frame(|ui| {
                    if frame == 0 {
                        ui.scroll_start();
                    }
                    if frame < 6 {
                        ui.scroll(-8.0, 0.0);
                    }
                    if frame == 6 {
                        ui.scroll_end();
                    }
                });
                if cycle == 0 && frame == 9 {
                    capture(samples.scene.ui, "turn");
                } else if cycle == 0 && std::env::var_os("COSTS_CALENDAR_SNAP_ALL").is_some() {
                    capture(samples.scene.ui, &format!("turn{frame}"));
                }
            }
            samples.scene.ui.scroll_momentum_end();
            assert_ne!(
                caption(samples.scene.ui),
                before,
                "each swipe turns a month"
            );
        }
        samples.report("calendar continuous month turns");
    }
    ui.scroll_end().scroll_momentum_end().settle();
    rest(ui);
    capture(ui, "settled");
    println!("calendar ends at {:?}", caption(ui));
}
