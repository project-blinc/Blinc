#![cfg(feature = "headless")]
//! Animated scenes painted two ways, frame by frame, and compared.
//!
//! The reference is a full paint every frame: the tree walked with the animated values as they are
//! at that frame, which is right by construction. The other is the path a window takes when only
//! animated values have changed: the cached batch patched (`apply_binding_deltas` for motion
//! bindings, `apply_css_deltas` for CSS animations and transitions) and blitted. Each path replays
//! the same scripted timeline in a session of its own, so the animation state at a step is the
//! same in both. Bound values are driven by spring targets, as in a window; a value written in
//! place is on its target and the compositor sees nothing animating. The path each frame took is
//! read from the frame timing events.
//!
//! A new way of painting animation (the node table) is a third `Path`: it is held to the
//! reference, not to the patched path, whose differences are listed below as known.
//!
//! `BLINC_ANIM_PATHS_OUT=<dir>` writes a contact sheet for each scene: the frames that matter,
//! each as reference, painted path and difference (red where they differ).

use blinc_animation::AnimatedValue;
use blinc_core::reactive::signal;
use blinc_core::{Affine2D, Color, Transform};
use blinc_layout::div::{Div, div};
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_recorder::testing::{CapturedFrame, ScreenshotExporter, diff_image, difference};
use blinc_theme::{ColorScheme, HybridTheme};
use std::sync::{Arc, Mutex};

const W: u32 = 320;
const H: u32 = 240;

/// How a frame of an animation is painted.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Path {
    /// A full paint every frame: the reference.
    Full,
    /// As a window paints a frame in which only animated values changed.
    Patched,
}

/// What a scene does at each step before the frame is painted.
type Script = Box<dyn FnMut(&mut Headless, usize)>;

struct Scene {
    name: &'static str,
    steps: usize,
    step_ms: u32,
    build: Box<dyn Fn(&mut Headless) -> Script>,
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
        include_bytes!("../../../examples/blinc_component_demos/assets/NotoSans-Regular.ttf")
            .to_vec(),
    );
    Some(ui)
}

/// Which way each frame was painted, read from the frame timing events the renderer emits.
#[derive(Clone, Default)]
struct PathLog(Arc<Mutex<Vec<String>>>);

