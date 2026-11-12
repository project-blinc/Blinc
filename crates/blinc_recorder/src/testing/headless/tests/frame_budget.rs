//! What a frame costs, by stage, in the scenes the GPU-deferral plan is about. A measurement,
//! not a check, so it is ignored by default:
//!
//! ```text
//! cargo test -p blinc_recorder --features headless --lib frame_budget -- --ignored --nocapture
//! ```
//!
//! Each frame is timed in four parts. `input` is what a window would have delivered before the
//! frame (a scroll, a resize). `update` is the session's frame: the scheduler tick, the
//! animation integrators, style and layout. `render` is the CPU time inside the paint call, where
//! the tree is walked, elements are collected, the batch is patched or rebuilt, uploaded and
//! encoded. `gpu` is the time from the paint call's return until the device reports idle, less
//! the time an empty submission takes to report idle (the floor, printed in the header). It is
//! queue latency plus the GPU's own time, only meaningful on a hardware adapter, and cannot
//! resolve work shorter than the floor's noise; a timeline trace is the tool for that. The paint
//! call's own breakdown (`walker`, `collect`, `encode`, the rest of the call) and the fast path
//! it took come from the renderer's trace events.
//!
//! Frames are 16 ms of session time. The session does not pace itself, so these are costs per
//! frame, not frame rates. `moves` says whether the first and last timed frames differ: a scene
//! that draws the same pixels throughout is not animating, whatever its name says.

use super::super::Headless;
use blinc_core::Color;
use blinc_core::reactive::{Signal, signal};
use blinc_layout::div::{Div, div};
use blinc_layout::prelude::*;
use blinc_layout::widgets::scroll::{Scroll, ScrollPhysics};
use blinc_theme::{ColorScheme, HybridTheme};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

const W: u32 = 1280;
const H: u32 = 720;
const FRAME_MS: u32 = 16;
const WARMUP: usize = 6;

/// What the renderer's trace events said about one frame.
#[derive(Default, Clone)]
struct Captured {
    walker: Option<u64>,
    collect: Option<u64>,
    encode: Option<u64>,
    path: Option<String>,
}

/// Catches the renderer's frame-timing events.
#[derive(Clone, Default)]
struct Collector(Arc<Mutex<Captured>>);

impl Subscriber for Collector {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.target() == "blinc_app::frame_timing"
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        #[derive(Default)]
        struct Fields {
            message: String,
            walker: Option<u64>,
            collect: Option<u64>,
            gpu: Option<u64>,
            path: Option<String>,
        }
        impl Visit for Fields {
            fn record_u64(&mut self, field: &Field, value: u64) {
                match field.name() {
                    "paint_walker_us" => self.walker = Some(value),
                    "collect_elements_us" => self.collect = Some(value),
                    "gpu_us" => self.gpu = Some(value),
                    _ => {}
                }
            }
            fn record_str(&mut self, field: &Field, value: &str) {
                match field.name() {
                    "path" => self.path = Some(value.to_string()),
                    "message" => self.message = value.to_string(),
                    _ => {}
                }
            }
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                let text = format!("{value:?}");
                self.record_str(field, text.trim_matches('"'));
            }
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let mut captured = self.0.lock().unwrap();
        match fields.message.as_str() {
            "p4_breakdown" => {
                captured.walker = fields.walker;
                captured.collect = fields.collect;
                captured.encode = fields.gpu;
            }
            "fast_path" => {
                captured.path = fields.path;
                captured.encode = fields.gpu;
            }
            _ => {}
        }
    }
}

struct Sample {
    input: Duration,
    update: Duration,
    render: Duration,
    gpu: Duration,
    captured: Captured,
}

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W, H).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.load_font_data(
        include_bytes!(
            "../../../../../../examples/blinc_component_demos/assets/NotoSans-Regular.ttf"
        )
        .to_vec(),
    );
    Some(ui)
}

