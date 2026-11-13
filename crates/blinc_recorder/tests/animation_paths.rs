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
use blinc_layout::motion::motion;
use blinc_layout::prelude::*;
use blinc_layout::widgets::ScrollbarVisibility;
use blinc_recorder::testing::{CapturedFrame, ScreenshotExporter, diff_image, difference};
use blinc_recorder::{Headless, MotionPaint};
use blinc_theme::{ColorScheme, HybridTheme};
use std::sync::{Arc, Mutex};

const W: u32 = 320;
const H: u32 = 240;

/// How a frame of an animation is painted.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Path {
    /// A full paint every frame: the reference, with motion-bound subtrees promoted as a window
    /// promotes them.
    Full,
    /// A full paint every frame with no promotion: every subtree painted as vectors, so the frame
    /// has the edges a texture blit softens. What the node table is held to.
    Vector,
    /// As a window paints a frame in which only animated values changed.
    Patched,
    /// The vector frames, with motion-bound primitives moved by rows of the GPU node table.
    Table,
    /// The vector frames, with the same rows applied on the CPU: what a device with no table
    /// gets when motion is not baked.
    Rows,
}

/// What a scene does at each step before the frame is painted.
type Script = Box<dyn FnMut(&mut Headless, usize)>;

struct Scene {
    name: &'static str,
    steps: usize,
    step_ms: u32,
    build: Box<dyn Fn(&mut Headless) -> Script>,
}

fn session(path: Path) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    // The paths run with the table as they say whatever the environment says.
    let paint = match path {
        Path::Full | Path::Patched => MotionPaint::Promoted,
        Path::Vector | Path::Rows => MotionPaint::Vector,
        Path::Table => MotionPaint::NodeTable,
    };
    // `BLINC_ANIM_PATHS_SCALE=2` runs the scenes at a retina density.
    let density = std::env::var("BLINC_ANIM_PATHS_SCALE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(1.0);
    let mut ui = Headless::with_motion_paint_at(W, H, density, paint).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    // The state-style pass runs when hover or focus changes, as in a window; run every frame it
    // restyles nodes, and each frame would look like a change to the layout.
    ui.gate_state_styles();
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
        matches!(
            metadata.target(),
            "blinc_app::frame_timing" | "blinc_app::node_rows"
        )
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
            outcome: String,
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                let text = format!("{value:?}").trim_matches('"').to_string();
                match field.name() {
                    "message" => self.message = text,
                    "path" => self.path = text,
                    "outcome" => self.outcome = text,
                    _ => {}
                }
            }
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        match fields.message.as_str() {
            "fast_path" => self.0.lock().unwrap().push(fields.path),
            "node_rows" => self
                .0
                .lock()
                .unwrap()
                .push(format!("rows:{}", fields.outcome)),
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
    let mut ui = session(path)?;
    let mut script = (scene.build)(&mut ui);
    ui.settle();
    // Content that loads in the background, an image, has to be in before the scene starts: the
    // frame is taken until three in a row are the same. The last is a full paint of the scene at
    // rest, which is the cache the patched path patches.
    let mut rest = ui.snapshot().expect("a frame");
    let mut same = 0;
    for _ in 0..50 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let next = ui.snapshot().expect("a frame");
        same = if next.data == rest.data { same + 1 } else { 0 };
        rest = next;
        if same == 3 {
            break;
        }
    }
    log.0.lock().unwrap().clear();
    let mut frames = Vec::new();
    let mut paths = Vec::new();
    for step in 0..scene.steps {
        script(&mut ui, step);
        ui.advance(scene.step_ms);
        let frame = match path {
            Path::Full | Path::Vector => ui.snapshot(),
            Path::Patched | Path::Table | Path::Rows => ui.snapshot_fast(),
        };
        frames.push(frame.expect("a frame"));
        // One painting event per frame, and a `rows:` event when the node table was asked to move
        // the batch. Keep the painting event that was last, and say what the table did.
        let mut seen = log.0.lock().unwrap();
        let painted = seen
            .iter()
            .rev()
            .find(|e| !e.starts_with("rows:"))
            .cloned()
            .unwrap_or_default();
        let rows = seen.iter().rev().find(|e| e.starts_with("rows:"));
        paths.push(match rows {
            Some(rows) => format!("{painted} {rows}"),
            None => painted,
        });
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

/// One bound card moves beside another that is bound but stays where it is painted, so it sits in
/// the static batch while the first is in the dynamic one.
fn beside_settled_scene() -> Scene {
    let steps = 12;
    Scene {
        name: "motion beside a settled one",
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let (moving, still) = (bound(ui, 0.0), bound(ui, 0.0));
            let handle = moving.clone();
            ui.mount(move |_| {
                stage()
                    .child(
                        div()
                            .w(120.0)
                            .h(64.0)
                            .child(card(true).motion().translate_y(moving.clone())),
                    )
                    .child(
                        div()
                            .w(120.0)
                            .h(64.0)
                            .child(card(true).motion().translate_y(still.clone())),
                    )
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                handle.lock().unwrap().set_target(-40.0 * t);
            })
        }),
    }
}

/// A bound card in a clipped box (the clip is an ancestor's and stays), or one that clips its own
/// children (the clip is the node's and moves and grows with it).
fn clip_scene(own: bool) -> Scene {
    let steps = 14;
    Scene {
        name: if own {
            "motion with its own clip"
        } else {
            "motion in a clipped box"
        },
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let (x, s) = (bound(ui, 0.0), bound(ui, 1.0));
            let handles = (x.clone(), s.clone());
            ui.mount(move |_| {
                if own {
                    let clipping = div()
                        .w(120.0)
                        .h(64.0)
                        .rounded(14.0)
                        .overflow_clip()
                        .bg(Color::rgb(0.16, 0.18, 0.24))
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .child(div().w(200.0).h(16.0).bg(Color::rgb(0.9, 0.4, 0.2)))
                        .child(text("Own clip").size(14.0).color(Color::WHITE));
                    stage().child(
                        div()
                            .w(120.0)
                            .h(64.0)
                            .child(clipping.motion().translate_x(x.clone()).scale(s.clone())),
                    )
                } else {
                    stage().child(
                        div()
                            .w(160.0)
                            .h(84.0)
                            .rounded(14.0)
                            .overflow_clip()
                            .bg(Color::rgb(0.16, 0.18, 0.24))
                            .flex_row()
                            .items_center()
                            .justify_center()
                            .child(div().w(120.0).h(64.0).child(
                                card(true).motion().translate_x(x.clone()).scale(s.clone()),
                            )),
                    )
                }
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                handles.0.lock().unwrap().set_target(50.0 * t);
                handles.1.lock().unwrap().set_target(1.0 + 0.3 * t);
            })
        }),
    }
}