impl tracing::Subscriber for PathLog {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.target() == "blinc_app::frame_timing"
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        #[derive(Default)]
        struct Fields {
            message: String,
            path: String,
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                let text = format!("{value:?}").trim_matches('"').to_string();
                match field.name() {
                    "message" => self.message = text,
                    "path" => self.path = text,
                    _ => {}
                }
            }
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        match fields.message.as_str() {
            "fast_path" => self.0.lock().unwrap().push(fields.path),
            "p4_breakdown" => self.0.lock().unwrap().push("full".into()),
            _ => {}
        }
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

/// Run `scene` with every frame painted by `path`.
fn run(scene: &Scene, path: Path) -> Option<Vec<CapturedFrame>> {
    run_logged(scene, path).map(|(frames, _)| frames)
}

/// `run`, and how each frame was painted: `full`, or the fast path it took.
fn run_logged(scene: &Scene, path: Path) -> Option<(Vec<CapturedFrame>, Vec<String>)> {
    let log = PathLog::default();
    let _guard = tracing::subscriber::set_default(log.clone());
    let mut ui = session()?;
    let mut script = (scene.build)(&mut ui);
    ui.settle();
    // The cache the patched path patches comes from a full paint of the scene at rest.
    ui.snapshot().expect("a frame");
    log.0.lock().unwrap().clear();
    let mut frames = Vec::new();
    let mut paths = Vec::new();
    for step in 0..scene.steps {
        script(&mut ui, step);
        ui.advance(scene.step_ms);
        let frame = match path {
            Path::Full => ui.snapshot(),
            Path::Patched => ui.snapshot_fast(),
        };
        frames.push(frame.expect("a frame"));
        // One event per painted frame; keep the last, which is the frame just painted.
        let mut seen = log.0.lock().unwrap();
        paths.push(seen.last().cloned().unwrap_or_default());
        seen.clear();
    }
    Some((frames, paths))
}

/// A motion-bound value, held by the session's scheduler.
fn bound(ui: &mut Headless, initial: f32) -> Arc<Mutex<AnimatedValue>> {
    let handle = ui.context().animations.lock().unwrap().handle();
    Arc::new(Mutex::new(AnimatedValue::with_default(handle, initial)))
}

/// A card with a border, a shadow and, if asked, a label: what animated UI mostly is.
fn card(label: bool) -> Div {
    let card = div()
        .w(120.0)
        .h(64.0)
        .rounded(14.0)
        .bg(Color::rgb(0.2, 0.45, 0.9))
        .border(3.0, Color::rgba(1.0, 1.0, 1.0, 0.9))
        .shadow_params(0.0, 6.0, 12.0, Color::rgba(0.0, 0.0, 0.0, 0.5))
        .flex_row()
        .items_center()
        .justify_center();
    if label {
        card.child(text("Card").size(18.0).color(Color::WHITE))
    } else {
        card
    }
}

fn stage() -> Div {
    div()
        .w(W as f32)
        .h(H as f32)
        .bg(Color::rgb(0.09, 0.1, 0.13))
        .flex_row()
        .items_center()
        .justify_center()
}

#[derive(Clone, Copy, Default)]
struct Channels {
    translate: bool,
    scale: bool,
    rotate: bool,
    fade: bool,
    /// Drive the values as a window does: set a target and let the spring follow, so the value is
    /// away from its target while it moves. Otherwise they are written in place.
    spring: bool,
}

/// A card under a motion container, its bound values scripted from rest to a far pose.
fn motion_scene(name: &'static str, channels: Channels, label: bool, steps: usize) -> Scene {
    Scene {
        name,
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let (x, y) = (bound(ui, 0.0), bound(ui, 0.0));
            let (scale, rotation, opacity) = (bound(ui, 1.0), bound(ui, 0.0), bound(ui, 1.0));
            let handles = (
                x.clone(),
                y.clone(),
                scale.clone(),
                rotation.clone(),
                opacity.clone(),
            );
            ui.mount(move |_| {
                // The motion container fills its parent, so the parent is the card's size and the
                // pivot is the card's own centre.
                let mut motion = card(label).motion();
                if channels.translate {
                    motion = motion.translate_x(x.clone()).translate_y(y.clone());
                }
                if channels.scale {
                    motion = motion.scale(scale.clone());
                }
                if channels.rotate {
                    motion = motion.rotate(rotation.clone());
                }
                if channels.fade {
                    motion = motion.opacity(opacity.clone());
                }
                stage().child(div().w(120.0).h(64.0).child(motion))
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                let set = |v: &Arc<Mutex<AnimatedValue>>, value: f32| {
                    let mut v = v.lock().unwrap();
                    if channels.spring {
                        v.set_target(value)
                    } else {
                        v.set_immediate(value)
                    }
                };
                set(&handles.0, 60.0 * t);
                set(&handles.1, -30.0 * t);
                set(&handles.2, 1.0 + 0.4 * t);
                set(&handles.3, 35.0 * t);
                set(&handles.4, 1.0 - 0.6 * t);
            })
        }),
    }
}

/// A motion container inside another: the outer one moves, the inner one scales and turns about
/// its own centre, which the outer one has moved.
fn nested_motion_scene(label: bool, spring: bool) -> Scene {
    let steps = 12;
    Scene {
        name: if label {
            "motion nested"
        } else {
            "motion nested, no text"
        },
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let outer_x = bound(ui, 0.0);
            let (scale, rotation) = (bound(ui, 1.0), bound(ui, 0.0));
            let handles = (outer_x.clone(), scale.clone(), rotation.clone());
            ui.mount(move |_| {
                let inner = card(label)
                    .motion()
                    .scale(scale.clone())
                    .rotate(rotation.clone());
                let outer = div()
                    .w(120.0)
                    .h(64.0)
                    .child(inner)
                    .motion()
                    .translate_x(outer_x.clone());
                stage().child(div().w(120.0).h(64.0).child(outer))
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                for (handle, value) in [
                    (&handles.0, 60.0 * t),
                    (&handles.1, 1.0 + 0.25 * t),
                    (&handles.2, 20.0 * t),
                ] {
                    let mut handle = handle.lock().unwrap();
                    if spring {
                        handle.set_target(value)
                    } else {
                        handle.set_immediate(value)
                    }
                }
            })
        }),
    }
}

