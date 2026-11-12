#![cfg(feature = "headless")]
//! Animated scenes painted two ways, frame by frame, and compared.
//!
//! The reference is a full paint every frame: the tree walked with the animated values as they are
//! at that frame, which is right by construction. The other is the path a window takes when only
//! animated values have changed: the cached batch patched (`apply_binding_deltas` for motion
//! bindings, `apply_css_deltas` for CSS animations and transitions) and blitted. Each path replays
//! the same scripted timeline in a session of its own, so the animation state at a step is the
//! same in both.
//!
//! A new way of painting animation (the node table) is a third `Path`: it is held to the
//! reference, not to the patched path, whose differences are listed below as known.
//!
//! `BLINC_ANIM_PATHS_OUT=<dir>` writes a contact sheet for each scene: the frames that matter,
//! each as reference, painted path and difference (red where they differ).

use blinc_animation::AnimatedValue;
use blinc_core::Color;
use blinc_core::reactive::signal;
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

/// Run `scene` with every frame painted by `path`.
fn run(scene: &Scene, path: Path) -> Option<Vec<CapturedFrame>> {
    let mut ui = session()?;
    let mut script = (scene.build)(&mut ui);
    ui.settle();
    // The cache the patched path patches comes from a full paint of the scene at rest.
    ui.snapshot().expect("a frame");
    let mut frames = Vec::new();
    for step in 0..scene.steps {
        script(&mut ui, step);
        ui.advance(scene.step_ms);
        let frame = match path {
            Path::Full => ui.snapshot(),
            Path::Patched => ui.snapshot_fast(),
        };
        frames.push(frame.expect("a frame"));
    }
    Some(frames)
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
                    v.lock().unwrap().set_immediate(value)
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
fn nested_motion_scene() -> Scene {
    let steps = 12;
    Scene {
        name: "motion nested",
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let outer_x = bound(ui, 0.0);
            let (scale, rotation) = (bound(ui, 1.0), bound(ui, 0.0));
            let handles = (outer_x.clone(), scale.clone(), rotation.clone());
            ui.mount(move |_| {
                let inner = card(true)
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
                handles.0.lock().unwrap().set_immediate(60.0 * t);
                handles.1.lock().unwrap().set_immediate(1.0 + 0.25 * t);
                handles.2.lock().unwrap().set_immediate(20.0 * t);
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
                handle.lock().unwrap().set_immediate(1.0 - 0.7 * t);
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
    };
    vec![
        motion_scene(
            "motion translate",
            Channels {
                translate: true,
                ..Default::default()
            },
            true,
            12,
        ),
        motion_scene(
            "motion scale",
            Channels {
                scale: true,
                ..Default::default()
            },
            true,
            12,
        ),
        motion_scene(
            "motion rotate",
            Channels {
                rotate: true,
                ..Default::default()
            },
            true,
            12,
        ),
        motion_scene(
            "motion fade",
            Channels {
                fade: true,
                ..Default::default()
            },
            true,
            12,
        ),
        motion_scene("motion all", all, true, 12),
        motion_scene("motion all, no text", all, false, 12),
        nested_motion_scene(),
        overlap_fade_scene(),
        transition_scene("css transition", true),
        transition_scene("css transition, no text", false),
        keyframe_scene("css keyframes", true),
        keyframe_scene("css keyframes, no text", false),
    ]
}

struct Report {
    name: &'static str,
    steps: usize,
    /// Per step: pixels that differ by more than two levels, and the largest channel difference.
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
        self.per_step.iter().filter(|(p, _)| *p > 0).count()
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
        let diff = diff_image(&reference[step], &painted[step], 2);
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
    let painted = run(scene, Path::Patched)?;
    let per_step = reference
        .iter()
        .zip(&painted)
        .map(|(a, b)| {
            let d = difference(a, b, 2);
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
    // The patched path is held to the reference exactly: a frame that differs by more than the
    // two levels edge rounding allows is a bug in the patch, and `BLINC_ANIM_PATHS_OUT` shows it.
    let off: Vec<String> = reports
        .iter()
        .filter(|r| r.differing_steps() > 0)
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
}