/// A bound card with a layer effect (a drop shadow the group casts), which sizes its layer by
/// what is under it.
fn effect_scene(label: bool) -> Scene {
    let steps = 12;
    Scene {
        name: if label {
            "motion with a layer effect"
        } else {
            "motion with a layer effect, no text"
        },
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let (x, s, r) = (bound(ui, 0.0), bound(ui, 1.0), bound(ui, 0.0));
            let handles = (x.clone(), s.clone(), r.clone());
            ui.mount(move |_| {
                let card = card_at(label, 1.0).drop_shadow_effect(
                    6.0,
                    8.0,
                    10.0,
                    Color::rgba(0.0, 0.0, 0.0, 0.7),
                );
                stage().child(
                    div().w(120.0).h(64.0).child(
                        card.motion()
                            .translate_x(x.clone())
                            .scale(s.clone())
                            .rotate(r.clone()),
                    ),
                )
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                handles.0.lock().unwrap().set_target(40.0 * t);
                handles.1.lock().unwrap().set_target(1.0 + 0.3 * t);
                handles.2.lock().unwrap().set_target(25.0 * t);
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

/// A 1x1 image the card shows, scaled up.
const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNg+M/wHwAEAQH/cetH5QAAAABJRU5ErkJggg==";

#[derive(Clone, Copy)]
enum ImageKind {
    /// Translate and scale.
    Move,
    /// Fade: the card is a group, which keeps its image in the static batch.
    Fade,
    /// Turn and scale.
    Turn,
    /// Translate and scale, with a rounded image.
    Rounded,
    /// An outer move over an inner scale and turn.
    Nested,
    /// Translate and scale inside a box that clips.
    Clipped,
}

/// A bound card holding an image, which is not a primitive in the batch.
fn image_scene(name: &'static str, kind: ImageKind) -> Scene {
    let steps = 12;
    Scene {
        name,
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let (x, s, r, opacity) = (
                bound(ui, 0.0),
                bound(ui, 1.0),
                bound(ui, 0.0),
                bound(ui, 1.0),
            );
            let handles = (x.clone(), s.clone(), r.clone(), opacity.clone());
            ui.mount(move |_| {
                let picture = blinc_layout::image(PIXEL)
                    .size(40.0, 40.0)
                    .fade_in(std::time::Duration::ZERO);
                let picture = if matches!(kind, ImageKind::Rounded) {
                    picture.rounded(10.0)
                } else {
                    picture
                };
                let card = card(false).child(div().absolute().left(10.0).top(8.0).child(picture));
                match kind {
                    ImageKind::Move | ImageKind::Rounded => stage().child(
                        div()
                            .w(120.0)
                            .h(64.0)
                            .child(card.motion().translate_x(x.clone()).scale(s.clone())),
                    ),
                    ImageKind::Fade => stage().child(
                        div()
                            .w(120.0)
                            .h(64.0)
                            .child(card.motion().opacity(opacity.clone())),
                    ),
                    ImageKind::Turn => stage().child(
                        div()
                            .w(120.0)
                            .h(64.0)
                            .child(card.motion().rotate(r.clone()).scale(s.clone())),
                    ),
                    ImageKind::Nested => {
                        let inner = card.motion().scale(s.clone()).rotate(r.clone());
                        let outer = div()
                            .w(120.0)
                            .h(64.0)
                            .child(inner)
                            .motion()
                            .translate_x(x.clone());
                        stage().child(div().w(120.0).h(64.0).child(outer))
                    }
                    ImageKind::Clipped => stage().child(
                        div()
                            .w(160.0)
                            .h(84.0)
                            .rounded(14.0)
                            .overflow_clip()
                            .bg(Color::rgb(0.16, 0.18, 0.24))
                            .flex_row()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .w(120.0)
                                    .h(64.0)
                                    .child(card.motion().translate_x(x.clone()).scale(s.clone())),
                            ),
                    ),
                }
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                handles.0.lock().unwrap().set_target(40.0 * t);
                handles.1.lock().unwrap().set_target(1.0 + 0.3 * t);
                handles.2.lock().unwrap().set_target(25.0 * t);
                handles.3.lock().unwrap().set_target(1.0 - 0.6 * t);
            })
        }),
    }
}

/// The repro window's "fade under a tile" tile as it builds it: a keyed wobbly value, a card with
/// a label in a motion container that fades, and a tile after it that overlaps.
fn repro_fade_scene() -> Scene {
    let steps = 40;
    Scene {
        name: "motion fade, built as the repro does",
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let o = ui.context().use_animated_value_for(
                "repro:under:o",
                1.0,
                blinc_animation::SpringConfig::wobbly(),
            );
            let handle = o.clone();
            ui.mount(move |_| {
                let tile_after = div()
                    .w(40.0)
                    .h(24.0)
                    .ml(-15.0)
                    .bg(Color::rgb(0.95, 0.85, 0.2));
                let stage = div()
                    .w(220.0)
                    .h(130.0)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .rounded(10.0)
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w(100.0)
                            .h(56.0)
                            .child(card(true).motion().opacity(o.clone())),
                    )
                    .child(tile_after);
                // As the window's tile wraps it: a title, a note, and a clickable box.
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .flex_row()
                    .flex_wrap()
                    .gap(5.0)
                    .child(
                        div()
                            .flex_col()
                            .gap(1.0)
                            .child(text("Fade under a tile").size(13.0).color(Color::WHITE))
                            .child(
                                text("the yellow tile stays above")
                                    .size(10.0)
                                    .color(Color::WHITE),
                            )
                            .child(div().on_click(|_| {}).child(stage)),
                    )
            });
            Box::new(move |_, step| {
                if step == 0 {
                    handle.lock().unwrap().set_target(0.3);
                }
            })
        }),
    }
}

/// A labelled card that fades with a tile painted after it, which keeps its place in the static
/// batch, beside a card that moves, which the rows move. The first has to keep fading.
fn fade_beside_a_mover_scene() -> Scene {
    let steps = 24;
    Scene {
        name: "motion fade under a tile, beside others",
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let (o, o2, x) = (bound(ui, 1.0), bound(ui, 1.0), bound(ui, 0.0));
            let handles = (o.clone(), o2.clone(), x.clone());
            ui.mount(move |_| {
                let plain = div()
                    .w(100.0)
                    .h(56.0)
                    .child(card(true).motion().opacity(o2.clone()));
                let faded = div()
                    .w(100.0)
                    .h(56.0)
                    .child(card(true).motion().opacity(o.clone()));
                let tile = div()
                    .w(40.0)
                    .h(24.0)
                    .ml(-15.0)
                    .bg(Color::rgb(0.95, 0.85, 0.2));
                let mover = div()
                    .w(60.0)
                    .h(30.0)
                    .child(card(false).motion().translate_x(x.clone()));
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(4.0)
                    .child(plain)
                    .child(div().flex_row().child(faded).child(tile))
                    .child(mover)
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                handles.0.lock().unwrap().set_target(1.0 - 0.7 * t);
                handles.1.lock().unwrap().set_target(1.0 - 0.6 * t);
                handles.2.lock().unwrap().set_target(30.0 * t);
            })
        }),
    }
}