/// What an empty submission takes to report idle: the part of `gpu` that is not work.
fn idle_floor(ui: &Headless) -> Duration {
    let mut waits: Vec<Duration> = (0..40)
        .map(|_| {
            let start = Instant::now();
            ui.app.queue().submit([]);
            let _ = ui.app.device().poll(wgpu::PollType::Wait);
            start.elapsed()
        })
        .collect();
    waits.sort_unstable();
    waits[waits.len() / 2]
}

/// One frame: `drive` (the input a window would have delivered), the session's frame, then the
/// paint the way a window paints it, then a wait for the device.
fn one_frame(
    ui: &mut Headless,
    collector: &Collector,
    floor: Duration,
    drive: &mut dyn FnMut(&mut Headless),
) -> Sample {
    *collector.0.lock().unwrap() = Captured::default();
    let start = Instant::now();
    drive(ui);
    let driven = Instant::now();
    ui.frame(FRAME_MS);
    let updated = Instant::now();
    let tree = ui.tree.as_ref().expect("nothing is mounted");
    ui.app
        .render_tree_with_motion_opt(
            tree,
            &ui.render_state,
            &ui.view,
            Some(&ui.texture),
            ui.width,
            ui.height,
            true,
        )
        .expect("paint");
    let rendered = Instant::now();
    let _ = ui.app.device().poll(wgpu::PollType::Wait);
    let done = Instant::now();
    Sample {
        input: driven - start,
        update: updated - driven,
        render: rendered - updated,
        gpu: (done - rendered).saturating_sub(floor),
        captured: collector.0.lock().unwrap().clone(),
    }
}

fn pick(values: &mut [u64], percentile: f64) -> u64 {
    values.sort_unstable();
    if values.is_empty() {
        return 0;
    }
    let at = ((values.len() - 1) as f64 * percentile).round() as usize;
    values[at]
}

fn ms(us: u64) -> String {
    format!("{:6.2}", us as f64 / 1000.0)
}

/// `median / p95` in milliseconds.
fn cell(mut values: Vec<u64>) -> String {
    if values.is_empty() {
        return format!("{:>15}", "-");
    }
    let median = pick(&mut values, 0.5);
    let p95 = pick(&mut values, 0.95);
    format!("{} /{}", ms(median), ms(p95))
}

fn header(ui: &Headless, floor: Duration) {
    let info = ui.app.adapter().get_info();
    println!();
    println!(
        "frame budget: {} ({:?}), {W}x{H}, ms per frame as median / p95; gpu is the wait for idle \
         less the {:.2} ms an empty submission takes",
        info.name,
        info.backend,
        floor.as_secs_f64() * 1000.0
    );
    println!(
        "{:<28}{:>6} {:>15} | {:>15} | {:>15} | {:>15} | {:>15} | {:>15} | {:>15} | {:>15} | moves | fast path taken",
        "scene",
        "nodes",
        "input",
        "update",
        "render cpu",
        "gpu",
        "paint walker",
        "collect",
        "encode (rest)",
        "total cpu"
    );
}

fn report(name: &str, nodes: usize, moves: bool, samples: &[Sample]) {
    let us =
        |f: &dyn Fn(&Sample) -> Option<u64>| -> Vec<u64> { samples.iter().filter_map(f).collect() };
    let mut paths: Vec<(String, usize)> = Vec::new();
    for sample in samples {
        let label = sample
            .captured
            .path
            .clone()
            .unwrap_or_else(|| "full".to_string());
        match paths.iter_mut().find(|(p, _)| *p == label) {
            Some((_, n)) => *n += 1,
            None => paths.push((label, 1)),
        }
    }
    let paths = paths
        .iter()
        .map(|(p, n)| format!("{p}:{n}"))
        .collect::<Vec<_>>()
        .join(" ");
    println!(
        "{name:<28}{nodes:>6} {} | {} | {} | {} | {} | {} | {} | {} | {:^5} | {paths}",
        cell(us(&|s| Some(s.input.as_micros() as u64))),
        cell(us(&|s| Some(s.update.as_micros() as u64))),
        cell(us(&|s| Some(s.render.as_micros() as u64))),
        cell(us(&|s| Some(s.gpu.as_micros() as u64))),
        cell(us(&|s| s.captured.walker)),
        cell(us(&|s| s.captured.collect)),
        cell(us(&|s| s.captured.encode)),
        cell(us(&|s| Some(
            (s.input + s.update + s.render).as_micros() as u64
        ))),
        if moves { "yes" } else { "NO" },
    );
}