/// Two overlapping children under one fading container. The group fades as one, so where they
/// overlap the second does not show the first through it.
fn overlap_fade_scene() -> Scene {
    let steps = 12;
    Scene {
        name: "motion fade, overlapping",
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let opacity = bound(ui, 1.0);
            let handle = opacity.clone();
            ui.mount(move |_| {
                let group = div()
                    .w(120.0)
                    .h(64.0)
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .left(0.0)
                            .top(0.0)
                            .w(84.0)
                            .h(50.0)
                            .rounded(10.0)
                            .bg(Color::rgb(0.9, 0.25, 0.2)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(36.0)
                            .top(14.0)
                            .w(84.0)
                            .h(50.0)
                            .rounded(10.0)
                            .bg(Color::rgb(0.2, 0.75, 0.35)),
                    )
                    .motion()
                    .opacity(opacity.clone());
                stage().child(div().w(120.0).h(64.0).child(group))
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                handle.lock().unwrap().set_target(1.0 - 0.7 * t);
            })
        }),
    }
}

const TRANSITION_CSS: &str = "
.card { transition: transform 300ms ease-out, opacity 300ms ease-out; }
.card.lift { transform: translateY(-14px) scale(1.15) rotate(8deg); opacity: 0.6; }
";

/// Cards under a CSS transition of transform and opacity.
fn transition_scene(name: &'static str, label: bool) -> Scene {
    Scene {
        name,
        steps: 20,
        step_ms: 16,
        build: Box::new(move |ui| {
            let lift = signal(false);
            ui.css(TRANSITION_CSS);
            ui.mount(move |_| {
                (0..2).fold(stage().gap(6.0), |row, _| {
                    row.child(card(label).class("card").class_when("lift", lift))
                })
            });
            Box::new(move |_, step| {
                if step == 0 {
                    lift.set(true);
                }
            })
        }),
    }
}

const KEYFRAME_CSS: &str = "
@keyframes bob {
  from { transform: translateY(0px) scale(1) rotate(0deg); opacity: 1; }
  to { transform: translateY(-14px) scale(1.15) rotate(8deg); opacity: 0.6; }
}
.card { animation: bob 320ms ease-in-out infinite alternate; }
";

/// Cards under a CSS keyframe animation.
fn keyframe_scene(name: &'static str, label: bool) -> Scene {
    Scene {
        name,
        steps: 20,
        step_ms: 16,
        build: Box::new(move |ui| {
            ui.css(KEYFRAME_CSS);
            ui.mount(move |_| {
                (0..2).fold(stage().gap(6.0), |row, _| {
                    row.child(card(label).class("card"))
                })
            });
            Box::new(|_, _| {})
        }),
    }
}

fn scenes() -> Vec<Scene> {
    let all = Channels {
        translate: true,
        scale: true,
        rotate: true,
        fade: true,
        spring: true,
    };
    let one = |name, channel: fn(&mut Channels), label| {
        let mut channels = Channels {
            spring: true,
            ..Default::default()
        };
        channel(&mut channels);
        motion_scene(name, channels, label, 12)
    };
    vec![
        one("motion translate", |c| c.translate = true, true),
        one("motion translate, no text", |c| c.translate = true, false),
        one("motion scale", |c| c.scale = true, true),
        one("motion scale, no text", |c| c.scale = true, false),
        one("motion rotate", |c| c.rotate = true, true),
        one("motion rotate, no text", |c| c.rotate = true, false),
        one("motion fade", |c| c.fade = true, true),
        one("motion fade, no text", |c| c.fade = true, false),
        motion_scene("motion all", all, true, 12),
        motion_scene("motion all, no text", all, false, 12),
        nested_motion_scene(true, true),
        nested_motion_scene(false, true),
        overlap_fade_scene(),
        transition_scene("css transition", true),
        transition_scene("css transition, no text", false),
        keyframe_scene("css keyframes", true),
        keyframe_scene("css keyframes, no text", false),
    ]
}

/// A pixel differs when a channel is further than this from the reference: edge rounding is not a
/// difference.
const LEVELS: u8 = 2;

/// Scenes in which the patched path is known to differ from a full paint. They are listed so the
/// rest are held to the reference; a scene that stops differing is an error until it is taken off
/// the list. A label stays where the last full paint put it while a bound value moves, and an
/// outer binding moving an inner one leaves the inner box and its shadow in the wrong place.
const KNOWN_DIFFERENT: &[&str] = &[
    "motion translate",
    "motion scale",
    "motion rotate",
    "motion nested",
    "motion nested, no text",
];

