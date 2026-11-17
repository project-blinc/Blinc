//! Script the real page's calendar without driving a desktop window.
//!
//! `COSTS_CALENDAR=200 COSTS_CALENDAR_PHASE=pull` measures movement without month creation;
//! `turn` includes month creation and its slide, and `rest` measures the same viewport idle.
//! Omit the phase for all three. `COSTS_NO_TRACE=1` removes path tracing while profiling.
//! `COSTS_CALENDAR_SNAP=<prefix>` saves cached and full-paint references separately from timings.

use super::{Headless, PathLog, Scene, bring_into_view, rest};
use std::time::Instant;

struct Samples<'a> {
    scene: Scene<'a>,
    input: Vec<f64>,
    total: Vec<f64>,
}

impl<'a> Samples<'a> {
    fn new(ui: &'a mut Headless, log: PathLog) -> Self {
        Self {
            scene: Scene::new(ui, log),
            input: Vec::new(),
            total: Vec::new(),
        }
    }

    fn frame(&mut self, input: impl FnOnce(&mut Headless)) {
        let start = Instant::now();
        input(self.scene.ui);
        self.input.push(start.elapsed().as_secs_f64() * 1000.0);
        self.scene.step();
        self.total.push(start.elapsed().as_secs_f64() * 1000.0);
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
    }
}

fn caption(ui: &Headless) -> Vec<String> {
    let tree = ui.tree();
    let mut pending: Vec<_> = ui.find(".cn-calendar-caption").into_iter().collect();
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
    let node = ui.find(".cn-calendar-months").expect("month viewport");
    let bounds = ui.tree().get_absolute_bounds(node).expect("month bounds");
    ui.move_to(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    rest(ui);
    println!("calendar at {bounds:?}, caption {:?}", caption(ui));

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
        let mut samples = Samples::new(ui, log);
        for cycle in 0..cycles {
            let before = caption(samples.scene.ui);
            for frame in 0..36 {
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
    capture(ui, "settled");
    println!("calendar ends at {:?}", caption(ui));
}