/// A list of labelled cards in a scroll container, scrolled by the wheel a notch a frame. The
/// whole list is long, so most of it is out of the viewport.
fn scroll_scene(
    name: &'static str,
    rows: usize,
    notch: f32,
    bar: blinc_layout::widgets::ScrollbarVisibility,
) -> Scene {
    let steps = 20;
    Scene {
        name,
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            // Scrolled by a reference, so the same script gives the same frames on every run: the
            // wheel's physics run on the wall clock.
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound = reference.clone();
            ui.mount(move |_| {
                let mut list = div().flex_col().gap(2.0).w(W as f32 - 20.0).p(2.0);
                for i in 0..rows {
                    let hue = (i % 7) as f32 / 7.0;
                    list = list.child(
                        div()
                            .w(W as f32 - 28.0)
                            .h(28.0)
                            .rounded(6.0)
                            .bg(Color::rgb(0.2 + 0.4 * hue, 0.45, 0.9 - 0.4 * hue))
                            .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.5))
                            .flex_row()
                            .items_center()
                            .child(text(format!("Row {i}")).size(13.0).color(Color::WHITE)),
                    );
                }
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .child(
                        scroll()
                            .bind(&bound)
                            .scrollbar_visibility(bar)
                            .w(W as f32)
                            .h(H as f32)
                            .child(list),
                    )
            });
            Box::new(move |ui, step| {
                reference.set_scroll_offset(0.0, notch * (step + 1) as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A scroll list beside a card that moves, as in a window where other things are animating: the
/// rows that move the card move the list too.
fn scroll_beside_movers_scene() -> Scene {
    let steps = 20;
    Scene {
        name: "scroll a list beside a mover",
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let (x, s) = (bound(ui, 0.0), bound(ui, 1.0));
            let handles = (x.clone(), s.clone());
            let reference = blinc_layout::selector::ScrollRef::new();
            let bound_ref = reference.clone();
            ui.mount(move |_| {
                let mut list = div().flex_col().gap(1.0).p(1.0);
                for i in 0..60 {
                    list = list.child(
                        div()
                            .w_full()
                            .h(22.0)
                            .rounded(5.0)
                            .bg(Color::rgb(0.2 + 0.05 * (i % 7) as f32, 0.45, 0.8))
                            .flex_row()
                            .items_center()
                            .child(text(format!("Row {i}")).size(12.0).color(Color::WHITE)),
                    );
                }
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .flex_row()
                    .gap(4.0)
                    .child(
                        div()
                            .w(100.0)
                            .h(56.0)
                            .child(card(true).motion().translate_x(x.clone()).scale(s.clone())),
                    )
                    .child(scroll().bind(&bound_ref).w(150.0).h(H as f32).child(list))
            });
            Box::new(move |ui, step| {
                let t = (step + 1) as f32 / steps as f32;
                handles.0.lock().unwrap().set_target(20.0 * t);
                handles.1.lock().unwrap().set_target(1.0 + 0.2 * t);
                reference.set_scroll_offset(0.0, 12.0 * (step + 1) as f32);
                ui.settle_scroll();
            })
        }),
    }
}

/// A bound card with a static tile painted after it, over where the card passes. The tile stays
/// above the card, as in a single paint in tree order. `fade` makes the card a group, and
/// `foreground` gives that group a child painted in the foreground stream.
fn later_tile_scene(name: &'static str, fade: bool, foreground: bool, label: bool) -> Scene {
    let steps = 12;
    Scene {
        name,
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let (x, opacity) = (bound(ui, 0.0), bound(ui, 1.0));
            let handles = (x.clone(), opacity.clone());
            ui.mount(move |_| {
                let mut card = card(label);
                if foreground {
                    card = card.child(
                        div()
                            .absolute()
                            .left(10.0)
                            .top(8.0)
                            .w(60.0)
                            .h(40.0)
                            .foreground()
                            .bg(Color::rgb(0.2, 0.8, 0.4)),
                    );
                }
                let mut motion = card.motion().translate_x(x.clone());
                if fade {
                    motion = motion.opacity(opacity.clone());
                }
                // An ordinary flex sibling, pulled back over the card.
                let tile = div()
                    .w(40.0)
                    .h(24.0)
                    .ml(-20.0)
                    .bg(Color::rgb(0.95, 0.85, 0.2));
                stage()
                    .child(div().w(120.0).h(64.0).child(motion))
                    .child(tile)
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                handles.0.lock().unwrap().set_target(40.0 * t);
                handles.1.lock().unwrap().set_target(1.0 - 0.5 * t);
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

const FLIP_CSS: &str = ".slide { transition: transform 300ms ease-out; }";

/// Cards that a banner appearing above them pushes down, each sliding from where it was to where
/// layout puts it (a FLIP transition): laid out and painted at the new place, drawn offset from it.
fn flip_scene(name: &'static str, label: bool) -> Scene {
    Scene {
        name,
        steps: 22,
        step_ms: 16,
        build: Box::new(move |ui| {
            let open = signal(false);
            ui.css(FLIP_CSS);
            ui.mount(move |_| {
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .flex_col()
                    .items_center()
                    .gap(8.0)
                    .p(8.0)
                    .when(open, |d| {
                        d.child(
                            div()
                                .w(200.0)
                                .h(46.0)
                                .rounded(8.0)
                                .bg(Color::rgb(0.9, 0.5, 0.2)),
                        )
                    })
                    .child(card(label).id("slide-a").class("slide"))
                    .child(card(label).id("slide-b").class("slide"))
            });
            Box::new(move |_, step| {
                if step == 0 {
                    open.set(true);
                }
            })
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
        beside_settled_scene(),
        clip_scene(false),
        clip_scene(true),
        effect_scene(false),
        effect_scene(true),
        image_scene("motion, an image", ImageKind::Move),
        image_scene("motion fade, an image", ImageKind::Fade),
        image_scene("motion rotate, an image", ImageKind::Turn),
        image_scene("motion, a rounded image", ImageKind::Rounded),
        image_scene("motion nested, an image", ImageKind::Nested),
        image_scene("motion in a clipped box, an image", ImageKind::Clipped),
        later_tile_scene("motion, a tile after it", false, false, false),
        later_tile_scene("motion fade, a tile after it", true, false, false),
        later_tile_scene(
            "motion fade, foreground, a tile after it",
            true,
            true,
            false,
        ),
        later_tile_scene("motion fade, a label, a tile after it", true, false, true),
        repro_fade_scene(),
        scroll_scene("scroll a list", 60, 12.0, ScrollbarVisibility::Auto),
        scroll_beside_movers_scene(),
        scroll_scene(
            "scroll a list, scrollbar always",
            60,
            12.0,
            ScrollbarVisibility::Always,
        ),
        scroll_scene(
            "scroll a list, no scrollbar",
            60,
            12.0,
            ScrollbarVisibility::Never,
        ),
        scroll_scene(
            "scroll a list, past its window",
            120,
            55.0,
            ScrollbarVisibility::Always,
        ),
        fade_beside_a_mover_scene(),
        transition_scene("css transition", true),
        transition_scene("css transition, no text", false),
        keyframe_scene("css keyframes", true),
        keyframe_scene("css keyframes, no text", false),
        flip_scene("flip", true),
        flip_scene("flip, no text", false),
        flip_grid_scene(30),
    ]
}

/// A pixel differs when a channel is further than this from the reference: edge rounding is not a
/// difference.
const LEVELS: u8 = 2;

/// Scenes in which the patched path is known to differ from a full paint. They are listed so the
/// rest are held to the reference; a scene that stops differing is an error until it is taken off
/// the list. A label stays where the last full paint put it while a bound value moves, an outer
/// binding moving an inner one leaves the inner box and its shadow in the wrong place, and an
/// image does not follow the card it is in.
const KNOWN_DIFFERENT: &[&str] = &[
    "motion translate",
    "motion scale",
    "motion rotate",
    "motion nested",
    "motion nested, no text",
    "motion in a clipped box",
    "motion with its own clip",
    "motion, an image",
    "motion rotate, an image",
    "motion, a rounded image",
    "motion nested, an image",
    "motion in a clipped box, an image",
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

fn compare(
    scene: &Scene,
    path: Path,
) -> Option<(Report, Vec<CapturedFrame>, Vec<CapturedFrame>, Vec<String>)> {
    let reference = run(
        scene,
        if matches!(path, Path::Table | Path::Rows) {
            Path::Vector
        } else {
            Path::Full
        },
    )?;
    let (painted, paths) = run_logged(scene, path)?;
    let mut tally = std::collections::BTreeMap::<&str, usize>::new();
    for path in &paths {
        *tally.entry(path.as_str()).or_default() += 1;
    }
    eprintln!("{:<26} {path:?} path took {tally:?}", scene.name);
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
        paths,
    ))
}

/// Paint every scene both ways and report how the patched path differs from the reference.
#[test]
fn the_patched_path_against_the_reference() {
    let mut reports = Vec::new();
    for scene in scenes() {
        let Some((report, reference, painted, _)) = compare(&scene, Path::Patched) else {
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

/// Scenes whose every frame the node table moves alone: no full paint, rows only, so a
/// difference from the vector paint is the table's.
const ON_ROWS: &[&str] = &[
    "motion translate",
    "motion translate, no text",
    "motion scale",
    "motion scale, no text",
    "motion rotate",
    "motion rotate, no text",
    "motion fade",
    "motion fade, no text",
    "motion all",
    "motion all, no text",
    "motion nested",
    "motion nested, no text",
    "motion fade, overlapping",
    "motion beside a settled one",
    "motion in a clipped box",
    "motion with its own clip",
    "motion with a layer effect, no text",
    "motion with a layer effect",
    "motion, an image",
    "motion rotate, an image",
    "motion, a rounded image",
    "motion nested, an image",
    "motion in a clipped box, an image",
    "scroll a list",
    "scroll a list, scrollbar always",
    "scroll a list, no scrollbar",
];

/// Paint every scene with the node table and hold the ones it moves alone to the vector paint.
/// Every scene on `path` is held to the vector paint, and those in `ON_ROWS` must be moved by
/// rows.
fn rows_against_the_reference(path: Path) {
    let mut off = Vec::new();
    for scene in scenes() {
        let Some((report, reference, painted, paths)) = compare(&scene, path) else {
            return;
        };
        let (step, (pixels, channel)) = report.worst();
        eprintln!(
            "{:<26} {:>2}/{:<2} frames differ; worst step {step}: {pixels} px, channel {channel}",
            report.name,
            report.differing_steps(),
            report.steps,
        );
        if let Ok(dir) = std::env::var("BLINC_ANIM_PATHS_OUT") {
            let steps = sheet_steps(&report);
            let folder = format!("{dir}/{}", format!("{path:?}").to_lowercase());
            let file = format!("{folder}/{}.png", report.name.replace([' ', ','], "_"));
            std::fs::create_dir_all(&folder).unwrap();
            ScreenshotExporter::save_png(&sheet(&reference, &painted, &steps), file).unwrap();
        }
        // Every scene is held to the vector paint; those in ON_ROWS must also be moved by rows.
        let on_patcher = paths
            .iter()
            .filter(|p| p.starts_with("full") || !p.ends_with("rows:applied"))
            .count();
        if on_patcher > 0 && ON_ROWS.contains(&report.name) {
            off.push(format!(
                "{}: {on_patcher} of {} frames were not moved by rows",
                report.name, report.steps
            ));
        }
        if let Some((at, (pixels, channel))) = report
            .per_step
            .iter()
            .copied()
            .enumerate()
            .find(|(_, (pixels, _))| *pixels > 0)
        {
            off.push(format!(
                "{} differs from the vector paint (step {at}: {pixels} px, channel {channel})",
                report.name
            ));
        }
    }
    assert!(off.is_empty(), "rows on {path:?}: {}", off.join("; "));
}

#[test]
fn the_node_table_against_the_reference() {
    rows_against_the_reference(Path::Table);
}

#[test]
fn the_cpu_rows_against_the_reference() {
    rows_against_the_reference(Path::Rows);
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
        let mut css = session(Path::Full).unwrap();
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

/// How much a label changes a frame: the summed channel difference between the frame and the same
/// frame without it. A turned label has the ink of an upright one.
fn ink(with: &CapturedFrame, without: &CapturedFrame) -> f64 {
    let mut total = 0.0;
    for y in 0..with.height as usize {
        for x in 0..with.width as usize {
            let (p, q) = (pixel(with, x, y), pixel(without, x, y));
            total += (0..3).map(|i| (p[i] - q[i]).abs() as f64).sum::<f64>();
        }
    }
    total
}

/// A turned glyph's quad is larger than the glyph; what lies in the corners is not another glyph
/// from the atlas.
#[test]
fn a_turned_label_is_not_smeared() {
    let channels = Channels {
        rotate: true,
        ..Default::default()
    };
    let (Some(rest), Some(rest_bare)) = (
        run(
            &motion_scene("rest", Channels::default(), true, 1),
            Path::Full,
        ),
        run(
            &motion_scene("rest", Channels::default(), false, 1),
            Path::Full,
        ),
    ) else {
        return;
    };
    let upright = ink(&rest[0], &rest_bare[0]);
    let with = run(&motion_scene("turned", channels, true, 12), Path::Full).unwrap();
    let without = run(&motion_scene("turned", channels, false, 12), Path::Full).unwrap();
    for step in 0..12 {
        let ratio = ink(&with[step], &without[step]) / upright;
        eprintln!("step {step:>2}: {ratio:.2} of the upright ink");
        assert!(
            ratio < 1.15,
            "step {step}: the turned label has {ratio:.2} times the ink of an upright one"
        );
    }
}

/// What the windowed repro shows: several bound nodes of every kind in one tree, flipped together
/// to a far pose and back, so that they settle at different times and some are settled while
/// others move.
fn many_scene(mask: u8) -> Scene {
    let steps = 70;
    Scene {
        name: "motion many",
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let mini = |label: Option<&str>| {
                let card = div()
                    .w(80.0)
                    .h(44.0)
                    .rounded(10.0)
                    .bg(Color::rgb(0.2, 0.45, 0.9))
                    .border(2.0, Color::rgba(1.0, 1.0, 1.0, 0.9))
                    .shadow_params(0.0, 4.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.5))
                    .flex_row()
                    .items_center()
                    .justify_center();
                match label {
                    Some(l) => card.child(text(l).size(12.0).color(Color::WHITE)),
                    None => card,
                }
            };
            let holder = || div().w(80.0).h(44.0);
            let (tx, ty) = (bound(ui, 0.0), bound(ui, 0.0));
            let sc = bound(ui, 1.0);
            let rot = bound(ui, 0.0);
            let (ax, as_, ar, ao) = (
                bound(ui, 0.0),
                bound(ui, 1.0),
                bound(ui, 0.0),
                bound(ui, 1.0),
            );
            let (nx, ns, nr) = (bound(ui, 0.0), bound(ui, 1.0), bound(ui, 0.0));
            let txx = bound(ui, 0.0);
            let handles = [
                (tx.clone(), 0.0, 30.0),
                (ty.clone(), 0.0, -10.0),
                (sc.clone(), 1.0, 1.4),
                (rot.clone(), 0.0, 40.0),
                (ax.clone(), 0.0, -20.0),
                (as_.clone(), 1.0, 1.3),
                (ar.clone(), 0.0, -30.0),
                (ao.clone(), 1.0, 0.5),
                (nx.clone(), 0.0, 25.0),
                (ns.clone(), 1.0, 1.2),
                (nr.clone(), 0.0, 20.0),
                (txx.clone(), 0.0, 30.0),
            ];
            ui.mount(move |_| {
                let nested_inner = mini(Some("Nest"))
                    .motion()
                    .scale(ns.clone())
                    .rotate(nr.clone());
                let nested = holder()
                    .child(nested_inner)
                    .motion()
                    .translate_x(nx.clone());
                let mut root = div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .flex_row()
                    .flex_wrap()
                    .gap(5.0)
                    .p(3.0);
                if mask & 1 != 0 {
                    root = root.child(
                        holder().child(
                            mini(Some("Move"))
                                .motion()
                                .translate_x(tx.clone())
                                .translate_y(ty.clone()),
                        ),
                    );
                }
                if mask & 2 != 0 {
                    root =
                        root.child(holder().child(mini(Some("Zoom")).motion().scale(sc.clone())));
                }
                if mask & 4 != 0 {
                    root =
                        root.child(holder().child(mini(Some("Turn")).motion().rotate(rot.clone())));
                }
                if mask & 8 != 0 {
                    root = root.child(
                        holder().child(
                            mini(None)
                                .motion()
                                .translate_x(ax.clone())
                                .scale(as_.clone())
                                .rotate(ar.clone())
                                .opacity(ao.clone()),
                        ),
                    );
                }
                if mask & 16 != 0 {
                    root = root.child(holder().child(nested));
                }
                if mask & 32 != 0 {
                    root = root.child(
                        div().w(80.0).h(30.0).child(
                            motion()
                                .translate_x(txx.clone())
                                .items_center()
                                .justify_center()
                                .child(text("Just text").size(12.0).color(Color::WHITE)),
                        ),
                    );
                }
                root
            });
            Box::new(move |_, step| {
                // Far at the start, back at the middle.
                if step == 0 || step == steps / 2 {
                    let far = step == 0;
                    for (value, rest, target) in &handles {
                        value
                            .lock()
                            .unwrap()
                            .set_target(if far { *target } else { *rest });
                    }
                }
            })
        }),
    }
}

/// Several bound nodes in one tree, flipped together and back so they settle at different times,
/// are moved by rows on every frame and match the vector paint, and so does each part of them
/// alone. A node that moves only text, a card beside settled ones and a card that scales are
/// the cases where the node table once left text behind or fell back to full paints.
#[test]
fn the_node_table_beside_other_bindings() {
    let mut off = Vec::new();
    for mask in [0b111111u8, 0b100000, 0b100001, 0b000001, 0b010010] {
        let scene = many_scene(mask);
        let Some((report, reference, painted, paths)) = compare(&scene, Path::Table) else {
            return;
        };
        if let Ok(dir) = std::env::var("BLINC_ANIM_PATHS_OUT") {
            let steps = sheet_steps(&report);
            std::fs::create_dir_all(&dir).unwrap();
            ScreenshotExporter::save_png(
                &sheet(&reference, &painted, &steps),
                format!("{dir}/many_{mask:06b}.png"),
            )
            .unwrap();
        }
        let full = paths.iter().filter(|p| p.starts_with("full")).count();
        let unmoved = paths
            .iter()
            .filter(|p| !p.ends_with("rows:applied"))
            .count();
        if full > 0 || unmoved > 0 {
            off.push(format!(
                "{mask:06b}: {full} full paints, {unmoved} frames not moved by rows"
            ));
        }
        if let Some((at, (pixels, channel))) = report
            .per_step
            .iter()
            .copied()
            .enumerate()
            .find(|(_, (pixels, _))| *pixels > 0)
        {
            off.push(format!(
                "{mask:06b} differs from the vector paint (step {at}: {pixels} px, channel {channel})"
            ));
        }
    }
    assert!(off.is_empty(), "the node table: {}", off.join("; "));
}

/// A card like `card`, every length multiplied by `k`: what layout would make of it at that size.
fn card_at(label: bool, k: f32) -> Div {
    let card = div()
        .w(120.0 * k)
        .h(64.0 * k)
        .rounded(14.0 * k)
        .bg(Color::rgb(0.2, 0.45, 0.9))
        .border(3.0 * k, Color::rgba(1.0, 1.0, 1.0, 0.9))
        .flex_row()
        .items_center()
        .justify_center();
    if label {
        card.child(text("Card").size(18.0 * k).color(Color::WHITE))
    } else {
        card
    }
}

/// A card zoomed from 1x towards `to` by a spring; the scale it has at each frame is recorded.
fn zoom_scene(to: f32, label: bool, seen: Arc<Mutex<Vec<f32>>>) -> Scene {
    Scene {
        name: "zoom",
        steps: 40,
        step_ms: 16,
        build: Box::new(move |ui| {
            let s = bound(ui, 1.0);
            let handle = s.clone();
            let seen = seen.clone();
            ui.mount(move |_| {
                stage().child(
                    div()
                        .w(120.0)
                        .h(64.0)
                        .child(card_at(label, 1.0).motion().scale(s.clone())),
                )
            });
            Box::new(move |_, step| {
                if step == 0 {
                    handle.lock().unwrap().set_target(to);
                }
                seen.lock().unwrap().push(handle.lock().unwrap().get());
            })
        }),
    }
}

/// The same card laid out at `k` times the size, painted once.
fn native_scene(k: f32, label: bool) -> Scene {
    Scene {
        name: "native",
        steps: 1,
        step_ms: 16,
        build: Box::new(move |ui| {
            ui.mount(move |_| {
                stage().child(div().w(120.0 * k).h(64.0 * k).child(card_at(label, k)))
            });
            Box::new(|_, _| {})
        }),
    }
}

/// How sharp the pixels a label adds are: the energy of their gradient over the energy of the
/// pixels. Blur lowers it.
fn sharpness(with: &CapturedFrame, without: &CapturedFrame) -> f64 {
    let (w, h) = (with.width as usize, with.height as usize);
    let ink = |x: usize, y: usize| -> f64 {
        let (p, q) = (pixel(with, x, y), pixel(without, x, y));
        (0..3).map(|i| (p[i] - q[i]).abs() as f64).sum::<f64>()
    };
    let (mut grad, mut energy) = (0.0, 0.0);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let v = ink(x, y);
            let (dx, dy) = (ink(x + 1, y) - ink(x - 1, y), ink(x, y + 1) - ink(x, y - 1));
            grad += dx * dx + dy * dy;
            energy += v * v;
        }
    }
    grad / energy.max(1.0)
}

#[test]
fn zoomed_text_is_as_sharp_as_text_laid_out_at_that_size() {
    let mut report = Vec::new();
    for path in [Path::Vector, Path::Table] {
        let (seen_with, seen_without) = (
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(Vec::new())),
        );
        let (Some(with), Some(without)) = (
            run(&zoom_scene(2.0, true, seen_with.clone()), path),
            run(&zoom_scene(2.0, false, seen_without), path),
        ) else {
            return;
        };
        let scale = *seen_with.lock().unwrap().last().unwrap();
        let (Some(native_with), Some(native_without)) = (
            run(&native_scene(scale, true), Path::Vector),
            run(&native_scene(scale, false), Path::Vector),
        ) else {
            return;
        };
        let zoomed = sharpness(with.last().unwrap(), without.last().unwrap());
        let native = sharpness(&native_with[0], &native_without[0]);
        eprintln!(
            "SHARP {path:?}: scale {scale:.3}, zoomed {zoomed:.3}, native {native:.3}, ratio {:.2}",
            zoomed / native
        );
        report.push((path, zoomed / native));
    }
    for (path, ratio) in report {
        assert!(
            ratio > 0.95,
            "{path:?}: zoomed text is {ratio:.2} as sharp as text laid out at that size"
        );
    }
}

/// A card with a label at rest, under a motion container or not.
fn rest_scene(in_motion: bool, label: bool) -> Scene {
    Scene {
        name: "rest",
        steps: 1,
        step_ms: 16,
        build: Box::new(move |ui| {
            let x = bound(ui, 0.0);
            ui.mount(move |_| {
                let card = card_at(label, 1.0);
                // An odd-sized holder puts the label at a fractional position.
                let holder = div().w(121.0).h(65.0);
                stage().child(if in_motion {
                    holder.child(card.motion().translate_x(x.clone()))
                } else {
                    holder.child(card)
                })
            });
            Box::new(|_, _| {})
        }),
    }
}

#[test]
fn text_under_a_motion_container_is_as_sharp_as_static_text() {
    for density in ["1", "2"] {
        // SAFETY: the test runs alone in its process under nextest.
        unsafe { std::env::set_var("BLINC_ANIM_PATHS_SCALE", density) };
        let mut sharp = Vec::new();
        for in_motion in [false, true] {
            let (Some(with), Some(without)) = (
                run(&rest_scene(in_motion, true), Path::Vector),
                run(&rest_scene(in_motion, false), Path::Vector),
            ) else {
                return;
            };
            sharp.push(sharpness(&with[0], &without[0]));
        }
        eprintln!(
            "REST density {density}: static {:.3}, under a motion container {:.3}, ratio {:.2}",
            sharp[0],
            sharp[1],
            sharp[1] / sharp[0]
        );
        assert!(
            sharp[1] / sharp[0] > 0.97,
            "density {density}: a label under a motion container is {:.2} as sharp as a static one",
            sharp[1] / sharp[0]
        );
    }
}

/// A labelled card moved to a fractional offset by a spring and left to settle.
fn settle_scene(label: bool, dx: f32, to_scale: f32) -> Scene {
    Scene {
        name: "settle",
        steps: 90,
        step_ms: 16,
        build: Box::new(move |ui| {
            let (x, s) = (bound(ui, 0.0), bound(ui, 1.0));
            let handles = (x.clone(), s.clone());
            ui.mount(move |_| {
                stage().child(
                    div().w(120.0).h(64.0).child(
                        card_at(label, 1.0)
                            .motion()
                            .translate_x(x.clone())
                            .scale(s.clone()),
                    ),
                )
            });
            Box::new(move |_, step| {
                if step == 0 {
                    handles.0.lock().unwrap().set_target(dx);
                    handles.1.lock().unwrap().set_target(to_scale);
                }
            })
        }),
    }
}

#[test]
fn text_settled_after_an_animation_is_as_sharp_as_text_at_rest() {
    for (dx, scale) in [(30.37, 1.0), (0.0, 1.5), (23.61, 1.35)] {
        let (Some(with), Some(without)) = (
            run(&settle_scene(true, dx, scale), Path::Table),
            run(&settle_scene(false, dx, scale), Path::Table),
        ) else {
            return;
        };
        let last = with.len() - 1;
        let settled = sharpness(&with[last], &without[last]);
        // The same pose painted fully, with nothing in flight: what the label should look like.
        let (Some(vw), Some(vo)) = (
            run(&settle_scene(true, dx, scale), Path::Vector),
            run(&settle_scene(false, dx, scale), Path::Vector),
        ) else {
            return;
        };
        let painted = sharpness(&vw[last], &vo[last]);
        eprintln!(
            "SETTLE dx {dx} scale {scale}: table {settled:.3}, full paint {painted:.3}, ratio {:.2}",
            settled / painted
        );
        assert!(
            settled / painted > 0.97,
            "dx {dx} scale {scale}: the settled label is {:.2} as sharp as a full paint of that pose",
            settled / painted
        );
    }
}

/// A grid of `count` small cards, each with a translate and a scale of its own and a label,
/// all moving together.
fn grid_scene(count: usize) -> Scene {
    let steps = 60;
    Scene {
        name: "motion grid",
        steps,
        step_ms: 16,
        build: Box::new(move |ui| {
            let values: Vec<_> = (0..count)
                .map(|_| (bound(ui, 0.0), bound(ui, 1.0)))
                .collect();
            let handles = values.clone();
            ui.mount(move |_| {
                let mut grid = div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13));
                grid = grid.flex_row().flex_wrap();
                for (x, s) in &values {
                    let card = div()
                        .w(26.0)
                        .h(14.0)
                        .rounded(3.0)
                        .bg(Color::rgb(0.2, 0.45, 0.9))
                        .border(1.0, Color::rgba(1.0, 1.0, 1.0, 0.9))
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .child(text("ab").size(8.0).color(Color::WHITE));
                    grid = grid.child(
                        div()
                            .w(30.0)
                            .h(16.0)
                            .child(card.motion().translate_x(x.clone()).scale(s.clone())),
                    );
                }
                grid
            });
            Box::new(move |_, step| {
                let t = (step + 1) as f32 / steps as f32;
                for (x, s) in &handles {
                    x.lock().unwrap().set_target(12.0 * t);
                    s.lock().unwrap().set_target(1.0 + 0.2 * t);
                }
            })
        }),
    }
}

/// The wall time of each frame of `scene` painted by `path`, in milliseconds. A frame includes
/// the offscreen render and the read back, which every path pays alike.
fn frame_times(scene: &Scene, path: Path) -> Option<Vec<f64>> {
    let mut ui = session(path)?;
    let mut script = (scene.build)(&mut ui);
    ui.settle();
    let mut rest = ui.snapshot().expect("a frame");
    for _ in 0..3 {
        rest = ui.snapshot().expect("a frame");
    }
    drop(rest);
    let mut times = Vec::new();
    for step in 0..scene.steps {
        script(&mut ui, step);
        ui.advance(scene.step_ms);
        let started = std::time::Instant::now();
        let frame = match path {
            Path::Full | Path::Vector => ui.snapshot(),
            Path::Patched | Path::Table | Path::Rows => ui.snapshot_fast(),
        };
        frame.expect("a frame");
        times.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    Some(times)
}

/// Not a check: what a frame costs on each path with many bound cards. Run with
/// `--run-ignored all --no-capture`.
#[test]
#[ignore]
fn what_a_frame_costs_with_many_bound_cards() {
    for count in [10, 40, 140] {
        let scene = grid_scene(count);
        eprintln!(
            "{count} cards, {} frames each (median / p90 ms):",
            scene.steps
        );
        for (name, path) in [
            ("promoted, full paint", Path::Full),
            ("vectors, full paint", Path::Vector),
            ("promoted, patched", Path::Patched),
            ("rows on the CPU", Path::Rows),
            ("node table", Path::Table),
        ] {
            let Some(mut times) = frame_times(&scene, path) else {
                return;
            };
            // The first frames after a script starts include one-off work.
            times.drain(..3);
            times.sort_by(|a, b| a.total_cmp(b));
            let median = times[times.len() / 2];
            let p90 = times[times.len() * 9 / 10];
            eprintln!("  {name:<22} {median:>7.2} / {p90:>7.2}");
        }
    }
}

/// Pixels that are the image's green, in a frame.
fn green_pixels(frame: &CapturedFrame) -> usize {
    (0..frame.width as usize * frame.height as usize)
        .filter(|i| {
            let at = i * 4;
            frame.data[at] < 60 && frame.data[at + 1] > 200 && frame.data[at + 2] < 60
        })
        .count()
}

/// The image is the card's child, so it is above the card's own background and grows with the
/// card, on every path that paints it as a window does.
#[test]
fn an_image_in_a_moving_card_is_above_the_card_and_grows_with_it() {
    let scene = image_scene("motion, an image", ImageKind::Move);
    for path in [Path::Full, Path::Vector, Path::Table] {
        let Some(frames) = run(&scene, path) else {
            return;
        };
        let first = green_pixels(&frames[0]);
        let last = green_pixels(frames.last().unwrap());
        // 40 x 40 logical pixels at rest, 1.3 times across by the end.
        assert!(
            first >= 1500,
            "{path:?}: the image is not above the card ({first} px)"
        );
        assert!(
            last as f32 >= first as f32 * 1.4,
            "{path:?}: the image did not grow with the card ({first} px, then {last} px)"
        );
    }
}

/// Every bound node is recorded by the paint, whichever batch its content went into: a node that
/// is not recorded is not seen to move once other nodes are moved by rows, and nothing repaints it.
#[test]
fn every_bound_node_is_recorded_whichever_batch_it_is_painted_into() {
    for scene in [
        fade_beside_a_mover_scene(),
        repro_fade_scene(),
        later_tile_scene("fade, a label, a tile after it", true, false, true),
        image_scene("an image", ImageKind::Fade),
    ] {
        for path in [Path::Rows, Path::Full] {
            let Some(mut ui) = session(path) else {
                return;
            };
            let _ = (scene.build)(&mut ui);
            ui.settle();
            ui.snapshot().expect("a frame");
            let bound = ui.tree().motion_bindings_map().len();
            let recorded = ui.tree().composite_bindings().len();
            assert_eq!(
                recorded, bound,
                "{} ({path:?}): {recorded} of {bound} bound nodes are recorded",
                scene.name
            );
        }
    }
}

/// Not a check: what a scrolled frame costs on each path. Run with
/// `--run-ignored all --no-capture`.
#[test]
#[ignore]
fn what_a_scrolled_frame_costs() {
    for rows in [60, 400] {
        let scene = scroll_scene("scroll", rows, 12.0, ScrollbarVisibility::Auto);
        eprintln!(
            "{rows} rows, {} frames each (median / p90 ms):",
            scene.steps
        );
        for (name, path) in [
            ("promoted, full paint", Path::Full),
            ("vectors, full paint", Path::Vector),
            ("rows on the CPU", Path::Rows),
            ("node table", Path::Table),
        ] {
            let Some(mut times) = frame_times(&scene, path) else {
                return;
            };
            times.drain(..2);
            times.sort_by(|a, b| a.total_cmp(b));
            let median = times[times.len() / 2];
            let p90 = times[times.len() * 9 / 10];
            eprintln!("  {name:<22} {median:>7.2} / {p90:>7.2}");
        }
    }
}

/// The scroll scene really scrolls, and every path shows the same frames.
#[test]
fn scrolling_moves_the_list_and_every_path_agrees() {
    let scene = scroll_scene("scroll", 60, 12.0, ScrollbarVisibility::Auto);
    let (Some(full), Some(rows), Some(table)) = (
        run(&scene, Path::Vector),
        run(&scene, Path::Rows),
        run(&scene, Path::Table),
    ) else {
        return;
    };
    let first = &full[0];
    let last = full.last().unwrap();
    assert!(
        difference(first, last, LEVELS).differing > 2000,
        "the list did not scroll"
    );
    for (name, frames) in [("rows", &rows), ("table", &table)] {
        for (i, (a, b)) in full.iter().zip(frames).enumerate() {
            let d = difference(a, b, LEVELS);
            assert_eq!(
                d.differing, 0,
                "{name}: step {i} differs by {} px",
                d.differing
            );
        }
    }
}

/// The scrollbar of a container on rows is drawn from its configuration and the physics, as the
/// walker draws it for the rest: it is there, it follows the scroll, and it is the same pixels.
#[test]
fn the_scrollbar_of_a_container_on_rows_is_drawn_and_follows_the_scroll() {
    for visibility in [ScrollbarVisibility::Always, ScrollbarVisibility::Never] {
        let scene = scroll_scene("scroll", 120, 30.0, visibility);
        let (Some(reference), Some(rows)) = (run(&scene, Path::Vector), run(&scene, Path::Rows))
        else {
            return;
        };
        // A column through the track, which is the container's edge, as a run of pixels.
        let track = |frame: &CapturedFrame| -> Vec<[i32; 3]> {
            (0..H as usize)
                .map(|y| pixel(frame, W as usize - 7, y))
                .collect()
        };
        let drawn = |frame: &CapturedFrame| {
            let column = track(frame);
            column.iter().any(|p| *p != column[H as usize / 2])
        };
        for (i, (a, b)) in reference.iter().zip(&rows).enumerate() {
            assert_eq!(track(a), track(b), "{visibility:?}: step {i}");
        }
        match visibility {
            ScrollbarVisibility::Never => {
                assert!(
                    !rows.iter().any(drawn),
                    "a scrollbar that is never shown was drawn"
                );
            }
            _ => {
                assert!(rows.iter().all(drawn), "the scrollbar is not drawn");
                assert_ne!(
                    track(&rows[0]),
                    track(rows.last().unwrap()),
                    "the thumb did not follow the scroll"
                );
            }
        }
    }
}

/// A FLIP slide plays on rows: the cards move for most of the frames and the frames the walker
/// paints are the one where layout changed and the one where the slide ends.
#[test]
fn a_flip_slide_is_moved_by_rows() {
    for scene in [flip_scene("flip", true), flip_scene("flip, no text", false)] {
        let Some((frames, paths)) = run_logged(&scene, Path::Rows) else {
            return;
        };
        let moved = frames.windows(2).filter(|w| w[0].data != w[1].data).count();
        assert!(
            moved >= 12,
            "{}: the picture changed in only {moved} frames: it does not slide",
            scene.name
        );
        let by_rows = paths.iter().filter(|p| p.ends_with("rows:applied")).count();
        let full = paths.iter().filter(|p| p.starts_with("full")).count();
        assert!(
            by_rows >= 14 && full <= 3,
            "{}: {by_rows} frames moved by rows, {full} painted in full: {paths:?}",
            scene.name
        );
    }
}

/// A banner that appears above a wrapped grid of `count` tiles and pushes every one down, each
/// sliding from where it was (FLIP).
fn flip_grid_scene(count: usize) -> Scene {
    Scene {
        name: "flip grid",
        steps: 24,
        step_ms: 16,
        build: Box::new(move |ui| {
            let open = signal(false);
            ui.css(FLIP_CSS);
            ui.mount(move |_| {
                let mut grid = div()
                    .w(W as f32)
                    .h(H as f32)
                    .bg(Color::rgb(0.09, 0.1, 0.13))
                    .flex_row()
                    .flex_wrap()
                    .gap(4.0)
                    .p(4.0)
                    .when(open, |d| {
                        d.child(
                            div()
                                .w_full()
                                .h(40.0)
                                .rounded(8.0)
                                .bg(Color::rgb(0.9, 0.5, 0.2)),
                        )
                    });
                for i in 0..count {
                    grid = grid.child(
                        div()
                            .id(format!("tile-{i}"))
                            .class("slide")
                            .w(34.0)
                            .h(34.0)
                            .rounded(6.0)
                            .bg(Color::rgb(0.2, 0.45, 0.9))
                            .border(2.0, Color::rgba(1.0, 1.0, 1.0, 0.8))
                            .child(text(format!("{i}")).size(11.0).color(Color::WHITE)),
                    );
                }
                grid
            });
            Box::new(move |_, step| {
                if step == 0 {
                    open.set(true);
                }
            })
        }),
    }
}

/// Not a check: what a frame of a FLIP slide costs, written into render props and painted in
/// full, against moved by rows. Run with `--run-ignored all --no-capture`.
#[test]
#[ignore]
fn what_a_flip_frame_costs() {
    for count in [20, 48, 90] {
        let scene = flip_grid_scene(count);
        eprintln!(
            "{count} tiles, {} frames each (median / p90 ms):",
            scene.steps
        );
        for (name, path) in [
            ("props, full paint", Path::Full),
            ("rows on the CPU", Path::Rows),
            ("node table", Path::Table),
        ] {
            let Some(mut times) = frame_times(&scene, path) else {
                return;
            };
            times.drain(..3);
            times.sort_by(|a, b| a.total_cmp(b));
            eprintln!(
                "  {name:<20} {:>7.2} / {:>7.2}",
                times[times.len() / 2],
                times[times.len() * 9 / 10]
            );
        }
    }
}