struct Report {
    name: &'static str,
    steps: usize,
    /// Per step: pixels that differ by more than `LEVELS`, and the largest channel difference.
    per_step: Vec<(usize, u8)>,
}

impl Report {
    fn worst(&self) -> (usize, (usize, u8)) {
        self.per_step
            .iter()
            .copied()
            .enumerate()
            .max_by_key(|(_, (pixels, channel))| (*pixels, *channel))
            .unwrap()
    }

    fn differing_steps(&self) -> usize {
        self.per_step
            .iter()
            .filter(|(pixels, _)| *pixels > 0)
            .count()
    }
}

/// The frames that matter on a sheet: the first, the last and the worst, and what lies between.
fn sheet_steps(report: &Report) -> Vec<usize> {
    let mut steps = vec![0, report.steps / 3, 2 * report.steps / 3, report.steps - 1];
    steps.push(report.worst().0);
    steps.sort_unstable();
    steps.dedup();
    steps
}

/// Reference, painted path and difference side by side, one row for each of `steps`.
fn sheet(reference: &[CapturedFrame], painted: &[CapturedFrame], steps: &[usize]) -> CapturedFrame {
    let (w, h) = (reference[0].width as usize, reference[0].height as usize);
    let mut data = vec![255u8; w * 3 * h * steps.len() * 4];
    for (row, &step) in steps.iter().enumerate() {
        let diff = diff_image(&reference[step], &painted[step], LEVELS);
        for (column, frame) in [&reference[step], &painted[step], &diff]
            .into_iter()
            .enumerate()
        {
            for y in 0..h {
                let from = y * w * 4;
                let to = ((row * h + y) * w * 3 + column * w) * 4;
                data[to..to + w * 4].copy_from_slice(&frame.data[from..from + w * 4]);
            }
        }
    }
    CapturedFrame::new(data, (w * 3) as u32, (h * steps.len()) as u32)
}

fn compare(scene: &Scene) -> Option<(Report, Vec<CapturedFrame>, Vec<CapturedFrame>)> {
    let reference = run(scene, Path::Full)?;
    let (painted, paths) = run_logged(scene, Path::Patched)?;
    let mut tally = std::collections::BTreeMap::<&str, usize>::new();
    for path in &paths {
        *tally.entry(path.as_str()).or_default() += 1;
    }
    eprintln!("{:<26} patched path took {tally:?}", scene.name);
    let per_step = reference
        .iter()
        .zip(&painted)
        .map(|(a, b)| {
            let d = difference(a, b, LEVELS);
            (d.differing, d.max_channel)
        })
        .collect();
    Some((
        Report {
            name: scene.name,
            steps: scene.steps,
            per_step,
        },
        reference,
        painted,
    ))
}

/// Paint every scene both ways and report how the patched path differs from the reference.
#[test]
fn the_patched_path_against_the_reference() {
    let mut reports = Vec::new();
    for scene in scenes() {
        let Some((report, reference, painted)) = compare(&scene) else {
            return;
        };
        let (step, (pixels, channel)) = report.worst();
        eprintln!(
            "{:<26} {:>2}/{:<2} frames differ; worst step {step}: {pixels} px, channel {channel}",
            report.name,
            report.differing_steps(),
            report.steps,
        );
        assert!(
            reference[0].data.iter().any(|&b| b != 0),
            "{}: the reference frame is empty",
            report.name
        );
        if let Ok(dir) = std::env::var("BLINC_ANIM_PATHS_OUT") {
            let steps = sheet_steps(&report);
            let file = format!("{dir}/{}.png", report.name.replace([' ', ','], "_"));
            std::fs::create_dir_all(&dir).unwrap();
            ScreenshotExporter::save_png(&sheet(&reference, &painted, &steps), file).unwrap();
        }
        reports.push(report);
    }
    // Every scene not listed is held to the reference: a frame that differs by more than edge
    // rounding is a bug in the patch, and `BLINC_ANIM_PATHS_OUT` shows it.
    let off: Vec<String> = reports
        .iter()
        .filter(|r| r.differing_steps() > 0 && !KNOWN_DIFFERENT.contains(&r.name))
        .map(|r| {
            let (step, (pixels, channel)) = r.worst();
            format!("{} (step {step}: {pixels} px, channel {channel})", r.name)
        })
        .collect();
    assert!(
        off.is_empty(),
        "the patched path differs from a full paint in: {}",
        off.join(", ")
    );
    let fixed: Vec<&str> = reports
        .iter()
        .filter(|r| r.differing_steps() == 0 && KNOWN_DIFFERENT.contains(&r.name))
        .map(|r| r.name)
        .collect();
    assert!(
        fixed.is_empty(),
        "these scenes match a full paint now; take them off KNOWN_DIFFERENT: {}",
        fixed.join(", ")
    );
}