/// Run a scene: warm up, then time `frames` frames. `drive` gets the index of the timed frame.
fn run(
    name: &str,
    ui: &mut Headless,
    floor: Duration,
    frames: usize,
    mut drive: impl FnMut(&mut Headless, usize),
) {
    let collector = Collector::default();
    let _guard = tracing::subscriber::set_default(collector.clone());
    for _ in 0..WARMUP {
        one_frame(ui, &collector, floor, &mut |_| {});
    }
    let mut samples = Vec::with_capacity(frames);
    let mut first = None;
    for i in 0..frames {
        samples.push(one_frame(ui, &collector, floor, &mut |ui| drive(ui, i)));
        if i == 0 {
            first = ui.read_frame().ok();
        }
    }
    let last = ui.read_frame().ok();
    let moves = match (first, last) {
        (Some(a), Some(b)) => a.data != b.data,
        _ => false,
    };
    report(name, ui.tree().layout().len(), moves, &samples);
}

/// A wheel event under the pointer, delivered as the window delivers it. `Headless::scroll` runs
/// a whole frame after it, which a window does not do in its input handler.
fn wheel(ui: &mut Headless, delta_y: f32) {
    let (x, y) = ui.pointer;
    if let Some(tree) = ui.tree.as_mut()
        && let Some(hit) = ui.ctx.event_router.hit_test(tree, x, y)
    {
        tree.dispatch_scroll_chain(hit.node, &hit.ancestors, x, y, 0.0, delta_y);
    }
    // A scroll invalidates the paint cache, as the runner does.
    ui.app.invalidate_render_cache_tagged("had_scroll");
}

fn cards(count: usize, lift: Signal<bool>, with_text: bool) -> Div {
    let edge = Color::rgba(0.3, 0.35, 0.5, 0.8);
    (0..count).fold(
        div()
            .w_full()
            .h_full()
            .flex_row()
            .flex_wrap()
            .content_start()
            .gap(1.0)
            .p(2.0),
        |grid, i| {
            let tint = (i % 7) as f32 / 7.0;
            let card = div()
                .class("card")
                .class_when("lift", lift)
                .w(96.0)
                .h(56.0)
                .rounded(8.0)
                .bg(Color::rgba(0.15 + tint * 0.3, 0.3, 0.6 - tint * 0.3, 1.0))
                .border(1.0, edge)
                .items_center()
                .justify_center();
            grid.child(if with_text {
                card.child(text(format!("Card {i}")).size(12.0).color(Color::WHITE))
            } else {
                card
            })
        },
    )
}

const CARD_CSS: &str = "
.card { transition: transform 300ms ease-out, opacity 300ms ease-out; }
.card.lift { transform: translateY(-6px) scale(1.04); opacity: 0.85; }
";

/// Cards ease transform and opacity through CSS transitions. The transition is started before the
/// warm-up, so the timed frames are all inside it: 6 warm-up and 12 timed frames are 288 ms of
/// the 300.
fn card_scene(ui: &mut Headless, floor: Duration, name: &str, with_text: bool) {
    let lift = signal(false);
    ui.css(CARD_CSS);
    ui.mount(move |_| cards(200, lift, with_text)).settle();
    lift.set(true);
    ui.frame(FRAME_MS);
    assert!(ui.busy(), "the cards are not animating");
    run(name, ui, floor, 12, |_, _| {});
}

const CARD_ANIMATION_CSS: &str = "
@keyframes bob {
  from { transform: translateY(0px) scale(1); opacity: 1; }
  to { transform: translateY(-6px) scale(1.04); opacity: 0.8; }
}
.card { animation: bob 800ms ease-in-out infinite alternate; }
";