/// Where a set of pixels sits and which way its long side points.
struct Blob {
    centre: (f32, f32),
    /// Degrees, in (-90, 90], of the long axis in image coordinates.
    axis: f32,
    pixels: usize,
}

fn pixel(frame: &CapturedFrame, x: usize, y: usize) -> [i32; 3] {
    let at = (y * frame.width as usize + x) * 4;
    [
        frame.data[at] as i32,
        frame.data[at + 1] as i32,
        frame.data[at + 2] as i32,
    ]
}

fn blob(frame: &CapturedFrame, keep: impl Fn(usize, usize) -> bool) -> Blob {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let hits: Vec<(f64, f64)> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| keep(x, y))
        .map(|(x, y)| (x as f64, y as f64))
        .collect();
    assert!(!hits.is_empty(), "no pixels selected");
    let n = hits.len() as f64;
    let cx = hits.iter().map(|p| p.0).sum::<f64>() / n;
    let cy = hits.iter().map(|p| p.1).sum::<f64>() / n;
    let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
    for (x, y) in hits.iter().copied() {
        xx += (x - cx) * (x - cx);
        yy += (y - cy) * (y - cy);
        xy += (x - cx) * (y - cy);
    }
    Blob {
        centre: (cx as f32, cy as f32),
        axis: (0.5 * (2.0 * xy).atan2(xx - yy)).to_degrees() as f32,
        pixels: hits.len(),
    }
}

/// The pixels the label adds to a frame: where it differs from the same frame without it.
fn label_blob(with: &CapturedFrame, without: &CapturedFrame) -> Blob {
    blob(with, |x, y| {
        let (p, q) = (pixel(with, x, y), pixel(without, x, y));
        (0..3).map(|i| (p[i] - q[i]).abs()).sum::<i32>() > 120
    })
}

/// The card in a frame: its blue fill.
fn card_blob(frame: &CapturedFrame) -> Blob {
    blob(frame, |x, y| {
        let [r, g, b] = pixel(frame, x, y);
        b > 170 && r < 110 && g > 70 && g < 170
    })
}

/// What a scripted motion scene has bound at `step` of `steps`, as the CSS transform that puts the
/// card in the same place: translate, then scale, then turn, about the card's centre.
fn css_equivalent(channels: Channels, step: usize, steps: usize) -> Transform {
    let t = (step + 1) as f32 / steps as f32;
    let (dx, dy) = if channels.translate {
        (60.0 * t, -30.0 * t)
    } else {
        (0.0, 0.0)
    };
    let s = if channels.scale { 1.0 + 0.4 * t } else { 1.0 };
    let (sin, cos) = (if channels.rotate { 35.0 * t } else { 0.0 })
        .to_radians()
        .sin_cos();
    Transform::Affine2D(Affine2D {
        elements: [s * cos, s * sin, -s * sin, s * cos, dx, dy],
    })
}

/// Text turns with the card it is in and stays at its centre however the other channels move, and
/// a bound turn lays the label out as the CSS transform for the same pose does.
#[test]
fn a_label_turns_with_its_card() {
    let channels = |translate, scale| Channels {
        translate,
        scale,
        rotate: true,
        fade: false,
        spring: false,
    };
    let cases = [
        ("rotate", channels(false, false), false),
        ("scale and rotate", channels(false, true), false),
        ("translate, scale and rotate", channels(true, true), false),
        ("nested", channels(false, false), true),
    ];
    let steps = 12;
    for (name, ch, nested) in cases {
        let scene = |label| {
            if nested {
                nested_motion_scene(label, false)
            } else {
                motion_scene("label", ch, label, steps)
            }
        };
        let (Some(with), Some(without)) = (
            run(&scene(true), Path::Full),
            run(&scene(false), Path::Full),
        ) else {
            return;
        };
        // The label's long axis at rest: a few glyphs are not a straight bar.
        let Some(rest) = run(
            &motion_scene("rest", Channels::default(), true, 1),
            Path::Full,
        ) else {
            return;
        };
        let rest_bare = run(
            &motion_scene("rest", Channels::default(), false, 1),
            Path::Full,
        )
        .unwrap();
        let rest_axis = label_blob(&rest[0], &rest_bare[0]).axis;

        for step in 0..steps {
            let card = card_blob(&without[step]);
            let label = label_blob(&with[step], &without[step]);
            assert!(
                label.pixels > 30,
                "{name} step {step}: the label left no pixels"
            );
            let mut turn = label.axis - rest_axis - card.axis;
            turn -= 180.0 * (turn / 180.0).round();
            let off = ((label.centre.0 - card.centre.0).powi(2)
                + (label.centre.1 - card.centre.1).powi(2))
            .sqrt();
            eprintln!(
                "{name:<28} step {step:>2}: card {:>5.1} deg, label {turn:>4.1} deg off, {off:>3.1} px from centre",
                card.axis
            );
            // Rotated glyphs are not straight bars, so the bound is loose.
            if card.axis.abs() >= 10.0 {
                assert!(
                    turn.abs() <= 8.0,
                    "{name} step {step}: the label is {turn} deg off the card"
                );
            }
            assert!(
                off <= 3.0,
                "{name} step {step}: the label is {off} px off the card's centre"
            );
        }

        if nested {
            continue;
        }
        let mut css = session().unwrap();
        for step in [3, 7, 11] {
            let transform = css_equivalent(ch, step, steps);
            css.mount(move |_| {
                stage().child(
                    div()
                        .w(120.0)
                        .h(64.0)
                        .child(card(true).transform(transform.clone())),
                )
            });
            css.settle();
            let expected = css.snapshot().expect("a frame");
            let d = difference(&with[step], &expected, 8);
            eprintln!(
                "{name:<28} step {step:>2}: {} px differ from the CSS transform",
                d.differing
            );
            assert!(
                d.differing < 60,
                "{name} step {step}: the bound pose differs from its CSS transform in {} px",
                d.differing
            );
        }
    }
}

/// Which way the shadow lies from the card: the darkness the shadow adds to the stage, as a
/// weighted centre relative to the card's own centre, in degrees clockwise from straight down.
/// The card's shadow is offset straight down, so a card turned by an angle casts it that angle
/// round.
fn shadow_direction(frame: &CapturedFrame) -> f32 {
    let stage = [23, 26, 33];
    let (w, h) = (frame.width as usize, frame.height as usize);
    let card = card_blob(frame).centre;
    let (mut total, mut sx, mut sy) = (0.0f64, 0.0f64, 0.0f64);
    for y in 0..h {
        for x in 0..w {
            let p = pixel(frame, x, y);
            let dark: i32 = (0..3).map(|i| (stage[i] - p[i]).max(0)).sum();
            total += dark as f64;
            sx += dark as f64 * x as f64;
            sy += dark as f64 * y as f64;
        }
    }
    assert!(total > 0.0, "no shadow on the stage");
    let (dx, dy) = (sx / total - card.0 as f64, sy / total - card.1 as f64);
    (-dx).atan2(dy).to_degrees() as f32
}

/// A shadow turns with the element it belongs to, offset and all.
#[test]
fn a_shadow_turns_with_its_card() {
    let cases = [
        (
            "rotate",
            Channels {
                rotate: true,
                ..Default::default()
            },
        ),
        (
            "translate, scale and rotate",
            Channels {
                translate: true,
                scale: true,
                rotate: true,
                fade: false,
                spring: false,
            },
        ),
    ];
    for (name, channels) in cases {
        let Some(frames) = run(&motion_scene("shadow", channels, false, 12), Path::Full) else {
            return;
        };
        for (step, frame) in frames.iter().enumerate() {
            let turn = card_blob(frame).axis;
            let direction = shadow_direction(frame);
            eprintln!(
                "{name:<28} step {step:>2}: card {turn:>5.1} deg, shadow lies {direction:>5.1} deg round"
            );
            if turn.abs() >= 10.0 {
                assert!(
                    (direction - turn).abs() <= 4.0,
                    "{name} step {step}: the card is turned {turn} deg and its shadow lies {direction} deg round"
                );
            }
        }
    }
}