/// The same cards under a looping CSS keyframe animation, as the phone scene runs them.
fn card_animation_scene(ui: &mut Headless, floor: Duration, name: &str, with_text: bool) {
    let lift = signal(false);
    ui.css(CARD_ANIMATION_CSS);
    ui.mount(move |_| cards(200, lift, with_text)).settle();
    run(name, ui, floor, 12, |_, _| {});
}

#[test]
#[ignore = "a measurement, not a check"]
fn frame_budget() {
    let Some(mut ui) = session() else { return };
    let floor = idle_floor(&ui);
    header(&ui, floor);

    card_scene(&mut ui, floor, "200 cards, css transition", true);
    card_scene(&mut ui, floor, "200 cards, no text", false);
    card_animation_scene(&mut ui, floor, "200 cards, css animation", true);
    card_animation_scene(&mut ui, floor, "200 cards, animation, no text", false);

    // A long list, scrolled steadily, then flung.
    {
        let physics = Arc::new(Mutex::new(ScrollPhysics::default()));
        let held = Arc::clone(&physics);
        ui.css("");
        ui.mount(move |_| {
            let mut content = div().w_full().flex_col();
            for i in 0..2_000 {
                content = content.child(
                    div()
                        .w_full()
                        .h(32.0)
                        .flex_shrink_0()
                        .flex_row()
                        .items_center()
                        .px(3.0)
                        .bg(Color::rgb(0.1 + (i % 5) as f32 * 0.02, 0.12, 0.16))
                        .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE)),
                );
            }
            div().w(W as f32).h(H as f32).child(
                Scroll::with_physics(Arc::clone(&held))
                    .w(W as f32)
                    .h(H as f32)
                    .child(content),
            )
        })
        .settle();
        ui.move_to(640.0, 360.0);
        ui.scroll_start();
        run("list, steady scroll", &mut ui, floor, 40, |ui, _| {
            wheel(ui, -24.0)
        });
        // A trackpad fling: after the fingers lift, the OS sends the momentum as wheel deltas
        // that decay. A touch fling is physics the runner integrates (not measured here: the
        // session has no touch input).
        ui.scroll_end();
        run("list, trackpad fling", &mut ui, floor, 40, |ui, i| {
            wheel(ui, -90.0 * 0.93f32.powi(i as i32))
        });
    }

    // Three spinners on an otherwise idle window.
    ui.mount(|_| {
        div()
            .w_full()
            .h_full()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(8.0)
            .child(blinc_cn::spinner())
            .child(blinc_cn::spinner())
            .child(blinc_cn::spinner())
    })
    .settle();
    run("3 spinners, otherwise idle", &mut ui, floor, 40, |_, _| {});

    // A live resize: the runner rebuilds the whole tree on each size.
    {
        let lift = signal(false);
        ui.css(CARD_CSS);
        ui.mount(move |_| cards(200, lift, true)).settle();
        run("resize, full rebuild", &mut ui, floor, 24, |ui, i| {
            let step = if i % 2 == 0 { 0.0 } else { 8.0 };
            ui.ctx.width = W as f32 - step;
            ui.rebuild_fresh();
        });
        ui.ctx.width = W as f32;
    }

    // A flow shader that reads time, in a canvas.
    {
        let shader = blinc_layout::flow_shader(
            "@flow m { target: fragment; input time: builtin(time); input uv: builtin(uv); \
             output color = vec4(sin(time) * 0.5 + 0.5, uv.x, uv.y, 1.0); }",
        )
        .expect("flow shader");
        ui.css("");
        ui.mount(move |_| {
            div()
                .w_full()
                .h_full()
                .child(canvas(|_, _| {}).flow(shader.clone()).w(640.0).h(360.0))
        })
        .settle();
        run("flow shader canvas", &mut ui, floor, 40, |_, _| {});
    }
}
